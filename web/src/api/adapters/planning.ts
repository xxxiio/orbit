/**
 * Planning read adapters (camelCase views). Re-exported from `adapters.ts` so
 * feature code keeps importing from `@/api/adapters`.
 *
 * These functions only rename fields. Every number, ordering, disposition and
 * reason is exactly what the server sent: no client-side scheduling or
 * readiness logic lives here.
 */
import type {
  ApprovedScheduleReceipt,
  PortfolioCapacityPlan,
  ProjectDependencyReport,
  SchedulePlan,
  TaskGraphReport,
  WorkBrief,
} from '../contracts'

/* ------------------------------------------------------------------ */
/* Task graph                                                          */
/* ------------------------------------------------------------------ */

export interface TaskGraphNodeView {
  taskId: string
  title: string
  status: string
  directPrerequisites: string[]
  allPrerequisites: string[]
  directDependents: string[]
  allDependents: string[]
  prerequisiteDepth: number
  unfinishedPrerequisites: string[]
  actionable: boolean
}

export interface TaskGraphView {
  revision: number
  dependencyOrder: string[]
  roots: string[]
  leaves: string[]
  nodes: TaskGraphNodeView[]
  edgeCount: number
}

export function toTaskGraph(report: TaskGraphReport): TaskGraphView {
  return {
    revision: report.revision,
    dependencyOrder: report.dependency_order,
    roots: report.roots,
    leaves: report.leaves,
    nodes: report.nodes.map((node) => ({
      taskId: node.task_id,
      title: node.title,
      status: node.status,
      directPrerequisites: node.direct_prerequisites,
      allPrerequisites: node.all_prerequisites,
      directDependents: node.direct_dependents,
      allDependents: node.all_dependents,
      prerequisiteDepth: node.prerequisite_depth,
      unfinishedPrerequisites: node.unfinished_prerequisites,
      actionable: node.actionable,
    })),
    edgeCount: report.edge_count,
  }
}

/* ------------------------------------------------------------------ */
/* Project dependencies                                                */
/* ------------------------------------------------------------------ */

export interface ProjectDependencyView {
  revision: number
  evaluatedAtMs: number
  projectFilter: string | null
  summaries: Array<{
    projectId: string | null
    total: number
    actionable: number
    blocked: number
    completed: number
    externalPrerequisiteCount: number
    externalBlockedTaskCount: number
    outgoingDependencyCount: number
    criticalTaskIds: string[]
    readyTaskIds: string[]
  }>
  tasks: Array<{
    taskId: string
    projectId: string | null
    milestoneId: string | null
    title: string
    actionable: boolean
    blockingInternal: string[]
    externalBlockerCount: number
    downstreamInProject: number
    downstreamOtherProjects: number
    longestPrerequisiteChain: number
    deadlineAtMs: number | null
  }>
  bottlenecks: Array<{
    taskId: string
    projectId: string | null
    waitingDirect: number
    waitingTransitive: number
    waitingProjectCount: number
    crossProjectWaiting: number
    pendingEstimatedMinutes: number
    missingEstimateCount: number
  }>
  crossProjectEdgeCount: number
  warnings: string[]
}

export function toProjectDependencies(report: ProjectDependencyReport): ProjectDependencyView {
  return {
    revision: report.revision,
    evaluatedAtMs: report.evaluated_at_ms,
    projectFilter: report.project_filter,
    summaries: report.summaries.map((summary) => ({
      projectId: summary.project_id,
      total: summary.total,
      actionable: summary.actionable,
      blocked: summary.blocked,
      completed: summary.completed,
      externalPrerequisiteCount: summary.external_prerequisite_count,
      externalBlockedTaskCount: summary.external_blocked_task_count,
      outgoingDependencyCount: summary.outgoing_dependency_count,
      criticalTaskIds: summary.critical_task_ids,
      readyTaskIds: summary.ready_task_ids,
    })),
    tasks: report.tasks.map((task) => ({
      taskId: task.task_id,
      projectId: task.project_id,
      milestoneId: task.milestone_id,
      title: task.title,
      actionable: task.actionable,
      blockingInternal: task.blocking_internal,
      externalBlockerCount: task.external_blocker_count,
      downstreamInProject: task.downstream_in_project,
      downstreamOtherProjects: task.downstream_other_projects,
      longestPrerequisiteChain: task.longest_prerequisite_chain,
      deadlineAtMs: task.deadline_at_ms,
    })),
    bottlenecks: report.bottlenecks.map((bottleneck) => ({
      taskId: bottleneck.task_id,
      projectId: bottleneck.project_id,
      waitingDirect: bottleneck.waiting_direct,
      waitingTransitive: bottleneck.waiting_transitive,
      waitingProjectCount: bottleneck.waiting_project_count,
      crossProjectWaiting: bottleneck.cross_project_waiting,
      pendingEstimatedMinutes: bottleneck.pending_estimated_minutes,
      missingEstimateCount: bottleneck.missing_estimate_count,
    })),
    crossProjectEdgeCount: report.cross_project_edge_count,
    warnings: report.warnings,
  }
}

/* ------------------------------------------------------------------ */
/* Work brief                                                          */
/* ------------------------------------------------------------------ */

export interface WorkBriefView {
  revision: number
  evaluatedAtMs: number
  endsAtMs: number
  availableMinutes: number
  proposedMinutes: number
  scheduledCount: number
  readyCount: number
  blockedCount: number
  deferredCount: number
  completedCount: number
  items: Array<{
    id: string
    title: string
    projectId: string | null
    milestoneId: string | null
    state: string
    recommendedRank: number | null
    proposedStartMs: number | null
    proposedEndMs: number | null
    deadlineAtMs: number | null
    blockedBy: string[]
    unblocks: string[]
    explanation: string[]
    portfolioRank: number | null
    sameProjectDownstream: number
    otherProjectDownstream: number
    immediatelyUnblocked: number
    portfolioReasons: string[]
  }>
  projects: Array<{
    projectId: string | null
    scheduled: number
    ready: number
    blocked: number
    deferred: number
    completed: number
  }>
  milestones: Array<{
    projectId: string | null
    milestoneId: string | null
    scheduled: number
    ready: number
    blocked: number
    deferred: number
    completed: number
    proposedMinutes: number
    overdueTaskIds: string[]
    prerequisiteBottlenecks: Array<{
      prerequisiteId: string
      waitingTaskIds: string[]
      waitingCount: number
    }>
    leverageActions: Array<{
      taskId: string
      portfolioRank: number
      immediatelyUnblocked: number
      crossProjectDownstream: number
      downstreamTotal: number
    }>
  }>
  warnings: string[]
  portfolioActionableTotal: number
  crossProjectLeverageCount: number
}

export function toWorkBrief(brief: WorkBrief): WorkBriefView {
  return {
    revision: brief.revision,
    evaluatedAtMs: brief.evaluated_at_ms,
    endsAtMs: brief.ends_at_ms,
    availableMinutes: brief.available_minutes,
    proposedMinutes: brief.proposed_minutes,
    scheduledCount: brief.scheduled_count,
    readyCount: brief.ready_count,
    blockedCount: brief.blocked_count,
    deferredCount: brief.deferred_count,
    completedCount: brief.completed_count,
    items: brief.items.map((item) => ({
      id: item.id,
      title: item.title,
      projectId: item.project_id,
      milestoneId: item.milestone_id,
      state: item.state,
      recommendedRank: item.recommended_rank,
      proposedStartMs: item.proposed_start_ms,
      proposedEndMs: item.proposed_end_ms,
      deadlineAtMs: item.deadline_at_ms,
      blockedBy: item.blocked_by,
      unblocks: item.unblocks,
      explanation: item.explanation,
      portfolioRank: item.portfolio_rank,
      sameProjectDownstream: item.same_project_downstream,
      otherProjectDownstream: item.other_project_downstream,
      immediatelyUnblocked: item.immediately_unblocked,
      portfolioReasons: item.portfolio_reasons,
    })),
    projects: brief.projects.map((project) => ({
      projectId: project.project_id,
      scheduled: project.scheduled,
      ready: project.ready,
      blocked: project.blocked,
      deferred: project.deferred,
      completed: project.completed,
    })),
    milestones: brief.milestones.map((milestone) => ({
      projectId: milestone.project_id,
      milestoneId: milestone.milestone_id,
      scheduled: milestone.scheduled,
      ready: milestone.ready,
      blocked: milestone.blocked,
      deferred: milestone.deferred,
      completed: milestone.completed,
      proposedMinutes: milestone.proposed_minutes,
      overdueTaskIds: milestone.overdue_task_ids,
      prerequisiteBottlenecks: milestone.prerequisite_bottlenecks.map((bottleneck) => ({
        prerequisiteId: bottleneck.prerequisite_id,
        waitingTaskIds: bottleneck.waiting_task_ids,
        waitingCount: bottleneck.waiting_count,
      })),
      leverageActions: milestone.leverage_actions.map((action) => ({
        taskId: action.task_id,
        portfolioRank: action.portfolio_rank,
        immediatelyUnblocked: action.immediately_unblocked,
        crossProjectDownstream: action.cross_project_downstream,
        downstreamTotal: action.downstream_total,
      })),
    })),
    warnings: brief.warnings,
    portfolioActionableTotal: brief.portfolio_actionable_total,
    crossProjectLeverageCount: brief.cross_project_leverage_count,
  }
}

/* ------------------------------------------------------------------ */
/* Schedule plan                                                       */
/* ------------------------------------------------------------------ */

export interface SchedulePlanView {
  taskRevision: number
  scheduleRevision: number
  startsAtMs: number
  endsAtMs: number
  availableMinutes: number
  allocatedMinutes: number
  occupiedMinutes: number
  freeWindows: Array<{ startsAtMs: number; endsAtMs: number; minutes: number }>
  slots: Array<{
    taskId: string
    title: string
    projectId: string | null
    startsAtMs: number
    endsAtMs: number
    minutes: number
    ranking: number
    explanation: string[]
  }>
  unplaced: Array<{ taskId: string; reason: string }>
  warnings: string[]
}

export function toSchedulePlan(plan: SchedulePlan): SchedulePlanView {
  return {
    taskRevision: plan.task_revision,
    scheduleRevision: plan.schedule_revision,
    startsAtMs: plan.starts_at_ms,
    endsAtMs: plan.ends_at_ms,
    availableMinutes: plan.available_minutes,
    allocatedMinutes: plan.allocated_minutes,
    occupiedMinutes: plan.occupied_minutes,
    freeWindows: plan.free_windows.map((window) => ({
      startsAtMs: window.starts_at_ms,
      endsAtMs: window.ends_at_ms,
      minutes: window.minutes,
    })),
    slots: plan.slots.map((slot) => ({
      taskId: slot.task_id,
      title: slot.title,
      projectId: slot.project_id,
      startsAtMs: slot.starts_at_ms,
      endsAtMs: slot.ends_at_ms,
      minutes: slot.minutes,
      ranking: slot.ranking,
      explanation: slot.explanation,
    })),
    unplaced: plan.unplaced.map((entry) => ({ taskId: entry.task_id, reason: entry.reason })),
    warnings: plan.warnings,
  }
}

/* ------------------------------------------------------------------ */
/* Portfolio capacity                                                  */
/* ------------------------------------------------------------------ */

export interface PortfolioCapacityView {
  revision: number
  evaluatedAtMs: number
  projectFilter: string | null
  capacityMinutes: number
  assignedMinutes: number
  unassignedMinutes: number
  actionableTotal: number
  blockedTotal: number
  chosen: Array<{
    taskId: string
    title: string
    projectId: string | null
    minutes: number
    portfolioRank: number
    immediateUnlocks: number
    crossProjectDownstream: number
    rationale: string[]
  }>
  excluded: Array<{ taskId: string; reason: string }>
  warnings: string[]
}

export function toPortfolioCapacity(plan: PortfolioCapacityPlan): PortfolioCapacityView {
  return {
    revision: plan.revision,
    evaluatedAtMs: plan.evaluated_at_ms,
    projectFilter: plan.project_filter,
    capacityMinutes: plan.capacity_minutes,
    assignedMinutes: plan.assigned_minutes,
    unassignedMinutes: plan.unassigned_minutes,
    actionableTotal: plan.actionable_total,
    blockedTotal: plan.blocked_total,
    chosen: plan.chosen.map((item) => ({
      taskId: item.task_id,
      title: item.title,
      projectId: item.project_id,
      minutes: item.minutes,
      portfolioRank: item.portfolio_rank,
      immediateUnlocks: item.immediate_unlocks,
      crossProjectDownstream: item.cross_project_downstream,
      rationale: item.rationale,
    })),
    excluded: plan.excluded.map((entry) => ({ taskId: entry.task_id, reason: entry.reason })),
    warnings: plan.warnings,
  }
}

/* ------------------------------------------------------------------ */
/* Approval receipt                                                    */
/* ------------------------------------------------------------------ */

export interface ApprovedScheduleOutcome {
  acceptedRevision: number
  /** True when this exact command id had already been applied. */
  replayed: boolean
  scheduleBlockIds: string[]
}

export function toApprovedScheduleReceipt(
  receipt: ApprovedScheduleReceipt,
): ApprovedScheduleOutcome {
  return {
    acceptedRevision: receipt.accepted_revision,
    replayed: receipt.replayed,
    scheduleBlockIds: receipt.schedule_block_ids,
  }
}
