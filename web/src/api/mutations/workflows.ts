/**
 * Declared-workflow commands: upgrade, create, transition and properties.
 *
 * Work-item writes are serialised per item and fence on the item's cached
 * revision, because two commands sent from the same render would otherwise read
 * the same revision and the second would fence on one the first is about to
 * replace.
 */
import { useRef, useState } from 'react'
import { queryKeys } from '../queries'
import { ClientStateError, isAmbiguousError, isApiError } from '../errors'
import type {
  ApplyUpgradeBody,
  BlueprintUpgradePreview,
  CreateWorkItemBody,
  WorkItemPropertiesBody,
  WorkItemTransitionBody,
} from '../contracts'
import {
  applyUpgrade,
  createWorkItem,
  fetchWorkItem,
  transitionWorkItem,
  updateWorkItemProperties,
} from '../endpoints/workflows'
import {
  toWorkItem,
  toWorkflowUpgradeApplied,
  type WorkItem,
  type WorkflowUpgradeApplied,
} from '../adapters'
import { acceptWorkItemRecord, workItemRevision } from '../accepted-write'
import { useApiMutation, type CommandMutation } from './core'

/**
 * Confirm an upgrade. The fingerprint is read from the freshest cached preview
 * at mutation time; without a preview nothing is sent, because applying a
 * version the person has not been shown is exactly what the server forbids.
 */
export function useApplyUpgrade(): CommandMutation<
  WorkflowUpgradeApplied,
  { projectId: string; workflowId: string; version: number }
> {
  return useApiMutation<
    WorkflowUpgradeApplied,
    { projectId: string; workflowId: string; version: number }
  >({
    // The version may have been applied even if the response was lost.
    uncertainOnError: true,
    mutationFn: ({ projectId, workflowId, version }, queryClient) => {
      const preview = queryClient.getQueryData<BlueprintUpgradePreview>(
        queryKeys.upgradePreview(projectId, workflowId, version),
      )
      if (!preview) {
        throw new ClientStateError('Open the upgrade preview before applying it.')
      }
      const body: ApplyUpgradeBody = {
        project_id: projectId,
        id: workflowId,
        version,
        confirm: true,
        fingerprint: preview.fingerprint,
      }
      return applyUpgrade(body).then(toWorkflowUpgradeApplied)
    },
    affectedKeys: (variables) => [
      queryKeys.upgradePreview(variables.projectId, variables.workflowId, variables.version),
      queryKeys.upgradePreviewAll(),
      queryKeys.workflowBinding(variables.projectId),
      queryKeys.workItems(variables.projectId),
    ],
  })
}

/**
 * Everything the server needs to create one work item, including both ids.
 *
 * The item create route takes no command envelope (the receipt-bearing
 * `/blueprints/commands` route is a separate contract), so the caller-chosen
 * item `id` is the create's idempotency key: a retry of the same intent repeats
 * it and the server refuses to store a second row under it. `commandId` names
 * the same attempt for the caller's own bookkeeping and is part of the
 * variables for the same reason as in `useCreateTask` — both ids are minted
 * when the person acts, never inside `mutationFn`, because minting them there
 * made every retry a fresh create.
 */
export interface CreateWorkItemVariables {
  commandId: string
  id: string
  projectId: string
  kind: string
  properties?: Record<string, unknown>
}

/**
 * One in-flight command per work item. Two commands sent from the same stale
 * render would otherwise read the same revision and the second would fence on a
 * revision the first is about to replace. Waiting for the previous write also
 * lets the second read the revision the accepted response wrote back.
 *
 * The chain is module-level because a sheet's transition and property writes are
 * separate hook instances that still address the same item.
 */
const workItemWriteChains = new Map<string, Promise<void>>()

function serializeWorkItemWrite<T>(id: string, write: () => Promise<T>): Promise<T> {
  const previous = workItemWriteChains.get(id) ?? Promise.resolve()
  const result = previous.then(write, write)
  const tail = result.then(
    () => undefined,
    () => undefined,
  )
  workItemWriteChains.set(id, tail)
  void tail.then(() => {
    if (workItemWriteChains.get(id) === tail) workItemWriteChains.delete(id)
  })
  return result
}

export function useCreateWorkItem(): CommandMutation<WorkItem, CreateWorkItemVariables> & {
  /**
   * The item id whose first attempt may have been stored but has never been
   * confirmed. A caller must keep using it while this is set, even if the last
   * answer was a refusal: a duplicate refusal followed by a 404 read proves the
   * id was not stored *yet*, not that the earlier attempt is safe to abandon.
   */
  pinnedId: string | null
} {
  const uncertainCreates = useRef(new Set<string>())
  const [pinnedId, setPinnedId] = useState<string | null>(null)
  const mutation = useApiMutation<WorkItem, CreateWorkItemVariables>({
    // The item may exist even when the response never arrived.
    uncertainOnError: true,
    mutationFn: async ({ id, projectId, kind, properties }) => {
      const body: CreateWorkItemBody = {
        id,
        project_id: projectId,
        kind,
        ...(properties ? { properties } : {}),
      }
      try {
        const item = await createWorkItem(body).then(toWorkItem)
        uncertainCreates.current.delete(id)
        setPinnedId((current) => (current === id ? null : current))
        return item
      } catch (error) {
        if (isAmbiguousError(error)) {
          // Keep the attempt pinned: a changed payload must not mint a new id
          // while this one may already be stored.
          uncertainCreates.current.add(id)
          setPinnedId(id)
          throw error
        }
        if (uncertainCreates.current.has(id)) {
          // The retry repeats the id, so a stored first attempt answers with a
          // duplicate-insert refusal. Ask the server which fact is true.
          try {
            const existing = await fetchWorkItem(id)
            uncertainCreates.current.delete(id)
            setPinnedId((current) => (current === id ? null : current))
            return toWorkItem(existing)
          } catch (readError) {
            if (!isApiError(readError) || readError.status !== 404) {
              // The duplicate check itself failed. Say so: reporting the
              // create's outcome would guess, and swallowing this would hide a
              // real read failure behind the refusal below.
              throw readError
            }
          }
          // The server refused the insert and does not hold the id. The earlier
          // attempt is still the only safe retry, so keep it pinned — but the
          // answer we did get was the server's own refusal, not an unknown
          // outcome, so report that instead of a synthetic "unconfirmed".
          throw error
        }
        // A definitive refusal before any attempt could have been stored: this
        // id is not the pinned one, so a later payload may mint a fresh id.
        setPinnedId((current) => (current === id ? null : current))
        throw error
      }
    },
    affectedKeys: (variables) => [
      queryKeys.workItems(variables.projectId),
      queryKeys.upgradePreviewAll(),
    ],
    onAccepted: (item, _variables, queryClient) => {
      acceptWorkItemRecord(queryClient, item)
    },
  })

  return { ...mutation, pinnedId }
}

/**
 * Transition legality, guards and capabilities are decided by the server. The
 * accepted record is written back before the invalidation, so a second move
 * made before the list refetch settles fences on the revision the server just
 * reported instead of the pre-move one.
 */
export function useWorkItemTransition(): CommandMutation<WorkItem, { id: string; to: string }> {
  return useApiMutation<WorkItem, { id: string; to: string }>({
    uncertainOnError: true,
    invalidateOnRejection: true,
    mutationFn: ({ id, to }, queryClient) =>
      serializeWorkItemWrite(id, async () => {
        const body: WorkItemTransitionBody = {
          id,
          to,
          expected_revision: workItemRevision(queryClient, id),
        }
        const item = await transitionWorkItem(body).then(toWorkItem)
        // Write the accepted record back before the next queued command reads
        // its fence, not after the mutation settles.
        acceptWorkItemRecord(queryClient, item)
        return item
      }),
    affectedKeys: () => [queryKeys.workItemsAll(), queryKeys.upgradePreviewAll()],
    onAccepted: (item, _variables, queryClient) => {
      acceptWorkItemRecord(queryClient, item)
    },
  })
}

export function useUpdateWorkItemProperties(): CommandMutation<
  WorkItem,
  { id: string; changes: Record<string, unknown> }
> {
  return useApiMutation<WorkItem, { id: string; changes: Record<string, unknown> }>({
    uncertainOnError: true,
    invalidateOnRejection: true,
    mutationFn: ({ id, changes }, queryClient) =>
      serializeWorkItemWrite(id, async () => {
        const body: WorkItemPropertiesBody = {
          id,
          expected_revision: workItemRevision(queryClient, id),
          changes,
        }
        const item = await updateWorkItemProperties(body).then(toWorkItem)
        acceptWorkItemRecord(queryClient, item)
        return item
      }),
    affectedKeys: () => [queryKeys.workItemsAll(), queryKeys.upgradePreviewAll()],
    onAccepted: (item, _variables, queryClient) => {
      acceptWorkItemRecord(queryClient, item)
    },
  })
}
