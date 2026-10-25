import type { MilestoneStatus, ProjectStatus } from '@/api/contracts'
import { humanizeIdentifier } from '@/lib/humanize'

/**
 * The one place the project and milestone vocabularies are named. The server
 * stores snake_case values; nobody should see them raw in a badge.
 */
export const projectStatusLabels: Record<ProjectStatus, string> = {
  proposed: 'Proposed',
  active: 'Active',
  on_hold: 'On hold',
  completed: 'Completed',
  cancelled: 'Cancelled',
  archived: 'Archived',
}

export function projectStatusLabel(status: ProjectStatus): string {
  return projectStatusLabels[status] ?? humanizeIdentifier(status)
}

export const milestoneStatusLabels: Record<MilestoneStatus, string> = {
  planned: 'Planned',
  in_progress: 'In progress',
  blocked: 'Blocked',
  completed: 'Completed',
  cancelled: 'Cancelled',
}

/** Milestone records carry the raw server string, so accept and humanize it. */
export function milestoneStatusLabel(status: MilestoneStatus | string): string {
  return milestoneStatusLabels[status as MilestoneStatus] ?? humanizeIdentifier(status)
}
