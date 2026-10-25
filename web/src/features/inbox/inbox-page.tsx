import { useCallback, useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { InboxIcon, PlusIcon } from '@/components/ui/icons'
import { PageHeader } from '@/components/ui/page-header'
import { Panel } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { useInboxTasks, useProjects, useTaskCommand, useWorkspaceSnapshot, userMessage } from '@/hooks'
import { useDocumentTitle } from '@/hooks'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { useRowKeys } from '@/hooks'
import {
  ALL,
  ClearFiltersButton,
  filterAndSortTasks,
  inboxBulkActionOptions,
  inboxSortKey,
  inboxSortValue,
  sortOptions,
  TaskBulkBar,
  TaskFilterBar,
  TaskTable,
  useTaskBulkCommand,
  useTaskCommandState,
  useTaskDrawer,
  useTaskSelection,
  type TaskSort,
} from '@/features/tasks'
import { QuickCapture } from './quick-capture'

function InboxSkeleton() {
  return (
    <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading the Inbox">
      <div>
        <Skeleton className="h-2.5 w-16" />
        <Skeleton className="mt-3 h-8 w-40" />
        <Skeleton className="mt-3 h-3 w-80" />
      </div>
      <Skeleton className="h-14 w-full rounded-lg" />
      <div className="flex gap-2">
        <Skeleton className="h-9 flex-1 rounded-md" />
        <Skeleton className="h-9 w-32 rounded-md" />
        <Skeleton className="h-9 w-32 rounded-md" />
      </div>
      <div className="rounded-lg border border-line bg-surface">
        {Array.from({ length: 7 }, (_, index) => (
          <div key={index} className="flex items-center gap-4 border-b border-line px-4 py-3 last:border-b-0">
            <Skeleton className="h-4 w-4 rounded-xs" />
            <Skeleton className="h-3 flex-1" />
            <Skeleton className="h-3 w-24" />
            <Skeleton className="h-6 w-24 rounded-sm" />
          </div>
        ))}
      </div>
    </div>
  )
}

export function InboxPage() {
  useDocumentTitle('Inbox')
  const { openNewTask } = useWorkspaceActions()
  const { openTask } = useTaskDrawer()
  const snapshot = useWorkspaceSnapshot()
  const tasks = useInboxTasks()
  const projects = useProjects()
  const command = useTaskCommand()

  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string>(ALL)
  const [priorityFilter, setPriorityFilter] = useState<string>(ALL)
  const [sort, setSort] = useState<TaskSort>({ key: 'priority', direction: 'asc' })

  const filtered = useMemo(
    () => filterAndSortTasks(tasks, { search, status: statusFilter, priority: priorityFilter }, sort),
    [tasks, search, statusFilter, priorityFilter, sort],
  )

  const { selected, selectedTasks, toggleSelect, toggleAll, clear } = useTaskSelection({
    tasks,
    filtered,
    visible: filtered,
  })

  const toggle = useCallback(
    (taskId: string) => {
      toggleSelect(taskId)
    },
    [toggleSelect],
  )
  useRowKeys({ onToggleSelect: toggle })

  const commandState = useTaskCommandState(command)
  const { changeStatus, changeProject, changePriority, pending, error, errorTaskId, errorField } =
    commandState

  const bulk = useTaskBulkCommand({
    command,
    projects,
    selectedTasks,
    initialAction: 'project',
    onSucceeded: clear,
  })

  const clearFilters = () => {
    setSearch('')
    setStatusFilter(ALL)
    setPriorityFilter(ALL)
  }

  const filtersActive = search.trim().length > 0 || statusFilter !== ALL || priorityFilter !== ALL
  const captureFocus = () => document.querySelector<HTMLInputElement>('[data-quick-capture]')?.focus()

  if (snapshot.isPending && !snapshot.data) return <InboxSkeleton />

  if (snapshot.error && !snapshot.data) {
    return (
      <div className="flex flex-col gap-6">
        <PageHeader eyebrow="Triage" title="Inbox" description="Unassigned work waiting for a decision." />
        <Panel>
          <ErrorState
            title="The Inbox could not be loaded"
            description={userMessage(snapshot.error)}
            onRetry={() => void snapshot.refetch()}
            retrying={snapshot.isFetching}
          />
        </Panel>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        eyebrow="Triage"
        title="Inbox"
        description={
          tasks.length === 0
            ? 'Everything captured without a project waits here.'
            : `${tasks.length} unassigned task${tasks.length === 1 ? '' : 's'} waiting for a project, a priority or a status.`
        }
        actions={
          <Button leadingIcon={<PlusIcon size={16} />} onClick={() => openNewTask()}>
            New task
          </Button>
        }
      />

      <QuickCapture />

      <TaskFilterBar
        search={search}
        onSearchChange={setSearch}
        statusFilter={statusFilter}
        onStatusFilterChange={setStatusFilter}
        priorityFilter={priorityFilter}
        onPriorityFilterChange={setPriorityFilter}
        sortValue={inboxSortValue(sort)}
        onSortValueChange={(value) => setSort({ key: inboxSortKey(value), direction: 'asc' })}
        sortOptions={sortOptions}
        searchPlaceholder="Search the Inbox"
        searchLabel="Search the Inbox"
        sortLabel="Sort the Inbox"
      />

      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-ink-muted">
        <p aria-live="polite">
          {filtered.length === tasks.length
            ? `${tasks.length} task${tasks.length === 1 ? '' : 's'}`
            : `${filtered.length} of ${tasks.length} tasks`}
          {filtersActive ? <ClearFiltersButton onClick={clearFilters} /> : null}
        </p>
        <p className="hidden items-center gap-2 md:flex">
          <span>
            <kbd className="command-key">j</kbd> <kbd className="command-key">k</kbd> to move
          </span>
          <span>
            <kbd className="command-key">x</kbd> to select
          </span>
          <span>
            <kbd className="command-key">↵</kbd> to open
          </span>
        </p>
      </div>

      {selectedTasks.length > 0 ? (
        <TaskBulkBar
          selectedCount={selectedTasks.length}
          onClearSelection={clear}
          bulkAction={bulk.bulkAction}
          onBulkActionChange={bulk.selectBulkAction}
          actionOptions={inboxBulkActionOptions}
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
          tasks={filtered}
          projects={projects}
          caption="Unassigned tasks in the Inbox"
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
          showLabels
          empty={
            tasks.length === 0 ? (
              <EmptyState
                icon={<InboxIcon size={20} />}
                title="Inbox is clear"
                description="Capture your first task above, or press c anywhere in Orbit. Assigning a project moves work out of the Inbox."
                action={
                  <Button variant="secondary" size="sm" onClick={captureFocus}>
                    Capture a task
                  </Button>
                }
                className="min-h-[16rem]"
              />
            ) : (
              <EmptyState
                compact
                headingLevel="p"
                title="No tasks match these filters"
                description="Try a different word, or clear the filters to see everything in the Inbox."
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

      <p className="text-2xs leading-4 text-ink-faint">
        The Inbox is every task without a project. Status colours and counts come from the server snapshot.
      </p>
    </div>
  )
}
