/**
 * The read guard and row pipeline every resource surface shares.
 *
 * A list, table, board or metric panel over server data does the same four
 * things before it draws a row: wait for the workflow, wait for the resource
 * read, refuse a read that failed, then filter → sort → limit. Keeping that
 * here means a new source (knowledge notes, agent jobs) supplies a matcher and
 * a renderer, not a fourth copy of the guard or the pipeline.
 */
import type { ReactNode } from 'react'
import type { Workflow, WorkflowViewComponent } from '@/api/adapters'
import type { ApiError } from '@/api/errors'
import { Button } from '@/components/ui/button'
import { SkeletonText } from '@/components/ui/skeleton'
import { sortByDescriptor, type SortDeclaration } from '@/lib/declarations'
import {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  UnavailablePanel,
} from './panel'

export interface ResourceRows<T> {
  /** Rows that matched the declared filter, before sorting. */
  matched: T[]
  /** The matched rows in the declared order. */
  sorted: T[]
  /** The rows the surface renders: `sorted` cut to the declared limit. */
  limited: T[]
}

/**
 * filter → sort → limit. A declaration with no comparator keeps the server's
 * order, and a limit only cuts when the workflow declared a positive one.
 */
export function filterSortLimit<T>(
  rows: readonly T[],
  {
    matches,
    sort,
    limit,
  }: {
    matches?: (row: T) => boolean
    sort?: SortDeclaration<T> | null
    limit?: number | null
  },
): ResourceRows<T> {
  const matched = matches ? rows.filter(matches) : [...rows]
  const sorted = sort ? sortByDescriptor(matched, sort) : matched
  const limited = limit && limit > 0 ? sorted.slice(0, limit) : sorted
  return { matched, sorted, limited }
}

/** The page-owned read state of one resource, as a renderer receives it. */
export interface ResourceReadState {
  isPending: boolean
  error: ApiError | null
  retrying: boolean
  refetch: () => void
}

/**
 * The guard a surface renders instead of its body while the workflow or the
 * resource read is missing. Returns `null` when the body may draw.
 */
export function ResourceReadPrelude({
  title,
  label,
  count,
  workflow,
  workflowLoading,
  workflowRetry,
  state,
}: {
  /** The panel's title, already resolved to copy. */
  title: string
  /** The resource's plural name, used in the read-error sentence. */
  label: string
  /** How many rows the surface already holds. */
  count: number
  workflow: Workflow | undefined
  workflowLoading?: boolean
  workflowRetry?: () => void
  state: ResourceReadState | undefined
}): ReactNode {
  if (workflowLoading && !workflow) {
    return (
      <ComponentPanel title={title}>
        <div aria-busy="true">
          <SkeletonText lines={2} />
        </div>
      </ComponentPanel>
    )
  }
  if (!workflow) {
    return (
      <UnavailablePanel
        title={title}
        reason="The project's pinned workflow could not be read, so its work-item kinds, states and fields are unknown."
        hint="Nothing is guessed: the panel returns when the workflow is readable."
        action={
          <Button
            variant="secondary"
            size="sm"
            onClick={workflowRetry ?? state?.refetch}
            disabled={!workflowRetry && !state}
          >
            Retry the workflow
          </Button>
        }
      />
    )
  }
  if (state?.error) {
    return (
      <ComponentPanel title={title}>
        <InlineReadError
          label={label}
          error={state.error}
          onRetry={state.refetch}
          retrying={state.retrying}
        />
      </ComponentPanel>
    )
  }
  if (state?.isPending && count === 0) {
    return (
      <ComponentPanel title={title}>
        <div aria-busy="true">
          <SkeletonText lines={3} />
        </div>
      </ComponentPanel>
    )
  }
  return null
}

/** The stacked provenance lines a surface shows under its body. */
export function SurfaceFooter({ children }: { children: ReactNode }) {
  return <div className="flex flex-col gap-1">{children}</div>
}

/** The workflow's declared actions, when it declared any. */
export function DeclaredActionsFooter({ component }: { component: WorkflowViewComponent }) {
  if (component.actions.length === 0) return null
  return (
    <ProvenanceNote>
      <DeclaredActionsNote component={component} />
    </ProvenanceNote>
  )
}
