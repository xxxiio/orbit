/**
 * The contract between a declared workflow view and its renderers.
 *
 * The page owns the snapshot it already fetched (`tasks`, `milestones`) and the
 * resolved workflow binding; a renderer receives exactly that and reaches for
 * its own server reads through hooks. Nothing is drilled through the page that
 * a renderer could ask the server for itself.
 */
import type { Milestone, Project, Task, Workflow, WorkflowView, WorkflowViewComponent } from '@/api/adapters'

export interface ViewRendererProps {
  view: WorkflowView
  component: WorkflowViewComponent
  project: Project
  /** Tasks in this project, straight from the workspace snapshot. */
  tasks: Task[]
  /** Milestones in this project, straight from the workspace snapshot. */
  milestones: Milestone[]
  /** The resolved, pinned workflow definition (undefined while loading or unbound). */
  workflow: Workflow | undefined
}
