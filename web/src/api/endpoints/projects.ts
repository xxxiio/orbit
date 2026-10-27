import { api } from '../client'
import type {
  ProjectCommand,
  ProjectResult,
  ProjectStatusesResponse,
} from '../contracts'

const BASE = '/authoritative'

/**
 * Project commands: create and update share one route. Update can rename,
 * change status, or both; `expected_revision` fences each attempt.
 */
export function sendProjectCommand(command: ProjectCommand): Promise<ProjectResult> {
  return api.post<ProjectResult>(`${BASE}/project-commands`, command)
}

/**
 * The statuses and the targets the domain accepts from each. A read: the
 * browser never carries a second transition table.
 */
export function fetchProjectStatuses(signal?: AbortSignal): Promise<ProjectStatusesResponse> {
  return api.get<ProjectStatusesResponse>(`${BASE}/project-statuses`, { signal })
}
