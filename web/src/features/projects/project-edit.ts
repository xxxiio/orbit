/**
 * The pure rules behind the project identity form.
 *
 * The server is the authority (`Project::rename` / `Project::transition_to`);
 * this module only decides which fields a save should carry and which statuses
 * the control may offer. Keeping it framework-free makes both decisions
 * testable without a router or a form.
 */
import type { ProjectStatus } from '@/api/contracts'
import type { Project, ProjectStatusOption } from '@/api/adapters'

export interface ProjectDraft {
  name: string
  status: ProjectStatus
}

export interface ProjectUpdatePlan {
  /** `''` on update means "keep the stored name". */
  name: string
  status?: ProjectStatus
}

/** Client-side check before sending; the server still decides what a name is. */
export function projectNameError(name: string): string | null {
  return name.trim().length === 0 ? 'A project needs a name.' : null
}

/**
 * `null` means "nothing to send". A rename sends the trimmed name; a status
 * change always sends at least one field, with `name: ''` when the name is
 * unchanged.
 */
export function planProjectUpdate(project: Project, draft: ProjectDraft): ProjectUpdatePlan | null {
  const name = draft.name.trim()
  if (name.length === 0) return null

  const nameChanged = name !== project.name
  const statusChanged = draft.status !== project.status
  if (!nameChanged && !statusChanged) return null

  return {
    name: nameChanged ? name : '',
    ...(statusChanged ? { status: draft.status } : {}),
  }
}

/**
 * The stored status plus only the targets the server reported for it. A status
 * missing from the response, or a failed read, yields just the stored status so
 * the form can stay read-only instead of inventing a move.
 */
export function projectStatusChoices(
  statuses: readonly ProjectStatusOption[] | undefined,
  current: ProjectStatus,
): { choices: ProjectStatus[]; read: boolean } {
  const entry = statuses?.find((option) => option.status === current)
  if (!entry) return { choices: [current], read: false }
  return { choices: [...new Set<ProjectStatus>([current, ...entry.next])], read: true }
}
