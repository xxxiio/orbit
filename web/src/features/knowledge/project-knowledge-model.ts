/**
 * The project-scoped knowledge read (C9), grouped for display.
 *
 * The server decides inclusion: a note is in the response when it references the
 * project itself or one of the project's tasks, and it says which references
 * caused that. This module never re-derives inclusion — it only separates the
 * three honest cases so the panel can say *why* each note is here:
 *
 * - `direct`      the note names the project;
 * - `throughTasks`the note names a task of the project;
 * - `unattributed`the note was returned for a target that is not in this
 *                 project's task list. Never dropped: shown with its target and
 *                 named as unexpected rather than repaired.
 */
import type { ProjectKnowledgeNote, ProjectKnowledgeResponse, TypedReference } from '@/api/contracts'
import { formatTypedReference } from './reference-model'

export interface ProjectKnowledgeGroups {
  /** Server order (by path). */
  notes: ProjectKnowledgeNote[]
  direct: ProjectKnowledgeNote[]
  throughTasks: ProjectKnowledgeNote[]
  unattributed: ProjectKnowledgeNote[]
  /** The project's task ids, as the read returned them. */
  taskIds: string[]
}

function referencesOf(note: ProjectKnowledgeNote): TypedReference[] {
  return note.references ?? []
}

export function groupProjectKnowledge(response: ProjectKnowledgeResponse): ProjectKnowledgeGroups {
  const taskIds = response.tasks ?? []
  const knownTasks = new Set(taskIds)
  const direct: ProjectKnowledgeNote[] = []
  const throughTasks: ProjectKnowledgeNote[] = []
  const unattributed: ProjectKnowledgeNote[] = []

  for (const note of response.notes ?? []) {
    const references = referencesOf(note)
    const namesProject = references.some(
      (reference) => reference.kind === 'project' && reference.id === response.project_id,
    )
    const namesTask = references.some(
      (reference) => reference.kind === 'task' && knownTasks.has(reference.id),
    )
    if (namesProject) direct.push(note)
    else if (namesTask) throughTasks.push(note)
    else unattributed.push(note)
  }

  return { notes: response.notes ?? [], direct, throughTasks, unattributed, taskIds }
}

/** The references on one note that belong to this project, in server order. */
export function projectReferencesOf(
  note: ProjectKnowledgeNote,
  projectId: string,
  taskIds: readonly string[],
): TypedReference[] {
  const tasks = new Set(taskIds)
  return referencesOf(note).filter((reference) =>
    reference.kind === 'project' ? reference.id === projectId : tasks.has(reference.id),
  )
}

/** References the note lists that are not this project and not one of its tasks. */
export function foreignReferencesOf(
  note: ProjectKnowledgeNote,
  projectId: string,
  taskIds: readonly string[],
): TypedReference[] {
  const tasks = new Set(taskIds)
  return referencesOf(note).filter((reference) =>
    reference.kind === 'project' ? reference.id !== projectId : !tasks.has(reference.id),
  )
}

/**
 * The declared `knowledge_refs` columns this build can honour.
 * `references`/`reference` is the C9 addition; anything else is named as
 * unrepresentable rather than invented.
 */
const REPRESENTABLE_COLUMNS = new Set(['title', 'path', 'id', 'note_id', 'references', 'reference'])

export function unrepresentableColumns(columns: readonly string[]): string[] {
  return columns.filter((column) => !REPRESENTABLE_COLUMNS.has(column.toLowerCase()))
}

/** A short, stable descriptor of what pulled a note in, for a row subtitle. */
export function inclusionSummary(note: ProjectKnowledgeNote, projectId: string, taskIds: readonly string[]): string {
  const references = projectReferencesOf(note, projectId, taskIds)
  if (references.length === 0) {
    const foreign = referencesOf(note).map(formatTypedReference)
    return foreign.length === 0 ? 'No reference listed' : `References ${foreign.join(', ')}`
  }
  return `References ${references.map((reference) => `${reference.kind} ${reference.id}`).join(' · ')}`
}
