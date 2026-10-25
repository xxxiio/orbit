/**
 * The Agents workspace: jobs this device can see, attention first, then by state.
 *
 * Nothing here launches an agent. The overview is a read model; detail pages
 * offer explicit review and resume actions only.
 */
import { useMemo } from 'react'
import { Link } from 'react-router-dom'
import type { AgentJobOverview } from '@/api/adapters'
import {
  useAgentOverview,
  useDocumentTitle,
  useWorkspaceSnapshot,
  userMessage,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import {
  AgentsIcon,
  AlertCircleIcon,
  ArrowRightIcon,
  ClockIcon,
  FolderIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { PageHeader } from '@/components/ui/page-header'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { formatMoment } from '@/features/tasks'
import {
  AGENT_STATES,
  agentStateLabel,
  agentStateTone,
  nextActionLabel,
  shortAgentId,
} from './agent-model'

function OverviewSkeleton() {
  return (
    <div className="flex flex-col gap-5" aria-busy="true" aria-label="Loading agent jobs">
      <Skeleton className="h-20 rounded-lg" />
      <Skeleton className="h-48 rounded-lg" />
      <Skeleton className="h-56 rounded-lg" />
    </div>
  )
}

function JobRow({
  job,
  projectName,
  taskTitle,
}: {
  job: AgentJobOverview
  projectName: string | null
  taskTitle: string | null
}) {
  return (
    <li className="border-b border-line last:border-b-0">
      <Link
        to={`/agents/${encodeURIComponent(job.jobId)}`}
        data-agent-job={job.jobId}
        className="flex flex-col gap-1.5 px-4 py-3 transition-colors hover:bg-surface-hover/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/50"
      >
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">
            {taskTitle ?? shortAgentId(job.jobId)}
          </span>
          <Badge tone={agentStateTone(job.state)} dot>
            {agentStateLabel(job.state)}
          </Badge>
          {job.needsAttention ? <Badge tone="warning">Needs attention</Badge> : null}
        </span>
        <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-2xs text-ink-muted">
          <span className="inline-flex items-center gap-1">
            <FolderIcon size={11} className="opacity-70" />
            {projectName ?? job.projectId}
          </span>
          <span aria-hidden="true">·</span>
          <span className="font-mono">{shortAgentId(job.jobId)}</span>
          <span aria-hidden="true">·</span>
          <span>{job.handoffCount} handoff{job.handoffCount === 1 ? '' : 's'}</span>
          <span aria-hidden="true">·</span>
          <span>{nextActionLabel(job.nextAction)}</span>
          <span aria-hidden="true">·</span>
          <span className="inline-flex items-center gap-1">
            <ClockIcon size={11} />
            {formatMoment(job.updatedAtMs)}
          </span>
          <ArrowRightIcon size={12} className="ml-auto shrink-0 text-ink-faint" />
        </span>
      </Link>
    </li>
  )
}

function JobSection({
  title,
  description,
  jobs,
  projectName,
  taskTitle,
}: {
  title: string
  description: string
  jobs: AgentJobOverview[]
  projectName: (id: string) => string | null
  taskTitle: (id: string) => string | null
}) {
  return (
    <Panel className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-4 py-3">
        <h2 className="text-sm font-semibold text-ink">{title}</h2>
        <span className="rounded-full bg-surface-sunken px-1.5 py-0.5 text-2xs font-semibold tabular-nums text-ink-muted">
          {jobs.length}
        </span>
        <p className="hidden text-xs text-ink-muted sm:block">{description}</p>
      </div>
      <ul aria-label={title}>
        {jobs.map((job) => (
          <JobRow
            key={job.jobId}
            job={job}
            projectName={projectName(job.projectId)}
            taskTitle={taskTitle(job.taskId)}
          />
        ))}
      </ul>
    </Panel>
  )
}

export function AgentsOverviewPage() {
  useDocumentTitle('Agents')
  const overview = useAgentOverview()
  const snapshot = useWorkspaceSnapshot()

  const projects = useMemo(
    () => new Map((snapshot.data?.projects ?? []).map((project) => [project.id, project.name])),
    [snapshot.data?.projects],
  )
  const tasks = useMemo(
    () => new Map((snapshot.data?.tasks ?? []).map((task) => [task.id, task.title])),
    [snapshot.data?.tasks],
  )

  const attention = useMemo(
    () => (overview.data ?? []).filter((job) => job.needsAttention),
    [overview.data],
  )
  const attentionIds = useMemo(() => new Set(attention.map((job) => job.jobId)), [attention])
  const byState = useMemo(() => {
    const groups = new Map<string, AgentJobOverview[]>()
    for (const job of overview.data ?? []) {
      if (attentionIds.has(job.jobId)) continue
      const list = groups.get(job.state) ?? []
      list.push(job)
      groups.set(job.state, list)
    }
    return AGENT_STATES.filter((state) => groups.has(state)).map((state) => ({
      state,
      jobs: groups.get(state) ?? [],
    }))
  }, [overview.data, attentionIds])

  const running = (overview.data ?? []).filter((job) => job.state === 'running').length

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Workspace"
        title="Agents"
        description="Jobs this device can see. Reviewing a handoff records a decision; it never completes the Orbit task, and this browser never launches an agent."
        actions={
          <Button
            variant="secondary"
            size="sm"
            leadingIcon={<RefreshIcon size={15} />}
            onClick={() => {
              void overview.refetch()
              void snapshot.refetch()
            }}
            loading={overview.isFetching}
          >
            Refresh
          </Button>
        }
      />

      {overview.isPending && !overview.data ? (
        <OverviewSkeleton />
      ) : overview.error && !overview.data ? (
        <Panel>
          <ErrorState
            title="Agent jobs could not be read"
            description={userMessage(overview.error)}
            onRetry={() => void overview.refetch()}
            retrying={overview.isFetching}
          />
        </Panel>
      ) : (overview.data?.length ?? 0) === 0 ? (
        <Panel>
          <EmptyState
            icon={<AgentsIcon size={21} />}
            title="No agent jobs are visible to this device"
            description="Jobs are owned by the device that started them. An executor or the command line starts one; this browser never launches an agent."
            action={
              <Button asChild variant="secondary" size="sm">
                <Link to="/projects">Open projects</Link>
              </Button>
            }
            className="min-h-[20rem]"
          />
        </Panel>
      ) : (
        <>
          <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-3">
            <div className="bg-surface px-4 py-3">
              <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">Visible jobs</dt>
              <dd className="mt-1 font-display text-2xl tabular-nums leading-none text-ink">
                {overview.data?.length ?? 0}
              </dd>
            </div>
            <div className="bg-surface px-4 py-3">
              <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">Need attention</dt>
              <dd className="mt-1 font-display text-2xl tabular-nums leading-none text-warning-ink">
                {attention.length}
              </dd>
            </div>
            <div className="bg-surface px-4 py-3">
              <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">Running</dt>
              <dd className="mt-1 font-display text-2xl tabular-nums leading-none text-ink">{running}</dd>
            </div>
          </dl>

          {attention.length > 0 ? (
            <JobSection
              title="Needs attention"
              description="A handoff needs review, a review asked for changes, or the job is waiting."
              jobs={attention}
              projectName={(id) => projects.get(id) ?? null}
              taskTitle={(id) => tasks.get(id) ?? null}
            />
          ) : (
            <Panel>
              <PanelHeader
                title="Needs attention"
                description="No visible job is waiting for a person right now."
                actions={<Badge tone="success">All clear</Badge>}
              />
              <PanelBody>
                <p className="text-xs leading-5 text-ink-muted">
                  Reviews and resumes are explicit actions; a job that is running does not need a
                  click here.
                </p>
              </PanelBody>
            </Panel>
          )}

          {byState.map((group) => (
            <JobSection
              key={group.state}
              title={agentStateLabel(group.state)}
              description="Jobs in this state that are not in the attention list."
              jobs={group.jobs}
              projectName={(id) => projects.get(id) ?? null}
              taskTitle={(id) => tasks.get(id) ?? null}
            />
          ))}

          <p className="flex items-start gap-1.5 px-1 text-2xs leading-4 text-ink-faint">
            <AlertCircleIcon size={12} className="mt-0.5 shrink-0" />
            Agent jobs are device-wide. This page lists every job owned by this device, grouped by
            the server's state and attention flag; project and task names come from the workspace.
          </p>
        </>
      )}
    </div>
  )
}
