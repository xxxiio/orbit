/**
 * Saved-view configuration rules.
 *
 * A saved view is a *personal presentation preference*: the URL query string
 * plus the list/board display choice. It is never a workflow definition and
 * never a workflow view. These helpers are pure so the mapping between a route
 * and a saved configuration can be tested without a router.
 */
import type { WorkflowView } from '@/api/adapters'
import type {
  SavedViewDisplay,
  SavedViewScope,
} from '@/api/contracts'

export interface SavedViewConfig {
  scope: SavedViewScope
  projectId: string | null
  /** The query string without a leading `?`, exactly as the browser holds it. */
  query: string
  display: SavedViewDisplay
}

/** Strip the router's `?`; keep the rest byte-for-byte so restore is faithful. */
export function queryFromSearch(search: string): string {
  return search.startsWith('?') ? search.slice(1) : search
}

/** `?a=1` or an empty string; never a dangling `?`. */
export function searchFromQuery(query: string): string {
  return query.length > 0 ? `?${query}` : ''
}

function projectIdFromPath(pathname: string): string | null {
  const match = /^\/projects\/([^/]+)/.exec(pathname)
  if (!match) return null
  try {
    return decodeURIComponent(match[1])
  } catch {
    return match[1]
  }
}

/**
 * The scope a route belongs to. Management routes under a project
 * (`/projects/:id/settings`, `/planning`) are still project scope: a saved view
 * belongs to the project, not to the tool page it was saved from.
 */
export function savedViewConfigFromLocation(
  pathname: string,
  search: string,
  display: SavedViewDisplay,
): SavedViewConfig | null {
  const query = queryFromSearch(search)
  const projectId = projectIdFromPath(pathname)
  if (projectId !== null) return { scope: 'project', projectId, query, display }
  if (pathname === '/inbox' || pathname.startsWith('/inbox/')) {
    return { scope: 'inbox', projectId: null, query, display }
  }
  if (pathname === '/') return { scope: 'home', projectId: null, query, display }
  return null
}

/** Same scope, and for projects the same project id. */
export function savedViewMatchesConfig(
  view: { scope: SavedViewScope; projectId: string | null },
  config: Pick<SavedViewConfig, 'scope' | 'projectId'>,
): boolean {
  if (view.scope !== config.scope) return false
  if (config.scope === 'project') return view.projectId === config.projectId
  return view.projectId === null
}

/**
 * How a workflow view presents tasks: a board or a list. A view whose primary
 * component is not a task surface has no display preference, so a saved view
 * never replaces a workflow view with a different kind.
 */
export function projectViewDisplay(view: WorkflowView): SavedViewDisplay | null {
  const primary = view.components[0]
  if (!primary) return null
  const source = primary.source ?? 'tasks'
  if (source !== 'tasks') return null
  if (primary.type === 'kanban') return 'board'
  if (primary.type === 'table' || primary.type === 'task_list') return 'list'
  return null
}

/** The first declared view that presents tasks the requested way. */
export function findViewForDisplay(
  views: readonly WorkflowView[],
  display: SavedViewDisplay,
): WorkflowView | undefined {
  return views.find((view) => projectViewDisplay(view) === display)
}

/**
 * Client-side name check before sending. The server still owns the final rule;
 * an empty or whitespace-only name is never sent.
 */
export function savedViewNameError(name: string): string | null {
  return name.trim().length === 0 ? 'Give the saved view a name.' : null
}

/** A short description of the stored configuration, for the list. */
export function savedViewConfigLabel(config: Pick<SavedViewConfig, 'display' | 'query'>): string {
  const display = config.display === 'board' ? 'Board' : 'List'
  return config.query.length > 0 ? `${display} · ${config.query}` : `${display} · no extra filters`
}
