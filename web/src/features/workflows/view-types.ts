/**
 * The contract between a declared workflow view and its renderers.
 *
 * The page owns the snapshot it already fetched (`tasks`, `milestones`) and the
 * resolved workflow binding; a renderer receives exactly that and reaches for
 * its own server reads through hooks. Nothing is drilled through the page that
 * a renderer could ask the server for itself.
 *
 * The contract is source-driven: a component declares `source`, and `workItems`
 * is already part of the shape so I3 plugs a work-item renderer into the same
 * pipeline instead of adding a fourth copy of it. A renderer that has no read
 * for a source still refuses it by name (see `UnavailablePanel`).
 */
import type {
  Milestone,
  Project,
  Task,
  WorkItem,
  Workflow,
  WorkflowView,
  WorkflowViewComponent,
} from '@/api/adapters'
import type { ApiError } from '@/api/errors'

/**
 * The page-owned read state for `workItems`. A renderer must not open its own
 * read for a source the page already fetched: an empty array means "the server
 * returned no items", while `isPending` means "we do not know yet". Rendering
 * the first as the second is how an empty board starts looking real.
 */
export interface WorkItemsReadState {
  isPending: boolean
  error: ApiError | null
  retrying: boolean
  refetch: () => void
}

export interface ViewRendererProps {
  view: WorkflowView
  component: WorkflowViewComponent
  project: Project
  /** Tasks in this project, straight from the workspace snapshot. */
  tasks: Task[]
  /** Milestones in this project, straight from the workspace snapshot. */
  milestones: Milestone[]
  /**
   * Work items for this project, read once by the workspace page. A renderer
   * over `source: 'work_items'` reads them from here, not from a parallel read
   * of its own.
   */
  workItems: WorkItem[]
  /** Whether the page's work-item read is still running, failed, or ready. */
  workItemsState?: WorkItemsReadState
  /** The resolved, pinned workflow definition (undefined while loading or unbound). */
  workflow: Workflow | undefined
  /** True while the pinned workflow is still being read. */
  workflowLoading?: boolean
  /** Re-reads the pinned workflow; the retry target for a missing manifest. */
  workflowRetry?: () => void
}
