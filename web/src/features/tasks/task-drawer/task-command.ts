import type {
  CommandMutation,
  TaskCommandAttempt,
  TaskMutationOutcome,
} from '@/hooks'

/** The shared task command handle: attempts carry their own idempotency key. */
export type TaskCommand = CommandMutation<TaskMutationOutcome, TaskCommandAttempt>
