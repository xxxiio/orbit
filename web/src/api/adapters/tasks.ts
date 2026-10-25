/**
 * Task and task-detail rows as the UI stores them.
 *
 * Trap 1 lives here: `TaskRecord` omits the id keys when null while
 * `TaskDetailResponse` always sends them, so both encodings collapse to the same
 * UI value. Acceptance criteria, activity and dependency edges are the task's
 * own sub-reads and are adapted beside it.
 */
import {
  isTaskPriority,
  type AcceptanceCriterionResponse,
  type ActivityResponse,
  type TaskDependencyEdge,
  type TaskDetailResponse,
  type TaskPriority,
  type TaskRecord,
  type TaskStatus,
} from '../contracts'
import { asStatus } from './shared'

export interface Task {
  id: string
  title: string
  status: TaskStatus
  /** `null` means unassigned, whichever wire encoding produced it. */
  projectId: string | null
  milestoneId: string | null
  estimatedMinutes: number | null
  earliestStartAtMs: number | null
  deadlineAtMs: number | null
  description: string
  priority: TaskPriority | null
  labels: string[]
}

function toPriority(value: string | null | undefined): TaskPriority | null {
  // The authority rejects free-form priorities, so a stray value means "legacy
  // or damaged data"; showing no priority is safer than inventing one.
  if (value === null || value === undefined) return null
  return isTaskPriority(value) ? value : null
}

/**
 * Trap 1: `TaskRecord` omits the id keys when null, `TaskDetailResponse`
 * always sends them. `?? null` collapses both encodings to the same UI value.
 */
export function toTask(record: TaskRecord | TaskDetailResponse): Task {
  return {
    id: record.id,
    title: record.title,
    status: asStatus<TaskStatus>(record.status),
    projectId: record.project_id ?? null,
    milestoneId: record.milestone_id ?? null,
    estimatedMinutes: record.estimated_minutes,
    earliestStartAtMs: record.earliest_start_at_ms,
    deadlineAtMs: record.deadline_at_ms,
    description: record.description,
    priority: toPriority(record.priority),
    labels: record.labels,
  }
}

/** The detail endpoint has the same UI shape as a snapshot task. */
export function toTaskDetail(record: TaskDetailResponse): Task {
  return toTask(record)
}

export interface AcceptanceCriterion {
  id: string
  text: string
  completed: boolean
  completedAtMs: number | null
  position: number
}

export function toAcceptanceCriterion(record: AcceptanceCriterionResponse): AcceptanceCriterion {
  return {
    id: record.id,
    text: record.text,
    completed: record.completed,
    completedAtMs: record.completed_at_ms,
    position: record.position,
  }
}

export interface ActivityEntry {
  id: string
  kind: string
  /** `human` | `system` | `agent:<id>` | `external:<device>` */
  origin: string
  occurredAtMs: number
  summary: string
}

export function toActivityEntry(record: ActivityResponse): ActivityEntry {
  return {
    id: record.id,
    kind: record.kind,
    origin: record.origin,
    occurredAtMs: record.occurred_at_ms,
    summary: record.summary,
  }
}

export interface Dependency {
  taskId: string
  prerequisiteId: string
}

export function toDependency(edge: TaskDependencyEdge): Dependency {
  return { taskId: edge.task_id, prerequisiteId: edge.prerequisite_id }
}
