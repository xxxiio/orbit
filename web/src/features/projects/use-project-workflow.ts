import type { ApiError } from '@/api/errors'
import type { Workflow, WorkflowBinding } from '@/api/adapters'
import { useProjectBinding, useWorkflowDefinition } from '@/hooks'

export interface ProjectWorkflowState {
  binding: WorkflowBinding | null
  workflow: Workflow | undefined
  /** "Basic v1" — the pinned name and version, never a guess. */
  label: string | null
  loading: boolean
  error: ApiError | null
  refetch: () => void
}

/** The project's pinned workflow, resolved through its binding. */
export function useProjectWorkflow(projectId: string | undefined): ProjectWorkflowState {
  const binding = useProjectBinding(projectId)
  const definition = useWorkflowDefinition(binding.data?.workflowId, binding.data?.version)

  const label = definition.data
    ? `${definition.data.name} v${definition.data.version}`
    : binding.data
      ? `${binding.data.workflowId} v${binding.data.version}`
      : null

  return {
    binding: binding.data ?? null,
    workflow: definition.data,
    label,
    loading: binding.isPending || (Boolean(binding.data) && definition.isPending),
    error: binding.error ?? definition.error ?? null,
    refetch: () => {
      void binding.refetch()
      void definition.refetch()
    },
  }
}
