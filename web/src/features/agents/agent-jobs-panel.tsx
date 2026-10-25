/**
 * The declared `agent_jobs` panel and `list` source.
 *
 * The read is the device-wide overview: the server has already filtered it to
 * jobs this device owns. Rows link to the job detail; this panel never offers a
 * start or launch control, because starting a job is an executor-side action.
 */
import { useMemo } from 'react'
import { Link } from 'react-router-dom'
import type { WorkflowViewComponent } from '@/api/adapters'
import { useAgentOverview, useWorkspaceSnapshot, userMessage } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { AgentsIcon, LayersIcon } from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { SkeletonText } from '@/components/ui/skeleton'
import { humanizeIdentifier } from '@/lib/humanize'
import { workflowText } from '@/domain'
import {
  agentStateLabel,
  agentStateTone,
  nextActionLabel,
  reviewDecisionLabel,
  shortAgentId,
} from '@/features/agents/agent-model'
import { formatMoment } from '@/features/tasks'

function panelTitle(component: WorkflowViewComponent): string {
  const declared = workflowText(component.title)?.trim()
  if (declared) return declared
  if (component.type === 'agent_jobs') return 'Agent jobs'
  return humanizeIdentifier(component.type)
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

  if (source !== 'agent_jobs') {
    return (
      <Panel className="flex min-w-0 flex-col">
        <PanelHeader title={panelTitle(component)} className="flex-wrap gap-y-2" />
        <PanelBody className="min-w-0 flex-1 p-0">
          <EmptyState
            compact
            headingLevel="p"
            icon={<LayersIcon size={18} />}
            title="Not available in the browser yet"
            description={`This panel is declared over “${source}”, not agent jobs.`}
            className="min-h-[12rem]"
          />
        </PanelBody>
      </Panel>
    )
  }
  if (Object.keys(component.filter ?? {}).length > 0 || component.sort) {
    return (
      <Panel className="flex min-w-0 flex-col">
        <PanelHeader title={panelTitle(component)} className="flex-wrap gap-y-2" />
        <PanelBody className="min-w-0 flex-1 p-0">
          <EmptyState
            compact
            headingLevel="p"
            icon={<LayersIcon size={18} />}
            title="Not available in the browser yet"
            description="This build cannot evaluate a declared filter or sort over agent jobs. The overview is ordered by the server's update time; another order would not be the declaration."
            className="min-h-[12rem]"
          />
        </PanelBody>
      </Panel>
    )
  }

  if (overview.isPending && !overview.data) {
    return (
      <Panel className="flex min-w-0 flex-col">
        <PanelHeader title={panelTitle(component)} className="flex-wrap gap-y-2" />
        <PanelBody className="min-w-0 flex-1">
          <div aria-busy="true">
            <SkeletonText lines={3} />
          </div>
        </PanelBody>
      </Panel>
    )
  }
  if (overview.error && !overview.data) {
    return (
      <Panel className="flex min-w-0 flex-col">
        <PanelHeader title={panelTitle(component)} className="flex-wrap gap-y-2" />
        <PanelBody className="min-w-0 flex-1">
          <ErrorState
            compact
            title="Agent jobs could not be read"
            description={userMessage(overview.error)}
            onRetry={() => void overview.refetch()}
            retrying={overview.isFetching}
          />
        </PanelBody>
      </Panel>
    )
  }

  const limit = component.limit ?? 20
  const jobs = (overview.data ?? []).slice(0, limit)
  const attention = jobs.filter((job) => job.needsAttention).length
  const compact = component.type === 'list'

  return (
    <Panel className="flex min-w-0 flex-col">
      <PanelHeader
        title={panelTitle(component)}
        description={
          compact
            ? 'Every job this device can see, newest update first. A compact list; open one for review and resume.'
            : 'Jobs owned by this device, newest update first. Opening one is a read; the detail page offers review and resume only.'
        }
        className="flex-wrap gap-y-2"
      />
      <PanelBody className="min-w-0 flex-1 px-0 py-0">
        {jobs.length === 0 ? (
          <EmptyState
            compact
            headingLevel="p"
            icon={<AgentsIcon size={18} />}
            title="No agent jobs are visible to this device"
            description="Jobs are started by an executor or the command line, not by this panel."
            action={
              <Button asChild variant="secondary" size="sm">
                <Link to="/agents">Open Agents</Link>
              </Button>
            }
          />
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
      </PanelBody>
      <div className="border-t border-line px-5 py-3">
        <p className="text-2xs leading-4 text-ink-faint">
          {jobs.length} of {overview.data?.length ?? 0} visible jobs
          {attention > 0 ? ` · ${attention} need attention` : ''}. Agent jobs are device-wide;
          project and task names come from the workspace.
        </p>
        {component.actions.length > 0 ? (
          <p className="mt-1 text-2xs leading-4 text-ink-faint">
            The workflow also declares the {component.actions.map((action) => humanizeIdentifier(workflowText(action))).join(', ')} action
            {component.actions.length === 1 ? '' : 's'}; this panel renders its own links and does
            not attach an action it cannot complete.
          </p>
        ) : null}
      </div>
    </Panel>
  )
}
