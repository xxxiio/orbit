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

export interface MilestoneProgress {
  total: number
  completed: number
  blocked: number
  percent: number
}

export interface MilestoneActivityEntry {
  id: string
  kind: string
  actor: string
  occurredAtMs: number
  summary: string
}

export interface Milestone {
  id: string
  projectId: string
  name: string
  description: string
  status: string
  priority: string | null
  owner: string | null
  /** Scheduled start, when the milestone spans a window. */
  startAtMs: number | null
  /** Checkpoint date, when scheduled. */
  targetAtMs: number | null
  createdAtMs: number
  updatedAtMs: number
  taskIds: string[]
  workItemIds: string[]
  progress: MilestoneProgress
  activities: MilestoneActivityEntry[]
}

export function toMilestone(record: MilestoneRecord): Milestone {
  return {
    id: record.id,
    projectId: record.project_id,
    name: record.name,
    description: record.description,
    status: record.status,
    priority: record.priority,
    owner: record.owner,
    startAtMs: record.start_at_ms,
    targetAtMs: record.target_at_ms ?? null,
    createdAtMs: record.created_at_ms,
    updatedAtMs: record.updated_at_ms,
    taskIds: record.task_ids,
    workItemIds: record.work_item_ids,
    progress: {
      total: record.progress.total,
      completed: record.progress.completed,
      blocked: record.progress.blocked,
      percent: record.progress.percent,
    },
    activities: record.activities.map((activity) => ({
      id: activity.id,
      kind: activity.kind,
      actor: activity.actor,
      occurredAtMs: activity.occurred_at_ms,
      summary: activity.summary,
    })),
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
