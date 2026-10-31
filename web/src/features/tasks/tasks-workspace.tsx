import { useMemo } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { Milestone, Project, Task, WorkflowView, WorkflowViewComponent } from '@/api/adapters'
import { Button } from '@/components/ui/button'
import { PlusIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { UnavailablePanel } from '@/components/surface'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { SavedViewsButton } from '@/features/saved-views'
import {
  actionableFilterRefusal,
  componentTypeName,
  emptyTextOption,
  taskMatchesDeclaredFilter,
  taskSortDeclaration,
  unsupportedTaskFilterKeys,
} from '@/features/workflows/declarations'
import { BacklogView } from '@/features/backlog/backlog-view'
import {
  DEFAULT_PROJECT_COLUMNS,
  DEFAULT_TASK_SORT,
  TASKS_VIEW_OPTIONS,
  columnsFromComponent,
  readTaskListUrl,
  readTasksView,
  writeTaskListUrl,
  writeTasksView,
  type TasksView,
} from './task-list-state'
import {
  ALL,
  projectSortKey,
  projectSortOptions,
  projectSortValue,
  type SelectOption,
} from './task-list-model'
import { TaskFilterBar } from './task-filter-bar'
import { TaskBoard } from './task-board'
import { ProjectTaskList } from './project-task-list'

export interface TaskListDeclaration {
  /** The keys the list applies after the person's own filters. */
  filter: Record<string, string>
  /** A declaration this list cannot honour, named instead of ignored. */
  refusal: { reason: string; hint?: string } | null
}

/**
 * The declared `tasks` view configures the canonical Tasks list. Supported
 * filter keys narrow the rows with the shared task matcher; a key this build
 * cannot evaluate — including `actionable`, which the list does not read — is
 * refused by name rather than silently dropped (which would over-count).
 */
export function taskListDeclaration(component?: WorkflowViewComponent): TaskListDeclaration {
  const filter = component?.filter ?? {}
  const unsupported = unsupportedTaskFilterKeys(filter)
  if (unsupported.length > 0) {
    return {
      filter,
      refusal: {
        reason: `This list filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`,
      },
    }
  }
  const actionableRefusal = actionableFilterRefusal(filter)
  if (actionableRefusal) {
    return {
      filter,
      refusal: {
        reason: actionableRefusal,
        hint: 'The list is refused rather than showing the complement of the filter the workflow declared.',
      },
    }
  }
  // The list does not read the readiness report, so even a valid `actionable`
  // would select nothing. Refuse it and point at the board, which does read it.
  if (filter.actionable !== undefined) {
    return {
      filter,
      refusal: {
        reason: `This list filters tasks by actionable = “${filter.actionable}”, which the list does not read.`,
        hint: 'Switch to the Board view to apply an actionable filter; the list is refused rather than showing an empty set.',
      },
    }
  }
  return { filter, refusal: null }
}

/**
 * One Tasks tab, three presentations.
 *
 * The tab owns the control bar (search, status, priority, milestone, sort,
 * saved views, create) and the view switcher. Filters are URL state shared by
 * List, Board and Backlog, and all three render the same `tasks` array the page
 * already read — switching a view never opens a second task query or remounts
 * the workspace.
 */
export function TasksWorkspace({
  project,
  tasks,
  milestones,
  listView,
  boardView,
  showSavedViews = true,
}: {
  project: Project
  /** The project's tasks from the page's single `useProjectTasks` read. */
  tasks: Task[]
  milestones: Milestone[]
  /** A workflow-declared `tasks` view over tasks, if any. */
  listView?: WorkflowView
  /** A workflow-declared `board` view over tasks, if any. */
  boardView?: WorkflowView
  showSavedViews?: boolean
}) {
  const [searchParams, setSearchParams] = useSearchParams()
  const { openNewTask } = useWorkspaceActions()

  const view = readTasksView(searchParams)
  const listComponent = listView?.components[0]
  const boardComponent = boardView?.components[0]
  const activeComponent = view === 'board' ? boardComponent : listComponent
  const defaultSort = (activeComponent ? taskSortDeclaration(activeComponent).sort : null) ?? DEFAULT_TASK_SORT
  const defaultGroup = view === 'backlog' ? 'milestone' : 'none'
  const state = readTaskListUrl(searchParams, DEFAULT_PROJECT_COLUMNS, defaultSort, defaultGroup)
  const sortValue = projectSortValue(state.sort)
  const listDeclaration = taskListDeclaration(listComponent)
  const listFilter = useMemo(
    () =>
      listComponent && Object.keys(listComponent.filter ?? {}).length > 0
        ? (task: Task) => taskMatchesDeclaredFilter(task, listComponent.filter ?? {})
        : undefined,
    [listComponent],
  )

  const commit = (patch: Parameters<typeof writeTaskListUrl>[1]) => {
    setSearchParams((current) => writeTaskListUrl(current, patch, defaultSort), { replace: true })
  }

  const switchView = (next: TasksView) => {
    setSearchParams((current) => writeTasksView(current, next), { replace: true })
  }

  const milestoneOptions = useMemo<SelectOption[]>(
    () => [
      { value: ALL, label: 'Any milestone' },
      { value: 'none', label: 'No milestone' },
      ...milestones.map((milestone) => ({ value: milestone.id, label: milestone.name })),
    ],
    [milestones],
  )

  const listColumns = columnsFromComponent(listComponent)
  const listRowLimit = listComponent?.limit ?? undefined
  const listEmptyText = listComponent ? emptyTextOption(listComponent) ?? undefined : undefined
  const boardEmptyText = boardComponent ? emptyTextOption(boardComponent) ?? undefined : undefined

  return (
    <div className="flex flex-col gap-4">
      <section
        aria-label="Task controls"
        className="flex flex-col gap-3 rounded-lg border border-line bg-surface px-3 py-3 sm:px-4"
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <ViewSwitcher value={view} onChange={switchView} />
          <div className="flex flex-wrap items-center gap-2">
            {showSavedViews ? (
              <SavedViewsButton display={view === 'board' ? 'board' : 'list'} />
            ) : null}
            <Button
              size="sm"
              leadingIcon={<PlusIcon size={15} />}
              onClick={() => openNewTask(project.id)}
            >
              New task
            </Button>
          </div>
        </div>

        <TaskFilterBar
          search={state.search}
          onSearchChange={(value) => commit({ search: value, page: 1 })}
          statusFilter={state.status}
          onStatusFilterChange={(value) => commit({ status: value, page: 1 })}
          priorityFilter={state.priority}
          onPriorityFilterChange={(value) => commit({ priority: value, page: 1 })}
          milestoneFilter={state.milestone}
          onMilestoneFilterChange={(value) => commit({ milestone: value, page: 1 })}
          milestoneOptions={milestoneOptions}
          sortValue={sortValue}
          onSortValueChange={(value) =>
            commit({
              sort: {
                key: projectSortKey(value),
                direction: state.sort.direction,
              },
              page: 1,
            })
          }
          sortOptions={projectSortOptions}
          groupValue={view === 'board' ? undefined : state.group}
          onGroupValueChange={
            view === 'board' ? undefined : (group) => commit({ group, page: 1 })
          }
          searchPlaceholder={`Search tasks in ${project.name}`}
          searchLabel="Search tasks in this project"
          sortLabel="Sort tasks"
        />
      </section>

      {view === 'board' ? (
        <TaskBoard
          project={project}
          tasks={tasks}
          milestones={milestones}
          component={boardComponent}
          defaultSort={defaultSort}
          emptyText={boardEmptyText}
        />
      ) : view === 'backlog' ? (
        <BacklogView
          project={project}
          tasks={tasks}
          milestones={milestones}
          defaultSort={defaultSort}
        />
      ) : listDeclaration.refusal ? (
        <UnavailablePanel
          title={listComponent ? componentTypeName(listComponent) : 'Tasks'}
          reason={listDeclaration.refusal.reason}
          hint={listDeclaration.refusal.hint}
        />
      ) : (
        <ProjectTaskList
          project={project}
          tasks={tasks}
          milestones={milestones}
          defaultColumns={listColumns}
          defaultSort={defaultSort}
          taskFilter={listFilter}
          rowLimit={listRowLimit}
          emptyText={listEmptyText}
          showFilterBar={false}
          showSavedViews={false}
        />
      )}
    </div>
  )
}

/**
 * The one view selector. It writes `?view=` on the Tasks route; it is not a tab
 * strip and the sections above it stay stable while it changes.
 */
function ViewSwitcher({
  value,
  onChange,
}: {
  value: TasksView
  onChange: (value: TasksView) => void
}) {
  return (
    <div
      role="group"
      aria-label="Tasks view"
      className="inline-flex items-center gap-0.5 rounded-md border border-line bg-surface-sunken/70 p-0.5"
    >
      {TASKS_VIEW_OPTIONS.map((option) => {
        const active = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={active}
            onClick={() => onChange(option.value)}
            className={cn(
              'inline-flex h-7 items-center rounded-sm px-2.5 text-xs font-medium transition-colors',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
              active
                ? 'bg-surface text-ink shadow-soft'
                : 'text-ink-muted hover:text-ink',
            )}
          >
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
