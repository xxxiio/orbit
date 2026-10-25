/**
 * The declared `agent_jobs` panel and `list` source.
 *
 * The read is the device-wide overview: the server has already filtered it to
 * jobs this device owns. Rows link to the job detail; this panel never offers a
 * start or launch control, because starting a job is an executor-side action.
 *
 * Chrome comes from the shared surface kit (`@/components/ui`), so the read
 * ladder (loading → refused → error → rows) is the same one every source uses.
 */
import { useMemo, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import type { AgentJobOverview, WorkflowViewComponent } from '@/api/adapters'
import { formatMoment } from '@/domain'
import { emptyTextOption } from '@/features/workflows/declarations'
import { useAgentOverview, useWorkspaceSnapshot } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { AgentsIcon } from '@/components/ui/icons'
import { SkeletonText } from '@/components/ui/skeleton'
import {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  TableHeadCell,
  UnavailablePanel,
} from '@/components/ui'
import { humanizeIdentifier } from '@/lib/humanize'
import { workflowText } from '@/domain'
import {
  agentStateLabel,
  agentStateTone,
  nextActionLabel,
  reviewDecisionLabel,
  shortAgentId,
} from '@/features/agents/agent-model'

function panelTitle(component: WorkflowViewComponent): string {
  const declared = workflowText(component.title)?.trim()
  if (declared) return declared
  if (component.type === 'agent_jobs') return 'Agent jobs'
  return humanizeIdentifier(component.type)
}

/**
 * Columns a declared `agent_jobs` table can render from the overview read. A
 * name outside this table is refused by name rather than replaced with a
 * different column.
 */
const AGENT_JOB_COLUMNS: Record<string, string> = {
  job: 'Job',
  job_id: 'Job',
  id: 'Job',
  state: 'State',
  task: 'Task',
  task_id: 'Task',
  project: 'Project',
  project_id: 'Project',
  handoffs: 'Handoffs',
  handoff_count: 'Handoffs',
  next_action: 'Next action',
  review: 'Review',
  review_decision: 'Review',
  updated: 'Updated',
  updated_at: 'Updated',
  attention: 'Attention',
  needs_attention: 'Attention',
}

const DEFAULT_AGENT_JOB_COLUMNS = [
  'state',
  'task',
  'project',
  'handoffs',
  'next_action',
  'review',
  'updated',
]

function normaliseColumn(name: string): string {
  return name.trim().toLowerCase().replace(/[\s-]+/g, '_')
}

function columnCell(
  key: string,
  job: AgentJobOverview,
  tasks: Map<string, string>,
  projects: Map<string, string>,
): ReactNode {
  switch (key) {
    case 'state':
      return (
        <Badge tone={agentStateTone(job.state)} dot>
          {agentStateLabel(job.state)}
        </Badge>
      )
    case 'task':
    case 'task_id':
      return tasks.get(job.taskId) ?? shortAgentId(job.jobId)
    case 'project':
    case 'project_id':
      return projects.get(job.projectId) ?? job.projectId
    case 'handoffs':
    case 'handoff_count':
      return `${job.handoffCount} handoff${job.handoffCount === 1 ? '' : 's'}`
    case 'next_action':
      return nextActionLabel(job.nextAction)
    case 'review':
    case 'review_decision':
      return reviewDecisionLabel(job.reviewDecision)
    case 'updated':
    case 'updated_at':
      return formatMoment(job.updatedAtMs)
    case 'attention':
    case 'needs_attention':
      return job.needsAttention ? <Badge tone="warning">Needs attention</Badge> : '—'
    default:
      return shortAgentId(job.jobId)
  }
}

export function AgentJobsPanel({ component }: { component: WorkflowViewComponent }) {
  const overview = useAgentOverview()
  const snapshot = useWorkspaceSnapshot()
  const source = component.source ?? 'agent_jobs'

  const projects = useMemo(
    () => new Map((snapshot.data?.projects ?? []).map((project) => [project.id, project.name])),
    [snapshot.data?.projects],
  )
  const tasks = useMemo(
    () => new Map((snapshot.data?.tasks ?? []).map((task) => [task.id, task.title])),
    [snapshot.data?.tasks],
  )

  const table = component.type === 'table'
  const emptyText = emptyTextOption(component)
  const declaredColumns = (component.columns ?? [])
    .map((column) => column.trim())
    .filter((column) => column.length > 0)
  const columnKeys = table
    ? declaredColumns.length > 0
      ? declaredColumns.map(normaliseColumn)
      : DEFAULT_AGENT_JOB_COLUMNS
    : []
  const unsupportedColumns = columnKeys.filter((column) => !AGENT_JOB_COLUMNS[column])
  // Preserve the declared spelling in the refusal, not the normalised key.
  const unsupportedNames = unsupportedColumns.map(
    (column) =>
      declaredColumns.find((declared) => normaliseColumn(declared) === column) ?? column,
  )

  if (source !== 'agent_jobs') {
    return (
      <UnavailablePanel
        title={panelTitle(component)}
        reason={`This panel is declared over “${source}”, not agent jobs.`}
      />
    )
  }
  if (Object.keys(component.filter ?? {}).length > 0 || component.sort) {
    return (
      <UnavailablePanel
        title={panelTitle(component)}
        reason="This build cannot evaluate a declared filter or sort over agent jobs. The overview is ordered by the server's update time; another order would not be the declaration."
      />
    )
  }

  if (table && unsupportedNames.length > 0) {
    return (
      <UnavailablePanel
        title={panelTitle(component)}
        reason={`This table declares the ${unsupportedNames
          .map((name) => `“${name}”`)
          .join(', ')} column${unsupportedNames.length === 1 ? '' : 's'}. The agent-job overview exposes state, task, project, handoffs, next action, review and update time; the panel refuses rather than substituting a different column.`}
      />
    )
  }

  if (overview.isPending && !overview.data) {
    return (
      <ComponentPanel title={panelTitle(component)}>
        <div aria-busy="true">
          <SkeletonText lines={3} />
        </div>
      </ComponentPanel>
    )
  }
  if (overview.error && !overview.data) {
    return (
      <ComponentPanel title={panelTitle(component)}>
        <InlineReadError
          label="Agent jobs"
          error={overview.error}
          onRetry={() => void overview.refetch()}
          retrying={overview.isFetching}
        />
      </ComponentPanel>
    )
  }

  const limit = component.limit ?? 20
  const jobs = (overview.data ?? []).slice(0, limit)
  const attention = jobs.filter((job) => job.needsAttention).length
  const compact = component.type === 'list'

  return (
    <ComponentPanel
      title={panelTitle(component)}
      description={
        compact
          ? 'Every job this device can see, newest update first. A compact list; open one for review and resume.'
          : table
            ? 'The declared columns over this device\'s jobs, newest update first. Opening one is a read; the detail page offers review and resume only.'
            : 'Jobs owned by this device, newest update first. Opening one is a read; the detail page offers review and resume only.'
      }
      bodyClassName="px-0 py-0"
      footer={
        <>
          <ProvenanceNote>
            {jobs.length} of {overview.data?.length ?? 0} visible jobs
            {attention > 0 ? ` · ${attention} need attention` : ''}. Agent jobs are device-wide;
            project and task names come from the workspace.
          </ProvenanceNote>
          {component.actions.length > 0 ? (
            <div className="mt-1">
              <ProvenanceNote>
                <DeclaredActionsNote component={component} ownLabel="links" />
              </ProvenanceNote>
            </div>
          ) : null}
        </>
      }
    >
      {jobs.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<AgentsIcon size={18} />}
          title="No agent jobs are visible to this device"
          description={
            emptyText ?? 'Jobs are started by an executor or the command line, not by this panel.'
          }
          action={
            <Button asChild variant="secondary" size="sm">
              <Link to="/agents">Open Agents</Link>
            </Button>
          }
        />
      ) : table ? (
        <div className="overflow-x-auto">
          <table aria-label="Agent jobs" className="w-full border-collapse text-sm">
            <thead>
              <tr className="border-b border-line bg-surface-sunken/70 text-left">
                {columnKeys.map((column, index) => (
                  <TableHeadCell key={`${column}:${index}`}>{AGENT_JOB_COLUMNS[column]}</TableHeadCell>
                ))}
              </tr>
            </thead>
            <tbody>
              {jobs.map((job) => (
                <tr
                  key={job.jobId}
                  data-agent-job={job.jobId}
                  className="border-b border-line last:border-b-0 hover:bg-surface-hover/40"
                >
                  {columnKeys.map((column, index) => (
                    <td
                      key={`${column}:${index}`}
                      className="whitespace-nowrap px-4 py-2 align-middle text-xs text-ink-soft"
                    >
                      {index === 0 ? (
                        <Link
                          to={`/agents/${encodeURIComponent(job.jobId)}`}
                          className="underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                        >
                          {columnCell(column, job, tasks, projects)}
                        </Link>
                      ) : (
                        columnCell(column, job, tasks, projects)
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : compact ? (
        <ul aria-label="All agent jobs" className="divide-y divide-line">
          {jobs.map((job) => (
            <li key={job.jobId} data-agent-job={job.jobId}>
              <Link
                to={`/agents/${encodeURIComponent(job.jobId)}`}
                className="flex flex-wrap items-center gap-x-2 gap-y-1 px-4 py-1.5 text-2xs transition-colors hover:bg-surface-hover/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/50"
              >
                <Badge tone={agentStateTone(job.state)} dot>
                  {agentStateLabel(job.state)}
                </Badge>
                <span className="min-w-0 flex-1 truncate text-sm text-ink">
                  {tasks.get(job.taskId) ?? shortAgentId(job.jobId)}
                </span>
                <span className="font-mono text-ink-muted">{shortAgentId(job.jobId)}</span>
                <span className="text-ink-faint">{nextActionLabel(job.nextAction)}</span>
                <span className="text-ink-faint">{formatMoment(job.updatedAtMs)}</span>
              </Link>
            </li>
          ))}
        </ul>
      ) : (
        <ul aria-label="Agent jobs" className="divide-y divide-line">
          {jobs.map((job) => (
            <li key={job.jobId} data-agent-job={job.jobId}>
              <Link
                to={`/agents/${encodeURIComponent(job.jobId)}`}
                className="flex flex-col gap-1 px-4 py-2.5 transition-colors hover:bg-surface-hover/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/50"
              >
                <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">
                    {tasks.get(job.taskId) ?? shortAgentId(job.jobId)}
                  </span>
                  <Badge tone={agentStateTone(job.state)} dot>
                    {agentStateLabel(job.state)}
                  </Badge>
                  {job.needsAttention ? <Badge tone="warning">Needs attention</Badge> : null}
                </span>
                <span className="flex flex-wrap items-center gap-x-2 text-2xs text-ink-muted">
                  <span>{projects.get(job.projectId) ?? job.projectId}</span>
                  <span aria-hidden="true">·</span>
                  <span className="font-mono">{shortAgentId(job.jobId)}</span>
                  <span aria-hidden="true">·</span>
                  <span>{job.handoffCount} handoff{job.handoffCount === 1 ? '' : 's'}</span>
                  <span aria-hidden="true">·</span>
                  <span>{nextActionLabel(job.nextAction)}</span>
                  <span aria-hidden="true">·</span>
                  <span>{reviewDecisionLabel(job.reviewDecision)}</span>
                  <span aria-hidden="true">·</span>
                  <span>{formatMoment(job.updatedAtMs)}</span>
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </ComponentPanel>
  )
}
