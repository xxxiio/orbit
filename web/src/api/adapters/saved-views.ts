/**
 * Saved-view rows as the UI stores them.
 *
 * A saved view is a personal presentation preference: a name, a scope, a query
 * string and a list/board display choice. It is not a workflow view, and this
 * adapter never merges the two concepts.
 */
import type {
  SavedViewDisplay,
  SavedViewRecord,
  SavedViewResult,
  SavedViewScope,
} from '../contracts'
import { asStatus } from './shared'

export interface SavedView {
  id: string
  name: string
  scope: SavedViewScope
  projectId: string | null
  query: string
  display: SavedViewDisplay
  version: number
  createdAtMs: number
  updatedAtMs: number
}

export function toSavedView(record: SavedViewRecord): SavedView {
  return {
    id: record.id,
    name: record.name,
    scope: asStatus<SavedViewScope>(record.scope),
    projectId: record.project_id,
    query: record.query,
    display: asStatus<SavedViewDisplay>(record.display),
    version: record.version,
    createdAtMs: record.created_at_ms,
    updatedAtMs: record.updated_at_ms,
  }
}

export interface SavedViewOutcome {
  duplicate: boolean
  /** `null` for delete, because the server removes the row. */
  view: SavedView | null
}

export function toSavedViewOutcome(result: SavedViewResult): SavedViewOutcome {
  return {
    duplicate: result.duplicate,
    view: result.view ? toSavedView(result.view) : null,
  }
}
