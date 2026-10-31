import { useCallback, useEffect, useMemo } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { Milestone, Project, Task } from '@/api/adapters'
import {
  defaultNowMs,
  useProjects,
  useRowKeys,
  useTaskCommand,
  useTaskGraph,
  useTaskReadiness,
  userMessage,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { EmptyState } from '@/components/ui/empty-state'
import { IconButton } from '@/components/ui/icon-button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import {
  AlertCircleIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  GridIcon,
  LayersIcon,
  PlusIcon,
} from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { SavedViewsButton } from '@/features/saved-views'
import {
  ALL,
  filterAndSortTasks,
  projectBulkActionOptions,
  projectSortKey,
  projectSortOptions,
  projectSortValue,
  type TaskSort,
} from './task-list-model'
import { ClearFiltersButton, TaskBulkBar, TaskFilterBar } from './task-filter-bar'
import { isTerminalStatus, statusLabel } from './task-meta'
import { TaskTable } from './task-table'
import { useTaskBulkCommand } from './use-task-bulk-command'
import { useTaskCommandState } from './use-task-command-state'
import { useTaskDrawer } from './use-task-drawer'
import {
  DEFAULT_PROJECT_COLUMNS,
  DEFAULT_TASK_SORT,
  LOCKED_TASK_COLUMNS,
  TASK_COLUMN_LABELS,
  readTaskListUrl,
  windowTasks,
  writeTaskListUrl,
  type TaskColumnKey,
  type TaskGroupKey,
} from './task-list-state'

interface TaskGroupRow {
  key: string
  label: string
  rows: Task[]
  /** A real milestone record, when the group is one. */
  milestone?: Milestone
}

/**
 * The project task list.
 *
 * Everything that changes the view — query, status, priority, milestone, sort,
 * page, columns, selection — is in the URL. Commands are the same
 * server-issued ones the rest of the workspace uses; nothing here is stored
 * locally except the popover's own open state.
 *
 * The Tasks workspace renders one shared control bar above its three views, so
 * the list can hide its own filter row while keeping the same URL contract.
 */
export function ProjectTaskList({
  project,
  tasks,
  milestones,
  defaultColumns = DEFAULT_PROJECT_COLUMNS,
  defaultSort = DEFAULT_TASK_SORT,
  defaultGroup = 'none',
  taskFilter,
  rowLimit,
  emptyText,
  showSavedViews = false,
  showFilterBar = true,
}: {
  project: Project
  tasks: Task[]
  milestones: Milestone[]
  /** Columns declared by the workflow's table component, if it declared any. */
  defaultColumns?: readonly TaskColumnKey[]
  /** The workflow's declared sort, used until the URL chooses another one. */
  defaultSort?: TaskSort
  /** Grouping used until the URL chooses another one (Backlog defaults to milestone). */
  defaultGroup?: TaskGroupKey
  /** A surface-level scope, applied after the URL filters. */
  taskFilter?: (task: Task) => boolean
  /** The workflow's declared `limit`: the most rows this surface may show. */
  rowLimit?: number
  /** The workflow's declared empty copy, when it published any. */
  emptyText?: string
  /** Only the full project list owns a saved-view control; summaries do not. */
  showSavedViews?: boolean
  /** The Tasks workspace owns the shared controls, so its list hides this row. */
  showFilterBar?: boolean
}) {
  const [searchParams, setSearchParams] = useSearchParams()
  const state = readTaskListUrl(searchParams, defaultColumns, defaultSort, defaultGroup)
  const projects = useProjects()
  const { openNewTask } = useWorkspaceActions()
  const { openTask } = useTaskDrawer()
  const command = useTaskCommand()
  const nowMs = defaultNowMs()

  const needsReadiness = state.columns.includes('readiness')
  const needsGraph = state.columns.includes('dependencies')
  const readiness = useTaskReadiness(nowMs, needsReadiness)
  const graph = useTaskGraph(nowMs, needsGraph)

  const commit = useCallback(
    (patch: Parameters<typeof writeTaskListUrl>[1]) => {
      setSearchParams((current) => writeTaskListUrl(current, patch, defaultSort), {
        replace: true,
      })
    },
    [defaultSort, setSearchParams],
  )

  const scoped = useMemo(
    () => (taskFilter ? tasks.filter((task) => taskFilter(task)) : tasks),
    [tasks, taskFilter],
  )
  const filtered = useMemo(
    () => filterAndSortTasks(scoped, state, state.sort),
    // `state` is re-created from the URL on every render; depend on its values.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [scoped, state.search, state.status, state.priority, state.milestone, state.sort.key, state.sort.direction],
  )

  // The workflow's declared `limit` caps the set after the person's own filters
  // and sort, so a table can never present more rows than the declaration says.
  const capped = useMemo(() => {
    if (!rowLimit || rowLimit <= 0) return filtered
    return filtered.slice(0, rowLimit)
  }, [filtered, rowLimit])
  const windowed = useMemo(() => windowTasks(capped, state.page), [capped, state.page])
  const visible = windowed.rows
  const groups = useMemo<TaskGroupRow[] | null>(() => {
    if (state.group === 'none') return null
    if (state.group === 'milestone') {
      const byMilestone = new Map<string, Task[]>()
      for (const task of visible) {
        const key = task.milestoneId ?? '__none__'
        const rows = byMilestone.get(key) ?? []
        rows.push(task)
        byMilestone.set(key, rows)
      }
      const ordered: TaskGroupRow[] = milestones
        .filter((milestone) => byMilestone.has(milestone.id))
        .map((milestone) => ({
          key: milestone.id,
          label: milestone.name,
          rows: byMilestone.get(milestone.id) ?? [],
          milestone,
        }))
      const unassigned = byMilestone.get('__none__')
      if (unassigned?.length) {
        ordered.push({ key: '__none__', label: 'No milestone', rows: unassigned })
      }
      return ordered
    }
    const byValue = new Map<string, Task[]>()
    for (const task of visible) {
      const value = state.group === 'status' ? task.status : (task.priority ?? 'none')
      const rows = byValue.get(value) ?? []
      rows.push(task)
      byValue.set(value, rows)
    }
    return [...byValue.entries()].map<TaskGroupRow>(([value, rows]) => ({
      key: value,
      label:
        state.group === 'status'
          ? statusLabel(value as Task['status'])
          : value === 'none'
            ? 'No priority'
            : value.charAt(0).toUpperCase() + value.slice(1),
      rows,
    }))
  }, [visible, state.group, milestones])
  const taskIds = useMemo(() => new Set(tasks.map((task) => task.id)), [tasks])
  const selected = useMemo(
    () => new Set(state.selected.filter((id) => taskIds.has(id))),
    [state.selected, taskIds],
  )

  // A stale id in the URL (task deleted or moved away) is dropped from the
  // address too, so a shared link never resurrects a selection that is not real.
  useEffect(() => {
    if (state.selected.length === selected.size) return
    commit({ selected: [...selected] })
  }, [commit, selected, state.selected.length])

  useRowKeys({
    onToggleSelect: (taskId) => {
      const next = new Set(selected)
      if (next.has(taskId)) next.delete(taskId)
      else next.add(taskId)
      commit({ selected: [...next] })
    },
  })

  const { changeStatus, changeProject, changePriority, pending, error, errorTaskId, errorField } =
    useTaskCommandState(command)

  const selectedTasks = useMemo(
    () => capped.filter((task) => selected.has(task.id)),
    [capped, selected],
  )

  const bulk = useTaskBulkCommand({
    command,
    projects,
    selectedTasks,
    initialAction: 'status',
    includeInbox: true,
    onSucceeded: () => commit({ selected: [] }),
  })

  const readinessByTask = useMemo(
    () => new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry])),
    [readiness.data],
  )
  const graphByTask = useMemo(
    () => new Map((graph.data?.nodes ?? []).map((node) => [node.taskId, node])),
    [graph.data],
  )

  const clearFilters = () => {
    commit({ search: '', status: ALL, priority: ALL, milestone: ALL, page: 1 })
  }

  const filtersActive =
    state.search.trim().length > 0 ||
    state.status !== ALL ||
    state.priority !== ALL ||
    state.milestone !== ALL
  const openCount = capped.filter((task) => !isTerminalStatus(task.status)).length
  const readError =
    (needsReadiness ? readiness.error : null) ?? (needsGraph ? graph.error : null) ?? null
  const retryReads = () => {
    if (needsReadiness) void readiness.refetch()
    if (needsGraph) void graph.refetch()
  }

  const toggleColumn = (key: TaskColumnKey) => {
    if (LOCKED_TASK_COLUMNS.includes(key)) return
    const current = new Set(state.columns)
    if (current.has(key)) current.delete(key)
    else current.add(key)
    const next = [...current]
    // Keep the declared order; a picked column appends after what is already on.
    commit({ columns: next })
  }

  const milestoneOptions = useMemo(
    () => [
      { value: ALL, label: 'Any milestone' },
      { value: 'none', label: 'No milestone' },
      ...milestones.map((milestone) => ({ value: milestone.id, label: milestone.name })),
    ],
    [milestones],
  )

  const renderTable = (rows: Task[], caption: string) => (
    <TaskTable
      tasks={rows}
      projects={projects}
      milestones={milestones}
      caption={caption}
      selection={{
        selected,
        onToggle: (taskId) => {
          const next = new Set(selected)
          if (next.has(taskId)) next.delete(taskId)
          else next.add(taskId)
          commit({ selected: [...next] })
        },
        onToggleAll: () => {
          const allSelected = rows.length > 0 && rows.every((task) => selected.has(task.id))
          const next = new Set(selected)
          if (allSelected) {
            for (const task of rows) next.delete(task.id)
          } else {
            for (const task of rows) next.add(task.id)
          }
          commit({ selected: [...next] })
        },
      }}
      onOpen={(task) => openTask(task.id)}
      onStatusChange={changeStatus}
      onProjectChange={changeProject}
      onPriorityChange={changePriority}
      pending={pending}
      errorTaskId={errorTaskId}
      error={error}
      errorField={errorField}
      sort={state.sort}
      onSortChange={(sort: TaskSort) => commit({ sort, page: 1 })}
      columns={state.columns}
      readiness={needsReadiness ? readinessByTask : undefined}
      dependencies={needsGraph ? graphByTask : undefined}
      empty={
        tasks.length === 0 ? (
          <EmptyState
            icon={<LayersIcon size={20} />}
            title={emptyText ?? 'No tasks in this project yet'}
            description="Add the first task here, or assign an Inbox task to this project. This view stays empty until the server has a task."
            action={
              <Button
                variant="secondary"
                size="sm"
                leadingIcon={<PlusIcon size={14} />}
                onClick={() => openNewTask(project.id)}
              >
                New task in {project.name}
              </Button>
            }
            className="min-h-[16rem]"
          />
        ) : (
          <EmptyState
            compact
            headingLevel="p"
            title="No tasks match these filters"
            description="Try a different word, status or priority — or clear the filters to see the whole project."
            action={
              <Button variant="secondary" size="sm" onClick={clearFilters}>
                Clear filters
              </Button>
            }
          />
        )
      }
    />
  )

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <p className="text-xs text-ink-muted">
          <span className="tabular-nums">{capped.length}</span> task
          {capped.length === 1 ? '' : 's'} · <span className="tabular-nums">{openCount}</span> open
          {rowLimit && filtered.length > capped.length ? (
            <span className="ml-1 text-ink-faint">
              (the workflow shows at most {rowLimit})
            </span>
          ) : null}
          {filtersActive ? <ClearFiltersButton onClick={clearFilters} /> : null}
        </p>
        <p className="hidden items-center gap-1.5 text-2xs text-ink-muted md:flex">
          <kbd className="command-key">j</kbd>
          <kbd className="command-key">k</kbd>
          <span>move</span>
          <kbd className="command-key">x</kbd>
          <span>select</span>
          <kbd className="command-key">↵</kbd>
          <span>open</span>
          <kbd className="command-key">/</kbd>
          <span>search</span>
        </p>
      </div>

      {showFilterBar ? (
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
          sortValue={projectSortValue(state.sort)}
          onSortValueChange={(value) =>
            commit({ sort: { key: projectSortKey(value), direction: 'asc' }, page: 1 })
          }
          sortOptions={projectSortOptions}
          groupValue={state.group}
          onGroupValueChange={(group) => commit({ group, page: 1 })}
          searchPlaceholder={`Search tasks in ${project.name}`}
          searchLabel="Search tasks in this project"
          sortLabel="Sort tasks"
        />
      ) : null}

      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <ColumnPicker
            columns={state.columns}
            onToggle={toggleColumn}
            onReset={() => commit({ columns: [...defaultColumns] })}
          />
          {showSavedViews ? <SavedViewsButton display="list" /> : null}
        </div>
        <WindowSummary total={capped.length} window={windowed} />
      </div>

      {readError ? (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-warning-ink">
          <AlertCircleIcon size={14} className="shrink-0" />
          <span className="min-w-0 flex-1">
            {needsReadiness && readiness.error
              ? 'Readiness could not be read, so those cells are empty.'
              : 'Dependencies could not be read, so those cells are empty.'}{' '}
            {userMessage(readError)}
          </span>
          <Button variant="quiet" size="sm" onClick={retryReads}>
            Retry
          </Button>
        </div>
      ) : null}

      {selectedTasks.length > 0 ? (
        <TaskBulkBar
          selectedCount={selectedTasks.length}
          onClearSelection={() => commit({ selected: [] })}
          bulkAction={bulk.bulkAction}
          onBulkActionChange={bulk.selectBulkAction}
          actionOptions={projectBulkActionOptions}
          bulkValue={bulk.bulkValue}
          onBulkValueChange={bulk.setBulkValue}
          bulkOptions={bulk.bulkOptions}
          progress={bulk.bulk}
          error={bulk.bulkError}
          onApply={() => void bulk.applyBulk()}
        />
      ) : null}

      {groups ? (
        <div className="flex flex-col gap-3">
          {groups.map((group) => (
            <details
              key={group.key}
              open
              data-milestone-group={group.milestone?.id ?? undefined}
              className="overflow-hidden rounded-lg border border-line bg-surface"
            >
              <summary className="flex cursor-pointer list-none items-center justify-between gap-3 bg-surface-sunken/60 px-3.5 py-2.5 text-sm font-medium text-ink marker:content-none hover:bg-surface-hover">
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate">{group.label}</span>
                  <span className="text-2xs font-normal tabular-nums text-ink-muted">
                    {group.rows.length}
                  </span>
                </span>
                {group.milestone ? (
                  <span className="hidden shrink-0 items-center gap-2 text-2xs font-normal text-ink-muted sm:flex">
                    <span className="tabular-nums">
                      {group.milestone.progress.completed}/{group.milestone.progress.total}
                    </span>
                    <span className="h-1.5 w-24 overflow-hidden rounded-full bg-line">
                      <span
                        className="block h-full rounded-full bg-accent"
                        style={{ width: `${group.milestone.progress.percent}%` }}
                      />
                    </span>
                    <span className="tabular-nums">{group.milestone.progress.percent}%</span>
                  </span>
                ) : null}
              </summary>
              {renderTable(group.rows, `${project.name} — ${group.label}`)}
            </details>
          ))}
        </div>
      ) : (
        renderTable(visible, `Tasks in ${project.name}`)
      )}

      {rowLimit && filtered.length > capped.length ? (
        <p className="text-2xs leading-4 text-ink-muted">
          The workflow limits this surface to {rowLimit} row{rowLimit === 1 ? '' : 's'};{' '}
          {filtered.length - capped.length} more match and are not shown. Narrow the filters to see
          them.
        </p>
      ) : null}

      {windowed.windowed ? (
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-2xs tabular-nums text-ink-muted">
            Showing {(windowed.page - 1) * 50 + 1}–{Math.min(windowed.page * 50, capped.length)} of{' '}
            {capped.length} matching tasks. Only one page is rendered, so large projects stay
            responsive.
          </p>
          <div className="flex items-center gap-1">
            <IconButton
              label="Previous page"
              icon={<ChevronLeftIcon size={15} />}
              size="sm"
              variant="secondary"
              disabled={windowed.page <= 1}
              onClick={() => commit({ page: windowed.page - 1 })}
            />
            <span className="px-2 text-2xs tabular-nums text-ink-muted">
              {windowed.page} / {windowed.pageCount}
            </span>
            <IconButton
              label="Next page"
              icon={<ChevronRightIcon size={15} />}
              size="sm"
              variant="secondary"
              disabled={windowed.page >= windowed.pageCount}
              onClick={() => commit({ page: windowed.page + 1 })}
            />
          </div>
        </div>
      ) : null}
    </div>
  )
}

function WindowSummary({
  total,
  window,
}: {
  total: number
  window: ReturnType<typeof windowTasks<Task>>
}) {
  return (
    <p className="text-2xs tabular-nums text-ink-muted">
      {window.windowed
        ? `Page ${window.page} of ${window.pageCount} · ${total} matching tasks`
        : `All ${total} matching task${total === 1 ? '' : 's'} rendered`}
    </p>
  )
}

function ColumnPicker({
  columns,
  onToggle,
  onReset,
}: {
  columns: readonly TaskColumnKey[]
  onToggle: (key: TaskColumnKey) => void
  onReset: () => void
}) {
  const all = Object.keys(TASK_COLUMN_LABELS) as TaskColumnKey[]
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="secondary" size="sm" leadingIcon={<GridIcon size={14} />}>
          Columns
          <span className="ml-1 text-2xs font-normal text-ink-muted">{columns.length}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-3">
        <p className="text-xs font-semibold text-ink">Visible columns</p>
        <p className="mt-0.5 text-2xs leading-4 text-ink-muted">
          The task title is always shown. Choices are kept in the URL.
        </p>
        <div className="mt-2 flex flex-col gap-1.5">
          {all.map((key) => {
            const locked = LOCKED_TASK_COLUMNS.includes(key)
            return (
              <label
                key={key}
                className={cn(
                  'flex items-center gap-2 text-xs',
                  locked ? 'text-ink-muted' : 'cursor-pointer text-ink-soft',
                )}
              >
                <Checkbox
                  checked={columns.includes(key)}
                  disabled={locked}
                  onCheckedChange={() => onToggle(key)}
                  aria-label={`Show ${TASK_COLUMN_LABELS[key]} column`}
                />
                {TASK_COLUMN_LABELS[key]}
              </label>
            )
          })}
        </div>
        <Button variant="quiet" size="sm" className="mt-2 px-0" onClick={onReset}>
          Reset to the workflow&apos;s columns
        </Button>
      </PopoverContent>
    </Popover>
  )
}
