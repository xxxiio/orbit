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
import type { Milestone, WorkItem, WorkflowView, WorkflowViewComponent } from '@/api/adapters'
import type { ReadinessEntry } from '@/hooks'
import { workflowText } from './workflow-copy'

export type ViewComponentFamily = 'tasks' | 'board' | 'milestones' | 'schedule' | 'other'

const TASK_LIST_TYPES = new Set(['task_list', 'table'])
const BOARD_TYPES = new Set(['kanban'])

/**
 * The data family a component renders. `timeline` is declared twice in the
 * bundled workflows (milestones and schedule), so its source decides the family.
 */
export function componentFamily(component: WorkflowViewComponent): ViewComponentFamily {
  const source = component.source ?? null
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
  kind: 'new_task' | 'new_milestone'
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
  const families = view.components.map(componentFamily)
  if (families.includes('tasks') || families.includes('board')) {
    return { kind: 'new_task', label: 'New task' }
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
      const expected = value.toLowerCase() === 'true'
      if (!readiness || readiness.actionable !== expected) return false
    }
  }
  return true
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
  return declared || componentTypeNames[component.type] || 'View panel'
}
