import { api } from '../client'
import { withQuery } from '../http'
import type { AuthoritySnapshotResponse, HistoryPrefixResponse } from '../contracts'

const BASE = '/authoritative'

/** The whole authoritative projection. One request; no pagination exists. */
export function fetchSnapshot(signal?: AbortSignal): Promise<AuthoritySnapshotResponse> {
  return api.get<AuthoritySnapshotResponse>(`${BASE}/snapshot`, { signal })
}

/** `revision` is required; the server returns the fingerprint for that revision. */
export function fetchHistoryPrefix(
  revision: number,
  signal?: AbortSignal,
): Promise<HistoryPrefixResponse> {
  return api.get<HistoryPrefixResponse>(withQuery(`${BASE}/history-prefix`, { revision }), { signal })
}
