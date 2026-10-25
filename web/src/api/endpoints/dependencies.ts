import { api } from '../client'
import { withQuery } from '../http'
import type {
  NextActionsReport,
  TaskDependenciesResponse,
  TaskReadinessSnapshot,
} from '../contracts'

const BASE = '/authoritative'

export function fetchTaskDependencies(signal?: AbortSignal): Promise<TaskDependenciesResponse> {
  return api.get<TaskDependenciesResponse>(`${BASE}/task-dependencies`, { signal })
}

/**
 * `now_ms` is caller-supplied so an online evaluation and an offline one agree
 * on the exact observation time. The client never computes readiness itself.
 */
export function fetchTaskReadiness(nowMs: number, signal?: AbortSignal): Promise<TaskReadinessSnapshot> {
  return api.get<TaskReadinessSnapshot>(withQuery(`${BASE}/task-readiness`, { now_ms: nowMs }), { signal })
}

export function fetchNextActions(
  nowMs: number,
  limit = 20,
  signal?: AbortSignal,
): Promise<NextActionsReport> {
  return api.get<NextActionsReport>(
    withQuery(`${BASE}/next-actions`, { now_ms: nowMs, limit }),
    { signal },
  )
}
