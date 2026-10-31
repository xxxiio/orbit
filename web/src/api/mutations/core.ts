/**
 * The mutation frame every command hook shares.
 *
 * Four rules are enforced here, once:
 *
 * 1. **Nothing is shown as saved before the server answered.** There is no
 *    optimistic update and no rollback theatre: `disposition` only becomes
 *    `accepted` after a response.
 * 2. **Revision fences are read at mutation time.** `expected_revision` comes
 *    from the query cache inside `mutationFn`, so a snapshot refresh between
 *    render and click cannot produce a stale fence.
 * 3. **Idempotency keys are allocated by the caller.** `command_id` (and the
 *    entity id for creates) travel in the mutation variables, minted when the
 *    person acts, so pressing the same button again after an unconfirmed
 *    failure replays the same receipt instead of creating a second entity.
 * 4. **Failures are honest.** A rejected command is `rejected`; a write whose
 *    outcome the server may have applied anyway (lost response, 5xx, knowledge
 *    filesystem publication) is `uncertain`, never silently retried.
 *
 * The command families live beside this module, one per domain
 * (`tasks`, `projects`, `milestones`, `workflows`, `attachments`, `knowledge`,
 * `agents`, `planning`); `mutations.ts` re-exports them all for one iteration.
 */
import { useMutation, useQueryClient, type QueryClient, type QueryKey } from '@tanstack/react-query'
import { ApiError, ClientStateError, classifyError, errorInfo, isAmbiguousError } from '../errors'
import { handleUnauthorized } from '../session-store'
import { queryKeys } from '../queries'
import type { AuthoritySnapshotResponse, MilestoneSnapshot } from '../contracts'

/**
 * `idle`      nothing sent yet
 * `pending`   in flight; show a spinner, not a checkmark
 * `accepted`  the server accepted and stored the change
 * `rejected`  the server refused; nothing was written
 * `uncertain` the request failed in a way that may still have applied
 */
export type MutationDisposition = 'idle' | 'pending' | 'accepted' | 'rejected' | 'uncertain'

export interface CommandMutation<TData, TVariables> {
  mutate: (variables: TVariables) => void
  mutateAsync: (variables: TVariables) => Promise<TData>
  isPending: boolean
  error: ApiError | null
  disposition: MutationDisposition
  data: TData | undefined
  /** The variables of the latest attempt; useful for per-row pending states. */
  variables: TVariables | undefined
  /** Present for a 429 when the response body carries a retry window. */
  retryAfterSeconds: number | null
  reset: () => void
}

interface ApiMutationOptions<TData, TVariables> {
  mutationFn: (variables: TVariables, queryClient: QueryClient) => Promise<TData>
  /** Invalidated on acceptance, and refetched on a conflict. */
  affectedKeys?: (variables: TVariables) => readonly QueryKey[]
  /** The server can apply the write and still return an error (imports, knowledge). */
  uncertainOnError?: boolean
  /**
   * Invalidate `affectedKeys` on a definitive refusal too. Ledger writes that
   * fence on a revision the client may already have outgrown (work items answer
   * a stale fence with 400, not 409) must refresh on every rejected write, not
   * only on a conflict.
   */
  invalidateOnRejection?: boolean
  onAccepted?: (data: TData, variables: TVariables, queryClient: QueryClient) => void
}

/** Errors whose meaning is "we do not know whether the write landed". */
function dispositionOf(
  error: ApiError | null,
  isPending: boolean,
  isSuccess: boolean,
  uncertainOnError: boolean,
): MutationDisposition {
  if (isPending) return 'pending'
  if (isSuccess) return 'accepted'
  if (!error) return 'idle'
  if (uncertainOnError && isAmbiguousError(error)) return 'uncertain'
  return 'rejected'
}

export function useApiMutation<TData, TVariables>({
  mutationFn,
  affectedKeys,
  uncertainOnError = false,
  invalidateOnRejection = false,
  onAccepted,
}: ApiMutationOptions<TData, TVariables>): CommandMutation<TData, TVariables> {
  const queryClient = useQueryClient()

  const invalidate = (keys: readonly QueryKey[] | undefined) => {
    for (const queryKey of keys ?? []) {
      void queryClient.invalidateQueries({ queryKey })
    }
  }

  const mutation = useMutation<TData, ApiError, TVariables>({
    mutationFn: async (variables) => {
      try {
        return await mutationFn(variables, queryClient)
      } catch (error) {
        if (error instanceof ApiError) throw error
        throw new ApiError(0, 'Could not reach the Orbit server.', error)
      }
    },
    onSuccess: (data, variables) => {
      onAccepted?.(data, variables, queryClient)
      invalidate(affectedKeys?.(variables))
    },
    onError: (error, variables) => {
      const kind = classifyError(error)
      if (kind === 'unauthorized') {
        handleUnauthorized(queryClient)
        return
      }
      if (kind === 'conflict' || invalidateOnRejection) {
        // The server's message explains the conflict; refetch so the next
        // attempt is fenced against the version that actually won. A stale
        // work-item revision is reported as 400, so those writes opt into the
        // same refresh on any definitive refusal.
        invalidate(affectedKeys?.(variables))
      }
    },
  })

  return {
    mutate: mutation.mutate,
    mutateAsync: mutation.mutateAsync,
    isPending: mutation.isPending,
    error: mutation.error ?? null,
    disposition: dispositionOf(mutation.error ?? null, mutation.isPending, mutation.isSuccess, uncertainOnError),
    data: mutation.data,
    variables: mutation.variables,
    retryAfterSeconds: mutation.error ? errorInfo(mutation.error).retryAfterSeconds : null,
    reset: mutation.reset,
  }
}

/**
 * Cache coherence lives in `./accepted-write`: the revision guards, the task
 * record write-back and the work-item record write-back are one helper family
 * shared by every command hook. Re-exported here because this module is the
 * command surface `@/hooks` publishes.
 */
export {
  acceptLedgerRevision,
  acceptProjectRecord,
  acceptRevision,
  acceptSavedView,
  acceptTaskRecord,
  acceptWorkItemRecord,
  removeSavedView,
  workItemRecord,
  workItemRevision,
} from '../accepted-write'

/**
 * The freshest revision this browser knows about. Read inside `mutationFn`,
 * never captured at mount: a snapshot refresh between render and click must
 * fence the command against the new revision.
 */
export function snapshotRevision(queryClient: QueryClient): number {
  const snapshot = queryClient.getQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot())
  if (!snapshot) {
    throw new ClientStateError('The workspace is still loading. Try again in a moment.')
  }
  return snapshot.revision
}

/**
 * The milestones snapshot carries the same global ledger revision as the task
 * snapshot (both project `MAX(cursor)`); the cached *shape* differs, not the
 * fence. Read it at mutation time so a refreshed milestone list always wins.
 */
export function milestoneRevision(queryClient: QueryClient): number {
  const snapshot = queryClient.getQueryData<MilestoneSnapshot>(queryKeys.milestones())
  if (!snapshot) {
    throw new ClientStateError('The milestone list is still loading. Try again in a moment.')
  }
  return snapshot.revision
}
