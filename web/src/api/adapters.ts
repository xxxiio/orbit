/**
 * Wire → UI mapping. One function per resource, and the only place internal
 * names are renamed ("workflow", "work item") and snake_case becomes camelCase.
 *
 * Nothing here applies business rules: values come from the server as sent.
 * The only normalisation is structural (absent vs null ids, see trap 1) and
 * boundary assertions for string enums the authority already validates.
 */
import type {
  AcceptanceCriterionResponse,
  ActivityResponse,
  AttachmentView,
  BlueprintBindingRecord,
  BlueprintKindDefinition,
  BlueprintManifest,
  BlueprintUpgradeApplied,
  BlueprintUpgradePreview,
  BlueprintView,
  BlueprintWorkItemRecord,
  FieldKind,
  HabitRecord,
  KindUpgradeDiff,
  MetricRecord,
  MilestoneRecord,
  MilestoneResult,
  NextAction,
  ProjectRecord,
  RoutineRecord,
  ScheduleRecord,
  StrategyRecord,
  TaskAttention,
  TaskDetailResponse,
  TaskDependencyEdge,
  TaskReadiness,
  TaskRecord,
  Transition,
  TransitionGuard,
  UpgradeNotice,
  ViewComponent,
  ViewUpgradeDiff,
  WorkflowActionDefinition,
} from './contracts'
import { isTaskPriority, type PlanningDisposition, type ProjectStatus, type TaskPriority, type TaskStatus } from './contracts'

/* ------------------------------------------------------------------ */
/* Projects, milestones, tasks                                         */
/* ------------------------------------------------------------------ */

export interface Project {
  id: string
  name: string
  status: ProjectStatus
}

/** Boundary assertion: the authority rejects statuses outside this union. */
function asStatus<T extends string>(value: string): T {
  return value as T
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

/* ------------------------------------------------------------------ */
/* Schedule, strategies, routines, evidence                            */
/* ------------------------------------------------------------------ */

export interface ScheduleBlock {
  id: string
  title: string
  startsAtMs: number
  endsAtMs: number
  durationMinutes: number
  flexibility: string
  targetKind: string
  targetId: string | null
}

export function toScheduleBlock(record: ScheduleRecord): ScheduleBlock {
  return {
    id: record.id,
    title: record.title,
    startsAtMs: record.starts_at_ms,
    endsAtMs: record.ends_at_ms,
    durationMinutes: record.duration_minutes,
    flexibility: record.flexibility,
    targetKind: record.target_kind,
    targetId: record.target_id,
  }
}

export interface StrategyCriterion {
  key: string
  description: string
  required: boolean
  status: string
}

export interface Strategy {
  projectId: string
  mode: string
  objective: string | null
  progressSignals: string[]
  criteria: StrategyCriterion[]
}

export function toStrategy(record: StrategyRecord): Strategy {
  return {
    projectId: record.project_id,
    mode: record.mode,
    objective: record.objective,
    progressSignals: record.progress_signals,
    criteria: record.criteria.map((criterion) => ({ ...criterion })),
  }
}

export interface Routine {
  id: string
  name: string
  habitIds: string[]
}

export function toRoutine(record: RoutineRecord): Routine {
  return { id: record.id, name: record.name, habitIds: record.habit_ids }
}

export interface Habit {
  id: string
  name: string
  cadence: string
}

export function toHabit(record: HabitRecord): Habit {
  return { id: record.id, name: record.name, cadence: record.cadence }
}

export interface Metric {
  id: string
  baseline: number
  target: number
  direction: string
}

export function toMetric(record: MetricRecord): Metric {
  return { id: record.id, baseline: record.baseline, target: record.target, direction: record.direction }
}

/* ------------------------------------------------------------------ */
/* Workspace snapshot                                                  */
/* ------------------------------------------------------------------ */

export interface WorkspaceSnapshot {
  revision: number
  historyFingerprint: string
  projects: Project[]
  tasks: Task[]
  taskDependencies: Dependency[]
  milestones: Milestone[]
  strategies: Strategy[]
  schedule: ScheduleBlock[]
  routines: Routine[]
  habits: Habit[]
  metrics: Metric[]
}

export function toWorkspaceSnapshot(snapshot: {
  revision: number
  history_fingerprint: string
  projects: ProjectRecord[]
  tasks: TaskRecord[]
  task_dependencies: TaskDependencyEdge[]
  milestones: MilestoneRecord[]
  strategies: StrategyRecord[]
  schedule: ScheduleRecord[]
  routines: RoutineRecord[]
  habits: HabitRecord[]
  metrics: MetricRecord[]
}): WorkspaceSnapshot {
  return {
    revision: snapshot.revision,
    historyFingerprint: snapshot.history_fingerprint,
    projects: snapshot.projects.map(toProject),
    tasks: snapshot.tasks.map(toTask),
    taskDependencies: snapshot.task_dependencies.map(toDependency),
    milestones: snapshot.milestones.map(toMilestone),
    strategies: snapshot.strategies.map(toStrategy),
    schedule: snapshot.schedule.map(toScheduleBlock),
    routines: snapshot.routines.map(toRoutine),
    habits: snapshot.habits.map(toHabit),
    metrics: snapshot.metrics.map(toMetric),
  }
}

/* ------------------------------------------------------------------ */
/* Planning reads (already server-derived; adapted for camelCase only) */
/* ------------------------------------------------------------------ */

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

/* ------------------------------------------------------------------ */
/* Workflows (wire name: blueprints)                                   */
/* ------------------------------------------------------------------ */

export interface WorkItemField {
  id: string
  kind: FieldKind
  required: boolean
  default: unknown
}

export interface WorkflowAction {
  id: string
  capability: string
  description: string
  parameters: Record<string, unknown>
}

export interface WorkItemTransition {
  from: string
  to: string
  requiredCapability: string | null
  guards: TransitionGuard[]
  actions: WorkflowAction[]
}

export interface WorkItemKind {
  id: string
  name: string
  initialState: string
  states: string[]
  fields: WorkItemField[]
  transitions: WorkItemTransition[]
}

export interface WorkflowViewComponent {
  id: string
  type: string
  title: string | null
  source: string | null
  aggregate: string | null
  field: string | null
  groupBy: string | null
  filter: Record<string, string>
  sort: string | null
  descending: boolean
  limit: number | null
  columns: string[]
  text: string | null
  actions: string[]
  options: Record<string, unknown>
}

export interface WorkflowView {
  id: string
  title: string
  description: string
  layout: { type: 'vertical' | 'horizontal' | 'grid'; columns: number }
  components: WorkflowViewComponent[]
}

export interface Workflow {
  id: string
  version: number
  name: string
  description: string
  kinds: WorkItemKind[]
  requiredCapabilities: string[]
  templates: Array<{ id: string; name: string; items: Array<{ key: string; kind: string; properties: Record<string, unknown>; prerequisites: string[] }> }>
  views: WorkflowView[]
}

export interface WorkflowBinding {
  projectId: string
  workflowId: string
  version: number
}

export interface WorkItem {
  id: string
  projectId: string
  kind: string
  state: string
  properties: Record<string, unknown>
  revision: number
}

export interface WorkflowVersionRef {
  workflowId: string
  version: number
}

function toWorkItemField(field: { id: string; kind: FieldKind; required: boolean; default: unknown }): WorkItemField {
  return { id: field.id, kind: field.kind, required: field.required, default: field.default }
}

function toWorkflowAction(action: WorkflowActionDefinition): WorkflowAction {
  return {
    id: action.id,
    capability: action.capability,
    description: action.description,
    parameters: action.parameters,
  }
}

function toWorkItemTransition(transition: Transition): WorkItemTransition {
  return {
    from: transition.from,
    to: transition.to,
    requiredCapability: transition.required_capability,
    guards: transition.guards,
    actions: transition.actions.map(toWorkflowAction),
  }
}

export function toWorkItemKind(kind: BlueprintKindDefinition): WorkItemKind {
  return {
    id: kind.id,
    name: kind.name,
    initialState: kind.initial_state,
    states: kind.states,
    fields: kind.fields.map(toWorkItemField),
    transitions: kind.transitions.map(toWorkItemTransition),
  }
}

function toWorkflowViewComponent(component: ViewComponent): WorkflowViewComponent {
  return {
    id: component.id,
    type: component.type,
    title: component.title,
    source: component.source,
    aggregate: component.aggregate,
    field: component.field,
    groupBy: component.group_by,
    filter: component.filter,
    sort: component.sort,
    descending: component.descending,
    limit: component.limit,
    columns: component.columns,
    text: component.text,
    actions: component.actions,
    options: component.options,
  }
}

export function toWorkflowView(view: BlueprintView): WorkflowView {
  return {
    id: view.id,
    title: view.title,
    description: view.description,
    layout: view.layout,
    components: view.components.map(toWorkflowViewComponent),
  }
}

export function toWorkflow(manifest: BlueprintManifest): Workflow {
  return {
    id: manifest.id,
    version: manifest.version,
    name: manifest.name,
    description: manifest.description,
    kinds: manifest.work_items.map(toWorkItemKind),
    requiredCapabilities: manifest.required_capabilities,
    templates: manifest.templates.map((template) => ({
      id: template.id,
      name: template.name,
      items: template.items.map((item) => ({
        key: item.key,
        kind: item.kind,
        properties: item.properties,
        prerequisites: item.prerequisites,
      })),
    })),
    views: manifest.views.map(toWorkflowView),
  }
}

export function toWorkflowBinding(binding: BlueprintBindingRecord): WorkflowBinding {
  return {
    projectId: binding.project_id,
    workflowId: binding.blueprint_id,
    version: binding.version,
  }
}

export function toWorkItem(item: BlueprintWorkItemRecord): WorkItem {
  return {
    id: item.id,
    projectId: item.project_id,
    kind: item.kind,
    state: item.state,
    properties: item.properties,
    revision: item.revision,
  }
}

/* ---- Upgrade preview ---- */

export interface WorkflowUpgradeNotice {
  code: string
  message: string
  subjects: string[]
}

export interface WorkflowActionRef {
  transition: string
  action: string
  capability: string
}

export interface WorkflowKindUpgrade {
  kind: string
  added: boolean
  removed: boolean
  initialStateBefore: string | null
  initialStateAfter: string | null
  statesAdded: string[]
  statesRemoved: string[]
  fieldsAdded: WorkItemField[]
  fieldsRemoved: WorkItemField[]
  fieldsChanged: Array<{ id: string; before: WorkItemField; after: WorkItemField }>
  transitionsAdded: string[]
  transitionsRemoved: string[]
  transitionsChanged: Array<{
    transition: string
    requiredCapabilityBefore: string | null
    requiredCapabilityAfter: string | null
    guardsChanged: boolean
  }>
  actionsAdded: WorkflowActionRef[]
  actionsRemoved: WorkflowActionRef[]
  requiredCapabilitiesAdded: string[]
  requiredCapabilitiesRemoved: string[]
}

export interface WorkflowViewUpgrade {
  added: string[]
  removed: string[]
  changed: Array<{
    view: string
    layoutBefore: string
    layoutAfter: string
    componentsAdded: string[]
    componentsRemoved: string[]
    componentsChanged: string[]
  }>
}

export interface WorkflowUpgradePreview {
  schema: string
  projectId: string
  projectRevision: number
  current: WorkflowVersionRef | null
  target: WorkflowVersionRef
  kinds: WorkflowKindUpgrade[]
  views: WorkflowViewUpgrade
  warnings: WorkflowUpgradeNotice[]
  blockers: WorkflowUpgradeNotice[]
  fingerprint: string
}

export interface WorkflowUpgradeApplied extends WorkflowUpgradePreview {
  binding: WorkflowBinding
  previous: WorkflowVersionRef | null
  alreadyApplied: boolean
}

function toNotice(notice: UpgradeNotice): WorkflowUpgradeNotice {
  return { code: notice.code, message: notice.message, subjects: notice.subjects }
}

function toKindUpgrade(diff: KindUpgradeDiff): WorkflowKindUpgrade {
  return {
    kind: diff.kind,
    added: diff.added,
    removed: diff.removed,
    initialStateBefore: diff.initial_state_before,
    initialStateAfter: diff.initial_state_after,
    statesAdded: diff.states_added,
    statesRemoved: diff.states_removed,
    fieldsAdded: diff.fields_added.map((field) => ({
      id: field.id,
      kind: { type: field.field_type.kind as FieldKind['type'], options: field.field_type.options } as FieldKind,
      required: field.required,
      default: field.default,
    })),
    fieldsRemoved: diff.fields_removed.map((field) => ({
      id: field.id,
      kind: { type: field.field_type.kind as FieldKind['type'], options: field.field_type.options } as FieldKind,
      required: field.required,
      default: field.default,
    })),
    fieldsChanged: diff.fields_changed.map((change) => ({
      id: change.id,
      before: {
        id: change.before.id,
        kind: {
          type: change.before.field_type.kind as FieldKind['type'],
          options: change.before.field_type.options,
        } as FieldKind,
        required: change.before.required,
        default: change.before.default,
      },
      after: {
        id: change.after.id,
        kind: {
          type: change.after.field_type.kind as FieldKind['type'],
          options: change.after.field_type.options,
        } as FieldKind,
        required: change.after.required,
        default: change.after.default,
      },
    })),
    transitionsAdded: diff.transitions_added,
    transitionsRemoved: diff.transitions_removed,
    transitionsChanged: diff.transitions_changed.map((change) => ({
      transition: change.transition,
      requiredCapabilityBefore: change.required_capability_before,
      requiredCapabilityAfter: change.required_capability_after,
      guardsChanged: change.guards_changed,
    })),
    actionsAdded: diff.actions_added.map((action) => ({ ...action })),
    actionsRemoved: diff.actions_removed.map((action) => ({ ...action })),
    requiredCapabilitiesAdded: diff.required_capabilities_added,
    requiredCapabilitiesRemoved: diff.required_capabilities_removed,
  }
}

function toViewUpgrade(diff: ViewUpgradeDiff): WorkflowViewUpgrade {
  return {
    added: diff.added,
    removed: diff.removed,
    changed: diff.changed.map((change) => ({
      view: change.view,
      layoutBefore: change.layout_before,
      layoutAfter: change.layout_after,
      componentsAdded: change.components_added,
      componentsRemoved: change.components_removed,
      componentsChanged: change.components_changed,
    })),
  }
}

function toPreviewFields(preview: BlueprintUpgradePreview): WorkflowUpgradePreview {
  return {
    schema: preview.schema,
    projectId: preview.project_id,
    projectRevision: preview.project_revision,
    current: preview.current
      ? { workflowId: preview.current.blueprint_id, version: preview.current.version }
      : null,
    target: { workflowId: preview.target.blueprint_id, version: preview.target.version },
    kinds: preview.kinds.map(toKindUpgrade),
    views: toViewUpgrade(preview.views),
    warnings: preview.warnings.map(toNotice),
    blockers: preview.blockers.map(toNotice),
    fingerprint: preview.fingerprint,
  }
}

export function toWorkflowUpgradePreview(preview: BlueprintUpgradePreview): WorkflowUpgradePreview {
  return toPreviewFields(preview)
}

export function toWorkflowUpgradeApplied(applied: BlueprintUpgradeApplied): WorkflowUpgradeApplied {
  return {
    ...toPreviewFields(applied),
    binding: toWorkflowBinding(applied.binding),
    previous: applied.previous
      ? { workflowId: applied.previous.blueprint_id, version: applied.previous.version }
      : null,
    alreadyApplied: applied.already_applied,
  }
}

/* ------------------------------------------------------------------ */
/* Attachments                                                         */
/* ------------------------------------------------------------------ */

export interface Attachment {
  id: string
  ownerKind: string
  ownerId: string
  filename: string
  contentType: string
  sizeBytes: number
  checksum: string
  createdAtMs: number
  description: string | null
  isImage: boolean
}

export function toAttachment(view: AttachmentView): Attachment {
  return {
    id: view.id,
    ownerKind: view.owner_kind,
    ownerId: view.owner_id,
    filename: view.filename,
    contentType: view.content_type,
    sizeBytes: view.size_bytes,
    checksum: view.checksum,
    createdAtMs: view.created_at_ms,
    description: view.description,
    isImage: view.is_image,
  }
}

/* ------------------------------------------------------------------ */
/* Planning domain (split for size; same import path)                  */
/* ------------------------------------------------------------------ */

export * from './adapters/planning'

