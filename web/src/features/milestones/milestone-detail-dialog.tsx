import { useEffect, useMemo, useState } from 'react'
import type { Milestone, Task, WorkItem } from '@/api/adapters'
import type { MilestoneStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import { newCommandId, useMilestones, useMilestoneCommand, userMessage } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { AlertCircleIcon, ArrowRightIcon, PlusIcon, TrashIcon } from '@/components/ui/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { InlineReadError } from '@/components/surface'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { useToast } from '@/components/ui/toast'
import { humanizeIdentifier } from '@/lib/humanize'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { isTaskCompleted, milestoneStatusLabel } from '@/domain'
import {
  formatMilestoneDate,
  milestoneChildTasks,
  milestoneChildWorkItems,
  milestoneDateSummary,
  milestoneNextStatuses,
  milestoneProgressLabel,
  workItemLabel,
} from './milestone-model'

const NO_TASK = '__pick_a_task__'

/** The work-item read the caller owns; the dialog never guesses membership. */
export interface MilestoneWorkItemsState {
  isPending: boolean
  error: ApiError | null
  retrying?: boolean
  refetch?: () => void
}

/**
 * One milestone's progress, membership and history.
 *
 * Membership commands are never applied optimistically: each one is sent, and
 * the dialog renders the record the milestones query returns afterwards. A
 * refusal is quoted verbatim and the stored membership is left as it was.
 */
export function MilestoneDetailDialog({
  milestone,
  tasks,
  workItems = [],
  workItemsState,
  open,
  onOpenChange,
  onOpenTask,
  onEdit,
}: {
  milestone: Milestone | null
  tasks: Task[]
  workItems?: WorkItem[]
  /** The caller's work-item read; a pending/failed read is never an empty list. */
  workItemsState?: MilestoneWorkItemsState
  open: boolean
  onOpenChange: (open: boolean) => void
  onOpenTask?: (taskId: string) => void
  onEdit?: (milestone: Milestone) => void
}) {
  const command = useMilestoneCommand()
  const { toast } = useToast()
  const { openNewTask } = useWorkspaceActions()
  const query = useMilestones(milestone?.projectId)
  const [error, setError] = useState<string | null>(null)
  const [taskToAssign, setTaskToAssign] = useState(NO_TASK)
  const [confirmDelete, setConfirmDelete] = useState(false)

  useEffect(() => {
    setError(null)
    setTaskToAssign(NO_TASK)
    setConfirmDelete(false)
    command.reset()
    // Reset on the selected milestone only; command identity is stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [milestone?.id, open])

  // The freshest accepted record: the command's own answer when it names this
  // milestone, else the read, else the card's copy. Membership and progress
  // are never applied optimistically.
  const live = useMemo(() => {
    if (!milestone) return null
    if (command.data?.milestone?.id === milestone.id) return command.data.milestone
    return query.data?.find((entry) => entry.id === milestone.id) ?? milestone
  }, [milestone, query.data, command.data])

  if (!live) return null

  const childTasks = milestoneChildTasks(live, tasks)
  const childWorkItems = milestoneChildWorkItems(live, workItems)
  const assignedTaskIds = new Set(live.taskIds)
  const assignableTasks = tasks.filter((task) => !assignedTaskIds.has(task.id))
  const hasWork = live.taskIds.length > 0 || live.workItemIds.length > 0
  const nextStatuses = milestoneNextStatuses(live.status)
  const progress = live.progress
  const blocked = progress.blocked

  const run = async (action: () => Promise<unknown>) => {
    setError(null)
    try {
      await action()
      return true
    } catch (failure) {
      setError(userMessage(failure))
      return false
    }
  }

  const assignTask = async () => {
    if (taskToAssign === NO_TASK) return
    const accepted = await run(() =>
      command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'assign_task', milestoneId: live.id, taskId: taskToAssign },
      }),
    )
    if (accepted) setTaskToAssign(NO_TASK)
  }

  const unassignTask = (taskId: string) =>
    run(() =>
      command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'unassign_task', taskId, milestoneId: live.id },
      }),
    )

  const unassignWorkItem = (itemId: string) =>
    run(() =>
      command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'unassign_work_item', itemId, milestoneId: live.id },
      }),
    )

  const changeStatus = (status: MilestoneStatus) =>
    run(() =>
      command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'change_status', milestoneId: live.id, status },
      }),
    )

  const remove = async () => {
    const accepted = await run(() =>
      command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'delete', milestoneId: live.id },
      }),
    )
    if (accepted) {
      toast({ tone: 'success', title: 'Milestone deleted' })
      onOpenChange(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={live.name}
        description={`Milestone in this project · ${milestoneProgressLabel(live)}`}
        size="lg"
      >
        <div className="flex flex-col gap-5">
          <div className="flex flex-wrap items-center gap-2">
            <Badge
              tone={
                live.status === 'completed'
                  ? 'success'
                  : live.status === 'blocked'
                    ? 'danger'
                    : live.status === 'in_progress'
                      ? 'accent'
                      : 'neutral'
              }
              dot
            >
              {milestoneStatusLabel(live.status)}
            </Badge>
            <span className="text-xs text-ink-muted">{milestoneDateSummary(live)}</span>
            {live.owner ? <span className="text-xs text-ink-muted">Owner {live.owner}</span> : null}
            {live.priority ? (
              <Badge tone="neutral">{humanizeIdentifier(live.priority)} priority</Badge>
            ) : null}
            <div className="ml-auto flex items-center gap-1">
              <Button
                variant="secondary"
                size="sm"
                leadingIcon={<PlusIcon size={14} />}
                onClick={() => {
                  onOpenChange(false)
                  openNewTask(live.projectId, live.id)
                }}
              >
                New task
              </Button>
              {onEdit ? (
                <Button
                  variant="quiet"
                  size="sm"
                  onClick={() => {
                    onOpenChange(false)
                    onEdit(live)
                  }}
                >
                  Edit milestone
                </Button>
              ) : null}
            </div>
          </div>

          <section aria-label="Progress" className="rounded-md border border-line bg-surface-sunken/25 p-3">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
              <p className="text-sm font-medium text-ink">{milestoneProgressLabel(live)}</p>
              <span className="text-sm tabular-nums text-ink-muted">{progress.percent}%</span>
            </div>
            <div
              role="progressbar"
              aria-label={`${live.name} progress`}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={progress.percent}
              className="mt-2 h-2 overflow-hidden rounded-full bg-line"
            >
              <span
                className="block h-full rounded-full bg-accent"
                style={{ width: `${progress.percent}%` }}
              />
            </div>
            <dl className="mt-3 grid grid-cols-3 gap-2 text-xs">
              <div>
                <dt className="text-2xs font-semibold tracking-label text-ink-faint">Total</dt>
                <dd className="tabular-nums text-ink-soft">{progress.total}</dd>
              </div>
              <div>
                <dt className="text-2xs font-semibold tracking-label text-ink-faint">Completed</dt>
                <dd className="tabular-nums text-ink-soft">{progress.completed}</dd>
              </div>
              <div>
                <dt className="text-2xs font-semibold tracking-label text-ink-faint">Blocked</dt>
                <dd className={blocked > 0 ? 'tabular-nums text-danger-ink' : 'tabular-nums text-ink-soft'}>
                  {blocked}
                </dd>
              </div>
            </dl>
          </section>

          {live.description.trim().length > 0 ? (
            <section aria-label="Description" className="border-t border-line pt-4">
              <h3 className="text-sm font-semibold text-ink">Description</h3>
              <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-ink-soft">
                {live.description}
              </p>
            </section>
          ) : null}

          <section aria-label="Status" className="border-t border-line pt-4">
            <Field
              label="Status"
              hint={
                nextStatuses.length === 0
                  ? 'This milestone is in a terminal state; it cannot move again.'
                  : 'Only the next states the server allows are offered.'
              }
            >
              <Select
                value={live.status}
                onValueChange={(value) => void changeStatus(value as MilestoneStatus)}
                disabled={command.isPending || nextStatuses.length === 0}
              >
                <SelectTrigger className="w-[12rem]" aria-label="Milestone status">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={live.status}>{milestoneStatusLabel(live.status)}</SelectItem>
                  {nextStatuses.map((status) => (
                    <SelectItem key={status} value={status}>
                      {milestoneStatusLabel(status)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          </section>

          <section aria-label="Tasks" className="border-t border-line pt-4">
            <h3 className="text-sm font-semibold text-ink">Tasks ({childTasks.length})</h3>
            {childTasks.length === 0 ? (
              <p className="mt-1 text-xs leading-5 text-ink-muted">
                No tasks are assigned yet. Pick one below, or assign it from the task drawer.
              </p>
            ) : (
              <ul className="mt-2 divide-y divide-line rounded-md border border-line">
                {childTasks.map((task) => (
                  <li key={task.id} className="flex items-center gap-2 px-3 py-2 text-sm">
                    <span className="min-w-0 flex-1 truncate">{task.title}</span>
                    <span
                      className={cnTaskStatus(task)}
                    >
                      {isTaskCompleted(task.status) ? 'Done' : humanizeIdentifier(task.status)}
                    </span>
                    {onOpenTask ? (
                      <Button
                        variant="quiet"
                        size="sm"
                        aria-label={`Open ${task.title}`}
                        onClick={() => {
                          onOpenChange(false)
                          onOpenTask(task.id)
                        }}
                      >
                        <ArrowRightIcon size={14} />
                      </Button>
                    ) : null}
                    <Button
                      variant="quiet"
                      size="sm"
                      disabled={command.isPending}
                      aria-label={`Remove ${task.title} from the milestone`}
                      onClick={() => void unassignTask(task.id)}
                    >
                      Remove
                    </Button>
                  </li>
                ))}
              </ul>
            )}

            {assignableTasks.length > 0 ? (
              <div className="mt-2 flex flex-wrap items-end gap-2">
                <Field label="Assign a task" className="min-w-0 flex-1">
                  <Select
                    value={taskToAssign}
                    onValueChange={setTaskToAssign}
                    disabled={command.isPending}
                  >
                    <SelectTrigger aria-label="Task to assign to this milestone">
                      <SelectValue placeholder="Choose a task" />
                    </SelectTrigger>
                    <SelectContent>
                      {assignableTasks.map((task) => (
                        <SelectItem key={task.id} value={task.id}>
                          {task.title}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </Field>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={taskToAssign === NO_TASK || command.isPending}
                  loading={command.isPending && command.pendingCommand === 'assign_task'}
                  onClick={() => void assignTask()}
                >
                  Assign
                </Button>
              </div>
            ) : (
              <p className="mt-2 text-2xs leading-4 text-ink-muted">
                Every task in this project is already on this milestone.
              </p>
            )}
          </section>

          <section aria-label="Work items" className="border-t border-line pt-4">
            <h3 className="text-sm font-semibold text-ink">
              Work items ({live.workItemIds.length})
            </h3>
            {workItemsState?.error ? (
              <div className="mt-2">
                <InlineReadError
                  label="Work items"
                  error={workItemsState.error}
                  onRetry={workItemsState.refetch ?? (() => undefined)}
                  retrying={workItemsState.retrying}
                />
              </div>
            ) : null}
            {workItemsState?.isPending && childWorkItems.length === 0 ? (
              <div className="mt-2 flex flex-col gap-2" aria-busy="true" aria-label="Loading Work Items">
                <Skeleton className="h-4 w-2/3" />
                <Skeleton className="h-3 w-1/2" />
              </div>
            ) : childWorkItems.length === 0 ? (
              workItemsState?.error ? null : (
                <p className="mt-1 text-xs leading-5 text-ink-muted">
                  No Work Items are assigned yet. Assign one from its details sheet.
                </p>
              )
            ) : (
              <ul className="mt-2 divide-y divide-line rounded-md border border-line">
                {childWorkItems.map((item) => (
                  <li key={item.id} className="flex items-center gap-2 px-3 py-2 text-sm">
                    <span className="min-w-0 flex-1 truncate">{workItemLabel(item)}</span>
                    <span className="text-2xs text-ink-muted">{humanizeIdentifier(item.state)}</span>
                    <Button
                      variant="quiet"
                      size="sm"
                      disabled={command.isPending}
                      aria-label={`Remove ${workItemLabel(item)} from the milestone`}
                      onClick={() => void unassignWorkItem(item.id)}
                    >
                      Remove
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section aria-label="Activity" className="border-t border-line pt-4">
            <h3 className="text-sm font-semibold text-ink">Activity</h3>
            {live.activities.length === 0 ? (
              <p className="mt-1 text-xs text-ink-muted">No recorded activity yet.</p>
            ) : (
              <ul className="mt-2 flex flex-col gap-1.5">
                {live.activities.slice(0, 8).map((entry) => (
                  <li key={entry.id} className="flex items-start gap-2 text-xs text-ink-soft">
                    <span className="mt-1 h-1.5 w-1.5 shrink-0 rounded-full bg-line-strong" />
                    <span className="min-w-0 flex-1">{entry.summary}</span>
                    <time className="shrink-0 text-2xs text-ink-muted">
                      {formatMilestoneDate(entry.occurredAtMs)}
                    </time>
                  </li>
                ))}
              </ul>
            )}
          </section>

          {error ? (
            <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              {error}
            </p>
          ) : null}

          <section aria-label="Delete milestone" className="border-t border-line pt-4">
            {hasWork ? (
              <p className="mb-2 text-2xs leading-4 text-ink-muted">
                This milestone still owns work. The server refuses to delete it until its tasks and
                Work Items are removed; that reason is shown above if you try.
              </p>
            ) : (
              <p className="mb-2 text-2xs leading-4 text-ink-muted">
                Deleting a milestone cannot be undone.
              </p>
            )}
            {confirmDelete ? (
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="danger"
                  size="sm"
                  loading={command.isPending && command.pendingCommand === 'delete'}
                  onClick={() => void remove()}
                >
                  Delete milestone
                </Button>
                <Button variant="quiet" size="sm" onClick={() => setConfirmDelete(false)}>
                  Keep milestone
                </Button>
              </div>
            ) : (
              <Button
                variant="quiet"
                size="sm"
                leadingIcon={<TrashIcon size={14} />}
                onClick={() => setConfirmDelete(true)}
              >
                Delete milestone
              </Button>
            )}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  )
}

/** Task status text without importing the task drawer's presentation module. */
function cnTaskStatus(task: Task): string {
  if (task.status === 'blocked') return 'text-2xs font-medium text-danger-ink'
  if (isTaskCompleted(task.status)) return 'text-2xs font-medium text-success-ink'
  return 'text-2xs text-ink-muted'
}
