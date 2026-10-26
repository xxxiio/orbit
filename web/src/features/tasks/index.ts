/**
 * The shared task surface.
 *
 * `features/tasks` is the workspace kernel: Inbox, Projects, Home and the app
 * shell all render tasks through this barrel. Keep it explicit — add a name here
 * only when another module may depend on it.
 *
 * Not part of the surface: `compact-select` (row-private) and the drawer's
 * section modules under `task-drawer/` (composition detail).
 */
export { TaskTable, type TaskTableProps } from './task-table'
export { TaskRowMeta } from './task-row-meta'
export { ProjectTaskList } from './project-task-list'

export {
  formatDate,
  formatDateTime,
  formatDay,
  formatLongDay,
  formatMoment,
  formatMinutes,
  formatTimeOfDay,
  formatWeekday,
  freshnessLabel,
  isDueToday,
  isOverdue,
  isTerminalStatus,
  priorityLabels,
  priorityRank,
  priorityTones,
  projectLookup,
  readinessReasonLabel,
  relativeTime,
  sortByDeadline,
  statusDots,
  statusLabel,
  statusLabels,
  statusTones,
  TASK_PRIORITIES,
  TASK_STATUSES,
  taskTitlesById,
} from './task-meta'

export {
  NO_PRIORITY,
  priorityOptions,
  prioritySelectOptions,
  statusOptions,
  statusOptionsFor,
  TaskControlError,
  TaskPriorityControl,
  TaskProjectControl,
  TaskStatusControl,
} from './task-controls'

export {
  ALL,
  bulkActionLabel,
  bulkValueOptions,
  compareTasks,
  filterAndSortTasks,
  filterTasks,
  inboxBulkActionOptions,
  inboxSortKey,
  inboxSortValue,
  priorityFilterOptions,
  projectBulkActionOptions,
  projectSortKey,
  projectSortOptions,
  projectSortValue,
  sortOptions,
  statusFilterOptions,
} from './task-list-model'

export type {
  BulkAction,
  SelectOption,
  SortDirection,
  TaskListFilters,
  TaskSort,
  TaskSortKey,
} from './task-list-model'

export { ClearFiltersButton, TaskBulkBar, TaskFilterBar } from './task-filter-bar'
export { TaskDetailDrawer } from './task-drawer'
export { CreateTaskDialog } from './create-task-dialog'
export { useTaskBulkCommand } from './use-task-bulk-command'
export { useTaskCommandState } from './use-task-command-state'
export type { TaskCommandField, TaskCommandPending } from './use-task-command-state'
export { writeAcceptedTask } from './task-cache'
export { useTaskDrawer } from './use-task-drawer'
export { useTaskSelection } from './use-task-selection'
export {
  DEFAULT_PROJECT_COLUMNS,
  DEFAULT_TASK_SORT,
  LOCKED_TASK_COLUMNS,
  TASK_COLUMN_LABELS,
  columnsFromComponent,
  mapDeclaredColumn,
  readColumns,
  readTaskListUrl,
  windowTasks,
  writeTaskListUrl,
  type TaskColumnKey,
  type TaskListUrlPatch,
  type TaskListUrlState,
  type TaskWindow,
} from './task-list-state'
