/**
 * One read of the enrolled executors (C10), shaped for the launch surfaces.
 *
 * The query cache is shared, so the task drawer, the Agents page and the launch
 * dialog all see the same answer; this hook only adds the presentation state and
 * the sentence that explains why a launch control is off.
 */
import { useMemo } from 'react'
import { useAgentExecutors } from '@/hooks'
import { executorBlocker, executorReadState, type ExecutorReadState } from '../launch-model'

export interface ExecutorState {
  state: ExecutorReadState
  /** Named reason launching is disabled, or `null` when an executor is known. */
  blocker: string | null
  refetch: () => void
  fetching: boolean
}

export function useExecutorState(): ExecutorState {
  const query = useAgentExecutors()
  const state = useMemo(
    () =>
      executorReadState({
        pending: query.isPending,
        // A failed background refetch keeps the last good list on screen; only a
        // read with no data at all is presented as a failure.
        error: query.data ? null : query.error,
        executors: query.data,
      }),
    [query.isPending, query.data, query.error],
  )

  return {
    state,
    blocker: executorBlocker(state),
    refetch: () => void query.refetch(),
    fetching: query.isFetching,
  }
}
