/**
 * Reserved tool ids and the tab list a workflow may contribute.
 *
 * The workspace reserves ids for its own tools. A workflow that declares one of
 * these must not shadow the tool: the declared view is not rendered and the page
 * says so, while the tool keeps its route.
 */
import type { Workflow, WorkflowView } from '@/api/adapters'

export const RESERVED_VIEW_IDS: ReadonlySet<string> = new Set(['planning', 'settings'])

export function isReservedViewId(id: string): boolean {
  return RESERVED_VIEW_IDS.has(id)
}

/**
 * `undefined` means no workflow could be resolved (use the fallback views);
 * an empty array means the pinned workflow truthfully declares no views.
 */
export function declaredWorkspaceViews(
  workflow: Workflow | undefined,
  fallback: readonly WorkflowView[],
): WorkflowView[] {
  return workflow ? workflow.views : [...fallback]
}

/** Declared views a person may navigate to: reserved tool ids are excluded. */
export function workspaceViewTabs(views: readonly WorkflowView[]): WorkflowView[] {
  return views.filter((view) => !isReservedViewId(view.id))
}

/** Declared views that collide with a tool id, so the page can say so. */
export function reservedDeclaredViews(views: readonly WorkflowView[]): WorkflowView[] {
  return views.filter((view) => isReservedViewId(view.id))
}
