/**
 * The one task meta cluster: priority, labels, milestone, due date, estimate,
 * readiness and dependency counts as they are rendered next to a task's title.
 *
 * The board card and the table's stacked row show the same information; they
 * used to own two copies that drifted (a board said "No due date" while the row
 * said "No deadline"). `variant` only chooses the arrangement the surface
 * already had, never a different vocabulary.
 */
import type { ReadinessEntry, Task } from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { CalendarIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import {
  formatMinutes,
  formatMoment,
  isOverdue,
  priorityLabels,
  priorityTones,
  readinessReasonLabel,
} from './task-meta'

export interface TaskRowMetaProps {
  task: Task
  variant?: 'card' | 'row'
  /** Card: the project the task belongs to. */
  projectName?: string
  /** Row: the milestone name, when the row's column set includes milestones. */
  milestoneName?: string
  showPriority?: boolean
  showMilestone?: boolean
  showLabels?: boolean
  showEstimate?: boolean
  showReadiness?: boolean
  showDependencies?: boolean
  /** Server readiness for this task, when the Readiness column is on. */
  readiness?: ReadinessEntry
  /** How many tasks this one unblocks, when the Dependencies column is on. */
  unblocks?: number
}

export function TaskDueChip({ task }: { task: Task }) {
  if (task.deadlineAtMs === null) {
    return <span className="text-xs text-ink-faint">No deadline</span>
  }
  const overdue = isOverdue(task, Date.now())
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 whitespace-nowrap text-xs',
        overdue ? 'font-medium text-danger-ink' : 'text-ink-muted',
      )}
      title={formatMoment(task.deadlineAtMs)}
    >
      <CalendarIcon size={12} className="shrink-0 opacity-70" />
      {formatMoment(task.deadlineAtMs)}
    </span>
  )
}

export function TaskRowMeta({
  task,
  variant = 'row',
  projectName,
  milestoneName,
  showPriority = true,
  showMilestone = false,
  showLabels = false,
  showEstimate = false,
  showReadiness = false,
  showDependencies = false,
  readiness,
  unblocks,
}: TaskRowMetaProps) {
  if (variant === 'card') {
    return (
      <>
        <div className="mt-1 flex flex-wrap items-center gap-1.5">
          {projectName ? (
            <span className="text-2xs font-medium text-ink-muted">{projectName}</span>
          ) : null}
          {task.priority ? (
            <Badge tone={priorityTones[task.priority]}>{priorityLabels[task.priority]}</Badge>
          ) : null}
          {task.labels.slice(0, 2).map((label) => (
            <Badge key={label} tone="neutral" className="lowercase">
              {label}
            </Badge>
          ))}
          {task.labels.length > 2 ? (
            <span className="text-2xs text-ink-faint">+{task.labels.length - 2}</span>
          ) : null}
        </div>
        {task.deadlineAtMs !== null ? (
          <p
            className={cn(
              'mt-1 flex items-center gap-1 text-2xs',
              isOverdue(task, Date.now()) ? 'font-medium text-danger-ink' : 'text-ink-muted',
            )}
          >
            <CalendarIcon size={11} className="shrink-0 opacity-70" />
            {formatMoment(task.deadlineAtMs)}
          </p>
        ) : (
          <p className="mt-1 text-2xs text-ink-faint">No due date</p>
        )}
      </>
    )
  }

  return (
    <>
      {showPriority && task.priority ? (
        <Badge tone={priorityTones[task.priority]}>{priorityLabels[task.priority]}</Badge>
      ) : null}
      {showMilestone && milestoneName ? <Badge tone="neutral">{milestoneName}</Badge> : null}
      {showLabels
        ? task.labels.slice(0, 2).map((label) => (
            <Badge key={label} tone="neutral" className="lowercase">
              {label}
            </Badge>
          ))
        : null}
      <TaskDueChip task={task} />
      {showEstimate && task.estimatedMinutes !== null ? (
        <span className="text-2xs tabular-nums text-ink-muted">
          {formatMinutes(task.estimatedMinutes)}
        </span>
      ) : null}
      {showReadiness ? (
        <span className="text-2xs text-ink-faint">
          {readiness ? readinessReasonLabel(readiness.reason) : 'Readiness not reported'}
        </span>
      ) : null}
      {showDependencies && unblocks !== undefined ? (
        <span className="text-2xs text-ink-faint">unblocks {unblocks}</span>
      ) : null}
    </>
  )
}
