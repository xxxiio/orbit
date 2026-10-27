/**
 * Unified search rows as the UI shows them.
 *
 * The server owns grouping, ordering, matching and truncation; this module only
 * renames the wire fields and supplies the one label table for the three
 * groups. Anything the server did not send (a group, a count, a subtitle) is
 * never invented here.
 */
import type {
  SearchGroupRecord,
  SearchResponse,
  SearchResultKind,
  SearchResultRecord,
} from '../contracts'

export interface SearchResult {
  kind: SearchResultKind
  id: string
  title: string
  /** Server-composed plain text: a status, a `status · project` line, or a path. */
  subtitle: string | null
  projectId: string | null
  path: string | null
}

export interface SearchGroup {
  kind: SearchResultKind
  truncated: boolean
  results: SearchResult[]
}

export interface SearchView {
  revision: number
  query: string
  limit: number
  groups: SearchGroup[]
  /** Rows actually returned across all groups (not the match total). */
  total: number
}

const searchGroupLabels: Record<SearchResultKind, string> = {
  project: 'Projects',
  task: 'Tasks',
  note: 'Knowledge',
}

export function searchGroupLabel(kind: SearchResultKind): string {
  return searchGroupLabels[kind] ?? 'Results'
}

function toSearchResult(record: SearchResultRecord): SearchResult {
  return {
    kind: record.kind,
    id: record.id,
    title: record.title,
    subtitle: record.subtitle,
    projectId: record.project_id,
    path: record.path,
  }
}

function toSearchGroup(record: SearchGroupRecord): SearchGroup {
  return {
    kind: record.kind,
    truncated: record.truncated,
    results: record.results.map(toSearchResult),
  }
}

export function toSearchView(response: SearchResponse): SearchView {
  const groups = response.groups.map(toSearchGroup)
  return {
    revision: response.revision,
    query: response.query,
    limit: response.limit,
    groups,
    total: groups.reduce((sum, group) => sum + group.results.length, 0),
  }
}
