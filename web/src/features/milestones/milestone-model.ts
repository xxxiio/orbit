/**
 * Milestone presentation rules.
 *
 * A milestone is a project checkpoint. The server owns its lifecycle edges,
 * progress counts and membership rules; everything here only turns those
 * values into labels, form drafts and card copy so every milestone surface
 * reads the same record the same way.
 */
import type { Milestone, Task, WorkItem } from '@/api/adapters'
import type { MilestoneStatus } from '@/api/contracts'
import type { MilestoneDraft } from '@/api/endpoints/milestones'

export const MILESTONE_PRIORITY_OPTIONS = [
  { value: 'none', label: 'No priority' },
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'urgent', label: 'Urgent' },
] as const

/**
 * The status edges `orbit_domain::MilestoneStatus::can_transition_to` applies.
 * Offering only these keeps an edit from proposing a move the server refuses.
 */
const MILESTONE_STATUS_EDGES: Record<MilestoneStatus, readonly MilestoneStatus[]> = {
  planned: ['in_progress', 'blocked', 'cancelled'],
  in_progress: ['blocked', 'completed', 'cancelled'],
  blocked: ['in_progress', 'cancelled'],
  completed: [],
  cancelled: [],
}

/** The legal next states for a stored status. A terminal state has none. */
export function milestoneNextStatuses(status: string): readonly MilestoneStatus[] {
  return MILESTONE_STATUS_EDGES[status as MilestoneStatus] ?? []
}

/** The states an edit control may offer: the current one plus legal moves. */
export function milestoneStatusChoices(status: string): readonly MilestoneStatus[] {
  return [status as MilestoneStatus, ...milestoneNextStatuses(status)].filter(
    (candidate, index, all) => all.indexOf(candidate) === index,
  )
}

/** The editable half of a milestone: no status; status has its own command. */
export interface MilestoneFormDraft {
  name: string
  description: string
  priority: string | null
  owner: string | null
  startAtMs: number | null
  targetAtMs: number | null
}

export function emptyMilestoneDraft(): MilestoneFormDraft {
  return {
    name: '',
    description: '',
    priority: null,
    owner: null,
    startAtMs: null,
    targetAtMs: null,
  }
}

export function milestoneDraftFrom(milestone: Milestone): MilestoneFormDraft {
  return {
    name: milestone.name,
    description: milestone.description,
    priority: milestone.priority,
    owner: milestone.owner,
    startAtMs: milestone.startAtMs,
    targetAtMs: milestone.targetAtMs,
  }
}

/** Shape a form draft for the update command. Empty owner means "no owner". */
export function toMilestoneDraft(draft: MilestoneFormDraft): MilestoneDraft {
  const owner = draft.owner?.trim() ?? ''
  return {
    name: draft.name.trim(),
    description: draft.description,
    priority: draft.priority ?? null,
    owner: owner.length > 0 ? owner : null,
    startAtMs: draft.startAtMs,
    targetAtMs: draft.targetAtMs,
  }
}

/** Client checks that mirror the server's own field rules. */
export function validateMilestoneDraft(draft: MilestoneFormDraft): string | null {
  if (draft.name.trim().length === 0) return 'Give the milestone a name.'
  if (
    draft.startAtMs !== null &&
    draft.targetAtMs !== null &&
    draft.startAtMs > draft.targetAtMs
  ) {
    return 'The start date must be on or before the target date.'
  }
  return null
}

export function hasMilestoneDraft(draft: MilestoneFormDraft): boolean {
  return (
    draft.name.trim().length > 0 ||
    draft.description.trim().length > 0 ||
    draft.priority !== null ||
    (draft.owner?.trim().length ?? 0) > 0 ||
    draft.startAtMs !== null ||
    draft.targetAtMs !== null
  )
}

/** A local calendar date for `<input type="date">`. */
export function dateInputValue(ms: number | null | undefined): string {
  if (ms == null) return ''
  const date = new Date(ms)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/**
 * Local midnight for a date input. Returns `null` for an empty or impossible
 * date, so a typo never becomes a different day.
 */
export function parseDateInput(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null
  const [year, month, day] = value.split('-').map(Number)
  const date = new Date(year, month - 1, day, 0, 0, 0, 0)
  if (
    date.getFullYear() !== year ||
    date.getMonth() !== month - 1 ||
    date.getDate() !== day
  ) {
    return null
  }
  return date.getTime()
}

export function formatMilestoneDate(ms: number | null | undefined): string {
  if (ms == null) return 'No date'
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(ms))
}

/**
 * The schedule sentence a card shows. Never invents a date: a target-only
 * milestone is a checkpoint, not a one-day span.
 */
export function milestoneDateSummary(milestone: Milestone): string {
  if (milestone.startAtMs != null && milestone.targetAtMs != null) {
    return `${formatMilestoneDate(milestone.startAtMs)} – ${formatMilestoneDate(milestone.targetAtMs)}`
  }
  if (milestone.targetAtMs != null) {
    return `Target ${formatMilestoneDate(milestone.targetAtMs)}`
  }
  if (milestone.startAtMs != null) {
    return `Starts ${formatMilestoneDate(milestone.startAtMs)}`
  }
  return 'No dates'
}

/** A compact, honest progress sentence for a card or dialog header. */
export function milestoneProgressLabel(milestone: Milestone): string {
  const { total, completed } = milestone.progress
  if (total === 0) {
    return milestone.status === 'completed' ? 'Completed' : 'No work yet'
  }
  return `${completed}/${total} done`
}

/** Tasks the server reports as members of this milestone. */
export function milestoneChildTasks(milestone: Milestone, tasks: readonly Task[]): Task[] {
  const ids = new Set(milestone.taskIds)
  return tasks.filter((task) => ids.has(task.id))
}

/** Work Items the server reports as members of this milestone. */
export function milestoneChildWorkItems(
  milestone: Milestone,
  workItems: readonly WorkItem[],
): WorkItem[] {
  const ids = new Set(milestone.workItemIds)
  return workItems.filter((item) => ids.has(item.id))
}

/** The milestone a Work Item belongs to, or `null` when it belongs to none. */
export function milestoneIdForWorkItem(
  milestones: readonly Milestone[],
  itemId: string,
): string | null {
  for (const milestone of milestones) {
    if (milestone.workItemIds.includes(itemId)) return milestone.id
  }
  return null
}

/** A readable Work Item label: its declared name when present, else its id. */
export function workItemLabel(item: WorkItem): string {
  for (const key of ['title', 'name', 'subject']) {
    const value = item.properties[key]
    if (typeof value === 'string' && value.trim().length > 0) return value.trim()
  }
  return item.id
}

/**
 * A milestone is at risk only when it has a target in the past, is not already
 * finished, and still has open work. The server stores no such flag.
 */
export function isMilestoneAtRisk(milestone: Milestone, nowMs: number): boolean {
  if (milestone.targetAtMs == null) return false
  if (milestone.status === 'completed' || milestone.status === 'cancelled') return false
  const { total, completed } = milestone.progress
  if (total > 0 && completed >= total) return false
  return milestone.targetAtMs < nowMs
}
