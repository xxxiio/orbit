/**
 * Write an accepted task command's response back into the read caches.
 *
 * The task drawer keeps a full record in memory and sends `set_details` as a
 * *full* record (description, priority and labels together). Without a write
 * back, a second edit made in the window between "server accepted" and "refetch
 * settled" would be built from the pre-write record and silently revert the
 * first edit. The server's response is the newest record this client has seen,
 * so it replaces every field the record carries.
 *
 * This is cache coherence, not an optimistic update: it runs only after an
 * accepted response, and it writes exactly what the server returned.
 */
import type { QueryClient } from '@tanstack/react-query'
import { queryKeys, type Task } from '@/hooks'
import type { AuthoritySnapshotResponse, TaskDetailResponse } from '@/api/contracts'

/** The record fields a task command response carries (detail wire shape). */
function recordFields(task: Task) {
  return {
    title: task.title,
    status: task.status,
    project_id: task.projectId,
    milestone_id: task.milestoneId,
    estimated_minutes: task.estimatedMinutes,
    earliest_start_at_ms: task.earliestStartAtMs,
    deadline_at_ms: task.deadlineAtMs,
    description: task.description,
    priority: task.priority,
    labels: [...task.labels],
  }
}

/** The same fields in the snapshot's shape, where absent ids are omitted. */
function snapshotFields(task: Task) {
  return {
    ...recordFields(task),
    project_id: task.projectId ?? undefined,
    milestone_id: task.milestoneId ?? undefined,
  }
}

export function writeAcceptedTask(queryClient: QueryClient, task: Task): void {
  queryClient.setQueryData<TaskDetailResponse>(queryKeys.task(task.id), (previous) =>
    previous ? { ...previous, ...recordFields(task) } : previous,
  )

  queryClient.setQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot(), (previous) => {
    if (!previous) return previous
    let changed = false
    const tasks = previous.tasks.map((record) => {
      if (record.id !== task.id) return record
      changed = true
      return { ...record, ...snapshotFields(task) }
    })
    return changed ? { ...previous, tasks } : previous
  })
}
