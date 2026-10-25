/**
 * Task detail = header + acceptance criteria + activity, three independent
 * reads that share one `enabled: !!id` gate. Keeping them separate means a
 * failed activity request does not blank the header.
 */
import { useAcceptanceCriteria, useTaskActivity, useTaskRecord } from '@/api/queries'
import { ApiError } from '@/api/errors'
import type { AcceptanceCriterion, ActivityEntry, Task } from '@/api/adapters'

export interface TaskDetail {
  task: Task | undefined
  criteria: AcceptanceCriterion[]
  activity: ActivityEntry[]
  isLoading: boolean
  error: ApiError | null
  refetch: () => void
}

const EMPTY_CRITERIA: AcceptanceCriterion[] = []
const EMPTY_ACTIVITY: ActivityEntry[] = []

export function useTaskDetail(id: string | undefined): TaskDetail {
  const task = useTaskRecord(id)
  const criteria = useAcceptanceCriteria(id)
  const activity = useTaskActivity(id)

  const error = task.error ?? criteria.error ?? activity.error ?? null

  return {
    task: task.data,
    criteria: criteria.data ?? EMPTY_CRITERIA,
    activity: activity.data ?? EMPTY_ACTIVITY,
    isLoading: Boolean(id) && (task.isPending || criteria.isPending || activity.isPending),
    error,
    refetch: () => {
      void task.refetch()
      void criteria.refetch()
      void activity.refetch()
    },
  }
}
