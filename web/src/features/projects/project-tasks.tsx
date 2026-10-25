import { useEffect, useMemo, useState } from 'react'
import type { Milestone, Project, Task } from '@/api/adapters'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { IconButton } from '@/components/ui/icon-button'
import { ChevronLeftIcon, ChevronRightIcon, LayersIcon, PlusIcon } from '@/components/ui/icons'
import { useTaskCommand } from '@/hooks'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { useRowKeys } from '@/hooks'
import {
  ALL,
  ClearFiltersButton,
  filterAndSortTasks,
  isTerminalStatus,
  projectBulkActionOptions,
  projectSortKey,
  projectSortOptions,
  projectSortValue,
  TaskBulkBar,
  TaskFilterBar,
  TaskTable,
  useTaskBulkCommand,
  useTaskCommandState,
  useTaskDrawer,
  useTaskSelection,
  type TaskSort,
} from '@/features/tasks'

const PAGE_SIZE = 50

export function ProjectTasks({
  project,
  tasks,
  projects,
  milestones,
}: {
  project: Project
  tasks: Task[]
  projects: Project[]
  milestones: Milestone[]
}) {
  const { openNewTask } = useWorkspaceActions()
  const { openTask } = useTaskDrawer()
  const command = useTaskCommand()

  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState(ALL)
  const [priorityFilter, setPriorityFilter] = useState(ALL)
  const [sort, setSort] = useState<TaskSort>({ key: 'due', direction: 'asc' })
  const [page, setPage] = useState(1)

  useEffect(() => {
    setPage(1)
  }, [search, statusFilter, priorityFilter, sort])

  const filtered = useMemo(
    () => filterAndSortTasks(tasks, { search, status: statusFilter, priority: priorityFilter }, sort),
    [tasks, search, statusFilter, priorityFilter, sort],
  )

  const needsPaging = filtered.length > 200
  const pageCount = needsPaging ? Math.max(1, Math.ceil(filtered.length / PAGE_SIZE)) : 1
  const safePage = Math.min(page, pageCount)
  const visible = needsPaging
    ? filtered.slice((safePage - 1) * PAGE_SIZE, safePage * PAGE_SIZE)
    : filtered

  const { selected, selectedTasks, toggleSelect, toggleAll, clear } = useTaskSelection({
    tasks,
    filtered,
    visible,
  })

  useRowKeys({ onToggleSelect: toggleSelect })

  const { changeStatus, changeProject, changePriority, pending, error, errorTaskId, errorField } =
    useTaskCommandState(command)

  const bulk = useTaskBulkCommand({
    command,
    projects,
    selectedTasks,
    initialAction: 'status',
    includeInbox: true,
    onSucceeded: clear,
  })

  const clearFilters = () => {
    setSearch('')
    setStatusFilter(ALL)
    setPriorityFilter(ALL)
  }

  const filtersActive = search.trim().length > 0 || statusFilter !== ALL || priorityFilter !== ALL
  const openCount = filtered.filter((task) => !isTerminalStatus(task.status)).length

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <p className="text-xs text-ink-muted">
          <span className="tabular-nums">{filtered.length}</span> task
          {filtered.length === 1 ? '' : 's'} · <span className="tabular-nums">{openCount}</span> open
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
        </p>
      </div>

      <TaskFilterBar
        search={search}
        onSearchChange={setSearch}
        statusFilter={statusFilter}
        onStatusFilterChange={setStatusFilter}
        priorityFilter={priorityFilter}
        onPriorityFilterChange={setPriorityFilter}
        sortValue={projectSortValue(sort)}
        onSortValueChange={(value) => setSort({ key: projectSortKey(value), direction: 'asc' })}
        sortOptions={projectSortOptions}
        searchPlaceholder={`Search tasks in ${project.name}`}
        searchLabel="Search tasks in this project"
        sortLabel="Sort tasks"
      />

      {selectedTasks.length > 0 ? (
        <TaskBulkBar
          selectedCount={selectedTasks.length}
          onClearSelection={clear}
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

      <div>
        <TaskTable
          tasks={visible}
          projects={projects}
          milestones={milestones}
          caption={`Tasks in ${project.name}`}
          selection={{ selected, onToggle: toggleSelect, onToggleAll: toggleAll }}
          onOpen={(task) => openTask(task.id)}
          onStatusChange={changeStatus}
          onProjectChange={changeProject}
          onPriorityChange={changePriority}
          pending={pending}
          errorTaskId={errorTaskId}
          error={error}
          errorField={errorField}
          sort={sort}
          onSortChange={setSort}
          showProject
          showPriority
          showMilestone
          empty={
            tasks.length === 0 ? (
              <EmptyState
                icon={<LayersIcon size={20} />}
                title="No tasks in this project yet"
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
                description="Try a different word, or clear the filters to see the whole project."
                action={
                  <Button variant="secondary" size="sm" onClick={clearFilters}>
                    Clear filters
                  </Button>
                }
              />
            )
          }
        />
      </div>

      {needsPaging ? (
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-2xs tabular-nums text-ink-muted">
            Showing {(safePage - 1) * PAGE_SIZE + 1}–{Math.min(safePage * PAGE_SIZE, filtered.length)} of{' '}
            {filtered.length}
          </p>
          <div className="flex items-center gap-1">
            <IconButton
              label="Previous page"
              icon={<ChevronLeftIcon size={15} />}
              size="sm"
              variant="secondary"
              disabled={safePage <= 1}
              onClick={() => setPage((current) => Math.max(1, current - 1))}
            />
            <span className="px-2 text-2xs tabular-nums text-ink-muted">
              {safePage} / {pageCount}
            </span>
            <IconButton
              label="Next page"
              icon={<ChevronRightIcon size={15} />}
              size="sm"
              variant="secondary"
              disabled={safePage >= pageCount}
              onClick={() => setPage((current) => Math.min(pageCount, current + 1))}
            />
          </div>
        </div>
      ) : null}
    </div>
  )
}
