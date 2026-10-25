import { api } from '../client'
import { withQuery } from '../http'
import type { DayPlan, WorkloadInsights } from '../contracts'

const BASE = '/authoritative'

export interface DayPlanParams {
  nowMs: number
  availableMinutes: number
  maxTasks?: number
  projectId?: string
  includeUnsized?: boolean
  defaultMinutes?: number
}

export interface WorkloadParams {
  nowMs: number
  /** Defaults to 7 days on the server (604800000). */
  horizonMs?: number
}

export function fetchDayPlan(
  { nowMs, availableMinutes, maxTasks, projectId, includeUnsized, defaultMinutes }: DayPlanParams,
  signal?: AbortSignal,
): Promise<DayPlan> {
  return api.get<DayPlan>(
    withQuery(`${BASE}/day-plan`, {
      now_ms: nowMs,
      available_minutes: availableMinutes,
      max_tasks: maxTasks,
      project_id: projectId,
      include_unsized: includeUnsized,
      default_minutes: defaultMinutes,
    }),
    { signal },
  )
}

export function fetchWorkload(
  { nowMs, horizonMs }: WorkloadParams,
  signal?: AbortSignal,
): Promise<WorkloadInsights> {
  return api.get<WorkloadInsights>(
    withQuery(`${BASE}/workload`, { now_ms: nowMs, horizon_ms: horizonMs }),
    { signal },
  )
}
