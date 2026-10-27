/**
 * Capability reads and the two browser-legal writes.
 *
 * The browser may ask for status, offers, one request and the item's request
 * history, and it may persist an intent (`request`) or record an operator's
 * reconciliation (`reconcile`). It never calls `claim` or `result`: those are
 * executor routes, they require a bearer credential plus an enrolled device, and
 * reaching them from a cookie session would be an impersonation attempt, not a
 * shortcut.
 */
import { api } from '../client'
import { withQuery } from '../http'
import type {
  CapabilityListResponse,
  CapabilityOffersResponse,
  CapabilityReconcileBody,
  CapabilityRequestBody,
  CapabilityRequestResponse,
  CapabilityShowResponse,
  CapabilityStatusResponse,
} from '../contracts'

const BASE = '/authoritative/capabilities'
const ACTIONS = '/authoritative/capability-actions'

/** Every catalog capability with its durable install, grant and enrollment facts. */
export function fetchCapabilityStatus(
  projectId: string | undefined,
  signal?: AbortSignal,
): Promise<CapabilityStatusResponse> {
  return api.get<CapabilityStatusResponse>(withQuery(BASE, { project_id: projectId }), { signal })
}

/** The actions the bound workflow offers from the item's current state. */
export function fetchCapabilityOffers(
  itemId: string,
  signal?: AbortSignal,
): Promise<CapabilityOffersResponse> {
  return api.get<CapabilityOffersResponse>(withQuery(ACTIONS, { item_id: itemId }), { signal })
}

/** Persist a fenced intent. `command_id` is the caller's idempotency key. */
export function requestCapabilityAction(
  body: CapabilityRequestBody,
): Promise<CapabilityRequestResponse> {
  return api.post<CapabilityRequestResponse>(`${ACTIONS}/request`, body)
}

/** One intent and, once an executor reported, its receipt. */
export function fetchCapabilityRequest(
  commandId: string,
  signal?: AbortSignal,
): Promise<CapabilityShowResponse> {
  return api.get<CapabilityShowResponse>(withQuery(`${ACTIONS}/show`, { command_id: commandId }), {
    signal,
  })
}

/**
 * The item's durable intents, oldest first as the server orders them. `state`
 * is an exact match server-side; an unknown state simply matches nothing.
 */
export function fetchCapabilityRequests(
  params: { itemId?: string; state?: string },
  signal?: AbortSignal,
): Promise<CapabilityListResponse> {
  return api.get<CapabilityListResponse>(
    withQuery(`${ACTIONS}/list`, { item_id: params.itemId, state: params.state }),
    { signal },
  )
}

/**
 * Settle a claimed or uncertain intent under this device's identity. Refuses
 * `pending` (nothing ran yet) with 409. It never dispatches anything.
 */
export function reconcileCapabilityAction(
  body: CapabilityReconcileBody,
): Promise<CapabilityShowResponse> {
  return api.post<CapabilityShowResponse>(`${ACTIONS}/reconcile`, body)
}
