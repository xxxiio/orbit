import { useMemo } from 'react'
import { Link } from 'react-router-dom'
import type { Milestone, Project, Task } from '@/api/adapters'
import type { TaskStatus } from '@/api/contracts'
import {
  defaultNowMs,
  useMilestones,
  useNextActions,
  useTaskCommand,
  userMessage,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import {
  CalendarIcon,
  CheckCircleIcon,
  LayersIcon,
  LoaderIcon,
} from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import {
  formatDate,
  priorityLabels,
  priorityTones,
  statusLabel,
  statusTones,
  TASK_STATUSES,
  TaskControlError,
  TaskStatusControl,
  useTaskCommandState,
  useTaskDrawer,
} from '@/features/tasks'
import { milestoneStatusLabel } from './project-status'
import { isTaskCompleted, isTaskOpen } from './project-progress'
import type { ProjectWorkflowState } from './use-project-workflow'

function statusCounts(tasks: Task[]): Array<{ status: TaskStatus; count: number }> {
  const counts = new Map<TaskStatus, number>()
  for (const task of tasks) counts.set(task.status, (counts.get(task.status) ?? 0) + 1)
  return TASK_STATUSES.filter((status) => (counts.get(status) ?? 0) > 0).map((status) => ({
    status,
    count: counts.get(status) ?? 0,
  }))
}

export function ProjectOverview({
  project,
  tasks,
  workflow,
}: {
  project: Project
  tasks: Task[]
  workflow: ProjectWorkflowState
}) {
  const { openTask } = useTaskDrawer()
  const command = useTaskCommand()
  const milestones = useMilestones(project.id)
  const nextActions = useNextActions(defaultNowMs(), 40)

  const projectActions = useMemo(
    () => (nextActions.data?.actions ?? []).filter((action) => action.projectId === project.id).slice(0, 6),
    [nextActions.data, project.id],
  )

  const taskById = useMemo(() => new Map(tasks.map((task) => [task.id, task])), [tasks])
  const breakdown = useMemo(() => statusCounts(tasks), [tasks])
  const open = tasks.filter((task) => isTaskOpen(task.status)).length
  const done = tasks.filter((task) => isTaskCompleted(task.status)).length
  const completion = tasks.length > 0 ? Math.round((done / tasks.length) * 100) : 0

  const { changeStatus, pending, error, errorTaskId } = useTaskCommandState(command)
  const pendingTaskId = pending?.field === 'status' ? pending.taskId : null

  return (
    <div className="grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_22rem]">
      <div className="flex min-w-0 flex-col gap-5">
        <Panel>
          <PanelHeader
            title="Progress"
            description={`${tasks.length} task${tasks.length === 1 ? '' : 's'} in this project · ${open} open`}
            actions={
              <span className="font-display text-2xl tabular-nums leading-none text-ink">
                {completion}
                <span className="ml-0.5 text-sm text-ink-muted">%</span>
              </span>
            }
          />
          <PanelBody>
            {tasks.length === 0 ? (
              <EmptyState
                compact
                headingLevel="p"
                title="No tasks in this project yet"
                description="Add the first task from the Tasks view; progress appears here as soon as the server has it."
                icon={<LayersIcon size={16} />}
              />
            ) : (
              <div className="flex flex-col gap-3">
                <div className="flex h-2 w-full gap-0.5 overflow-hidden rounded-full bg-surface-sunken">
                  {breakdown.map((entry) => (
                    <span
                      key={entry.status}
                      aria-hidden="true"
                      className={cn('h-full', statusBarClass(entry.status))}
                      style={{ width: `${(entry.count / tasks.length) * 100}%` }}
                    />
                  ))}
                </div>
                <ul className="grid grid-cols-2 gap-x-4 gap-y-1.5 sm:grid-cols-3">
                  {breakdown.map((entry) => (
                    <li key={entry.status} className="flex items-center gap-1.5 text-2xs text-ink-muted">
                      <span
                        aria-hidden="true"
                        className={cn('h-1.5 w-1.5 shrink-0 rounded-full', statusBarClass(entry.status))}
                      />
                      <span className="truncate">{statusLabel(entry.status)}</span>
                      <span className="ml-auto tabular-nums text-ink-soft">{entry.count}</span>
                    </li>
                  ))}
                </ul>
                <p className="text-2xs text-ink-faint">
                  Grouping and counts are display-only; readiness, blockers and deadlines come from
                  the server.
                </p>
              </div>
            )}
          </PanelBody>
        </Panel>

        <Panel>
          <PanelHeader
            title="Next actions"
            description="Ranked by the server for this project"
            actions={
              nextActions.isFetching ? (
                <LoaderIcon size={14} className="animate-spin text-ink-muted" />
              ) : null
            }
          />
          {nextActions.isPending ? (
            <PanelBody className="flex flex-col gap-2">
              <Skeleton className="h-3 w-2/3" />
              <Skeleton className="h-3 w-1/2" />
              <Skeleton className="h-3 w-3/5" />
            </PanelBody>
          ) : nextActions.error ? (
            <PanelBody className="flex items-center gap-2">
              <p className="min-w-0 flex-1 text-xs text-danger-ink">{userMessage(nextActions.error)}</p>
              <Button variant="quiet" size="sm" onClick={() => void nextActions.refetch()}>
                Retry
              </Button>
            </PanelBody>
          ) : projectActions.length === 0 ? (
            <PanelBody>
              <p className="text-xs leading-5 text-ink-muted">
                The server has no next actions for this project. Move a task to Ready when it can be
                started.
              </p>
            </PanelBody>
          ) : (
            <ul aria-label="Next actions">
              {projectActions.map((action) => {
                const task = taskById.get(action.taskId)
                const rowError = errorTaskId === action.taskId ? error : null
                return (
                  <li
                    key={action.taskId}
                    className="flex flex-col gap-2 border-b border-line px-5 py-3 last:border-b-0 sm:flex-row sm:items-center"
                  >
                    <div className="min-w-0 flex-1">
                      <button
                        type="button"
                        onClick={() => openTask(action.taskId)}
                        className="block max-w-full truncate rounded-sm text-left text-sm font-medium text-ink hover:text-accent-soft-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                      >
                        {action.title}
                      </button>
                      <p className="mt-0.5 flex flex-wrap items-center gap-x-2 text-2xs text-ink-muted">
                        <span className="tabular-nums">Rank {action.rank}</span>
                        {action.unblocksCount > 0 ? (
                          <span>
                            unblocks {action.unblocksCount} task{action.unblocksCount === 1 ? '' : 's'}
                          </span>
                        ) : null}
                        {action.deadlineAtMs ? (
                          <span
                            className={cn(
                              'inline-flex items-center gap-1',
                              action.overdue && 'font-medium text-danger-ink',
                            )}
                          >
                            <CalendarIcon size={11} />
                            {formatDate(action.deadlineAtMs)}
                          </span>
                        ) : null}
                        {task?.priority ? (
                          <Badge tone={priorityTones[task.priority]}>{priorityLabels[task.priority]}</Badge>
                        ) : null}
                      </p>
                      {rowError && task ? (
                        <TaskControlError error={rowError} field="status" taskStatus={task.status} />
                      ) : null}
                    </div>
                    {task ? (
                      <TaskStatusControl
                        task={task}
                        pending={pendingTaskId === task.id}
                        invalid={Boolean(rowError)}
                        onChange={(status) => changeStatus(task, status)}
                      />
                    ) : null}
                  </li>
                )
              })}
            </ul>
          )}
        </Panel>
      </div>

      <aside className="flex min-w-0 flex-col gap-5">
        <Panel>
          <PanelHeader
            title="Workflow"
            description="Pinned to this project"
            actions={
              workflow.label ? (
                <Badge tone="accent">{workflow.label}</Badge>
              ) : workflow.error ? (
                <Badge tone="neutral">Workflow unavailable</Badge>
              ) : (
                <Skeleton className="h-4 w-20" />
              )
            }
          />
          <PanelBody className="flex flex-col gap-3">
            {workflow.error ? (
              <div className="flex items-start gap-2">
                <p className="min-w-0 flex-1 text-xs leading-5 text-danger-ink">
                  {userMessage(workflow.error)}
                </p>
                <Button variant="quiet" size="sm" onClick={workflow.refetch}>
                  Retry
                </Button>
              </div>
            ) : null}
            <p className="text-sm leading-6 text-ink-soft">
              {workflow.workflow?.description ??
                (workflow.loading
                  ? 'Reading the pinned workflow…'
                  : 'This project has no workflow description.')}
            </p>
            {workflow.workflow ? (
              <div className="flex flex-wrap items-center gap-1.5 text-2xs text-ink-muted">
                <span className="font-mono">{workflow.workflow.id}</span>
                <span aria-hidden="true">·</span>
                <span className="tabular-nums">v{workflow.workflow.version}</span>
                <span aria-hidden="true">·</span>
                <span>
                  {workflow.workflow.kinds.length === 0
                    ? 'Task-only workflow'
                    : `${workflow.workflow.kinds.length} work-item type${
                        workflow.workflow.kinds.length === 1 ? '' : 's'
                      }`}
                </span>
              </div>
            ) : null}
            {workflow.workflow && workflow.workflow.views.length > 0 ? (
              <p className="text-2xs leading-4 text-ink-faint">
                Views: {workflow.workflow.views.map((view) => view.title).join(', ')}
              </p>
            ) : null}
          </PanelBody>
        </Panel>

        <Panel>
          <PanelHeader
            title="Milestones"
            description="From the server"
            actions={
              milestones.isFetching ? (
                <LoaderIcon size={14} className="animate-spin text-ink-muted" />
              ) : null
            }
          />
          {milestones.isPending ? (
            <PanelBody className="flex flex-col gap-2">
              <Skeleton className="h-3 w-2/3" />
              <Skeleton className="h-3 w-1/2" />
            </PanelBody>
          ) : milestones.error ? (
            <PanelBody className="flex items-center gap-2">
              <p className="min-w-0 flex-1 text-xs text-danger-ink">{userMessage(milestones.error)}</p>
              <Button variant="quiet" size="sm" onClick={() => void milestones.refetch()}>
                Retry
              </Button>
            </PanelBody>
          ) : (milestones.data ?? []).length === 0 ? (
            <PanelBody>
              <p className="text-xs leading-5 text-ink-muted">
                No milestones for this project. When one is added on the server, it appears here.
              </p>
            </PanelBody>
          ) : (
            <ul aria-label="Milestones">
              {(milestones.data ?? []).map((milestone) => (
                <MilestoneRow key={milestone.id} milestone={milestone} />
              ))}
            </ul>
          )}
        </Panel>

        <div className="flex items-center gap-2 px-1">
          <Link
            to={`/projects/${project.id}/tasks`}
            className="rounded-sm text-2xs font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          >
            All tasks
          </Link>
        </div>
      </aside>
    </div>
  )
}

function MilestoneRow({ milestone }: { milestone: Milestone }) {
  const tone =
    milestone.status === 'completed'
      ? 'success'
      : milestone.status === 'blocked'
        ? 'danger'
        : milestone.status === 'cancelled'
          ? 'neutral'
          : milestone.status === 'in_progress'
            ? 'accent'
            : 'info'
  return (
    <li className="flex items-center gap-2 border-b border-line px-5 py-2.5 last:border-b-0">
      {milestone.status === 'completed' ? (
        <CheckCircleIcon size={14} className="shrink-0 text-success" />
      ) : (
        <span aria-hidden="true" className="h-1.5 w-1.5 shrink-0 rounded-full bg-line-strong" />
      )}
      <span className="min-w-0 flex-1 truncate text-sm text-ink-soft">{milestone.name}</span>
      <Badge tone={tone}>{milestoneStatusLabel(milestone.status)}</Badge>
    </li>
  )
}

function statusBarClass(status: TaskStatus): string {
  return statusTones[status] === 'success'
    ? 'bg-success/80'
    : statusTones[status] === 'danger'
      ? 'bg-danger/70'
      : statusTones[status] === 'accent'
        ? 'bg-accent/80'
        : statusTones[status] === 'warning'
          ? 'bg-warning/70'
          : statusTones[status] === 'info'
            ? 'bg-info/70'
            : 'bg-line-strong'
}
