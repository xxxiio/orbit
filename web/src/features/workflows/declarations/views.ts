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
import type { Milestone, Task, WorkItem, Workflow, WorkflowView, WorkflowViewComponent } from '@/api/adapters'
import type { ReadinessEntry } from '@/hooks'
import { workflowText } from '@/domain'
import {
  milestoneMatchesDeclaredFilter,
  taskMatchesDeclaredFilter,
  workItemMatchesDeclaredFilter,
} from './filters'
import { hasOwnKey } from './columns'

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
 * `work_items` is a family of its own so the header action and badge come from
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
  kind: 'tasks' | 'milestones' | 'work_items'
  /** Used as the badge's accessible name suffix. */
  noun: 'tasks' | 'milestones' | 'work items'
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
  if (family === 'work_items') return { kind: 'work_items', noun: 'work items' }
  return null
}

export interface ViewBadgeCountInput {
  tasks: readonly Task[]
  milestones: readonly Milestone[]
  /**
   * The project's work items, when the page has read them. `undefined` means
   * "not read yet": a work-item badge is never guessed from an empty list.
   */
  workItems?: readonly WorkItem[]
  /** The pinned manifest, needed to resolve a board's default kind. */
  workflow?: Workflow
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
  { tasks, milestones, workItems, workflow, readinessByTask }: ViewBadgeCountInput,
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
  if (badge.kind === 'work_items') {
    // Without a read there is no honest number: an empty array is a fact, a
    // missing one is not.
    if (!workItems) return null
    const matching = workItems.filter((item) => workItemMatchesDeclaredFilter(item, filter))
    // A board shows one kind's states: the pinned kind, or the first declared
    // one. A table or list shows every kind the filter selects.
    const boardKind =
      component.type === 'kanban' ? (filter.kind?.trim() ?? workflow?.kinds[0]?.id) : undefined
    const visible = boardKind
      ? matching.filter((item) => item.kind.toLowerCase() === boardKind.toLowerCase())
      : matching
    return limit === null ? visible.length : Math.min(visible.length, limit)
  }
  const matching = milestones.filter((milestone) =>
    milestoneMatchesDeclaredFilter(milestone, filter),
  )
  return limit === null ? matching.length : Math.min(matching.length, limit)
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
