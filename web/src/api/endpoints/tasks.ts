import { api } from '../client'
import type {
  AcceptanceCriterionResponse,
  ActivityResponse,
  TaskCommand,
  TaskDetailResponse,
  TaskResult,
} from '../contracts'

const BASE = '/authoritative'

export function fetchTaskDetail(id: string, signal?: AbortSignal): Promise<TaskDetailResponse> {
  return api.get<TaskDetailResponse>(`${BASE}/tasks/${encodeURIComponent(id)}`, { signal })
}

export function fetchAcceptanceCriteria(
  id: string,
  signal?: AbortSignal,
): Promise<AcceptanceCriterionResponse[]> {
  return api.get<AcceptanceCriterionResponse[]>(
    `${BASE}/tasks/${encodeURIComponent(id)}/acceptance-criteria`,
    { signal },
  )
}

/** Newest first, as returned by the server. */
export function fetchTaskActivity(id: string, signal?: AbortSignal): Promise<ActivityResponse[]> {
  return api.get<ActivityResponse[]>(`${BASE}/tasks/${encodeURIComponent(id)}/activity`, { signal })
}

export function sendTaskCommand(command: TaskCommand): Promise<TaskResult> {
  return api.post<TaskResult>(`${BASE}/task-commands`, command)
}
