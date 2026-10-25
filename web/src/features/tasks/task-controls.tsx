import type { Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import { AlertCircleIcon } from '@/components/ui/icons'
import { classifyError, userMessage } from '@/hooks'
import type { ApiError } from '@/api/errors'
import { CompactSelect, type CompactOption } from './compact-select'
import { priorityLabels, statusLabel, statusLabels, TASK_PRIORITIES, TASK_STATUSES } from './task-meta'

const statusOptions: CompactOption[] = (TASK_STATUSES as readonly TaskStatus[]).map((status) => ({
  value: status,
  label: statusLabels[status],
}))

const priorityOptions: CompactOption[] = [
  { value: '', label: 'No priority' },
  ...(TASK_PRIORITIES as readonly TaskPriority[]).map((priority) => ({
    value: priority,
    label: priorityLabels[priority],
  })),
]

export interface TaskStatusControlProps {
  task: Task
  pending?: boolean
  invalid?: boolean
  onChange: (status: TaskStatus) => void
  className?: string
}

export function TaskStatusControl({
  task,
  pending,
  invalid,
  onChange,
  className,
}: TaskStatusControlProps) {
  return (
    <CompactSelect
      label={`Status for ${task.title}`}
      value={task.status}
      options={statusOptions}
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

/** Inline, row-scoped rejection. The message is the server's, scrubbed. */
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
