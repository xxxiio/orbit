import type { Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import type { CommandMutation, TaskCommandAttempt, TaskCommandInput, TaskMutationOutcome } from '@/hooks'
import { taskCommandAttempt, useCommandFeedback } from '@/hooks'

export type TaskCommandField = 'status' | 'project' | 'priority'

export interface TaskCommandPending {
  taskId: string
  field: TaskCommandField
}

function fieldOf(type: TaskCommandInput['type']): TaskCommandField {
  return type === 'change_status' ? 'status' : type === 'assign_project' ? 'project' : 'priority'
}

/**
 * Row-scoped command state: which row/field is in flight and which row (if any)
 * the server rejected or could not confirm. Every task list derives its pending
 * and error props from this one hook instead of repeating the
 * `command.variables.input.type` switch.
 */
export function useTaskCommandState(
  command: CommandMutation<TaskMutationOutcome, TaskCommandAttempt>,
) {
  const { run } = useCommandFeedback()

  const pending: TaskCommandPending | null =
    command.isPending && command.variables
      ? {
          taskId: command.variables.input.taskId,
          field: fieldOf(command.variables.input.type),
        }
      : null

  // Both definitive refusals and unconfirmed writes are surfaced on the row;
  // `TaskControlError` picks the honest copy for each.
  const error: ApiError | null =
    command.disposition === 'rejected' || command.disposition === 'uncertain'
      ? command.error
      : null
  const errorTaskId = error ? command.variables?.input.taskId ?? null : null
  const errorField: TaskCommandField | null = error && command.variables
    ? fieldOf(command.variables.input.type)
    : null

  const changeStatus = (task: Task, status: TaskStatus) => {
    void run('Status not changed', () =>
      command.mutateAsync(
        taskCommandAttempt({ type: 'change_status', taskId: task.id, status }),
      ),
    )
  }

  const changeProject = (task: Task, projectId: string | null) => {
    void run('Project not changed', () =>
      command.mutateAsync(
        taskCommandAttempt({ type: 'assign_project', taskId: task.id, projectId }),
      ),
    )
  }

  const changePriority = (task: Task, priority: TaskPriority | null) => {
    void run('Priority not changed', () =>
      command.mutateAsync(
        taskCommandAttempt({
          type: 'set_details',
          taskId: task.id,
          description: task.description,
          priority,
          labels: task.labels,
        }),
      ),
    )
  }

  return { pending, error, errorTaskId, errorField, changeStatus, changeProject, changePriority }
}
