/**
 * Capability actions: the status document, the offers for one item, and the
 * durable requests an executor claims and reports on.
 *
 * Two things are deliberately shaped here rather than in the components:
 *
 * - `claim_token` never reaches the UI. It is the executor's proof of claim; a
 *   browser holding it could pretend to be the runner, so the adapter drops it.
 * - An availability `code` is kept as the string the server sent. A newer server
 *   may add a code this build does not know, and inventing "unavailable" or
 *   folding it onto `not_authorized` would tell the person the wrong reason.
 */
import type {
  CapabilityActionContractRecord,
  CapabilityActionRequestRecord,
  CapabilityExecutorRecord,
  CapabilityOfferRecord,
  CapabilityReceiptRecord,
  CapabilityStatusRecord,
  CapabilityRequestState,
} from '../contracts'

export interface CapabilityExecutor {
  deviceId: string
  announcedAtMs: number
}

export interface CapabilityActionContract {
  id: string
  description: string
  inputSchema: Record<string, unknown>
  externalEffect: boolean
  requires: string[]
}

export interface CapabilityStatus {
  capabilityId: string
  name: string
  /** `read` | `mutate` | `external_effect`, kept verbatim. */
  permission: string
  requires: string[]
  installed: boolean
  granted: boolean
  executors: CapabilityExecutor[]
  actions: CapabilityActionContract[]
}

export interface CapabilityOffer {
  actionId: string
  capabilityId: string
  from: string
  to: string
  description: string
  inputSchema: Record<string, unknown>
  externalEffect: boolean
  available: boolean
  /** `null` when available, otherwise the server's code. */
  code: string | null
  /** `null` when available, otherwise the server's reason, verbatim. */
  reason: string | null
}

/** An item's offers, with the revision they were computed against. */
export interface CapabilityOffers {
  itemId: string
  itemRevision: number
  offers: CapabilityOffer[]
}

/** A state this build can describe; anything newer is kept as `unknown`. */
export type KnownCapabilityRequestState = CapabilityRequestState | 'unknown'

export interface CapabilityRequest {
  commandId: string
  itemId: string
  itemRevision: number
  actionId: string
  capabilityId: string
  from: string
  to: string
  state: KnownCapabilityRequestState
  /** The server's own token, for copy that must not guess. */
  recordedState: string
  inputs: Record<string, unknown>
  createdAtMs: number
  executorDeviceId: string | null
}

export interface CapabilityReceipt {
  outcome: string
  /** The stored detail document, verbatim: a JSON object or a string. */
  detail: unknown
  reportedAtMs: number
  reportedBy: string
}

export interface CapabilityRequestShow {
  request: CapabilityRequest
  receipt: CapabilityReceipt | null
}

const KNOWN_REQUEST_STATES: readonly KnownCapabilityRequestState[] = [
  'pending',
  'claimed',
  'succeeded',
  'failed',
  'uncertain',
]

export function toKnownRequestState(state: string): KnownCapabilityRequestState {
  return (KNOWN_REQUEST_STATES as readonly string[]).includes(state)
    ? (state as KnownCapabilityRequestState)
    : 'unknown'
}

function toCapabilityExecutor(executor: CapabilityExecutorRecord): CapabilityExecutor {
  return { deviceId: executor.device_id, announcedAtMs: executor.announced_at_ms }
}

function toActionContract(action: CapabilityActionContractRecord): CapabilityActionContract {
  return {
    id: action.id,
    description: action.description,
    inputSchema: action.input_schema,
    externalEffect: action.external_effect,
    requires: action.requires,
  }
}

export function toCapabilityStatus(status: CapabilityStatusRecord): CapabilityStatus {
  return {
    capabilityId: status.capability_id,
    name: status.name.trim() || status.capability_id,
    permission: status.permission,
    requires: status.requires,
    installed: status.installed,
    granted: status.granted,
    executors: status.executors.map(toCapabilityExecutor),
    actions: status.actions.map(toActionContract),
  }
}

export function toCapabilityOffer(offer: CapabilityOfferRecord): CapabilityOffer {
  return {
    actionId: offer.action_id,
    capabilityId: offer.capability_id,
    from: offer.from,
    to: offer.to,
    description: offer.description,
    inputSchema: offer.input_schema,
    externalEffect: offer.external_effect,
    available: offer.available,
    code: offer.code,
    reason: offer.reason,
  }
}

export function toCapabilityRequest(request: CapabilityActionRequestRecord): CapabilityRequest {
  return {
    commandId: request.command_id,
    itemId: request.item_id,
    itemRevision: request.item_revision,
    actionId: request.action_id,
    capabilityId: request.capability_id,
    from: request.from,
    to: request.to,
    state: toKnownRequestState(request.state),
    recordedState: request.state,
    inputs: request.inputs,
    createdAtMs: request.created_at_ms,
    executorDeviceId: request.executor_device_id,
  }
}

export function toCapabilityReceipt(receipt: CapabilityReceiptRecord): CapabilityReceipt {
  return {
    outcome: receipt.outcome,
    detail: receipt.detail,
    reportedAtMs: receipt.reported_at_ms,
    reportedBy: receipt.reported_by,
  }
}
