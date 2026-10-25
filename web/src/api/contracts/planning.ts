/**
 * Planning-domain wire contracts: dependencies, readiness, graph, day plan,
 * work brief, schedule plan, portfolio capacity and the explicit approval gate.
 *
 * Re-exported from `contracts.ts`, so every existing `import ... from
 * '../contracts'` keeps working. Types moved here are copied verbatim from the
 * Rust structs; nothing is normalised (adapters do the camelCase mapping).
 */

/* ------------------------------------------------------------------ */
/* Shared planning vocabulary                                          */
/* ------------------------------------------------------------------ */

export type ReadinessReason =
  | 'ready'
  | 'unfinished_prerequisites'
  | 'scheduled_for_later'
  | 'past_deadline'
  | 'inbox'
  | 'planned'
  | 'waiting'
  | 'blocked'
  | 'deferred'
  | 'in_progress'
  | 'review'
  | 'done'
  | 'cancelled'
  | 'unknown_status'

export type PlanningDisposition =
  | 'selected'
  | 'outside_project'
  | 'not_actionable'
  | 'insufficient_capacity'
  | 'missing_estimate'
  | 'task_limit'

export type AttentionReason =
  | 'overdue'
  | 'due_soon'
  | 'missing_estimate'
  | 'unfinished_prerequisites'
  | 'future_start'
  | 'waiting_workflow'
  | 'unblocks_others'

/* ------------------------------------------------------------------ */
/* Dependencies, readiness, next actions                               */
/* ------------------------------------------------------------------ */

export interface TaskDependencyEdge {
  task_id: string
  prerequisite_id: string
}

export interface TaskDependenciesResponse {
  revision: number
  edges: TaskDependencyEdge[]
}

export interface TaskReadiness {
  task_id: string
  title: string
  status: string
  actionable: boolean
  reason: ReadinessReason
  blocking_task_ids: string[]
  prerequisite_ids: string[]
  earliest_start_at_ms: number | null
  deadline_at_ms: number | null
  estimated_minutes: number | null
  project_id: string | null
}

export interface TaskReadinessSnapshot {
  revision: number
  edges: TaskDependencyEdge[]
  tasks: TaskReadiness[]
  actionable_count: number
  blocked_count: number
}

export interface NextAction {
  task_id: string
  title: string
  project_id: string | null
  rank: number
  deadline_at_ms: number | null
  estimated_minutes: number | null
  unblocks_task_ids: string[]
  unblocks_count: number
  overdue: boolean
  explanation: string[]
}

export interface NextActionsReport {
  revision: number
  evaluated_at_ms: number
  actionable_total: number
  blocked_total: number
  actions: NextAction[]
}

/* ------------------------------------------------------------------ */
/* Task graph (read-only)                                              */
/* ------------------------------------------------------------------ */

export interface TaskGraphNode {
  task_id: string
  title: string
  status: string
  direct_prerequisites: string[]
  all_prerequisites: string[]
  direct_dependents: string[]
  all_dependents: string[]
  /** Number of dependency edges on the longest prerequisite chain. */
  prerequisite_depth: number
  unfinished_prerequisites: string[]
  actionable: boolean
}

export interface TaskGraphReport {
  revision: number
  /** A prerequisite always appears before tasks depending on it. */
  dependency_order: string[]
  roots: string[]
  leaves: string[]
  nodes: TaskGraphNode[]
  edge_count: number
}

/* ------------------------------------------------------------------ */
/* Project dependency planning                                         */
/* ------------------------------------------------------------------ */

export interface ProjectDependencySummary {
  project_id: string | null
  total: number
  actionable: number
  blocked: number
  completed: number
  external_prerequisite_count: number
  external_blocked_task_count: number
  outgoing_dependency_count: number
  critical_task_ids: string[]
  ready_task_ids: string[]
}

export interface ProjectDependencyBottleneck {
  task_id: string
  project_id: string | null
  waiting_direct: number
  waiting_transitive: number
  waiting_project_count: number
  cross_project_waiting: number
  pending_estimated_minutes: number
  missing_estimate_count: number
}

export interface ProjectDependencyTask {
  task_id: string
  project_id: string | null
  milestone_id: string | null
  title: string
  actionable: boolean
  blocking_internal: string[]
  /** Count only: a scoped report never exposes foreign task identifiers. */
  external_blocker_count: number
  downstream_in_project: number
  downstream_other_projects: number
  longest_prerequisite_chain: number
  deadline_at_ms: number | null
}

export interface ProjectDependencyReport {
  revision: number
  evaluated_at_ms: number
  project_filter: string | null
  summaries: ProjectDependencySummary[]
  tasks: ProjectDependencyTask[]
  bottlenecks: ProjectDependencyBottleneck[]
  cross_project_edge_count: number
  warnings: string[]
}

/* ------------------------------------------------------------------ */
/* Day plan                                                            */
/* ------------------------------------------------------------------ */

export interface PlannedTask {
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
}

export interface ProjectDaySummary {
  project_id: string | null
  selected: number
  actionable: number
  blocked: number
  allocated_minutes: number
}

export interface DayPlan {
  revision: number
  now_ms: number
  available_minutes: number
  allocated_minutes: number
  remaining_minutes: number
  selected_count: number
  actionable_count: number
  blocked_count: number
  tasks: PlannedTask[]
  projects: ProjectDaySummary[]
  warnings: string[]
}

/* ------------------------------------------------------------------ */
/* Workload                                                            */
/* ------------------------------------------------------------------ */

export interface TaskAttention {
  task_id: string
  title: string
  project_id: string | null
  reasons: AttentionReason[]
  details: string[]
  blocking_ids: string[]
  downstream_count: number
  estimated_minutes: number | null
  deadline_at_ms: number | null
}

export interface ProjectWorkload {
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
}

export interface WorkloadInsights {
  revision: number
  evaluated_at_ms: number
  horizon_end_ms: number
  total_tasks: number
  unfinished_tasks: number
  total_known_remaining_minutes: number
  projects: ProjectWorkload[]
}

/* ------------------------------------------------------------------ */
/* Work brief (today/next queue)                                       */
/* ------------------------------------------------------------------ */

/** `rename_all = "snake_case"` on the Rust enum. */
export type WorkBriefItemState = 'scheduled' | 'ready' | 'blocked' | 'deferred' | 'completed'

export interface WorkBriefItem {
  id: string
  title: string
  project_id: string | null
  milestone_id: string | null
  state: WorkBriefItemState
  recommended_rank: number | null
  proposed_start_ms: number | null
  proposed_end_ms: number | null
  deadline_at_ms: number | null
  blocked_by: string[]
  unblocks: string[]
  explanation: string[]
  portfolio_rank: number | null
  same_project_downstream: number
  other_project_downstream: number
  immediately_unblocked: number
  portfolio_reasons: string[]
}

export interface ProjectWorkSummary {
  project_id: string | null
  scheduled: number
  ready: number
  blocked: number
  deferred: number
  completed: number
}

export interface MilestoneLeverageAction {
  task_id: string
  portfolio_rank: number
  immediately_unblocked: number
  cross_project_downstream: number
  downstream_total: number
}

export interface WorkBottleneck {
  prerequisite_id: string
  waiting_task_ids: string[]
  waiting_count: number
}

export interface MilestoneWorkSummary {
  project_id: string | null
  milestone_id: string | null
  scheduled: number
  ready: number
  blocked: number
  deferred: number
  completed: number
  proposed_minutes: number
  overdue_task_ids: string[]
  prerequisite_bottlenecks: WorkBottleneck[]
  leverage_actions: MilestoneLeverageAction[]
}

export interface WorkBrief {
  revision: number
  evaluated_at_ms: number
  ends_at_ms: number
  available_minutes: number
  proposed_minutes: number
  scheduled_count: number
  ready_count: number
  blocked_count: number
  deferred_count: number
  completed_count: number
  items: WorkBriefItem[]
  projects: ProjectWorkSummary[]
  milestones: MilestoneWorkSummary[]
  warnings: string[]
  portfolio_actionable_total: number
  cross_project_leverage_count: number
}

/* ------------------------------------------------------------------ */
/* Schedule plan (read-only window allocation)                         */
/* ------------------------------------------------------------------ */

export interface FreeWindow {
  starts_at_ms: number
  ends_at_ms: number
  minutes: number
}

export interface PlannedSlot {
  task_id: string
  title: string
  project_id: string | null
  starts_at_ms: number
  ends_at_ms: number
  minutes: number
  ranking: number
  explanation: string[]
}

export interface UnplacedTask {
  task_id: string
  reason: string
}

export interface SchedulePlan {
  task_revision: number
  schedule_revision: number
  starts_at_ms: number
  ends_at_ms: number
  available_minutes: number
  allocated_minutes: number
  occupied_minutes: number
  free_windows: FreeWindow[]
  slots: PlannedSlot[]
  unplaced: UnplacedTask[]
  warnings: string[]
}

/* ------------------------------------------------------------------ */
/* Portfolio capacity (read-only proposal, never a reservation)        */
/* ------------------------------------------------------------------ */

export interface PortfolioCapacityItem {
  task_id: string
  title: string
  project_id: string | null
  minutes: number
  portfolio_rank: number
  immediate_unlocks: number
  cross_project_downstream: number
  rationale: string[]
}

export interface PortfolioCapacityExclusion {
  task_id: string
  reason: string
}

export interface PortfolioCapacityPlan {
  revision: number
  evaluated_at_ms: number
  project_filter: string | null
  capacity_minutes: number
  assigned_minutes: number
  unassigned_minutes: number
  actionable_total: number
  blocked_total: number
  chosen: PortfolioCapacityItem[]
  excluded: PortfolioCapacityExclusion[]
  warnings: string[]
}

/* ------------------------------------------------------------------ */
/* Explicit plan approval gate                                         */
/* ------------------------------------------------------------------ */

/** The exact, revision-bound schedule command submitted for review. */
export interface ScheduleApprovalItem {
  task_id: string
  starts_at_ms: number
  ends_at_ms: number
  schedule_block_id: string
  activity_id: string
}

/** `deny_unknown_fields`; all identities are client-generated and stable. */
export interface ScheduleApprovalRequest {
  command_id: string
  expected_revision: number
  window_start_ms: number
  window_end_ms: number
  items: ScheduleApprovalItem[]
}

export interface ReviewedScheduleSlot {
  task_id: string
  schedule_block_id: string
  starts_at_ms: number
  ends_at_ms: number
  duration_minutes: number
}

/**
 * The server's review of an exact command. It carries the `fingerprint` and
 * `command_id` that must be echoed back unchanged at apply time.
 */
export interface PlanReview {
  expected_revision: number
  actor: string
  command_id: string
  fingerprint: string
  scheduled_block_ids: string[]
  scheduled_tasks: string[]
  scheduled_slots: ReviewedScheduleSlot[]
  total_reserved_minutes: number
  requires_explicit_approval: boolean
  creates_calendar_blocks: boolean
}

export interface PlanApproval {
  review: PlanReview
  approved: boolean
}

export interface ApprovedScheduleRequest {
  plan: ScheduleApprovalRequest
  approval: PlanApproval
}

export interface ApprovedScheduleReceipt {
  accepted_revision: number
  /** True when the original command id had already been applied. */
  replayed: boolean
  schedule_block_ids: string[]
}
