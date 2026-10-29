/**
 * Wire contracts for the Orbit authoritative HTTP API.
 *
 * Every type in this file mirrors a Rust type that orbit-server actually
 * serialises (see `.slim/deepwork/m31-web-app/api-contract.md`). Field names,
 * optionality and tag casing are copied from the structs, not normalised here:
 * mapping to camelCase UI names happens in `adapters.ts`, one function per
 * resource.
 *
 * Three serde traps are load-bearing and must not be "cleaned up":
 *
 * 1. `TaskRecord.project_id` / `.milestone_id` are *absent keys* when null
 *    (`skip_serializing_if = "Option::is_none"`), while `TaskDetailResponse`
 *    always emits them as `string | null`. Hence `project_id?: string` on the
 *    record and `project_id: string | null` on the detail. Both mean the same
 *    thing to a person; the adapter normalises them to `null`.
 *
 * 2. `MarkdownCommand` is the only tagged union that is *not* snake_case: the
 *    enum has `#[serde(tag = "kind")]` with no `rename_all`, so the wire tags
 *    are PascalCase (`{"kind":"Create"}`). Every other union here is tagged
 *    snake_case (`{"type":"change_status"}`, `{"operation":"set_text"}`,
 *    `{"kind":"folder"}`).
 *
 * 3. `GET /authoritative/blueprints/items` is polymorphic: `?id=` returns one
 *    object (404 when absent) and `?project_id=` returns an array. The shape
 *    depends on the query parameter, so this module exposes two distinct
 *    functions (`fetchWorkItem` / `fetchWorkItems`) and never branches on
 *    `Array.isArray` in a single helper.
 */

import type { TaskDependencyEdge } from './contracts/planning'

/* ------------------------------------------------------------------ */
/* Scalar unions (validated by the authority, never by this layer)      */
/* ------------------------------------------------------------------ */

export type TaskStatus =
  | 'inbox'
  | 'planned'
  | 'ready'
  | 'in_progress'
  | 'waiting'
  | 'blocked'
  | 'review'
  | 'deferred'
  | 'done'
  | 'cancelled'

export type TaskPriority = 'low' | 'medium' | 'high' | 'urgent'

export type ProjectStatus = 'proposed' | 'active' | 'on_hold' | 'completed' | 'cancelled' | 'archived'

export type MilestoneStatus = 'planned' | 'in_progress' | 'blocked' | 'completed' | 'cancelled'

export type ScheduleFlexibility = 'fixed' | 'movable'
export type ScheduleTargetKind = 'standalone' | 'task' | 'habit'

export type StrategyMode =
  | 'standard'
  | 'learning'
  | 'habit_driven'
  | 'metric_target'
  | 'maintenance'
  | 'custom'

export type CriterionStatus = 'pending' | 'satisfied' | 'waived'

export type ActivityKind =
  | 'created'
  | 'updated'
  | 'status_changed'
  | 'progress_updated'
  | 'blocker_opened'
  | 'blocker_resolved'
  | 'scheduled'
  | 'rescheduled'
  | 'completed'
  | 'attachment_added'
  | 'attachment_removed'
  /** The server renders custom kinds as the literal string `custom:<name>`. */
  | (string & {})

export type AttachmentOwnerKind = 'task' | 'project' | 'work_item' | 'knowledge' | 'agent_job'

export type AgentJobState =
  | 'queued'
  | 'preparing'
  | 'launching'
  | 'running'
  | 'waiting_input'
  | 'blocked'
  | 'completed'
  | 'failed'
  | 'interrupted'
  | 'unreachable'

export type AgentNextAction =
  | 'none'
  | 'respond_to_agent'
  | 'inspect_runtime'
  | 'review_handoff'
  | 'resume_or_rework'
  | 'inspect_missing_handoff'
  | 'check_task_acceptance'
  | 'inspect_and_resume'

/** Runtime guard for values that reach a `<Select>`; the server rejects others. */
export const TASK_STATUSES: readonly TaskStatus[] = [
  'inbox',
  'planned',
  'ready',
  'in_progress',
  'waiting',
  'blocked',
  'review',
  'deferred',
  'done',
  'cancelled',
]

export const TASK_PRIORITIES: readonly TaskPriority[] = ['low', 'medium', 'high', 'urgent']

export const PROJECT_STATUSES: readonly ProjectStatus[] = [
  'proposed',
  'active',
  'on_hold',
  'completed',
  'cancelled',
  'archived',
]

export const MILESTONE_STATUSES: readonly MilestoneStatus[] = [
  'planned',
  'in_progress',
  'blocked',
  'completed',
  'cancelled',
]

export function isTaskStatus(value: string): value is TaskStatus {
  return (TASK_STATUSES as readonly string[]).includes(value)
}

export function isTaskPriority(value: string): value is TaskPriority {
  return (TASK_PRIORITIES as readonly string[]).includes(value)
}

export function isMilestoneStatus(value: string): value is MilestoneStatus {
  return (MILESTONE_STATUSES as readonly string[]).includes(value)
}

export function isAttachmentOwnerKind(value: string): value is AttachmentOwnerKind {
  return ['task', 'project', 'work_item', 'knowledge', 'agent_job'].includes(value)
}

/* ------------------------------------------------------------------ */
/* Error envelope                                                      */
/* ------------------------------------------------------------------ */

/** `{"error":"<static message>"}` from `ErrorResponse` (main.rs:71). */
export interface ErrorEnvelope {
  error: string
}

/* ------------------------------------------------------------------ */
/* Projects                                                            */
/* ------------------------------------------------------------------ */

export interface ProjectRecord {
  id: string
  name: string
  status: string
}

export interface AuthorityProjectsResponse {
  revision: number
  projects: ProjectRecord[]
}

/**
 * `deny_unknown_fields`; `action` defaults to `create` when absent.
 * `workflow_id` is only read on the first accepted create, and an empty `name`
 * on `update` means "keep the stored name".
 */
export interface ProjectCommand {
  command_id: string
  expected_revision: number
  action?: 'create' | 'update'
  project_id: string
  name: string
  status?: string
  workflow_id?: string
}

export interface ProjectResult {
  revision: number
  duplicate: boolean
  project: ProjectRecord
}

/** One status and the targets the domain accepts from it (`GET /project-statuses`). */
export interface ProjectStatusRecord {
  status: string
  next: string[]
}

export interface ProjectStatusesResponse {
  statuses: ProjectStatusRecord[]
}

/* ------------------------------------------------------------------ */
/* Search                                                              */
/* ------------------------------------------------------------------ */

export type SearchResultKind = 'project' | 'task' | 'note'

/** One match. `subtitle` is composed by the server; the client never rebuilds it. */
export interface SearchResultRecord {
  kind: SearchResultKind
  id: string
  title: string
  subtitle: string | null
  project_id: string | null
  path: string | null
}

export interface SearchGroupRecord {
  kind: SearchResultKind
  truncated: boolean
  results: SearchResultRecord[]
}

/** Groups are always present in the order project, task, note. */
export interface SearchResponse {
  revision: number
  query: string
  limit: number
  groups: SearchGroupRecord[]
}

/* ------------------------------------------------------------------ */
/* Saved views                                                         */
/* ------------------------------------------------------------------ */

export type SavedViewScope = 'project' | 'inbox' | 'home'
export type SavedViewDisplay = 'list' | 'board'

/** A personal presentation preference, never a workflow view. */
export interface SavedViewRecord {
  id: string
  name: string
  scope: string
  project_id: string | null
  query: string
  display: string
  version: number
  created_at_ms: number
  updated_at_ms: number
}

export interface SavedViewListResponse {
  views: SavedViewRecord[]
}

/**
 * Tagged `type`, snake_case. `expected_version` 0 means create; rename and
 * delete must carry the live row version.
 */
export type SavedViewCommand =
  | {
      type: 'save'
      command_id: string
      view_id: string
      expected_version: number
      name: string
      scope: SavedViewScope
      project_id: string | null
      query: string
      display: SavedViewDisplay
    }
  | {
      type: 'rename'
      command_id: string
      view_id: string
      expected_version: number
      name: string
    }
  | {
      type: 'delete'
      command_id: string
      view_id: string
      expected_version: number
    }

export interface SavedViewResult {
  duplicate: boolean
  /** `null` for delete, because the server removes the record. */
  view: SavedViewRecord | null
}

/* ------------------------------------------------------------------ */
/* Tasks                                                               */
/* ------------------------------------------------------------------ */

/** Trap 1a: `project_id` / `milestone_id` are absent keys when null. */
export interface TaskRecord {
  id: string
  title: string
  status: string
  project_id?: string
  milestone_id?: string
  estimated_minutes: number | null
  earliest_start_at_ms: number | null
  deadline_at_ms: number | null
  description: string
  priority: string | null
  labels: string[]
}

/** Trap 1b: the same values, but both ids are always present. */
export interface TaskDetailResponse {
  id: string
  title: string
  status: string
  project_id: string | null
  milestone_id: string | null
  estimated_minutes: number | null
  earliest_start_at_ms: number | null
  deadline_at_ms: number | null
  description: string
  priority: string | null
  labels: string[]
  /** Derived from recorded activity; `null` when no history row exists. */
  created_at_ms: number | null
  updated_at_ms: number | null
}

export interface AcceptanceCriterionResponse {
  id: string
  text: string
  completed: boolean
  completed_at_ms: number | null
  position: number
}

export interface ActivityResponse {
  id: string
  kind: string
  /** `human` | `system` | `agent:<id>` | `external:<device>` */
  origin: string
  occurred_at_ms: number
  summary: string
}

export interface DomainTasksResponse {
  revision: number
  tasks: TaskRecord[]
}

export interface TaskResult {
  revision: number
  duplicate: boolean
  task: TaskRecord
}

/** Tagged `"type"`, snake_case. */
export type TaskAction =
  | { type: 'create'; task_id: string; title: string; project_id?: string }
  | { type: 'change_status'; task_id: string; status: string }
  | { type: 'rename'; task_id: string; title: string }
  | { type: 'assign_project'; task_id: string; project_id: string | null }
  | { type: 'assign_milestone'; task_id: string; milestone_id: string | null }
  | {
      type: 'set_planning'
      task_id: string
      estimated_minutes: number | null
      earliest_start_at_ms: number | null
      deadline_at_ms: number | null
    }
  | { type: 'add_prerequisite'; task_id: string; prerequisite_id: string }
  | { type: 'remove_prerequisite'; task_id: string; prerequisite_id: string }
  | {
      type: 'set_details'
      task_id: string
      description: string
      priority: string | null
      labels: string[]
    }
  | { type: 'acceptance_criteria'; task_id: string; change: AcceptanceCriteriaAction }

/** Tagged `"operation"`, snake_case. Positions are 0-based integers. */
export type AcceptanceCriteriaAction =
  | { operation: 'add'; criterion_id: string; text: string }
  | { operation: 'set_text'; criterion_id: string; text: string }
  | { operation: 'set_completed'; criterion_id: string; completed: boolean }
  | { operation: 'reorder'; criterion_id: string; position: number }
  | { operation: 'remove'; criterion_id: string }

export interface TaskCommand {
  command_id: string
  expected_revision: number
  action: TaskAction
}

/* ------------------------------------------------------------------ */
/* Milestones                                                          */
/* ------------------------------------------------------------------ */

export interface MilestoneRecord {
  id: string
  project_id: string
  name: string
  status: string
  /** Checkpoint date; absent means unscheduled, not a zero-length period. */
  target_at_ms?: number | null
}

export interface MilestoneSnapshot {
  revision: number
  milestones: MilestoneRecord[]
}

export interface MilestoneResult {
  revision: number
  duplicate: boolean
  /** `null` for `delete`. */
  milestone: MilestoneRecord | null
}

export type MilestoneAction =
  | { type: 'create'; milestone_id: string; project_id: string; name: string }
  | { type: 'rename'; milestone_id: string; name: string }
  | { type: 'change_status'; milestone_id: string; status: string }
  | { type: 'set_target'; milestone_id: string; target_at_ms: number | null }
  | { type: 'move_project'; milestone_id: string; project_id: string }
  | { type: 'delete'; milestone_id: string }

export interface MilestoneCommand {
  command_id: string
  expected_revision: number
  action: MilestoneAction
}

/* ------------------------------------------------------------------ */
/* Epics                                                               */
/* ------------------------------------------------------------------ */

export type EpicStatus = 'planned' | 'in_progress' | 'blocked' | 'completed' | 'cancelled'

export interface EpicProgressRecord {
  total: number
  completed: number
  blocked: number
  percent: number
}

export interface EpicActivityRecord {
  id: string
  epic_id: string
  kind: string
  actor: string
  occurred_at_ms: number
  summary: string
}

export interface EpicRecord {
  id: string
  project_id: string
  title: string
  description: string
  status: EpicStatus
  priority: string | null
  owner: string | null
  start_at_ms: number | null
  target_at_ms: number | null
  created_at_ms: number
  updated_at_ms: number
  task_ids: string[]
  work_item_ids: string[]
  progress: EpicProgressRecord
  activities: EpicActivityRecord[]
}

export interface EpicSnapshot {
  revision: number
  epics: EpicRecord[]
}

export interface EpicResult {
  revision: number
  duplicate: boolean
  epic: EpicRecord | null
}

export type EpicAction =
  | {
      type: 'create'
      epic_id: string
      project_id: string
      title: string
      description?: string
      priority?: string | null
      owner?: string | null
      start_at_ms?: number | null
      target_at_ms?: number | null
    }
  | {
      type: 'update'
      epic_id: string
      title: string
      description?: string
      priority?: string | null
      owner?: string | null
      start_at_ms?: number | null
      target_at_ms?: number | null
    }
  | { type: 'change_status'; epic_id: string; status: EpicStatus }
  | { type: 'assign_task'; epic_id: string; task_id: string }
  | { type: 'unassign_task'; task_id: string }
  | { type: 'assign_work_item'; epic_id: string; item_id: string }
  | { type: 'unassign_work_item'; item_id: string }
  | { type: 'delete'; epic_id: string }

export interface EpicCommand {
  command_id: string
  expected_revision: number
  action: EpicAction
}

/* ------------------------------------------------------------------ */
/* Schedule                                                            */
/* ------------------------------------------------------------------ */

export interface ScheduleRecord {
  id: string
  title: string
  starts_at_ms: number
  ends_at_ms: number
  duration_minutes: number
  flexibility: string
  target_kind: string
  target_id: string | null
}

export interface ScheduleSnapshot {
  revision: number
  blocks: ScheduleRecord[]
}

/* ------------------------------------------------------------------ */
/* Strategies, routines, evidence                                      */
/* ------------------------------------------------------------------ */

export interface CriterionRecord {
  key: string
  description: string
  required: boolean
  status: string
}

export interface StrategyRecord {
  project_id: string
  mode: string
  objective: string | null
  progress_signals: string[]
  criteria: CriterionRecord[]
}

export interface StrategySnapshot {
  revision: number
  strategies: StrategyRecord[]
}

export interface RoutineRecord {
  id: string
  name: string
  habit_ids: string[]
}

export interface RoutineSnapshot {
  revision: number
  routines: RoutineRecord[]
}

export interface HabitRecord {
  id: string
  name: string
  /** `daily` | `weekly:<n>` */
  cadence: string
}

export interface HabitOccurrenceRecord {
  id: string
  habit_id: string
  occurred_at_ms: number
}

export interface MetricRecord {
  id: string
  baseline: number
  target: number
  direction: string
}

export interface MetricObservationRecord {
  metric_id: string
  observed_at_ms: number
  value: number
}

export interface EvidenceSnapshot {
  revision: number
  history_fingerprint: string
  habits: HabitRecord[]
  habit_occurrences: HabitOccurrenceRecord[]
  metrics: MetricRecord[]
  metric_observations: MetricObservationRecord[]
}

/* ------------------------------------------------------------------ */
/* Snapshot (narrow slice — no workflows, items, notes or jobs)        */
/* ------------------------------------------------------------------ */

export interface AuthoritySnapshotResponse {
  revision: number
  history_fingerprint: string
  projects: ProjectRecord[]
  tasks: TaskRecord[]
  task_dependencies: TaskDependencyEdge[]
  habits: HabitRecord[]
  habit_occurrences: HabitOccurrenceRecord[]
  metrics: MetricRecord[]
  metric_observations: MetricObservationRecord[]
  schedule: ScheduleRecord[]
  routines: RoutineRecord[]
  milestones: MilestoneRecord[]
  strategies: StrategyRecord[]
}

export interface HistoryPrefixResponse {
  revision: number
  history_fingerprint: string
}

/* ------------------------------------------------------------------ */
/* Workflows (wire name: blueprints)                                   */
/* ------------------------------------------------------------------ */

export type FieldKind =
  | { type: 'text' }
  | { type: 'boolean' }
  | { type: 'integer' }
  | { type: 'choice'; options: string[] }

export interface Field {
  id: string
  kind: FieldKind
  required: boolean
  default: unknown
}

/** Tagged `"type"`, snake_case. */
export type TransitionGuard =
  | { type: 'property_equals'; field: string; value: unknown }
  | { type: 'property_present'; field: string }
  | { type: 'all_prerequisites_in_state'; state: string }

export interface WorkflowActionDefinition {
  id: string
  capability: string
  description: string
  parameters: Record<string, unknown>
}

export interface Transition {
  from: string
  to: string
  required_capability: string | null
  guards: TransitionGuard[]
  actions: WorkflowActionDefinition[]
}

/** The manifest's per-kind schema. UI name: `WorkItemKind`. */
export interface BlueprintKindDefinition {
  id: string
  name: string
  initial_state: string
  states: string[]
  fields: Field[]
  transitions: Transition[]
}

export interface ViewLayout {
  type: 'vertical' | 'horizontal' | 'grid'
  columns: number
}

/** `deny_unknown_fields`; every optional key is always serialised. */
export interface ViewComponent {
  id: string
  type: string
  title: string | null
  source: string | null
  aggregate: string | null
  field: string | null
  group_by: string | null
  filter: Record<string, string>
  sort: string | null
  descending: boolean
  limit: number | null
  columns: string[]
  text: string | null
  actions: string[]
  options: Record<string, unknown>
}

export interface BlueprintView {
  id: string
  title: string
  description: string
  layout: ViewLayout
  components: ViewComponent[]
}

export interface TemplateItem {
  key: string
  kind: string
  properties: Record<string, unknown>
  prerequisites: string[]
}

export interface ProjectTemplate {
  id: string
  name: string
  items: TemplateItem[]
}

/** The published manifest. UI name: `Workflow`. */
export interface BlueprintManifest {
  id: string
  version: number
  name: string
  description: string
  work_items: BlueprintKindDefinition[]
  required_capabilities: string[]
  templates: ProjectTemplate[]
  views: BlueprintView[]
}

export interface BlueprintBindingRecord {
  project_id: string
  blueprint_id: string
  version: number
}

/**
 * One trusted workflow this server can install. `version` is what an install
 * publishes; the UI compares it with the newest published version to offer an
 * upgrade instead of hiding an id that already exists at an older revision.
 */
export interface BundledWorkflowDescriptor {
  id: string
  version: number
  name: string
}

/** The domain's accepted status edges from one canonical status. */
export interface TaskTransitionsRecord {
  status: TaskStatus
  next: TaskStatus[]
}

export interface BlueprintWorkItemRecord {
  id: string
  project_id: string
  kind: string
  state: string
  properties: Record<string, unknown>
  /** Work items use `u64`, unlike the `i64` domain revisions. */
  revision: number
}

export interface TemplatePlan {
  items: BlueprintWorkItemRecord[]
  /** `[item_id, prerequisite_id]` pairs, in deterministic order. */
  prerequisites: [string, string][]
}

export interface CreateWorkItemBody {
  id: string
  project_id: string
  kind: string
  properties?: Record<string, unknown>
}

export interface WorkItemTransitionBody {
  id: string
  to: string
  expected_revision: number
}

export interface WorkItemPropertiesBody {
  id: string
  expected_revision: number
  changes: Record<string, unknown>
}

export interface BindWorkflowBody {
  project_id: string
  id: string
  version: number
}

/* ---- Upgrade preview (read-only comparison) ---- */

export interface UpgradeNotice {
  code: string
  message: string
  subjects: string[]
}

export interface WorkflowVersionRef {
  blueprint_id: string
  version: number
}

export interface FieldTypeView {
  kind: string
  options: string[]
}

export interface FieldDefinition {
  id: string
  field_type: FieldTypeView
  required: boolean
  default: unknown
}

export interface FieldChange {
  id: string
  before: FieldDefinition
  after: FieldDefinition
}

export interface TransitionChange {
  transition: string
  required_capability_before: string | null
  required_capability_after: string | null
  guards_changed: boolean
}

export interface ActionRef {
  transition: string
  action: string
  capability: string
}

export interface KindUpgradeDiff {
  kind: string
  added: boolean
  removed: boolean
  initial_state_before: string | null
  initial_state_after: string | null
  states_added: string[]
  states_removed: string[]
  fields_added: FieldDefinition[]
  fields_removed: FieldDefinition[]
  fields_changed: FieldChange[]
  transitions_added: string[]
  transitions_removed: string[]
  transitions_changed: TransitionChange[]
  actions_added: ActionRef[]
  actions_removed: ActionRef[]
  required_capabilities_added: string[]
  required_capabilities_removed: string[]
}

export interface ViewChange {
  view: string
  layout_before: string
  layout_after: string
  components_added: string[]
  components_removed: string[]
  components_changed: string[]
}

export interface ViewUpgradeDiff {
  added: string[]
  removed: string[]
  changed: ViewChange[]
}

export interface BlueprintUpgradePreview {
  schema: string
  project_id: string
  project_revision: number
  current: WorkflowVersionRef | null
  target: WorkflowVersionRef
  kinds: KindUpgradeDiff[]
  views: ViewUpgradeDiff
  warnings: UpgradeNotice[]
  blockers: UpgradeNotice[]
  fingerprint: string
}

/** The preview is `#[serde(flatten)]`ed into this response. */
export interface BlueprintUpgradeApplied extends BlueprintUpgradePreview {
  binding: BlueprintBindingRecord
  previous: WorkflowVersionRef | null
  already_applied: boolean
}

export interface ApplyUpgradeBody {
  project_id: string
  id: string
  version: number
  confirm: true
  /**
   * The fingerprint of the preview the person was actually shown. Required by
   * the server (400 without it, 409 on mismatch); the client never derives it
   * from the target version, because that would consent to an unseen preview.
   */
  fingerprint: string
}

/* ------------------------------------------------------------------ */
/* Capability actions (connected execution)                            */
/* ------------------------------------------------------------------ */

/**
 * The durable facts a connected request is decided from. Nothing here is
 * inferred from a provider being loaded in the server process: `installed` and
 * `granted` are stored rows and `executors` is the enrollment announcement.
 */
export interface CapabilityExecutorRecord {
  device_id: string
  announced_at_ms: number
}

/** `read` | `mutate` | `external_effect`; the durable install vocabulary. */
export type CapabilityPermission = 'read' | 'mutate' | 'external_effect'

/** One action of a contract, as the status document exposes it. */
export interface CapabilityActionContractRecord {
  id: string
  description: string
  /** JSON-schema subset: `{type:"object",properties:{…},required:[…]}`. */
  input_schema: Record<string, unknown>
  external_effect: boolean
  requires: string[]
}

export interface CapabilityStatusRecord {
  capability_id: string
  name: string
  permission: string
  requires: string[]
  installed: boolean
  granted: boolean
  executors: CapabilityExecutorRecord[]
  actions: CapabilityActionContractRecord[]
}

/** `GET /authoritative/capabilities?project_id=`. */
export interface CapabilityStatusResponse {
  capabilities: CapabilityStatusRecord[]
}

/**
 * The single availability vocabulary and its order. A client that receives
 * `capability_not_installed` must never be told to authorize first, and
 * `executor_missing` means "no executor announced it", never "not authorized".
 */
export type CapabilityAvailabilityCode =
  | 'capability_missing'
  | 'capability_disabled'
  | 'capability_not_installed'
  | 'executor_missing'
  | 'requirement_missing'
  | 'not_authorized'

/** One action offered for the item's current state. Code/reason are null when available. */
export interface CapabilityOfferRecord {
  action_id: string
  capability_id: string
  from: string
  to: string
  description: string
  input_schema: Record<string, unknown>
  external_effect: boolean
  available: boolean
  /** A stable code from `CapabilityAvailabilityCode`, or a newer server's code. */
  code: string | null
  reason: string | null
}

export interface CapabilityOffersResponse {
  item_id: string
  item_revision: number
  offers: CapabilityOfferRecord[]
}

/** `pending` → `claimed` → `succeeded` | `failed` | `uncertain`. Never reset. */
export type CapabilityRequestState =
  | 'pending'
  | 'claimed'
  | 'succeeded'
  | 'failed'
  | 'uncertain'

/**
 * One persisted intent. `claim_token` is deliberately absent: it is an executor's
 * evidence, and a browser that held it could impersonate a claim.
 */
export interface CapabilityActionRequestRecord {
  command_id: string
  item_id: string
  item_revision: number
  action_id: string
  capability_id: string
  from: string
  to: string
  state: string
  inputs: Record<string, unknown>
  created_at_ms: number
  executor_device_id: string | null
}

/** As reported by an executor, or by an operator's reconcile. `detail` is verbatim. */
export interface CapabilityReceiptRecord {
  outcome: string
  detail: unknown
  reported_at_ms: number
  reported_by: string
}

export interface CapabilityRequestResponse {
  request: CapabilityActionRequestRecord
}

export interface CapabilityShowResponse {
  request: CapabilityActionRequestRecord
  receipt: CapabilityReceiptRecord | null
}

export interface CapabilityListResponse {
  requests: CapabilityActionRequestRecord[]
}

export interface CapabilityRequestBody {
  command_id: string
  item_id: string
  expected_revision: number
  action_id: string
  inputs: Record<string, unknown>
  confirm: boolean
}

export type CapabilityReconcileDecision = 'confirm_succeeded' | 'confirm_failed'

export interface CapabilityReconcileBody {
  command_id: string
  decision: CapabilityReconcileDecision
  /** Capped by the server at 1000 characters. */
  detail: string
}

/* ------------------------------------------------------------------ */
/* Knowledge (file-authoritative Markdown)                             */
/* ------------------------------------------------------------------ */

export type KnowledgeNode =
  | { kind: 'folder'; name: string; path: string; children: KnowledgeNode[] }
  | { kind: 'note'; name: string; path: string }

export interface KnowledgeDocumentInfo {
  id: string
  path: string
  title: string
}

export interface KnowledgeFileRelations {
  outgoing: string[]
  backlinks: string[]
  unresolved: string[]
}

export interface MarkdownManifestEntry {
  note: KnowledgeDocumentInfo
  body_sha256: string
}

export interface MarkdownDocumentResponse {
  path: string
  body: string
}

export interface MarkdownByIdResponse {
  note: KnowledgeDocumentInfo
  body: string
}

/**
 * Trap 2: `#[serde(tag = "kind")]` with no `rename_all`, so these tags are
 * PascalCase. Sending `{"kind":"create"}` is a 422/400 extractor rejection.
 */
export type MarkdownCommand =
  | { kind: 'Create'; note_id: string; path: string; body: string }
  | { kind: 'Update'; note_id: string; expected: string; replacement: string }
  | { kind: 'Move'; note_id: string; from: string; to: string; expected: string }

export interface MarkdownCommandRequest {
  command_id: string
  command: MarkdownCommand
}

export interface MarkdownReceipt {
  command_id: string
  note_id: string
  revision: number
}

/**
 * The typed-reference grammar the runtime owns: a Markdown link whose text and
 * target are both `kind:id` (`[project:orbit](orbit://project:orbit)`). The
 * browser parses it only to describe what the note says; it never writes the
 * token itself — the reference command does.
 */
export type TypedReferenceKind = 'project' | 'task'

export interface TypedReference {
  kind: TypedReferenceKind
  id: string
}

export type ReferenceAction = 'add' | 'remove'

export interface MarkdownReferenceCommandRequest {
  command_id: string
  note_id: string
  /** `<kind>:<id>`, e.g. `project:orbit`. */
  target: string
  action: ReferenceAction
}

export interface MarkdownReferenceReceipt {
  command_id: string
  note_id: string
  revision: number
  /** `false` when the token was already present (add) or absent (remove). */
  changed: boolean
  target: string
  action: ReferenceAction
}

/** `GET /authoritative/markdown-references?target=project:orbit`, ordered by path. */
export interface MarkdownReferencesResponse {
  target: string
  notes: KnowledgeDocumentInfo[]
}

/** One note in a project's knowledge read, with the references that included it. */
export interface ProjectKnowledgeNote extends KnowledgeDocumentInfo {
  references: TypedReference[]
}

/** `GET /authoritative/project-knowledge?project_id=orbit`. */
export interface ProjectKnowledgeResponse {
  project_id: string
  notes: ProjectKnowledgeNote[]
  /** The project's task ids, so a reference can be named without another read. */
  tasks: string[]
}

export interface CreateFolderRequest {
  command_id: string
  command: { path: string }
}

export interface CreateFolderResponse {
  command_id: string
  path: string
}

export interface MoveFolderRequest {
  command_id: string
  command: { from: string; to: string }
}

export interface MoveFolderResponse {
  command_id: string
  from: string
  to: string
}

/* ------------------------------------------------------------------ */
/* Attachments                                                         */
/* ------------------------------------------------------------------ */

export interface AttachmentView {
  id: string
  owner_kind: string
  owner_id: string
  filename: string
  content_type: string
  size_bytes: number
  checksum: string
  created_at_ms: number
  description: string | null
  is_image: boolean
}

export interface AttachmentListResponse {
  attachments: AttachmentView[]
}

export interface AttachmentImportResult {
  revision: number
  duplicate: boolean
  removed: boolean
  attachment: AttachmentView | null
}

export interface AttachmentRemoveResult {
  revision: number
  removed: boolean
  duplicate: boolean
  content_removed: boolean
  already_removed: boolean
}

export interface AttachmentAssociateResult {
  revision: number
  duplicate: boolean
  attachment_id: string
}

export interface AttachmentRemoveRequest {
  command_id: string
  expected_revision: number
  attachment_id: string
  activity_id: string
}

export interface AttachmentAssociateRequest {
  command_id: string
  expected_revision: number
  source_id: string
  new_id: string
  owner_kind: string
  owner_id: string
  activity_id: string
}

/** Metadata travels in the query string; the file itself is the raw body. */
export interface AttachmentImportMetadata {
  command_id: string
  expected_revision: number
  attachment_id: string
  activity_id: string
  owner_kind: string
  owner_id: string
  filename: string
  content_type?: string
  description?: string
}

/* ------------------------------------------------------------------ */
/* Agent jobs                                                          */
/* ------------------------------------------------------------------ */

/**
 * `claim_state` (C13) is the machine claim, not the job state: `null` means no
 * executor has claimed the job. Optional here because a server older than C13
 * omits the key entirely, and "not reported" must not be read as "no claim".
 */
export type AgentClaimState = 'claimed' | 'confirmed' | 'ambiguous'

/** Every field is always present; `runtime_session_ref` may be null. */
export interface AgentJobView {
  id: string
  request_id: string
  project_id: string
  task_id: string
  state: string
  revision: number
  agent_kind: string
  machine_ref: string
  runtime_kind: string
  repository_ref: string
  runtime_session_ref: string | null
  created_at_ms: number
  updated_at_ms: number
  claim_state?: AgentClaimState | null
}

/**
 * One enrolled device that can claim a queued job (C10). The list is not
 * paginated: this is a single-user deployment and the device count is small, so
 * the client asks once and never caps the answer.
 */
export interface AgentExecutor {
  device_id: string
  active: boolean
  /** Most recent claim by that device, or null when it has never claimed. */
  last_claim_at_ms: number | null
  /** Non-terminal jobs assigned to it that it has claimed. */
  claimed_jobs: number
  /** `preparing` jobs assigned to it that no one has claimed yet. */
  preparing_jobs: number
}

/** An empty `executors` list is a valid answer: no executor is configured. */
export interface AgentExecutorListResponse {
  executors: AgentExecutor[]
}

export interface AgentJobOverviewItem {
  job_id: string
  project_id: string
  task_id: string
  state: string
  updated_at_ms: number
  handoff_count: number
  review_decision: string | null
  needs_attention: boolean
  next_action: string
}

export interface AgentHandoffEntry {
  handoff_id: string
  reported_at_ms: number
  payload: unknown
}

export interface AgentReview {
  job_id: string
  handoff_id: string
  decision: string
  reviewer: string
  reason: string
  reviewed_at_ms: number
  /** Approving a review never completes the task. */
  task_completed: boolean
}

export interface AgentResumeContext {
  job: AgentJobView
  original_brief: unknown
  latest_handoff: unknown
  handoff_count: number
  review: AgentReview | null
  worktree: unknown
  task_completion_inferred: boolean
  runtime_launch_authorized: boolean
}

export interface StartAgentJobRequest {
  job_id: string
  task_id: string
  agent_kind: string
  machine_ref: string
  repository_ref: string
  worktree: boolean
  at_ms: number
  /**
   * C14: optional. Trimmed by the server, empty treated as absent, over 4000
   * characters refused. Omitted here when the form left it blank.
   */
  objective?: string
}

export interface ReserveAgentJobRequest {
  job_id: string
  request_id: string
  project_id: string
  task_id: string
  agent_kind: string
  machine_ref: string
  runtime_kind: string
  repository_ref: string
  launch_revision: number
  created_at_ms: number
}

export interface AgentJobCommandRequest {
  command_id: string
  job_id: string
  expected_revision: number
  action: 'prepare' | 'interrupt' | 'mark_failed'
  at_ms: number
}

export interface SubmitAgentHandoffRequest {
  job_id: string
  handoff_id: string
  payload: unknown
  at_ms: number
}

export interface ReviewAgentHandoffRequest {
  job_id: string
  handoff_id: string
  decision: 'approved' | 'changes_requested'
  reason: string
  at_ms: number
}

export interface ResumeAgentJobRequest {
  prior_job_id: string
  new_job_id: string
  at_ms: number
}

export interface QueueSessionPromptRequest {
  command_id: string
  job_id: string
  message: string
  at_ms: number
}

/* ------------------------------------------------------------------ */
/* Web session                                                         */
/* ------------------------------------------------------------------ */

/** Returned by `POST /v1/web/session` (201) and `GET` (200). */
export interface WebSessionResponse {
  device_id: string
  expires_at_ms: number
}

export interface WebSessionRequest {
  device_id: string
  token: string
}

/* ------------------------------------------------------------------ */
/* Planning domain (split for size; same import path)                  */
/* ------------------------------------------------------------------ */

export * from './contracts/planning'

