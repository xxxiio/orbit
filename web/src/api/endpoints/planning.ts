/**
 * Planning reads + the explicit approval gate.
 *
 * Two rules are enforced here, not in the UI:
 *
 * 1. **Every required query parameter is caller-supplied.** `now_ms` is never
 *    read from `Date.now()` and `available_minutes` is never guessed. A missing
 *    or non-finite required value throws a typed `ClientStateError` before any
 *    request is made, so the wire never carries `undefined`.
 * 2. **Review and apply are one command.** `reviewPlan` returns the server's
 *    `PlanReview`; `applyApprovedPlan` echoes that review (its fingerprint and
 *    command id) and the exact reviewed window/items. It never regenerates the
 *    command id, and refuses locally when the review is absent or is not marked
 *    as requiring explicit approval.
 */
import { api } from '../client'
import { ClientStateError } from '../errors'
import { withQuery } from '../http'
import type {
  ApprovedScheduleReceipt,
  ApprovedScheduleRequest,
  DayPlan,
  PlanReview,
  PortfolioCapacityPlan,
  ProjectDependencyReport,
  ScheduleApprovalRequest,
  SchedulePlan,
  TaskGraphReport,
  WorkBrief,
  WorkloadInsights,
} from '../contracts'

const BASE = '/authoritative'

/* ------------------------------------------------------------------ */
/* Required-parameter guards                                           */
/* ------------------------------------------------------------------ */

function requireNumber(label: string, value: number | null | undefined): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new ClientStateError(`${label} is required and must be a number.`)
  }
  return Math.trunc(value)
}

function requirePositiveInteger(label: string, value: number | null | undefined): number {
  const number = requireNumber(label, value)
  if (number <= 0) {
    throw new ClientStateError(`${label} must be greater than zero.`)
  }
  return number
}

function optionalPositiveInteger(label: string, value: number | undefined): number | undefined {
  if (value === undefined) return undefined
  return requirePositiveInteger(label, value)
}

function optionalId(label: string, value: string | undefined): string | undefined {
  if (value === undefined) return undefined
  const trimmed = value.trim()
  if (trimmed.length === 0) {
    throw new ClientStateError(`${label} is not valid.`)
  }
  return trimmed
}

function requireWindow(startsAtMs: number, endsAtMs: number): { startsAtMs: number; endsAtMs: number } {
  const starts = requireNumber('A planning window start', startsAtMs)
  const ends = requireNumber('A planning window end', endsAtMs)
  if (ends <= starts) {
    throw new ClientStateError('The planning window must end after it starts.')
  }
  return { startsAtMs: starts, endsAtMs: ends }
}

function requireCommandId(value: string): string {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new ClientStateError('A reviewed command identity is required.')
  }
  if (value.length > 128) {
    throw new ClientStateError('That command identity is too long.')
  }
  return value
}

/* ------------------------------------------------------------------ */
/* Dependency / graph reads                                            */
/* ------------------------------------------------------------------ */

/**
 * `now_ms` is caller-supplied so an online evaluation and an offline one agree
 * on the exact observation time. The client never computes the graph itself.
 */
export async function fetchTaskGraph(nowMs: number, signal?: AbortSignal): Promise<TaskGraphReport> {
  return api.get<TaskGraphReport>(
    withQuery(`${BASE}/task-graph`, { now_ms: requireNumber('A reference time', nowMs) }),
    { signal },
  )
}

export interface ProjectDependenciesParams {
  nowMs: number
  projectId?: string
  includeUnassigned?: boolean
}

export async function fetchProjectDependencies(
  { nowMs, projectId, includeUnassigned }: ProjectDependenciesParams,
  signal?: AbortSignal,
): Promise<ProjectDependencyReport> {
  return api.get<ProjectDependencyReport>(
    withQuery(`${BASE}/project-dependencies`, {
      now_ms: requireNumber('A reference time', nowMs),
      project_id: optionalId('That project', projectId),
      include_unassigned: includeUnassigned,
    }),
    { signal },
  )
}

/* ------------------------------------------------------------------ */
/* Day plan, work brief, workload                                      */
/* ------------------------------------------------------------------ */

export interface DayPlanParams {
  nowMs: number
  /** Authored capacity: never defaulted to a guess. */
  availableMinutes: number
  maxTasks?: number
  projectId?: string
  includeUnsized?: boolean
  defaultMinutes?: number
}

export async function fetchDayPlan(
  { nowMs, availableMinutes, maxTasks, projectId, includeUnsized, defaultMinutes }: DayPlanParams,
  signal?: AbortSignal,
): Promise<DayPlan> {
  return api.get<DayPlan>(
    withQuery(`${BASE}/day-plan`, {
      now_ms: requireNumber('A reference time', nowMs),
      available_minutes: requirePositiveInteger('Available minutes', availableMinutes),
      max_tasks: optionalPositiveInteger('The task limit', maxTasks),
      project_id: optionalId('That project', projectId),
      include_unsized: includeUnsized,
      default_minutes: defaultMinutes === undefined ? undefined : requireNumber('A default estimate', defaultMinutes),
    }),
    { signal },
  )
}

export interface WorkBriefParams {
  nowMs: number
  endsAtMs: number
  limit?: number
  projectId?: string
  defaultMinutes?: number
}

export async function fetchWorkBrief(
  { nowMs, endsAtMs, limit, projectId, defaultMinutes }: WorkBriefParams,
  signal?: AbortSignal,
): Promise<WorkBrief> {
  const window = requireWindow(nowMs, endsAtMs)
  return api.get<WorkBrief>(
    withQuery(`${BASE}/work-brief`, {
      now_ms: window.startsAtMs,
      ends_at_ms: window.endsAtMs,
      limit: optionalPositiveInteger('The task limit', limit),
      project_id: optionalId('That project', projectId),
      default_minutes: defaultMinutes === undefined ? undefined : requireNumber('A default estimate', defaultMinutes),
    }),
    { signal },
  )
}

export interface WorkloadParams {
  nowMs: number
  /** Defaults to 7 days on the server (604800000). */
  horizonMs?: number
}

export async function fetchWorkload(
  { nowMs, horizonMs }: WorkloadParams,
  signal?: AbortSignal,
): Promise<WorkloadInsights> {
  return api.get<WorkloadInsights>(
    withQuery(`${BASE}/workload`, {
      now_ms: requireNumber('A reference time', nowMs),
      horizon_ms: optionalPositiveInteger('The workload horizon', horizonMs),
    }),
    { signal },
  )
}

/* ------------------------------------------------------------------ */
/* Schedule plan + portfolio capacity                                  */
/* ------------------------------------------------------------------ */

export interface SchedulePlanParams {
  startsAtMs: number
  endsAtMs: number
  maxTasks?: number
  projectId?: string
  defaultMinutes?: number
}

/** A read-only proposal. The server computes every slot; nothing is reserved. */
export async function fetchSchedulePlan(
  { startsAtMs, endsAtMs, maxTasks, projectId, defaultMinutes }: SchedulePlanParams,
  signal?: AbortSignal,
): Promise<SchedulePlan> {
  const window = requireWindow(startsAtMs, endsAtMs)
  return api.get<SchedulePlan>(
    withQuery(`${BASE}/schedule-plan`, {
      starts_at_ms: window.startsAtMs,
      ends_at_ms: window.endsAtMs,
      max_tasks: optionalPositiveInteger('The task limit', maxTasks),
      project_id: optionalId('That project', projectId),
      default_minutes: defaultMinutes === undefined ? undefined : requireNumber('A default estimate', defaultMinutes),
    }),
    { signal },
  )
}

export interface PortfolioCapacityParams {
  nowMs: number
  availableMinutes: number
  maxTasks?: number
  projectId?: string
  includeUnassigned?: boolean
  includeUnsized?: boolean
  defaultMinutes?: number
}

export async function fetchPortfolioCapacity(
  {
    nowMs,
    availableMinutes,
    maxTasks,
    projectId,
    includeUnassigned,
    includeUnsized,
    defaultMinutes,
  }: PortfolioCapacityParams,
  signal?: AbortSignal,
): Promise<PortfolioCapacityPlan> {
  return api.get<PortfolioCapacityPlan>(
    withQuery(`${BASE}/portfolio-capacity`, {
      now_ms: requireNumber('A reference time', nowMs),
      available_minutes: requirePositiveInteger('Available minutes', availableMinutes),
      max_tasks: optionalPositiveInteger('The task limit', maxTasks),
      project_id: optionalId('That project', projectId),
      include_unassigned: includeUnassigned,
      include_unsized: includeUnsized,
      default_minutes: defaultMinutes === undefined ? undefined : requireNumber('A default estimate', defaultMinutes),
    }),
    { signal },
  )
}

/* ------------------------------------------------------------------ */
/* Explicit approval gate                                              */
/* ------------------------------------------------------------------ */

/**
 * The review *and* the exact command it reviewed. The server's `PlanReview`
 * does not repeat the window/items, so keeping the pair together is the only
 * way to apply exactly what was shown.
 */
export interface ReviewedSchedulePlan {
  plan: ScheduleApprovalRequest
  review: PlanReview
}

function validateApprovalRequest(request: ScheduleApprovalRequest): ScheduleApprovalRequest {
  const window = requireWindow(request.window_start_ms, request.window_end_ms)
  const expectedRevision = requireNumber('The accepted revision', request.expected_revision)
  if (!Number.isInteger(expectedRevision) || expectedRevision < 0) {
    throw new ClientStateError('The accepted revision is not valid. Reload the workspace and review again.')
  }
  if (!Array.isArray(request.items) || request.items.length === 0) {
    throw new ClientStateError('There is nothing to approve in this plan.')
  }
  return {
    command_id: requireCommandId(request.command_id),
    expected_revision: expectedRevision,
    window_start_ms: window.startsAtMs,
    window_end_ms: window.endsAtMs,
    items: request.items,
  }
}

/** Review never writes: it returns the typed, fingerprinted command. */
export async function reviewPlan(
  request: ScheduleApprovalRequest,
  signal?: AbortSignal,
): Promise<PlanReview> {
  return api.post<PlanReview>(`${BASE}/plan-review`, validateApprovalRequest(request), { signal })
}

/**
 * Build the apply body from a review. Refuses with a typed client error when
 * the review is missing, was not marked as requiring explicit approval, has no
 * fingerprint, or does not match the reviewed command. The command id is always
 * the reviewed one; this function can only fail, never mint a replacement.
 */
export function buildApprovedScheduleRequest(
  reviewed: ReviewedSchedulePlan | null | undefined,
): ApprovedScheduleRequest {
  if (!reviewed || !reviewed.review || !reviewed.plan) {
    throw new ClientStateError('Review the plan before approving it.')
  }
  const { plan, review } = reviewed

  if (review.requires_explicit_approval !== true) {
    throw new ClientStateError(
      'This plan is not marked as requiring explicit approval, so it cannot be applied from here.',
    )
  }
  if (typeof review.fingerprint !== 'string' || review.fingerprint.trim().length === 0) {
    throw new ClientStateError('The reviewed plan has no fingerprint. Review the plan again.')
  }
  if (requireCommandId(review.command_id) !== plan.command_id) {
    throw new ClientStateError('The approval does not match the reviewed plan. Review the plan again.')
  }
  if (review.expected_revision !== plan.expected_revision) {
    throw new ClientStateError('The approval is bound to a different revision. Review the plan again.')
  }

  const validated = validateApprovalRequest(plan)
  return {
    plan: {
      command_id: review.command_id,
      expected_revision: review.expected_revision,
      window_start_ms: validated.window_start_ms,
      window_end_ms: validated.window_end_ms,
      items: validated.items,
    },
    approval: {
      review,
      approved: true,
    },
  }
}

/** The one real write in this module; only a reviewed command can reach it. */
export async function applyApprovedPlan(
  reviewed: ReviewedSchedulePlan | null | undefined,
  signal?: AbortSignal,
): Promise<ApprovedScheduleReceipt> {
  return api.post<ApprovedScheduleReceipt>(
    `${BASE}/plan-apply-approved`,
    buildApprovedScheduleRequest(reviewed),
    { signal },
  )
}
