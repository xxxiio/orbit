/**
 * Project and milestone rows as the UI stores them.
 *
 * A project carries the status the server accepted; a milestone mutation result
 * carries `null` for `delete`, when the server removed the record.
 */
import type {
  MilestoneRecord,
  MilestoneResult,
  ProjectRecord,
  ProjectStatus,
  ProjectStatusesResponse,
} from '../contracts'
import { asStatus } from './shared'

export interface Project {
  id: string
  name: string
  status: ProjectStatus
}

export function toProject(record: ProjectRecord): Project {
  return {
    id: record.id,
    name: record.name,
    status: asStatus<ProjectStatus>(record.status),
  }
}

/** One status and the transitions the domain accepts from it. */
export interface ProjectStatusOption {
  status: ProjectStatus
  next: ProjectStatus[]
}

/**
 * The server's transition table, not a client copy. A status the server did not
 * report keeps an empty edge list so the form can tell the person it is
 * read-only instead of guessing what the move should be.
 */
export function toProjectStatusOptions(
  response: ProjectStatusesResponse,
): ProjectStatusOption[] {
  return response.statuses.map((record) => ({
    status: asStatus<ProjectStatus>(record.status),
    next: record.next.map((status) => asStatus<ProjectStatus>(status)),
  }))
}

export interface Milestone {
  id: string
  projectId: string
  name: string
  status: string
  /** Checkpoint date, when scheduled. */
  targetAtMs?: number | null
}

export function toMilestone(record: MilestoneRecord): Milestone {
  return {
    id: record.id,
    projectId: record.project_id,
    name: record.name,
    status: record.status,
    targetAtMs: record.target_at_ms ?? null,
  }
}

export interface MilestoneMutationOutcome {
  revision: number
  duplicate: boolean
  /** `null` for `delete`, because the server removes the record. */
  milestone: Milestone | null
}

export function toMilestoneResult(result: MilestoneResult): MilestoneMutationOutcome {
  return {
    revision: result.revision,
    duplicate: result.duplicate,
    milestone: result.milestone ? toMilestone(result.milestone) : null,
  }
}
