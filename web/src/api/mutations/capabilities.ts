/**
 * Capability requests and operator reconciliations: the only two capability
 * writes a browser session is allowed to make.
 *
 * Both follow the house rules in `./core`: nothing is shown as recorded before
 * the server answered, the revision fence is read from the cache at mutation
 * time, the caller mints and keeps `command_id` for the whole attempt, and a
 * lost answer is `uncertain` rather than a silent second intent.
 *
 * Neither hook retries anything. A request is replayed only when a person sends
 * the same pinned command id again, and an `uncertain` outcome is settled only by
 * an explicit reconciliation — never by asking the server to run it again.
 */
import { queryKeys } from '../queries'
import { ClientStateError } from '../errors'
import type {
  CapabilityReconcileBody,
  CapabilityReconcileDecision,
  CapabilityRequestBody,
} from '../contracts'
import { reconcileCapabilityAction, requestCapabilityAction } from '../endpoints/capabilities'
import {
  toCapabilityReceipt,
  toCapabilityRequest,
  type CapabilityOffers,
  type CapabilityRequest,
  type CapabilityRequestShow,
} from '../adapters'
import { useApiMutation, type CommandMutation } from './core'

export interface CapabilityRequestVariables {
  /** Caller-minted idempotency key: the same attempt replays this exact intent. */
  commandId: string
  itemId: string
  /**
   * The revision whose offers the person was shown. The fence sent is the
   * offers' own `item_revision`, read below: asking for an action is consenting
   * to a specific offer, not to whatever the item looks like now.
   */
  itemRevision: number
  actionId: string
  inputs: Record<string, unknown>
  /** Required by the server for an effectful or destructive action. */
  confirm: boolean
}

export function useRequestCapabilityAction(): CommandMutation<
  CapabilityRequest,
  CapabilityRequestVariables
> {
  return useApiMutation<CapabilityRequest, CapabilityRequestVariables>({
    // The intent may be recorded even though the answer never arrived. Repeating
    // the same command id replays it, so this must not read as "not recorded".
    uncertainOnError: true,
    // An unavailable or stale answer means the durable facts moved; re-read the
    // offers and the history so the next attempt is fenced against them.
    invalidateOnRejection: true,
    mutationFn: async (
      { commandId, itemId, itemRevision, actionId, inputs, confirm },
      queryClient,
    ) => {
      const offers = queryClient.getQueryData<CapabilityOffers>(
        queryKeys.capabilityOffers(itemId, itemRevision),
      )
      if (!offers) {
        throw new ClientStateError(
          'Open this item’s actions again before requesting one: the server must re-check them first.',
        )
      }
      const body: CapabilityRequestBody = {
        command_id: commandId,
        item_id: itemId,
        expected_revision: offers.itemRevision,
        action_id: actionId,
        inputs,
        confirm,
      }
      const response = await requestCapabilityAction(body)
      return toCapabilityRequest(response.request)
    },
    affectedKeys: (variables) => [
      queryKeys.capabilityOffersAll(variables.itemId),
      queryKeys.capabilityRequests(variables.itemId),
      queryKeys.workItemsAll(),
    ],
  })
}

export interface CapabilityReconcileVariables {
  commandId: string
  itemId: string
  decision: CapabilityReconcileDecision
  /** Recorded verbatim; the server refuses more than 1000 characters. */
  detail: string
}

/**
 * Settle an `uncertain` intent with an explicit decision. It records what a
 * person decided, never a dispatch: the browser cannot run the action and the
 * server does not retry one.
 */
export function useReconcileCapabilityAction(): CommandMutation<
  CapabilityRequestShow,
  CapabilityReconcileVariables
> {
  return useApiMutation<CapabilityRequestShow, CapabilityReconcileVariables>({
    // The settlement is durable; a lost answer may already have recorded it.
    uncertainOnError: true,
    invalidateOnRejection: true,
    mutationFn: async ({ commandId, decision, detail }) => {
      const body: CapabilityReconcileBody = { command_id: commandId, decision, detail }
      const response = await reconcileCapabilityAction(body)
      return {
        request: toCapabilityRequest(response.request),
        receipt: response.receipt ? toCapabilityReceipt(response.receipt) : null,
      }
    },
    affectedKeys: (variables) => [
      queryKeys.capabilityRequest(variables.commandId),
      queryKeys.capabilityRequests(variables.itemId),
      queryKeys.workItemsAll(),
    ],
    onAccepted: (show, _variables, queryClient) => {
      // The server's answer is the new settled shape: store it instead of
      // refetching a receipt that cannot change on its own.
      queryClient.setQueryData(queryKeys.capabilityRequest(show.request.commandId), show)
    },
  })
}
