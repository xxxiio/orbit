/**
 * Project and milestone rows as the UI stores them.
 *
 * A project carries the status the server accepted; a milestone mutation result
 * carries `null` for `delete`, when the server removed the record.
 */
import type { MilestoneRecord, MilestoneResult, ProjectRecord, ProjectStatus } from '../contracts'
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

export interface Milestone {
  id: string
  projectId: string
  name: string
  status: string
}

export function toMilestone(record: MilestoneRecord): Milestone {
  return {
    id: record.id,
    projectId: record.project_id,
    name: record.name,
    status: record.status,
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
