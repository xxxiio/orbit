/**
 * Planning reads: readiness, next actions, the day plan and workload.
 *
 * These projections are already approved by the server; the adapters only
 * rename fields. No client-side scheduling, ranking or readiness rule lives
 * here.
 */
import type { NextAction, PlanningDisposition, TaskAttention, TaskReadiness, TaskStatus } from '../contracts'
import { asStatus } from './shared'

export interface ReadinessEntry {
  taskId: string
  title: string
  status: TaskStatus
  actionable: boolean
  reason: string
  blockingTaskIds: string[]
  prerequisiteIds: string[]
  earliestStartAtMs: number | null
  deadlineAtMs: number | null
  estimatedMinutes: number | null
  projectId: string | null
}

export function toReadinessEntry(entry: TaskReadiness): ReadinessEntry {
  return {
    taskId: entry.task_id,
    title: entry.title,
    status: asStatus<TaskStatus>(entry.status),
    actionable: entry.actionable,
    reason: entry.reason,
    blockingTaskIds: entry.blocking_task_ids,
    prerequisiteIds: entry.prerequisite_ids,
    earliestStartAtMs: entry.earliest_start_at_ms,
    deadlineAtMs: entry.deadline_at_ms,
    estimatedMinutes: entry.estimated_minutes,
    projectId: entry.project_id,
  }
}

export interface PlannedTaskEntry {
  taskId: string
  title: string
  projectId: string | null
  disposition: PlanningDisposition
  estimatedMinutes: number | null
  allocatedMinutes: number
  startOffsetMinutes: number | null
  blockingTaskIds: string[]
  explanation: string[]
  rank: number | null
}

export interface TaskReadinessView {
  revision: number
  actionableCount: number
  blockedCount: number
  tasks: ReadinessEntry[]
}

export function toTaskReadiness(snapshot: {
  revision: number
  tasks: TaskReadiness[]
  actionable_count: number
  blocked_count: number
}): TaskReadinessView {
  return {
    revision: snapshot.revision,
    actionableCount: snapshot.actionable_count,
    blockedCount: snapshot.blocked_count,
    tasks: snapshot.tasks.map(toReadinessEntry),
  }
}

export interface NextActionEntry {
  taskId: string
  title: string
  projectId: string | null
  rank: number
  deadlineAtMs: number | null
  estimatedMinutes: number | null
  unblocksTaskIds: string[]
  unblocksCount: number
  overdue: boolean
  explanation: string[]
}

export interface NextActionsView {
  revision: number
  evaluatedAtMs: number
  actionableTotal: number
  blockedTotal: number
  actions: NextActionEntry[]
}

export function toNextActions(report: {
  revision: number
  evaluated_at_ms: number
  actionable_total: number
  blocked_total: number
  actions: NextAction[]
}): NextActionsView {
  return {
    revision: report.revision,
    evaluatedAtMs: report.evaluated_at_ms,
    actionableTotal: report.actionable_total,
    blockedTotal: report.blocked_total,
    actions: report.actions.map((action) => ({
      taskId: action.task_id,
      title: action.title,
      projectId: action.project_id,
      rank: action.rank,
      deadlineAtMs: action.deadline_at_ms,
      estimatedMinutes: action.estimated_minutes,
      unblocksTaskIds: action.unblocks_task_ids,
      unblocksCount: action.unblocks_count,
      overdue: action.overdue,
      explanation: action.explanation,
    })),
  }
}

export interface DayPlanView {
  revision: number
  nowMs: number
  availableMinutes: number
  allocatedMinutes: number
  remainingMinutes: number
  selectedCount: number
  actionableCount: number
  blockedCount: number
  tasks: PlannedTaskEntry[]
  projects: Array<{
    projectId: string | null
    selected: number
    actionable: number
    blocked: number
    allocatedMinutes: number
  }>
  warnings: string[]
}

export function toDayPlan(plan: {
  revision: number
  now_ms: number
  available_minutes: number
  allocated_minutes: number
  remaining_minutes: number
  selected_count: number
  actionable_count: number
  blocked_count: number
  tasks: Array<{
    task_id: string
    title: string
    project_id: string | null
    disposition: PlanningDisposition
    estimated_minutes: number | null
    allocated_minutes: number
    start_offset_minutes: number | null
    blocking_task_ids: string[]
    explanation: string[]
    rank: number | null
  }>
  projects: Array<{
    project_id: string | null
    selected: number
    actionable: number
    blocked: number
    allocated_minutes: number
  }>
  warnings: string[]
}): DayPlanView {
  return {
    revision: plan.revision,
    nowMs: plan.now_ms,
    availableMinutes: plan.available_minutes,
    allocatedMinutes: plan.allocated_minutes,
    remainingMinutes: plan.remaining_minutes,
    selectedCount: plan.selected_count,
    actionableCount: plan.actionable_count,
    blockedCount: plan.blocked_count,
    tasks: plan.tasks.map((task) => ({
      taskId: task.task_id,
      title: task.title,
      projectId: task.project_id,
      disposition: task.disposition,
      estimatedMinutes: task.estimated_minutes,
      allocatedMinutes: task.allocated_minutes,
      startOffsetMinutes: task.start_offset_minutes,
      blockingTaskIds: task.blocking_task_ids,
      explanation: task.explanation,
      rank: task.rank,
    })),
    projects: plan.projects.map((project) => ({
      projectId: project.project_id,
      selected: project.selected,
      actionable: project.actionable,
      blocked: project.blocked,
      allocatedMinutes: project.allocated_minutes,
    })),
    warnings: plan.warnings,
  }
}

export interface WorkloadAttention {
  taskId: string
  title: string
  projectId: string | null
  reasons: string[]
  details: string[]
  blockingIds: string[]
  downstreamCount: number
  estimatedMinutes: number | null
  deadlineAtMs: number | null
}

export function toWorkloadAttention(entry: TaskAttention): WorkloadAttention {
  return {
    taskId: entry.task_id,
    title: entry.title,
    projectId: entry.project_id,
    reasons: entry.reasons,
    details: entry.details,
    blockingIds: entry.blocking_ids,
    downstreamCount: entry.downstream_count,
    estimatedMinutes: entry.estimated_minutes,
    deadlineAtMs: entry.deadline_at_ms,
  }
}

export interface ProjectWorkloadView {
  projectId: string | null
  total: number
  completed: number
  actionable: number
  blockedByPrerequisites: number
  waitingOrDeferred: number
  overdue: number
  missingEstimates: number
  knownRemainingMinutes: number
  attention: WorkloadAttention[]
}

export interface WorkloadView {
  revision: number
  evaluatedAtMs: number
  horizonEndMs: number
  totalTasks: number
  unfinishedTasks: number
  totalKnownRemainingMinutes: number
  projects: ProjectWorkloadView[]
}

export function toWorkload(report: {
  revision: number
  evaluated_at_ms: number
  horizon_end_ms: number
  total_tasks: number
  unfinished_tasks: number
  total_known_remaining_minutes: number
  projects: Array<{
    project_id: string | null
    total: number
    completed: number
    actionable: number
    blocked_by_prerequisites: number
    waiting_or_deferred: number
    overdue: number
    missing_estimates: number
    known_remaining_minutes: number
    attention: TaskAttention[]
  }>
}): WorkloadView {
  return {
    revision: report.revision,
    evaluatedAtMs: report.evaluated_at_ms,
    horizonEndMs: report.horizon_end_ms,
    totalTasks: report.total_tasks,
    unfinishedTasks: report.unfinished_tasks,
    totalKnownRemainingMinutes: report.total_known_remaining_minutes,
    projects: report.projects.map((project) => ({
      projectId: project.project_id,
      total: project.total,
      completed: project.completed,
      actionable: project.actionable,
      blockedByPrerequisites: project.blocked_by_prerequisites,
      waitingOrDeferred: project.waiting_or_deferred,
      overdue: project.overdue,
      missingEstimates: project.missing_estimates,
      knownRemainingMinutes: project.known_remaining_minutes,
      attention: project.attention.map(toWorkloadAttention),
    })),
  }
}
