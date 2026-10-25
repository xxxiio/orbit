/**
 * One agent job: state, runtime references, handoffs, review and resume context.
 *
 * The page is read-mostly. The only writes are an explicit review decision and
 * an explicit resume that queues a new job. Neither launches an executor and
 * neither completes the Orbit task; the copy says both facts on screen.
 */
import { useEffect, useReducer, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import {
  isApiError,
  newCommandId,
  uncertainMessage,
  useAgentHandoffs,
  useAgentJob,
  useAgentOverview,
  useAgentResumeContext,
  useAgentReview,
  useDocumentTitle,
  useResumeAgentJob,
  useSubmitAgentReview,
  useWorkspaceSnapshot,
  userMessage,
  type ApiError,
} from '@/hooks'
import type { AgentHandoff, AgentReview } from '@/api/adapters'
import type { TaskStatus } from '@/api/contracts'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import {
  AgentsIcon,
  AlertCircleIcon,
  ArrowLeftIcon,
  ClockIcon,
  FolderIcon,
  LoaderIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { PageHeader } from '@/components/ui/page-header'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Section } from '@/components/ui/section'
import { SkeletonText } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/components/ui/utils'
import { formatMoment, statusLabel } from '@/features/tasks'
import { useTaskDrawer } from '@/features/tasks'
import {
  agentKindLabel,
  agentStateLabel,
  agentStateTone,
  isAgentJobTerminal,
  nextActionExplanation,
  nextActionLabel,
  reviewDecisionLabel,
  shortAgentId,
} from './agent-model'

function JobSkeleton() {
  return (
    <div className="flex flex-col gap-5" aria-busy="true" aria-label="Loading agent job">
      <SkeletonText lines={2} />
      <div className="rounded-lg border border-line p-5">
        <SkeletonText lines={5} />
      </div>
      <div className="rounded-lg border border-line p-5">
        <SkeletonText lines={4} />
      </div>
    </div>
  )
}

function NotVisiblePanel({ id }: { id: string }) {
  return (
    <Panel>
      <EmptyState
        icon={<AgentsIcon size={21} />}
        title="This job is not visible to this device"
        description={
          <>
            Agent jobs belong to the device that started them. The server answered 403 for{' '}
            <span className="font-mono text-ink-soft">{id}</span>; it does not confirm whether the
            job exists on another device.
          </>
        }
        action={
          <Button asChild variant="secondary" size="sm">
            <Link to="/agents">Back to Agents</Link>
          </Button>
        }
        className="min-h-[20rem]"
      />
    </Panel>
  )
}

function JsonBlock({ label, value }: { label: string; value: unknown }) {
  let text: string
  try {
    text = JSON.stringify(value, null, 2) ?? 'null'
  } catch {
    text = String(value)
  }
  return (
    <div className="min-w-0">
      <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</p>
      <pre className="scrollbar-slim mt-1 max-h-72 overflow-auto rounded-md border border-line bg-surface-sunken p-3 font-mono text-2xs leading-5 text-ink-soft">
        {text}
      </pre>
    </div>
  )
}

function HandoffList({ handoffs }: { handoffs: AgentHandoff[] }) {
  if (handoffs.length === 0) {
    return (
      <p className="text-xs leading-5 text-ink-muted">
        This job has no handoffs yet. A review needs a persisted handoff; the executor submits it.
      </p>
    )
  }
  return (
    <ul aria-label="Handoffs" className="flex flex-col gap-2">
      {handoffs.map((handoff, index) => (
        <li key={handoff.handoffId} className="rounded-md border border-line bg-surface-sunken/25 px-3 py-2.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-sm font-medium text-ink">
              {index === handoffs.length - 1 ? 'Latest handoff' : `Handoff ${handoffs.length - index}`}
            </span>
            <span className="font-mono text-2xs text-ink-muted">{handoff.handoffId}</span>
            <span className="ml-auto inline-flex items-center gap-1 text-2xs text-ink-faint">
              <ClockIcon size={11} />
              {formatMoment(handoff.reportedAtMs)}
            </span>
          </div>
          <JsonBlock label="Payload" value={handoff.payload} />
        </li>
      ))}
    </ul>
  )
}

function ReviewSection({
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
  const attemptRef = useRef<{ key: string; atMs: number } | null>(null)
  // Clearing `attemptRef` does not re-render, and `blindRetry` is derived during
  // render; this forces the form to notice that the pinned attempt is gone.
  const [, bumpAttempt] = useReducer((value: number) => value + 1, 0)

  const latest = handoffs[handoffs.length - 1]
  const key = `${jobId}\u0000${latest?.handoffId ?? ''}\u0000${decision}\u0000${reason}`
  const uncertain = submit.disposition === 'uncertain'
  const refused = submit.disposition === 'rejected'
  const blindRetry = uncertain && attemptRef.current !== null && attemptRef.current.key !== key
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
      } else if (attemptRef.current) {
        attemptRef.current = null
        submit.reset()
        setLocalError(null)
        bumpAttempt()
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
    if (attemptRef.current?.key !== key) {
      attemptRef.current = { key, atMs: Date.now() }
    }
    try {
      await submit.mutateAsync({
        jobId,
        handoffId: latest.handoffId,
        decision,
        reason: reason.trim(),
        atMs: attemptRef.current.atMs,
      })
      attemptRef.current = null
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

export function AgentJobPage() {
  const { jobId } = useParams<{ jobId: string }>()
  const navigate = useNavigate()
  const job = useAgentJob(jobId)
  const handoffs = useAgentHandoffs(jobId)
  const review = useAgentReview(jobId)
  const resumeContext = useAgentResumeContext(jobId)
  const overview = useAgentOverview()
  const resume = useResumeAgentJob()
  const snapshot = useWorkspaceSnapshot()
  const { openTask } = useTaskDrawer()
  const resumeAttempt = useRef<{ newJobId: string; atMs: number } | null>(null)
  const task = snapshot.data?.tasks.find((candidate) => candidate.id === job.data?.taskId)
  const project = snapshot.data?.projects.find((candidate) => candidate.id === job.data?.projectId)

  useDocumentTitle(job.data ? `Agent job ${shortAgentId(job.data.id)}` : 'Agent job')

  if (!jobId) {
    return <NotVisiblePanel id="(missing)" />
  }

  if (job.isPending && !job.data) return <JobSkeleton />

  if (job.error && !job.data) {
    if (isApiError(job.error) && job.error.status === 403) return <NotVisiblePanel id={jobId} />
    return (
      <Panel>
        <ErrorState
          title="This agent job could not be read"
          description={userMessage(job.error)}
          onRetry={() => void job.refetch()}
          retrying={job.isFetching}
        />
      </Panel>
    )
  }

  if (!job.data) {
    // The ownership check ran before the lookup; a successful read that returns
    // null still means the job is not visible to this device.
    return <NotVisiblePanel id={jobId} />
  }

  const current = job.data
  const overviewItem = overview.data?.find((item) => item.jobId === current.id) ?? null
  const latestHandoff = handoffs.data?.[handoffs.data.length - 1] ?? null
  const canResume = isAgentJobTerminal(current.state)
  const refreshAll = async (): Promise<{ ok: boolean; review: AgentReview | null }> => {
    // `throwOnError` for the review read: a failed refetch resolves with the
    // cached value in React Query v5, and acting on that stale value could
    // discard an attempt that actually landed.
    const results = await Promise.allSettled([
      job.refetch(),
      handoffs.refetch(),
      review.refetch({ throwOnError: true }),
      resumeContext.refetch(),
      overview.refetch(),
      snapshot.refetch(),
    ])
    const reviewResult = results[2]
    if (reviewResult.status !== 'fulfilled') return { ok: false, review: null }
    return { ok: true, review: (reviewResult.value.data ?? null) as AgentReview | null }
  }

  const startResume = async () => {
    if (!resumeContext.data || resume.isPending) return
    if (!resumeAttempt.current) {
      resumeAttempt.current = { newJobId: newCommandId(), atMs: Date.now() }
    }
    const attempt = resumeAttempt.current
    try {
      const next = await resume.mutateAsync({
        priorJobId: current.id,
        newJobId: attempt.newJobId,
        atMs: attempt.atMs,
      })
      resumeAttempt.current = null
      navigate(`/agents/${encodeURIComponent(next.id)}`)
    } catch {
      // The mutation's disposition and error are rendered in the section.
    }
  }

  const resumeError: ApiError | null =
    resume.disposition === 'rejected' || resume.disposition === 'uncertain' ? resume.error : null

  return (
    <div className="flex flex-col gap-5">
      <Link
        to="/agents"
        className="inline-flex w-fit items-center gap-1.5 rounded-sm text-xs font-medium text-ink-muted transition-colors hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
      >
        <ArrowLeftIcon size={13} />
        All agent jobs
      </Link>

      <PageHeader
        eyebrow="Agent job"
        title={task?.title ?? `Job ${shortAgentId(current.id)}`}
        description={
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="inline-flex items-center gap-1">
              <FolderIcon size={12} className="opacity-70" />
              {project ? (
                <Link to={`/projects/${project.id}`} className="underline-offset-2 hover:underline">
                  {project.name}
                </Link>
              ) : (
                current.projectId
              )}
            </span>
            <span aria-hidden="true">·</span>
            {task ? (
              <button
                type="button"
                onClick={() => openTask(task.id)}
                className="rounded-sm underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
              >
                {task.title}
              </button>
            ) : (
              <span>Task {current.taskId}</span>
            )}
            <span aria-hidden="true">·</span>
            <span className="font-mono text-2xs">{current.id}</span>
          </span>
        }
        actions={
          <Button
            variant="secondary"
            size="sm"
            leadingIcon={<RefreshIcon size={15} />}
            onClick={() => void refreshAll()}
            loading={job.isFetching}
          >
            Refresh
          </Button>
        }
      />

      <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
        <div className="bg-surface px-4 py-3">
          <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">State</dt>
          <dd className="mt-1.5">
            <Badge tone={agentStateTone(current.state)} dot>
              {agentStateLabel(current.state)}
            </Badge>
          </dd>
        </div>
        <div className="bg-surface px-4 py-3">
          <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">Revision</dt>
          <dd className="mt-1 font-display text-2xl tabular-nums leading-none text-ink">
            {current.revision}
          </dd>
        </div>
        <div className="bg-surface px-4 py-3">
          <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">Handoffs</dt>
          <dd className="mt-1 font-display text-2xl tabular-nums leading-none text-ink">
            {handoffs.data?.length ?? '—'}
          </dd>
        </div>
        <div className="bg-surface px-4 py-3">
          <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">Review</dt>
          <dd className="mt-1.5">
            <Badge tone={review.data?.decision === 'approved' ? 'success' : review.data ? 'warning' : 'neutral'}>
              {reviewDecisionLabel(review.data?.decision)}
            </Badge>
          </dd>
        </div>
      </dl>

      <Panel className="overflow-hidden">
        <PanelHeader
          title="Job"
          description="What the authority stores for this job. Runtime references are labels, not a live connection."
        />
        <PanelBody>
          <dl className="grid gap-x-5 gap-y-3 sm:grid-cols-2 lg:grid-cols-3">
            <Fact label="Agent" value={agentKindLabel(current.agentKind)} />
            <Fact label="Machine" value={current.machineRef} mono />
            <Fact label="Runtime" value={current.runtimeKind} />
            <Fact label="Repository" value={current.repositoryRef} mono />
            <Fact label="Runtime session" value={current.runtimeSessionRef ?? 'Not reported'} mono />
            <Fact label="Request" value={current.requestId} mono />
            <Fact label="Created" value={formatMoment(current.createdAtMs)} />
            <Fact label="Updated" value={formatMoment(current.updatedAtMs)} />
            <Fact label="Next action" value={nextActionLabel(overviewItem?.nextAction ?? 'none')} />
          </dl>
          {review.data?.decision === 'changes_requested' || latestHandoff?.handoffId ? (
            <p className="mt-3 rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-muted">
              {latestHandoff
                ? 'A handoff is on record. Reviewing it is a separate decision; it does not accept the task.'
                : 'There is no handoff yet, so there is nothing to review.'}
            </p>
          ) : null}
        </PanelBody>
      </Panel>

      <Panel className="overflow-hidden">
        <PanelHeader
          title="Handoffs"
          description="Reported by the executor. Payloads are shown verbatim; this page does not reinterpret them."
          actions={<Badge tone="neutral">{handoffs.data?.length ?? 0}</Badge>}
        />
        <PanelBody>
          {handoffs.isPending && !handoffs.data ? (
            <div aria-busy="true">
              <SkeletonText lines={3} />
            </div>
          ) : handoffs.error && !handoffs.data ? (
            <ErrorState
              compact
              title="Handoffs could not be read"
              description={userMessage(handoffs.error)}
              onRetry={() => void handoffs.refetch()}
              retrying={handoffs.isFetching}
            />
          ) : (
            <HandoffList handoffs={handoffs.data ?? []} />
          )}
        </PanelBody>
      </Panel>

      <Panel className="overflow-hidden">
        <PanelHeader
          title="Review and resume"
          description="Both are explicit writes. Neither launches an agent; a machine executor claims a resumed job before it runs."
        />
        <div className="divide-y divide-line">
          {review.isPending && !review.data ? (
            <div className="px-5 py-4" aria-busy="true">
              <SkeletonText lines={2} />
            </div>
          ) : review.error && !review.data ? (
            <div className="px-5 py-4">
              <ErrorState
                compact
                title="The review could not be read"
                description={userMessage(review.error)}
                onRetry={() => void review.refetch()}
                retrying={review.isFetching}
              />
            </div>
          ) : (
            <ReviewSection
              jobId={current.id}
              handoffs={handoffs.data ?? []}
              review={review.data ?? null}
              taskStatus={task?.status ?? null}
              onRefresh={refreshAll}
            />
          )}

          <Section
            title="Resume context"
            description="What a new job would inherit: the original brief, the latest handoff and the review. The browser shows this; it does not launch the executor."
            aside={resumeContext.data ? <Badge tone="neutral">No launch from here</Badge> : null}
          >
            {resumeContext.isPending && !resumeContext.data ? (
              <div aria-busy="true">
                <SkeletonText lines={3} />
              </div>
            ) : resumeContext.error && !resumeContext.data ? (
              <ErrorState
                compact
                title="Resume context could not be read"
                description={userMessage(resumeContext.error)}
                onRetry={() => void resumeContext.refetch()}
                retrying={resumeContext.isFetching}
              />
            ) : resumeContext.data ? (
              <div className="flex flex-col gap-4">
                <div className="flex flex-wrap items-center gap-2 text-xs text-ink-muted">
                  <Badge tone="neutral">{resumeContext.data.handoffCount} handoffs</Badge>
                  <Badge tone={resumeContext.data.taskCompletionInferred ? 'danger' : 'neutral'}>
                    {resumeContext.data.taskCompletionInferred
                      ? 'Completion inferred'
                      : 'Completion not inferred'}
                  </Badge>
                  <span>
                    Resume creates a new queued job. A machine executor must claim it; this browser
                    never starts one.
                  </span>
                </div>
                <div className="grid gap-3 md:grid-cols-2">
                  <JsonBlock label="Original brief" value={resumeContext.data.originalBrief} />
                  <JsonBlock label="Latest handoff" value={resumeContext.data.latestHandoff} />
                  <JsonBlock label="Worktree" value={resumeContext.data.worktree} />
                  <JsonBlock
                    label="Review"
                    value={
                      resumeContext.data.review
                        ? {
                            decision: resumeContext.data.review.decision,
                            reviewer: resumeContext.data.review.reviewer,
                            reason: resumeContext.data.review.reason,
                            task_completed: resumeContext.data.review.taskCompleted,
                          }
                        : null
                    }
                  />
                </div>

                {resumeError ? (
                  <div
                    role="alert"
                    className={cn(
                      'rounded-md border px-3 py-2 text-xs leading-5',
                      resume.disposition === 'uncertain'
                        ? 'border-warning/40 bg-warning-soft text-warning-ink'
                        : 'border-danger/30 bg-danger-soft text-danger-ink',
                    )}
                  >
                    <p className="font-medium">
                      {resume.disposition === 'uncertain'
                        ? 'The resume was not confirmed.'
                        : 'The server refused this resume.'}
                    </p>
                    <p className="mt-0.5">
                      {resume.disposition === 'uncertain'
                        ? `${uncertainMessage(resumeError)} A new job may already exist; check the overview before retrying.`
                        : userMessage(resumeError)}
                    </p>
                  </div>
                ) : null}

                {canResume ? (
                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      size="sm"
                      onClick={() => void startResume()}
                      loading={resume.isPending}
                      disabled={!resumeContext.data}
                    >
                      {resume.disposition === 'uncertain' ? 'Retry the same resume' : 'Resume as a new job'}
                    </Button>
                    <span className="text-2xs text-ink-muted">
                      Prior job <span className="font-mono">{shortAgentId(current.id)}</span> stays as it is.
                    </span>
                  </div>
                ) : (
                  <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-muted">
                    This job is {agentStateLabel(current.state)}. The server resumes only a
                    completed, failed or interrupted job, so no successor can be queued while it is
                    still {agentStateLabel(current.state).toLowerCase()}.
                  </p>
                )}
                <p role="status" aria-live="polite" className="min-h-4 text-2xs text-ink-muted">
                  {resume.isPending ? (
                    <span className="inline-flex items-center gap-1.5">
                      <LoaderIcon size={12} className="animate-spin text-accent" />
                      Queueing the new job…
                    </span>
                  ) : null}
                </p>
              </div>
            ) : null}
          </Section>
        </div>
      </Panel>

      <p className="flex items-start gap-1.5 px-1 text-2xs leading-4 text-ink-faint">
        <AlertCircleIcon size={12} className="mt-0.5 shrink-0" />
        {nextActionExplanation(
          overviewItem?.nextAction ??
            (review.data?.decision === 'changes_requested'
              ? 'resume_or_rework'
              : latestHandoff
                ? 'review_handoff'
                : 'none'),
        )}{' '}
        No launch control exists in this browser.
      </p>
    </div>
  )
}

function Fact({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0">
      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</dt>
      <dd className={cn('break-words text-sm text-ink-soft', mono && 'font-mono text-xs')}>{value}</dd>
    </div>
  )
}
