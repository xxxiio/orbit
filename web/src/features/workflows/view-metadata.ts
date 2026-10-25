/**
 * Workflow view metadata: the parts of a declared view that are not its body.
 *
 * A view is data (id, title, layout, components). Everything a page needs to
 * decorate a tab — the primary header action, the tab badge — is derived from
 * the declared components here, so no screen has to test `view.id === 'tasks'`.
 * The rules are deliberately structural:
 *
 * - the *first* component is the view's primary surface, which decides the
 *   badge and the first-choice header action;
 * - a task-bearing component anywhere else still adds the New task action, so
 *   an overview that shows work can start work without a second click;
 * - `kanban`/`table`/`task_list` only count as task surfaces when their
 *   declared source is `tasks` (or absent), never for `work_items`.
 *
 * None of this decides what the server allows; it only names chrome.
 */
import type { Task } from '@/api/adapters'
import type { Milestone, WorkItem, Workflow, WorkflowView, WorkflowViewComponent } from '@/api/adapters'
import type { ReadinessEntry } from '@/hooks'
import { workflowText } from '@/domain'
import { compareTasks, type TaskSort, type TaskSortKey } from '@/features/tasks/task-list-model'

export type ViewComponentFamily =
  | 'tasks'
  | 'board'
  | 'milestones'
  | 'work_items'
  | 'schedule'
  | 'other'

const TASK_LIST_TYPES = new Set(['task_list', 'table'])
const BOARD_TYPES = new Set(['kanban'])
const WORK_ITEM_TYPES = new Set(['kanban', 'table', 'task_list', 'list', 'detail'])

/**
 * The data family a component renders. `timeline` is declared twice in the
 * bundled workflows (milestones and schedule), so its source decides the family.
 * `work_items` is a family of its own so I3's header action and badge come from
 * one rule instead of a fourth branch on a component type.
 */
export function componentFamily(component: WorkflowViewComponent): ViewComponentFamily {
  const source = component.source ?? null
  // A list or detail panel over work items is a work-item surface too: a view
  // that puts one first must still get the header create action and read as the
  // work-item family, not as "other".
  if (source === 'work_items' && WORK_ITEM_TYPES.has(component.type)) return 'work_items'
  if (BOARD_TYPES.has(component.type)) {
    return source === 'tasks' || source === null ? 'board' : 'other'
  }
  if (TASK_LIST_TYPES.has(component.type)) {
    return source === 'tasks' || source === null ? 'tasks' : 'other'
  }
  if (component.type === 'timeline') {
    if (source === 'schedule') return 'schedule'
    if (source === 'milestones' || source === null) return 'milestones'
    return 'other'
  }
  if (component.type === 'milestone_progress') {
    return source === 'milestones' || source === null ? 'milestones' : 'other'
  }
  return 'other'
}

export interface ViewHeaderAction {
  kind: 'new_task' | 'new_milestone' | 'new_work_item'
  label: string
}

/** The one primary action a view offers in the workspace header. */
export function viewHeaderAction(view: WorkflowView): ViewHeaderAction | null {
  const primary = view.components[0] ? componentFamily(view.components[0]) : null
  if (primary === 'tasks' || primary === 'board') {
    return { kind: 'new_task', label: 'New task' }
  }
  if (primary === 'milestones') {
    return { kind: 'new_milestone', label: 'New milestone' }
  }
  if (primary === 'work_items') {
    return { kind: 'new_work_item', label: 'New work item' }
  }
  const families = view.components.map(componentFamily)
  if (families.includes('tasks') || families.includes('board')) {
    return { kind: 'new_task', label: 'New task' }
  }
  if (families.includes('work_items')) {
    return { kind: 'new_work_item', label: 'New work item' }
  }
  return null
}

export interface ViewBadge {
  kind: 'tasks' | 'milestones'
  /** Used as the badge's accessible name suffix. */
  noun: 'tasks' | 'milestones'
}

/**
 * A count badge is only honest where the view's primary surface is that list.
 * A view that mixes metrics, detail and work gets no badge.
 */
export function viewBadge(view: WorkflowView): ViewBadge | null {
  const first = view.components[0]
  if (!first) return null
  const family = componentFamily(first)
  if (family === 'tasks' || family === 'board') return { kind: 'tasks', noun: 'tasks' }
  if (family === 'milestones') return { kind: 'milestones', noun: 'milestones' }
  return null
}

/* ------------------------------------------------------------------ */
/* Declared filters                                                    */
/* ------------------------------------------------------------------ */

/**
 * The filter vocabulary a component may declare over a data source. A filter
 * key outside this set is not silently ignored: the renderer refuses the panel
 * and says the filter is not supported, because over-counting would be a lie.
 */
const TASK_FILTER_KEYS = new Set(['status', 'priority', 'milestone_id', 'project_id', 'actionable'])
const MILESTONE_FILTER_KEYS = new Set(['status', 'project_id'])
const WORK_ITEM_FILTER_KEYS = new Set(['kind', 'state', 'project_id'])

export function unsupportedFilterKeys(
  filter: Record<string, string>,
  supported: ReadonlySet<string>,
): string[] {
  return Object.keys(filter).filter((key) => !supported.has(key))
}

export function unsupportedTaskFilterKeys(filter: Record<string, string>): string[] {
  return unsupportedFilterKeys(filter, TASK_FILTER_KEYS)
}

export function unsupportedMilestoneFilterKeys(filter: Record<string, string>): string[] {
  return unsupportedFilterKeys(filter, MILESTONE_FILTER_KEYS)
}

export function unsupportedWorkItemFilterKeys(filter: Record<string, string>): string[] {
  return unsupportedFilterKeys(filter, WORK_ITEM_FILTER_KEYS)
}

function matchesValue(actual: string | undefined, expected: string): boolean {
  return (actual ?? '').toLowerCase() === expected.toLowerCase()
}

/**
 * One reading of a declared `actionable` value, shared by the matcher, the
 * refusal and the panel copy. The manifest validator only bounds the string, so
 * a padded spelling such as `" true "` is valid: it is normalized once here so
 * the guard, the note and the filter can never disagree about what it meant.
 */
export function parseActionable(value: string | undefined): boolean | null {
  const normalized = value?.trim().toLowerCase()
  if (normalized === 'true') return true
  if (normalized === 'false') return false
  return null
}


/**
 * Apply a declared filter to a task. `actionable` is answered by the server's
 * readiness report; without it the task cannot match, so a pending readiness
 * read never overstates the count.
 */
export function taskMatchesDeclaredFilter(
  task: Task,
  filter: Record<string, string>,
  readiness?: ReadinessEntry | null,
): boolean {
  for (const [key, value] of Object.entries(filter)) {
    if (key === 'status' && !matchesValue(task.status, value)) return false
    if (key === 'priority' && !matchesValue(task.priority ?? 'none', value)) return false
    if (key === 'milestone_id' && !matchesValue(task.milestoneId ?? '', value)) return false
    if (key === 'project_id' && !matchesValue(task.projectId ?? '', value)) return false
    if (key === 'actionable') {
      const expected = parseActionable(value)
      if (expected === null || !readiness || readiness.actionable !== expected) return false
    }
  }
  return true
}

/**
 * `actionable` is a boolean in the manifest. Any other spelling would be read as
 * false by `taskMatchesDeclaredFilter`, silently selecting the wrong half of the
 * list, so it is refused by name instead.
 */
export function actionableFilterRefusal(filter: Record<string, string>): string | null {
  const value = filter.actionable
  if (value === undefined) return null
  if (parseActionable(value) !== null) return null
  return `This panel filters tasks by actionable = “${value}”; only true or false are supported.`
}

/** The board's copy for a valid `actionable` filter, derived from its value. */
export function actionableFilterNote(filter: Record<string, string>): string | null {
  const expected = parseActionable(filter.actionable)
  if (expected === true) {
    return 'Filtered to tasks the server marks actionable. Columns still show the server’s status.'
  }
  if (expected === false) {
    return 'Filtered to tasks the server does not mark actionable. Columns still show the server’s status.'
  }
  return null
}

export function milestoneMatchesDeclaredFilter(
  milestone: Milestone,
  filter: Record<string, string>,
): boolean {
  for (const [key, value] of Object.entries(filter)) {
    if (key === 'status' && !matchesValue(milestone.status, value)) return false
    if (key === 'project_id' && !matchesValue(milestone.projectId, value)) return false
  }
  return true
}

export function workItemMatchesDeclaredFilter(
  item: WorkItem,
  filter: Record<string, string>,
): boolean {
  for (const [key, value] of Object.entries(filter)) {
    if (key === 'kind' && !matchesValue(item.kind, value)) return false
    if (key === 'state' && !matchesValue(item.state, value)) return false
    if (key === 'project_id' && !matchesValue(item.projectId, value)) return false
  }
  return true
}

/**
 * A `filter.kind` that names a kind the pinned workflow does not declare is not
 * an empty result: it is a declaration this build cannot honour. Every
 * work-item surface refuses it with the same sentence instead of showing an
 * empty panel (or, worse, a count of rows that do not exist).
 */
export function workItemKindFilterRefusal(
  filter: Record<string, string>,
  workflow: Workflow | undefined,
): string | null {
  const declared = filter.kind?.trim()
  if (!declared) return null
  const known = (workflow?.kinds ?? []).some(
    (kind) => kind.id.toLowerCase() === declared.toLowerCase(),
  )
  if (known) return null
  return `This panel filters work items by the kind “${declared}”, which the pinned workflow does not declare.`
}

/** A data-source name as copy: `work_items` → "Work items". */
export function dataSourceName(source: string): string {
  const words = source.replace(/_/g, ' ')
  return words.charAt(0).toUpperCase() + words.slice(1)
}

/** Human-readable summary of a declared filter, e.g. `status is in_progress`. */
export function describeFilter(filter: Record<string, string>): string | null {
  const entries = Object.entries(filter)
  if (entries.length === 0) return null
  return entries
    .map(([key, value]) => `${key.replace(/_/g, ' ')} is ${value}`)
    .join(', ')
}

const PROJECT_FILTER_KEYS = new Set(['project_id', 'status'])

export function unsupportedProjectFilterKeys(filter: Record<string, string>): string[] {
  return unsupportedFilterKeys(filter, PROJECT_FILTER_KEYS)
}

/** A detail panel may declare a project filter; the project either matches or not. */
export function projectMatchesDeclaredFilter(
  project: { id: string; status: string },
  filter: Record<string, string>,
): boolean {
  for (const [key, value] of Object.entries(filter)) {
    if (key === 'project_id' && !matchesValue(project.id, value)) return false
    if (key === 'status' && !matchesValue(project.status, value)) return false
  }
  return true
}

/* ------------------------------------------------------------------ */
/* Metric declarations                                                 */
/* ------------------------------------------------------------------ */

export type MetricAggregate = 'count' | 'sum' | 'average' | 'min' | 'max'

const METRIC_AGGREGATES = new Set<string>(['count', 'sum', 'average', 'min', 'max'])

export interface MetricDeclaration {
  aggregate: MetricAggregate
  field: string | null
  /** The unit the value is shown in; `null` for a plain count. */
  unit: string | null
  /**
   * Multiplier from the stored field's unit to `unit`. 1 when no conversion is
   * needed. A declared display unit is never treated as a free label on a field
   * whose base unit this build knows (minutes): a mismatch is converted or the
   * panel is refused.
   */
  factor: number
  /** Non-null means the declaration cannot be honoured; render a refusal. */
  refusal: string | null
}

/** Fields whose name already states the unit this build can show. */
function isMinutesField(field: string): boolean {
  const normalized = field.toLowerCase()
  return (
    normalized === 'estimated_minutes' ||
    normalized === 'estimatedminutes' ||
    normalized === 'minutes' ||
    normalized.endsWith('_minutes')
  )
}

/** A declared display unit, when the manifest supplied one. */
function declaredUnit(component: WorkflowViewComponent): string | null {
  const raw = component.options?.unit
  if (typeof raw !== 'string') return null
  const trimmed = raw.trim()
  return trimmed.length > 0 ? trimmed : null
}

function isMetricAggregate(value: string): value is MetricAggregate {
  return METRIC_AGGREGATES.has(value)
}

/**
 * The numeric fields a source can aggregate. Tasks carry exactly one
 * (`estimated_minutes`); a work item carries the `integer` properties its kind
 * declares. Milestones and the schedule carry none.
 */
export function numericFieldsForSource(
  source: string,
  workflow: Workflow | undefined,
  filter: Record<string, string> = {},
): ReadonlySet<string> {
  if (source === 'tasks') return new Set(['estimated_minutes'])
  if (source === 'work_items') {
    const kindFilter = filter.kind?.toLowerCase()
    const fields = new Set<string>()
    for (const kind of workflow?.kinds ?? []) {
      if (kindFilter && kind.id.toLowerCase() !== kindFilter) continue
      for (const field of kind.fields) {
        if (field.kind.type === 'integer') fields.add(field.id)
      }
    }
    return fields
  }
  return new Set()
}

/* ------------------------------------------------------------------ */
/* Declared options                                                    */
/* ------------------------------------------------------------------ */

/**
 * The manifest validator bounds the *keys* of `options`, not the values, so a
 * panel reads each declared value here and refuses a spelling it cannot honour
 * instead of silently treating it as absent.
 */
export function declaredTextOption(
  component: WorkflowViewComponent,
  key: string,
): string | null {
  const raw = component.options?.[key]
  if (typeof raw !== 'string') return null
  const trimmed = raw.trim()
  return trimmed.length > 0 ? trimmed : null
}

/** `empty_text` is copy, never markup: the workflow cannot inject a component. */
export function emptyTextOption(component: WorkflowViewComponent): string | null {
  return declaredTextOption(component, 'empty_text')
}

export function metricCaption(component: WorkflowViewComponent): string | null {
  return declaredTextOption(component, 'caption')
}

export interface DeclaredBooleanOption {
  /** `undefined` when the option was not declared at all. */
  value: boolean | undefined
  refusal: string | null
}

export function declaredBooleanOption(
  component: WorkflowViewComponent,
  key: string,
): DeclaredBooleanOption {
  const raw = component.options?.[key]
  if (raw === undefined || raw === null) return { value: undefined, refusal: null }
  if (typeof raw === 'boolean') return { value: raw, refusal: null }
  if (typeof raw === 'string') {
    const normalized = raw.trim().toLowerCase()
    if (normalized === 'true') return { value: true, refusal: null }
    if (normalized === 'false') return { value: false, refusal: null }
  }
  return {
    value: undefined,
    refusal: `This panel sets ${key} to ${JSON.stringify(raw)}; only true or false are supported.`,
  }
}

export interface DeclaredNumberOption {
  value: number | null
  /** True when the declared number was clamped to the supported range. */
  clamped?: boolean
  refusal: string | null
}

export function declaredNumberOption(
  component: WorkflowViewComponent,
  key: string,
): DeclaredNumberOption {
  const raw = component.options?.[key]
  if (raw === undefined || raw === null) return { value: null, refusal: null }
  const parsed =
    typeof raw === 'number'
      ? raw
      : typeof raw === 'string' && raw.trim().length > 0
        ? Number(raw)
        : Number.NaN
  if (!Number.isFinite(parsed)) {
    return {
      value: null,
      refusal: `This panel sets ${key} to ${JSON.stringify(raw)}, which is not a number.`,
    }
  }
  return { value: parsed, refusal: null }
}

/** A board column may declare a pixel width; 180–420 keeps every column usable. */
export function columnWidthOption(component: WorkflowViewComponent): DeclaredNumberOption {
  const declared = declaredNumberOption(component, 'column_width')
  if (declared.value === null) return declared
  const clamped = Math.min(420, Math.max(180, Math.round(declared.value)))
  return {
    value: clamped,
    clamped: clamped !== declared.value,
    refusal: null,
  }
}

/** The note a board shows when the declared width was clamped, or null. */
export function columnWidthNote(component: WorkflowViewComponent): string | null {
  const width = columnWidthOption(component)
  if (width.value === null || !width.clamped) return null
  return `The workflow's declared column width was clamped to ${width.value} pixels; columns stay between 180 and 420 pixels so every one remains usable.`
}

export interface ListOptionDeclaration {
  showStatus: boolean
  showPriority: boolean
  /** `undefined` means the panel did not declare it. */
  showDependencies: boolean | undefined
  refusal: string | null
}

/** `list`/`task_list` visibility options, with one named refusal on a bad value. */
export function listOptionDeclaration(component: WorkflowViewComponent): ListOptionDeclaration {
  const status = declaredBooleanOption(component, 'show_status')
  const priority = declaredBooleanOption(component, 'show_priority')
  const dependencies = declaredBooleanOption(component, 'show_dependencies')
  return {
    showStatus: status.value ?? true,
    showPriority: priority.value ?? false,
    showDependencies: dependencies.value,
    refusal: status.refusal ?? priority.refusal ?? dependencies.refusal,
  }
}

/** A declared window value named in a refusal, e.g. `days`/`window`. */
export function declaredWindowOption(
  component: WorkflowViewComponent,
  key: 'days' | 'window',
): string | null {
  const raw = component.options?.[key]
  if (raw === undefined || raw === null) return null
  if (typeof raw === 'number' && Number.isFinite(raw)) return String(raw)
  if (typeof raw === 'string' && raw.trim().length > 0) return raw.trim()
  return JSON.stringify(raw)
}

/**
 * Resolve a metric's declared aggregate and field. A declaration this build
 * cannot evaluate gets a named reason instead of a count under a "sum" title.
 */
export function metricDeclaration(
  component: WorkflowViewComponent,
  workflow: Workflow | undefined,
): MetricDeclaration {
  const source = component.source ?? 'tasks'
  const raw = (component.aggregate ?? 'count').trim().toLowerCase()
  if (!isMetricAggregate(raw)) {
    return {
      aggregate: 'count',
      field: null,
      unit: null,
      factor: 1,
      refusal: `This panel asks for the “${component.aggregate}” aggregate, which this build cannot compute.`,
    }
  }

  const field = component.field?.trim() || null
  const numericFields = numericFieldsForSource(source, workflow, component.filter ?? {})

  if (raw !== 'count' && numericFields.size === 0) {
    return {
      aggregate: raw,
      field: null,
      unit: null,
      factor: 1,
      refusal: `${dataSourceName(source)} has no numeric field this build can aggregate; only a count is supported.`,
    }
  }

  if (field && !numericFields.has(field)) {
    const label = source === 'tasks' ? 'tasks' : dataSourceName(source)
    return {
      aggregate: raw,
      field: null,
      unit: null,
      factor: 1,
      refusal: `This panel aggregates the “${field}” field, which is not a numeric field on ${label}.`,
    }
  }

  if (raw !== 'count' && field === null) {
    return {
      aggregate: raw,
      field: null,
      unit: null,
      factor: 1,
      refusal: `A ${raw} aggregate needs the numeric field to aggregate, and this panel does not declare one.`,
    }
  }

  // A non-count number is only honest with a unit. Estimated/minutes fields
  // carry theirs in the name; any other integer field must say so in
  // `options.unit` rather than being shown as minutes.
  const unit = declaredUnit(component)
  if (raw !== 'count' && field !== null && !isMinutesField(field) && unit === null) {
    return {
      aggregate: raw,
      field,
      unit: null,
      factor: 1,
      refusal: `This panel aggregates the “${field}” field, which has no declared unit; add options.unit so the number is not shown as minutes.`,
    }
  }

  // A field whose name states its unit (minutes) can be *converted* to another
  // unit, but a declared unit must not be used as a bare label: "180 hours" for
  // 180 minutes is a lie. Known conversions are applied; anything else refuses.
  if (raw !== 'count' && field !== null && isMinutesField(field)) {
    const conversion = minutesConversion(unit)
    return {
      aggregate: raw,
      field,
      unit: conversion.unit,
      factor: conversion.factor,
      refusal: conversion.refusal,
    }
  }

  return {
    aggregate: raw,
    field,
    unit: raw === 'count' ? null : unit,
    factor: 1,
    refusal: null,
  }
}

interface UnitConversion {
  unit: string
  factor: number
  refusal: string | null
}

/** Minutes → the declared display unit. Unknown units are refused by name. */
function minutesConversion(unit: string | null): UnitConversion {
  if (unit === null) return { unit: 'minutes', factor: 1, refusal: null }
  const normalized = unit.trim().toLowerCase()
  if (['minute', 'minutes', 'min', 'mins'].includes(normalized)) {
    return { unit: 'minutes', factor: 1, refusal: null }
  }
  if (['hour', 'hours', 'h', 'hr', 'hrs'].includes(normalized)) {
    return { unit: 'hours', factor: 1 / 60, refusal: null }
  }
  if (['second', 'seconds', 's', 'sec', 'secs'].includes(normalized)) {
    return { unit: 'seconds', factor: 60, refusal: null }
  }
  return {
    unit: unit,
    factor: 1,
    refusal: `This panel displays a minutes field in “${unit}”; this build converts minutes to minutes, hours or seconds only.`,
  }
}

function numericProperty(value: unknown): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null
  if (typeof value === 'string' && value.trim().length > 0) {
    const parsed = Number(value)
    return Number.isFinite(parsed) ? parsed : null
  }
  return null
}

/**
 * Compute the declared aggregate over rows. `count` counts rows (or non-null
 * field values when a field is declared); the others ignore nulls and return
 * `null` when nothing is left to aggregate.
 */
export function aggregateMetric(
  declaration: MetricDeclaration,
  rows: ReadonlyArray<number | null>,
): number | null {
  if (declaration.aggregate === 'count') {
    return declaration.field
      ? rows.filter((value) => value !== null).length
      : rows.length
  }
  const values = rows.filter((value): value is number => value !== null && Number.isFinite(value))
  if (values.length === 0) return declaration.aggregate === 'sum' ? 0 : null
  const sum = values.reduce((total, value) => total + value, 0)
  if (declaration.aggregate === 'sum') return sum
  if (declaration.aggregate === 'average') return sum / values.length
  if (declaration.aggregate === 'min') return Math.min(...values)
  return Math.max(...values)
}

/** Convert an aggregated raw value into the declaration's display unit. */
export function metricDisplayValue(
  declaration: MetricDeclaration,
  value: number | null,
): number | null {
  if (value === null) return null
  if (declaration.aggregate === 'count') return value
  return value * declaration.factor
}

/** Task rows reduced to the declared numeric field (null when unestimated). */
export function taskMetricRows(tasks: readonly Task[], field: string | null): Array<number | null> {
  if (!field) return tasks.map(() => null)
  if (field === 'estimated_minutes' || field === 'estimatedMinutes') {
    return tasks.map((task) => task.estimatedMinutes)
  }
  return tasks.map(() => null)
}

export function workItemMetricRows(
  items: readonly WorkItem[],
  field: string | null,
): Array<number | null> {
  if (!field) return items.map(() => null)
  return items.map((item) => numericProperty(item.properties[field]))
}

/* ------------------------------------------------------------------ */
/* Sort declarations                                                   */
/* ------------------------------------------------------------------ */

const TASK_SORT_FIELDS: Record<string, TaskSortKey> = {
  title: 'title',
  name: 'title',
  status: 'status',
  priority: 'priority',
  due: 'due',
  due_date: 'due',
  deadline: 'due',
  deadline_at: 'due',
  project: 'project',
  project_id: 'project',
  milestone: 'milestone',
  milestone_id: 'milestone',
}

export interface TaskSortDeclaration {
  /** `null` keeps the server's order. */
  sort: TaskSort | null
  refusal: string | null
}

/**
 * Map a declared `sort`/`descending` pair onto the comparators task lists
 * already use. Fields the adapter does not carry (notably `updated`) are
 * refused by name instead of being dropped.
 */
export function taskSortDeclaration(component: WorkflowViewComponent): TaskSortDeclaration {
  const declared = component.sort?.trim().toLowerCase()
  if (!declared) return { sort: null, refusal: null }
  const key = hasOwnKey(TASK_SORT_FIELDS, declared) ? TASK_SORT_FIELDS[declared] : null
  if (!key) {
    return {
      sort: null,
      refusal: `This panel sorts tasks by “${component.sort}”, a field this build does not carry for tasks.`,
    }
  }
  return {
    sort: { key, direction: component.descending ? 'desc' : 'asc' },
    refusal: null,
  }
}

/** A stable, copied sort using the shared task comparator. */
export function sortByDeclaration(tasks: readonly Task[], sort: TaskSort | null): Task[] {
  if (!sort) return [...tasks]
  return [...tasks].sort((left, right) => compareTasks(left, right, sort))
}

/* ------------------------------------------------------------------ */
/* Reserved view ids + badge counts                                    */
/* ------------------------------------------------------------------ */

/**
 * Ids the workspace reserves for its own tools. A workflow that declares one of
 * these must not shadow the tool: the declared view is not rendered and the page
 * says so, while the tool keeps its route.
 */
export const RESERVED_VIEW_IDS: ReadonlySet<string> = new Set(['planning', 'settings'])

export function isReservedViewId(id: string): boolean {
  return RESERVED_VIEW_IDS.has(id)
}

/**
 * `undefined` means no workflow could be resolved (use the fallback views);
 * an empty array means the pinned workflow truthfully declares no views.
 */
export function declaredWorkspaceViews(
  workflow: Workflow | undefined,
  fallback: readonly WorkflowView[],
): WorkflowView[] {
  return workflow ? workflow.views : [...fallback]
}

/** Declared views a person may navigate to: reserved tool ids are excluded. */
export function workspaceViewTabs(views: readonly WorkflowView[]): WorkflowView[] {
  return views.filter((view) => !isReservedViewId(view.id))
}

/** Declared views that collide with a tool id, so the page can say so. */
export function reservedDeclaredViews(views: readonly WorkflowView[]): WorkflowView[] {
  return views.filter((view) => isReservedViewId(view.id))
}

export interface ViewBadgeCountInput {
  tasks: readonly Task[]
  milestones: readonly Milestone[]
  /** The server's readiness report, needed only by an `actionable` badge. */
  readinessByTask?: ReadonlyMap<string, ReadinessEntry>
}

function badgeLimitApplies(component: WorkflowViewComponent): boolean {
  if (component.limit === null || component.limit === undefined || component.limit <= 0) return false
  // Every data surface that renders a `limit` shows the same capped set a
  // badge must count (a table now slices too; see TableView/ProjectTaskList).
  return true
}

/**
 * The number a tab badge may show: the primary surface's rows after its declared
 * filter and limit, i.e. the same set its panel renders. `null` means no badge.
 */
export function viewBadgeCount(
  view: WorkflowView,
  { tasks, milestones, readinessByTask }: ViewBadgeCountInput,
): number | null {
  const badge = viewBadge(view)
  const component = view.components[0]
  if (!badge || !component) return null
  const filter = component.filter ?? {}
  const limit = badgeLimitApplies(component) ? (component.limit as number) : null
  if (badge.kind === 'tasks') {
    const matching = tasks.filter((task) =>
      taskMatchesDeclaredFilter(task, filter, readinessByTask?.get(task.id)),
    )
    return limit === null ? matching.length : Math.min(matching.length, limit)
  }
  const matching = milestones.filter((milestone) =>
    milestoneMatchesDeclaredFilter(milestone, filter),
  )
  return limit === null ? matching.length : Math.min(matching.length, limit)
}

/**
 * Own-property test for manifest-keyed tables. `in` and bracket lookup would
 * walk `Object.prototype`, so a type named `constructor` would find a built-in.
 */
export function hasOwnKey(table: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(table, key)
}

/* ------------------------------------------------------------------ */
/* Layout                                                              */
/* ------------------------------------------------------------------ */

/**
 * Static Tailwind classes for the declared layout. Columns above four are
 * clamped: more would make every panel unreadable, and the declaration is a
 * hint, not a promise about the viewport.
 */
export function viewLayoutClassName(view: WorkflowView): string {
  const columns = Math.min(4, Math.max(1, view.layout?.columns ?? 1))
  if (view.layout?.type === 'vertical') return 'grid gap-5'
  if (view.layout?.type === 'horizontal') {
    return columns >= 2 ? 'grid gap-5 md:grid-cols-2' : 'grid gap-5'
  }
  switch (columns) {
    case 2:
      return 'grid gap-5 md:grid-cols-2'
    case 3:
      return 'grid gap-5 md:grid-cols-2 xl:grid-cols-3'
    case 4:
      return 'grid gap-5 md:grid-cols-2 xl:grid-cols-4'
    default:
      return 'grid gap-5'
  }
}

/**
 * A table or a board needs the whole row; a metric or a text panel reads fine
 * in a grid cell. This keeps the declared layout without squeezing wide work.
 */
export function componentSpansFullWidth(
  component: WorkflowViewComponent,
  layout: WorkflowView['layout'] | undefined,
): boolean {
  if (!layout || layout.type === 'vertical') return true
  // A full task list (no declared limit) carries its own toolbar and table; a
  // summary strip with a limit is happy in a grid cell.
  const fullTaskList =
    component.type === 'task_list' &&
    (component.limit === null || component.limit === undefined)
  return component.type === 'table' || component.type === 'kanban' || fullTaskList
}

/* ------------------------------------------------------------------ */
/* Human names                                                         */
/* ------------------------------------------------------------------ */

/** Panel names for component types the browser does not render yet. */
export const componentTypeNames: Record<string, string> = {
  text: 'Text',
  list: 'List',
  table: 'Table',
  metric: 'Metric',
  detail: 'Detail',
  kanban: 'Board',
  task_list: 'Task list',
  timeline: 'Timeline',
  habit_tracker: 'Habit tracker',
  metric_history: 'Metric history',
  knowledge_refs: 'Knowledge references',
  agent_jobs: 'Agent jobs',
  milestone_progress: 'Milestone progress',
}

export function componentTypeName(component: WorkflowViewComponent): string {
  const declared = workflowText(component.title)?.trim()
  if (declared) return declared
  // A manifest type is data, not a key of Object.prototype: `constructor` or
  // `toString` must fall through to the generic name, never to a built-in.
  if (hasOwnKey(componentTypeNames, component.type)) {
    return componentTypeNames[component.type]
  }
  return 'View panel'
}
