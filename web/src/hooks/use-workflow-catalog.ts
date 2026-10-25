/**
 * Read the manifests behind the workflows the server offers.
 *
 * The create-project picker needs one structural fact the version list does not
 * carry: whether a workflow is the ordinary task workflow (it declares no
 * work-item kinds), which is the one the server pins when the client sends no
 * choice. Names and descriptions come from the same manifests, so the picker
 * never hardcodes a workflow id or label.
 */
import { useQueries } from '@tanstack/react-query'
import { fetchWorkflow } from '@/api/endpoints/workflows'
import { queryKeys, type AvailableWorkflow } from '@/api/queries'

export interface WorkflowCatalogEntry {
  id: string
  version: number
  name: string | null
  description: string | null
  /** No work-item kinds: ordinary task work, the server's default choice. */
  taskOnly: boolean
  isPending: boolean
}

export interface WorkflowCatalog {
  entries: WorkflowCatalogEntry[]
  /** The task-only workflow, or `null` until its manifest is readable. */
  defaultWorkflow: WorkflowCatalogEntry | null
}

export function useWorkflowCatalog(
  available: AvailableWorkflow[] | undefined,
): WorkflowCatalog {
  const results = useQueries({
    queries: (available ?? []).map((workflow) => ({
      queryKey: queryKeys.workflow(workflow.id, workflow.version),
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        fetchWorkflow(workflow.id, workflow.version, signal),
      staleTime: 5 * 60_000,
    })),
  })

  const entries: WorkflowCatalogEntry[] = results.map((result, index) => {
    const workflow = (available ?? [])[index]
    const manifest = result.data
    return {
      id: workflow.id,
      version: workflow.version,
      name: manifest?.name ?? null,
      description: manifest?.description ?? null,
      taskOnly: manifest ? manifest.work_items.length === 0 : false,
      isPending: result.isPending,
    }
  })

  return {
    entries,
    defaultWorkflow: entries.find((entry) => entry.taskOnly && !entry.isPending) ?? null,
  }
}
