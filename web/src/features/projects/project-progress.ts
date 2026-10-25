import type { TaskStatus } from '@/api/contracts'

/**
 * The one definition of project progress, shared by the projects index, the
 * project overview and Home's project rail.
 *
 * A task counts as **completed only when its status is `done`**. `cancelled`
 * work left the plan rather than finishing, so it is neither done nor open: it
 * contributes to the project's total and nothing else. Counting it as done (as
 * the projects index used to) made the same project read differently on two
 * screens.
 */
export function isTaskCompleted(status: TaskStatus): boolean {
  return status === 'done'
}

/** Open work: everything the server has not closed as done or cancelled. */
export function isTaskOpen(status: TaskStatus): boolean {
  return !isTaskCompleted(status) && status !== 'cancelled'
}
