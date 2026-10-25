import { api } from '../client'
import type { ProjectCommand, ProjectResult } from '../contracts'

const BASE = '/authoritative'

/**
 * The only project write the server offers: create. There is no rename,
 * status change, archive or delete route, so this module has one function.
 */
export function sendProjectCommand(command: ProjectCommand): Promise<ProjectResult> {
  return api.post<ProjectResult>(`${BASE}/project-commands`, command)
}
