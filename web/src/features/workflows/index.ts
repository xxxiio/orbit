/**
 * The workflow surface: view metadata, the component-type registry and the
 * milestone chrome a workflow view can offer.
 *
 * Projects compose workflows; workflows never import a project module. A view
 * is data and a component `type` is the only dispatch key — no screen may
 * branch on a workflow id or a view id.
 */
export {
  ViewSurface,
  isViewComponentImplemented,
  renderViewComponent,
  viewComponentRenderers,
} from './view-registry'
export {
  RESERVED_VIEW_IDS,
  aggregateMetric,
  componentFamily,
  componentTypeName,
  componentTypeNames,
  declaredWorkspaceViews,
  isReservedViewId,
  metricDeclaration,
  milestoneMatchesDeclaredFilter,
  numericFieldsForSource,
  projectMatchesDeclaredFilter,
  reservedDeclaredViews,
  taskMetricRows,
  taskSortDeclaration,
  unsupportedProjectFilterKeys,
  viewBadge,
  viewBadgeCount,
  viewHeaderAction,
  workspaceViewTabs,
  workItemMetricRows,
  type MetricAggregate,
  type MetricDeclaration,
  type TaskSortDeclaration,
  type ViewBadge,
  type ViewBadgeCountInput,
  type ViewComponentFamily,
  type ViewHeaderAction,
} from './view-metadata'
export type { ViewRendererProps, WorkItemsReadState } from './view-types'
export { MilestoneCreateDialog, NewMilestoneButton } from './milestone-create-dialog'
export { NewWorkItemButton, NewWorkItemDialog } from './work-item-create-dialog'
export { WorkItemSheet, type WorkItemSheetProps } from './work-item-sheet'
export { pinnedKindForView } from './work-item-model'
export { useWorkItemDrawer } from './use-work-item-drawer'
export {
  NoticeList,
  UpgradeDiff,
  UpgradeReceipt,
  noticeLabel,
} from './upgrade-preview'
