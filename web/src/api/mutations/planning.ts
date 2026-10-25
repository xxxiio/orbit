/**
 * The explicit plan review and its approval.
 *
 * The review is a read-only `POST`, so it deliberately does not use the
 * mutation disposition model: nothing was written. Approval applies exactly the
 * command that was reviewed, echoing its id and fingerprint.
 */
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { ApiError, classifyError } from '../errors'
import { handleUnauthorized } from '../session-store'
import { queryKeys } from '../queries'
import type { PlanReview, ScheduleApprovalRequest } from '../contracts'
import { applyApprovedPlan, reviewPlan, type ReviewedSchedulePlan } from '../endpoints/planning'
import { toApprovedScheduleReceipt, type ApprovedScheduleOutcome } from '../adapters'
import { acceptLedgerRevision } from '../accepted-write'
import { useApiMutation, type CommandMutation } from './core'

/**
 * The review is a read-only `POST` (it needs the same CSRF/session path), so it
 * deliberately does not use the mutation disposition model: there is no
 * `accepted`, because nothing was written. `review` is the server's `PlanReview`
 * verbatim, including `fingerprint`, `command_id` and
 * `requires_explicit_approval`; `reviewedPlan` keeps that review paired with the
 * exact command it reviewed so it can be applied without being rebuilt.
 */
export interface PlanReviewResult {
  reviewedPlan: ReviewedSchedulePlan | null
  review: PlanReview | null
  fingerprint: string | null
  requiresExplicitApproval: boolean | null
  isPending: boolean
  error: ApiError | null
  reviewPlan: (request: ScheduleApprovalRequest) => void
  reviewPlanAsync: (request: ScheduleApprovalRequest) => Promise<ReviewedSchedulePlan>
  reset: () => void
}

export function usePlanReview(): PlanReviewResult {
  const queryClient = useQueryClient()
  const mutation = useMutation<ReviewedSchedulePlan, ApiError, ScheduleApprovalRequest>({
    mutationFn: async (request) => {
      try {
        const review = await reviewPlan(request)
        return { plan: request, review }
      } catch (error) {
        if (error instanceof ApiError) throw error
        throw new ApiError(0, 'Could not reach the Orbit server.', error)
      }
    },
    onError: (error) => {
      if (classifyError(error) === 'unauthorized') {
        handleUnauthorized(queryClient)
      }
    },
  })

  return {
    reviewedPlan: mutation.data ?? null,
    review: mutation.data?.review ?? null,
    fingerprint: mutation.data?.review.fingerprint ?? null,
    requiresExplicitApproval: mutation.data?.review.requires_explicit_approval ?? null,
    isPending: mutation.isPending,
    error: mutation.error ?? null,
    reviewPlan: (request) => mutation.mutate(request),
    reviewPlanAsync: mutation.mutateAsync,
    reset: mutation.reset,
  }
}

/**
 * Apply exactly the reviewed command. The variables carry the `PlanReview`, so
 * there is no way to apply a plan that was not reviewed in this session; the
 * endpoint refuses locally (typed error) when the review is missing, has no
 * fingerprint, or is not marked `requires_explicit_approval`. The command id is
 * echoed from the review and never regenerated, and a replayed receipt surfaces
 * `replayed: true`.
 */
export function useApplyApprovedPlan(): CommandMutation<
  ApprovedScheduleOutcome,
  ReviewedSchedulePlan | null
> {
  return useApiMutation<ApprovedScheduleOutcome, ReviewedSchedulePlan | null>({
    // The write may have landed even if the response was lost.
    uncertainOnError: true,
    mutationFn: (reviewed) => applyApprovedPlan(reviewed).then(toApprovedScheduleReceipt),
    affectedKeys: () => [
      queryKeys.snapshot(),
      queryKeys.taskReadinessAll(),
      queryKeys.nextActionsAll(),
      queryKeys.taskGraphAll(),
      queryKeys.dayPlanAll(),
      queryKeys.schedulePlanAll(),
      queryKeys.workBriefAll(),
      queryKeys.portfolioCapacityAll(),
      queryKeys.workloadAll(),
    ],
    onAccepted: (outcome, _reviewed, queryClient) => {
      acceptLedgerRevision(queryClient, outcome.acceptedRevision)
    },
  })
}
