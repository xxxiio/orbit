/**
 * Record one review decision for a job's latest handoff.
 *
 * A review is a write, not a launch: approving never completes the Orbit task,
 * and the copy says so. The attempt is pinned per (job, handoff, decision,
 * reason) so an unconfirmed retry replays the same review instead of asking the
 * server to record a conflicting second one; only an explicit refresh that
 * finds no review discards it.
 */
import { useEffect, useState } from 'react'
import type { AgentHandoff, AgentReview } from '@/api/adapters'
import type { TaskStatus } from '@/api/contracts'
import {
  uncertainMessage,
  useAgentReview,
  useIdempotentAttempt,
  useSubmitAgentReview,
  userMessage,
  type ApiError,
} from '@/hooks'
import { formatMoment } from '@/domain'
import { statusLabel } from '@/features/tasks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Section } from '@/components/ui/section'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/components/ui/utils'
import { reviewDecisionLabel } from '@/features/agents/agent-model'

interface ReviewAttempt {
  key: string
  atMs: number
}

export function ReviewSection({
  jobId,
  handoffs,
  review,
  taskStatus,
  onRefresh,
}: {
  jobId: string
  handoffs: AgentHandoff[]
  review: ReturnType<typeof useAgentReview>['data']
  taskStatus: TaskStatus | null
  onRefresh: () => Promise<{ ok: boolean; review: AgentReview | null }>
}) {
  const submit = useSubmitAgentReview()
  const [decision, setDecision] = useState<'approved' | 'changes_requested'>('approved')
  const [reason, setReason] = useState('')
  const [localError, setLocalError] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const [refreshing, setRefreshing] = useState(false)
  const attempt = useIdempotentAttempt<ReviewAttempt>((key) => ({ key, atMs: Date.now() }))

  const latest = handoffs[handoffs.length - 1]
  const key = `${jobId}\u0000${latest?.handoffId ?? ''}\u0000${decision}\u0000${reason}`
  const uncertain = submit.disposition === 'uncertain'
  const refused = submit.disposition === 'rejected'
  const blindRetry = uncertain && attempt.isStale(key)
  const commandError: ApiError | null = uncertain || refused ? submit.error : null

  useEffect(() => {
    if (!announcement) return
    const timer = window.setTimeout(() => setAnnouncement(null), 6000)
    return () => window.clearTimeout(timer)
  }, [announcement])

  /**
   * An explicit refresh is the way out of an unconfirmed review. If the server
   * still has no review for this job, the pinned attempt did not land, so it is
   * safe to drop the key and let the person choose another decision. If a review
   * does exist, the recorded-review branch replaces this form.
   */
  const refresh = async () => {
    setRefreshing(true)
    try {
      const current = await onRefresh()
      if (!current.ok) {
        setLocalError('The job could not be refreshed. Try again before recording a decision.')
        return
      }
      if (current.review) {
        setAnnouncement('Refreshed: the review on record is shown below.')
      } else if (attempt.attempt) {
        attempt.clear()
        submit.reset()
        setLocalError(null)
        setAnnouncement(
          'Refreshed: no review is on record. The unconfirmed attempt was discarded; you can record a decision.',
        )
      } else {
        setAnnouncement('Refreshed: no review is on record yet.')
      }
    } catch {
      setLocalError('The job could not be refreshed. Try again before recording a decision.')
    } finally {
      setRefreshing(false)
    }
  }

  const record = async () => {
    if (!latest) {
      setLocalError('A review needs a persisted handoff. This job has none yet.')
      return
    }
    if (reason.trim().length === 0) {
      setLocalError('Add a short reason for the decision.')
      return
    }
    if (blindRetry) return
    setLocalError(null)
    const pinned = attempt.ensure(key)
    try {
      await submit.mutateAsync({
        jobId,
        handoffId: latest.handoffId,
        decision,
        reason: reason.trim(),
        atMs: pinned.atMs,
      })
      attempt.clear()
      setReason('')
      setAnnouncement(
        decision === 'approved'
          ? 'Review recorded as approved. The task was not completed.'
          : 'Review recorded as changes requested. The task is unchanged.',
      )
      void onRefresh()
    } catch {
      // The server's answer is rendered below.
    }
  }

  if (review) {
    return (
      <Section
        title="Review"
        description="The server records one review for this job. Approving a handoff never completes the Orbit task."
        aside={
          <Badge tone={review.decision === 'approved' ? 'success' : 'warning'}>
            {reviewDecisionLabel(review.decision)}
          </Badge>
        }
      >
        <dl className="grid gap-x-4 gap-y-2 sm:grid-cols-2">
          <div className="min-w-0">
            <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">Reviewer</dt>
            <dd className="font-mono text-xs text-ink-soft">{review.reviewer}</dd>
          </div>
          <div className="min-w-0">
            <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">Recorded</dt>
            <dd className="text-sm text-ink-soft">{formatMoment(review.reviewedAtMs)}</dd>
          </div>
          <div className="min-w-0 sm:col-span-2">
            <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">Reason</dt>
            <dd className="whitespace-pre-wrap text-sm leading-6 text-ink-soft">{review.reason}</dd>
          </div>
          <div className="min-w-0 sm:col-span-2">
            <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">Task effect</dt>
            <dd className="text-sm leading-6 text-ink-soft">
              <Badge tone={review.taskCompleted ? 'danger' : 'neutral'}>
                {review.taskCompleted ? 'Task completed' : 'Task not completed'}
              </Badge>{' '}
              A review decision is separate from task acceptance
              {taskStatus ? `; the task is still ${statusLabel(taskStatus)}` : ''}.
            </dd>
          </div>
        </dl>
        {review.decision === 'changes_requested' ? (
          <p className="mt-3 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink">
            Changes were requested. Resume below creates a new job for an executor; it does not
            launch one, and it does not infer task completion.
          </p>
        ) : (
          <p className="mt-3 text-xs leading-5 text-ink-muted">
            A conflicting second decision is refused by the server. This page does not offer one.
          </p>
        )}
        <p role="status" aria-live="polite" className="mt-2 min-h-4 text-2xs text-ink-muted">
          {announcement}
        </p>
      </Section>
    )
  }

  return (
    <Section
      title="Review"
      description="Read the latest handoff, then record one decision with a reason. The server refuses a conflicting second review."
      aside={latest ? <Badge tone="accent">Review available</Badge> : <Badge tone="neutral">No handoff</Badge>}
    >
      <div className="rounded-md border border-info/30 bg-info-soft/50 px-3 py-2 text-xs leading-5 text-info-ink">
        <p className="font-medium">Approving records a review. It does not complete the task.</p>
        <p className="mt-0.5">
          Task acceptance stays on the task itself{taskStatus ? ` (currently ${statusLabel(taskStatus)})` : ''}.
        </p>
      </div>

      <div className="mt-3 flex flex-wrap gap-2" role="group" aria-label="Review decision">
        <button
          type="button"
          onClick={() => setDecision('approved')}
          aria-pressed={decision === 'approved'}
          className={cn(
            'rounded-md border px-3 py-1.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
            decision === 'approved'
              ? 'border-success/50 bg-success-soft text-success-ink'
              : 'border-line bg-surface text-ink-soft hover:border-line-strong',
          )}
          disabled={!latest || submit.isPending}
        >
          Approve
        </button>
        <button
          type="button"
          onClick={() => setDecision('changes_requested')}
          aria-pressed={decision === 'changes_requested'}
          className={cn(
            'rounded-md border px-3 py-1.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
            decision === 'changes_requested'
              ? 'border-warning/50 bg-warning-soft text-warning-ink'
              : 'border-line bg-surface text-ink-soft hover:border-line-strong',
          )}
          disabled={!latest || submit.isPending}
        >
          Request changes
        </button>
      </div>

      <div className="mt-3">
        <label htmlFor="review-reason" className="text-sm font-medium text-ink">
          Reason <span aria-hidden="true" className="text-danger">*</span>
        </label>
        <Textarea
          id="review-reason"
          value={reason}
          onChange={(event) => {
            setReason(event.target.value)
            setLocalError(null)
          }}
          placeholder={
            decision === 'approved'
              ? 'What did you verify before approving?'
              : 'What needs to change before this can land?'
          }
          disabled={!latest || submit.isPending}
          className="mt-1.5 min-h-[4.5rem]"
        />
      </div>

      {!latest ? (
        <p className="mt-2 text-xs leading-5 text-warning-ink">
          A review needs a persisted handoff. Nothing can be recorded for this job yet.
        </p>
      ) : null}

      {localError ? (
        <p role="alert" className="mt-2 text-xs leading-5 text-danger-ink">
          {localError}
        </p>
      ) : null}

      {commandError ? (
        <div
          role="alert"
          className={cn(
            'mt-2 rounded-md border px-3 py-2 text-xs leading-5',
            uncertain
              ? 'border-warning/40 bg-warning-soft text-warning-ink'
              : 'border-danger/30 bg-danger-soft text-danger-ink',
          )}
        >
          <p className="font-medium">
            {uncertain ? 'The review was not confirmed.' : 'The server refused this review.'}
          </p>
          <p className="mt-0.5">
            {uncertain ? uncertainMessage(commandError) : userMessage(commandError)}
          </p>
          <p className="mt-1">
            Retry the same review unchanged, or refresh the job. A refresh that finds no review
            discards this unconfirmed attempt so you can choose a different decision.
          </p>
        </div>
      ) : null}

      {blindRetry ? (
        <p role="alert" className="mt-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink">
          These fields no longer match the review whose outcome is unknown. Retry it unchanged, or
          refresh the job. If no review is on record, the refresh discards the unconfirmed attempt
          and this form accepts a new decision.
        </p>
      ) : null}

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          onClick={() => void record()}
          loading={submit.isPending}
          disabled={!latest || blindRetry}
        >
          {uncertain && !blindRetry ? 'Retry the same review' : 'Record review'}
        </Button>
        <Button
          size="sm"
          variant="quiet"
          onClick={() => void refresh()}
          loading={refreshing}
          disabled={submit.isPending}
        >
          Refresh the job
        </Button>
      </div>

      <p role="status" aria-live="polite" className="mt-2 min-h-4 text-2xs text-ink-muted">
        {announcement}
      </p>
    </Section>
  )
}
