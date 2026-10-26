import type { Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import { AlertCircleIcon } from '@/components/ui/icons'
import { classifyError, isAmbiguousError, uncertainMessage, userMessage } from '@/hooks'
import type { ApiError } from '@/api/errors'
import { CompactSelect, type CompactOption } from './compact-select'
import { priorityLabels, statusLabel, statusLabels, TASK_PRIORITIES, TASK_STATUSES } from './task-meta'

/** The one status option list: row controls and list filters both use it. */
export const statusOptions: CompactOption[] = (TASK_STATUSES as readonly TaskStatus[]).map((status) => ({
  value: status,
  label: statusLabels[status],
}))

/**
 * The options a status control may offer. `allowedNext` is the server's edge
 * set for `current`; `null` means that read failed or has not resolved, so the
 * control keeps offering every status and the server remains the only refusal.
 * The current status is always present so the control can show the stored
 * value.
 */
export function statusOptionsFor(
  current: TaskStatus,
  allowedNext: readonly TaskStatus[] | null,
): CompactOption[] {
  if (allowedNext === null) return statusOptions
  const offered = new Set<TaskStatus>([current, ...allowedNext])
  return statusOptions.filter((option) => offered.has(option.value as TaskStatus))
}

/** The one priority option list; the empty value means "no priority". */
export const priorityOptions: CompactOption[] = [
  { value: '', label: 'No priority' },
  ...(TASK_PRIORITIES as readonly TaskPriority[]).map((priority) => ({
    value: priority,
    label: priorityLabels[priority],
  })),
]

/**
 * Radix `Select` treats `''` as "show the placeholder", so forms use an
 * explicit sentinel for the same list.
 */
export const NO_PRIORITY = '__none__'

export const prioritySelectOptions: CompactOption[] = [
  { value: NO_PRIORITY, label: 'No priority' },
  ...priorityOptions.filter((option) => option.value !== ''),
]

export interface TaskStatusControlProps {
  task: Task
  /**
   * The server's edge set for `task.status`, read once by the list that owns
   * the rows. `null` means the read failed or has not resolved, so the control
   * keeps offering every status and the server remains the only refusal.
   */
  allowedNext: readonly TaskStatus[] | null
  pending?: boolean
  invalid?: boolean
  onChange: (status: TaskStatus) => void
  className?: string
}

export function TaskStatusControl({
  task,
  allowedNext,
  pending,
  invalid,
  onChange,
  className,
}: TaskStatusControlProps) {
  return (
    <CompactSelect
      label={`Status for ${task.title}`}
      value={task.status}
      options={statusOptionsFor(task.status, allowedNext)}
      pending={pending}
      invalid={invalid}
      onChange={(value) => onChange(value as TaskStatus)}
      className={className}
    />
  )
}

export interface TaskProjectControlProps {
  task: Task
  projects: Array<{ id: string; name: string }>
  pending?: boolean
  invalid?: boolean
  onChange: (projectId: string | null) => void
  className?: string
}

export function TaskProjectControl({
  task,
  projects,
  pending,
  invalid,
  onChange,
  className,
}: TaskProjectControlProps) {
  const options: CompactOption[] = [
    { value: '', label: 'No project' },
    ...projects.map((project) => ({ value: project.id, label: project.name })),
  ]

  return (
    <CompactSelect
      label={`Project for ${task.title}`}
      value={task.projectId ?? ''}
      options={options}
      pending={pending}
      invalid={invalid}
      onChange={(value) => onChange(value.length > 0 ? value : null)}
      className={className}
    />
  )
}

export interface TaskPriorityControlProps {
  task: Task
  pending?: boolean
  invalid?: boolean
  onChange: (priority: TaskPriority | null) => void
  className?: string
}

export function TaskPriorityControl({
  task,
  pending,
  invalid,
  onChange,
  className,
}: TaskPriorityControlProps) {
  return (
    <CompactSelect
      label={`Priority for ${task.title}`}
      value={task.priority ?? ''}
      options={priorityOptions}
      pending={pending}
      invalid={invalid}
      onChange={(value) => onChange(value.length > 0 ? (value as TaskPriority) : null)}
      className={className}
    />
  )
}

/** Inline, row-scoped feedback. The message is the server's, scrubbed. */
export function TaskControlError({
  error,
  field,
  taskStatus,
}: {
  error: ApiError | null
  field?: 'status' | 'project' | 'priority' | null
  taskStatus?: TaskStatus
}) {
  if (!error) return null
  // An unconfirmed write is not a refusal: do not claim the value was rejected,
  // and do not imply the old value is still authoritative.
  if (isAmbiguousError(error)) {
    return (
      <p role="alert" className="mt-1 flex items-start gap-1 text-2xs leading-4 text-warning-ink">
        <AlertCircleIcon size={11} className="mt-0.5 shrink-0" />
        <span className="min-w-0">{uncertainMessage(error)}</span>
      </p>
    )
  }
  const rejectedTransition =
    field === 'status' && taskStatus && classifyError(error) === 'validation'
  const text = rejectedTransition
    ? `The server did not allow that status change from ${statusLabel(taskStatus)}. ${userMessage(error)}`
    : userMessage(error)

  return (
    <p role="alert" className="mt-1 flex items-start gap-1 text-2xs leading-4 text-danger-ink">
      <AlertCircleIcon size={11} className="mt-0.5 shrink-0" />
      <span className="min-w-0">{text}</span>
    </p>
  )
}
