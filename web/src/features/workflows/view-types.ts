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

export interface ViewRendererProps {
  view: WorkflowView
  component: WorkflowViewComponent
  project: Project
  /** Tasks in this project, straight from the workspace snapshot. */
  tasks: Task[]
  /** Milestones in this project, straight from the workspace snapshot. */
  milestones: Milestone[]
  /**
   * Work items for this project. Empty until I3 adds the workspace read; a
   * renderer over `source: 'work_items'` reads them from here, not from a
   * parallel prop of its own.
   */
  workItems: WorkItem[]
  /** The resolved, pinned workflow definition (undefined while loading or unbound). */
  workflow: Workflow | undefined
}
