import { api } from '../client'
import type {
  SavedViewCommand,
  SavedViewListResponse,
  SavedViewResult,
} from '../contracts'

const BASE = '/authoritative'

/** Live views for the authenticated actor only; deleted rows are not returned. */
export function fetchSavedViews(signal?: AbortSignal): Promise<SavedViewListResponse> {
  return api.get<SavedViewListResponse>(`${BASE}/saved-views`, { signal })
}

export function sendSavedViewCommand(command: SavedViewCommand): Promise<SavedViewResult> {
  return api.post<SavedViewResult>(`${BASE}/saved-views/commands`, command)
}
