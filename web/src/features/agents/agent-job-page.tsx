/**
 * One agent job: state, runtime references, handoffs, review and resume context.
 *
 * The page is read-mostly. The only writes are an explicit review decision and
 * an explicit resume that queues a new job. Neither launches an executor and
 * neither completes the Orbit task; the copy says both facts on screen.
 *
 * The read-only blocks live in `agent-job/blocks` and the review form in
 * `agent-job/review-section`; this module owns the reads, the job header and
 * the resume command.
 */
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
  useIdempotentAttempt,
  useResumeAgentJob,
  useWorkspaceSnapshot,
  userMessage,
  type ApiError,
} from '@/hooks'
import type { AgentReview } from '@/api/adapters'
import { formatMoment } from '@/domain'
import { useTaskDrawer } from '@/features/tasks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ErrorState } from '@/components/ui/error-state'
import {
  AlertCircleIcon,
  ArrowLeftIcon,
  FolderIcon,
  LoaderIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { PageHeader } from '@/components/ui/page-header'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Section } from '@/components/ui/section'
import { SkeletonText } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
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
import { Fact, HandoffList, JobSkeleton, JsonBlock, NotVisiblePanel } from './agent-job/blocks'
import { ReviewSection } from './agent-job/review-section'

interface ResumeAttempt {
  /** One resume per job: the same page cannot queue two successors. */
  key: string
  newJobId: string
  atMs: number
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
  const resumeAttempt = useIdempotentAttempt<ResumeAttempt>((key) => ({
    key,
    newJobId: newCommandId(),
    atMs: Date.now(),
  }))
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
    const pinned = resumeAttempt.ensure(current.id)
    try {
      const next = await resume.mutateAsync({
        priorJobId: current.id,
        newJobId: pinned.newJobId,
        atMs: pinned.atMs,
      })
      resumeAttempt.clear()
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
              <span>A task outside this snapshot</span>
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
