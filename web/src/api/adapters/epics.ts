/**
 * Epic rows as the UI consumes them.
 *
 * Progress and activity come from the server; the browser never recalculates a
 * hierarchy it did not author. Dates stay nullable: an epic without dates is
 * unscheduled, not zero-length.
 */
import type { EpicRecord, EpicResult, EpicStatus } from '../contracts'
import { asStatus } from './shared'

export interface EpicProgress {
  total: number
  completed: number
  blocked: number
  percent: number
}

export interface EpicActivityEntry {
  id: string
  kind: string
  actor: string
  occurredAtMs: number
  summary: string
}

export interface Epic {
  id: string
  projectId: string
  title: string
  description: string
  status: EpicStatus
  priority: string | null
  owner: string | null
  startAtMs: number | null
  targetAtMs: number | null
  createdAtMs: number
  updatedAtMs: number
  taskIds: string[]
  workItemIds: string[]
  progress: EpicProgress
  activities: EpicActivityEntry[]
}

export function toEpic(record: EpicRecord): Epic {
  return {
    id: record.id,
    projectId: record.project_id,
    title: record.title,
    description: record.description,
    status: asStatus<EpicStatus>(record.status),
    priority: record.priority,
    owner: record.owner,
    startAtMs: record.start_at_ms,
    targetAtMs: record.target_at_ms,
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
    activities: record.activities.map((entry) => ({
      id: entry.id,
      kind: entry.kind,
      actor: entry.actor,
      occurredAtMs: entry.occurred_at_ms,
      summary: entry.summary,
    })),
  }
}

export interface EpicMutationOutcome {
  revision: number
  duplicate: boolean
  epic: Epic | null
}

export function toEpicResult(result: EpicResult): EpicMutationOutcome {
  return {
    revision: result.revision,
    duplicate: result.duplicate,
    epic: result.epic ? toEpic(result.epic) : null,
  }
}
