/**
 * Write an accepted work-item response back into the project's item list.
 *
 * Transitions and property writes fence on the item's own `revision`, read from
 * the cached list at mutation time. Without this write-back, a second change
 * made between "server accepted" and "list refetch settled" would send the
 * pre-write revision and be refused 409 for no reason. This is cache
 * coherence, not an optimistic update: it runs only after an accepted response
 * and stores exactly the record the server returned.
 */
import type { QueryClient } from '@tanstack/react-query'
import type { BlueprintWorkItemRecord } from '@/api/contracts'
import type { WorkItem } from '@/api/adapters'
import { queryKeys } from '@/hooks'

export function workItemRecord(item: WorkItem): BlueprintWorkItemRecord {
  return {
    id: item.id,
    project_id: item.projectId,
    kind: item.kind,
    state: item.state,
    properties: item.properties,
    revision: item.revision,
  }
}

export function writeAcceptedWorkItem(queryClient: QueryClient, item: WorkItem): void {
  const record = workItemRecord(item)
  const entries = queryClient.getQueriesData<BlueprintWorkItemRecord[]>({
    queryKey: queryKeys.workItemsAll(),
  })
  for (const [key, items] of entries) {
    if (!items) continue
    const index = items.findIndex((candidate) => candidate.id === item.id)
    if (index >= 0) {
      const next = [...items]
      next[index] = record
      queryClient.setQueryData(key, next)
      continue
    }
    // A create accepted before the list's first refetch: append to *this*
    // project's list only (the key is `['work-items', projectId]`).
    if (key[1] === item.projectId) {
      queryClient.setQueryData(key, [...items, record])
    }
  }
}
