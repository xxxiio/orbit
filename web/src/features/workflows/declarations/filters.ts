/**
 * Declared filters: the vocabulary a component may filter a source by, and the
 * one matcher each source uses. A key outside the vocabulary is refused by
 * name rather than silently ignored, because over-counting would be a lie.
 */
import type { Milestone, Task, WorkItem, Workflow } from '@/api/adapters'
import type { ReadinessEntry } from '@/hooks'
import { humanizeIdentifier } from '@/lib/humanize'

const TASK_FILTER_KEYS = new Set(['status', 'priority', 'milestone_id', 'project_id', 'actionable'])
const MILESTONE_FILTER_KEYS = new Set(['status', 'project_id'])
const WORK_ITEM_FILTER_KEYS = new Set(['kind', 'state', 'project_id'])
const PROJECT_FILTER_KEYS = new Set(['project_id', 'status'])

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

export function unsupportedProjectFilterKeys(filter: Record<string, string>): string[] {
  return unsupportedFilterKeys(filter, PROJECT_FILTER_KEYS)
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

/**
 * A stored enum value as copy. Only snake_case identifiers are humanised:
 * `in_progress` is a wire spelling, while `m-1` is an id and `true` is a
 * literal, and neither should be re-cased into something it is not.
 */
function filterValueText(value: string): string {
  return /^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/.test(value) ? humanizeIdentifier(value) : value
}

/** Human-readable summary of a declared filter, e.g. `status is In progress`. */
export function describeFilter(filter: Record<string, string>): string | null {
  const entries = Object.entries(filter)
  if (entries.length === 0) return null
  return entries
    .map(([key, value]) => `${key.replace(/_/g, ' ')} is ${filterValueText(value)}`)
    .join(', ')
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
