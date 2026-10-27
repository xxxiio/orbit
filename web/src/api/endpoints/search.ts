import { api } from '../client'
import { withQuery } from '../http'
import type { SearchResponse } from '../contracts'

const BASE = '/authoritative'

export interface SearchParams {
  /** Clamped by the server to 1..=20; the client sends what it means to show. */
  limit: number
  /** Narrows projects by id and tasks by project; notes are not narrowed in I1. */
  projectId?: string | null
}

/**
 * One server query across projects, tasks and notes. An empty query is a client
 * mistake: callers keep the read disabled instead of asking the server to
 * refuse it.
 */
export function searchOrbit(
  query: string,
  params: SearchParams,
  signal?: AbortSignal,
): Promise<SearchResponse> {
  return api.get<SearchResponse>(
    withQuery(`${BASE}/search`, {
      q: query,
      limit: params.limit,
      project_id: params.projectId ?? undefined,
    }),
    { signal },
  )
}
