import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import {
  useInboxTasks,
  useProjects,
  useTaskCommand,
  useWorkspaceSnapshot,
  userMessage,
} from '@/hooks'
import { useDocumentTitle } from '@/app/use-document-title'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { IconButton } from '@/components/ui/icon-button'
import { Input } from '@/components/ui/input'
import {
  AlertCircleIcon,
  InboxIcon,
  PlusIcon,
  SearchIcon,
} from '@/components/ui/icons'
import { PageHeader } from '@/components/ui/page-header'
import { Panel } from '@/components/ui/panel'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { useWorkspaceActions } from '@/components/layout/workspace-actions'
import { useTaskDrawer } from '@/features/tasks/use-task-drawer'
import { useCommandFeedback, useRowKeys } from '@/features/tasks/use-command-feedback'
import { TaskTable, type TaskSort } from '@/features/tasks/task-table'
import {
  priorityRank,
  priorityLabels,
  sortByDeadline,
  statusLabels,
  TASK_PRIORITIES,
  TASK_STATUSES,
} from '@/features/tasks/task-meta'
import { QuickCapture } from './quick-capture'

type BulkAction = 'status' | 'priority' | 'project'

const ALL = '__all__'

const sortOptions: Array<{ value: string; label: string }> = [
  { value: 'priority', label: 'Priority' },
  { value: 'due', label: 'Due date' },
  { value: 'status', label: 'Status' },
  { value: 'title', label: 'Title' },
]
function sortKeyFor(value: string): TaskSort['key'] {
  if (value === 'due') return 'due'
  if (value === 'title') return 'title'
  return 'priority'
}

function compareTasks(left: Task, right: Task, sort: TaskSort): number {
  const direction = sort.direction === 'asc' ? 1 : -1
  if (sort.key === 'title') return left.title.localeCompare(right.title) * direction
  if (sort.key === 'due') return sortByDeadline(left, right) * direction
  if (sort.key === 'status') return left.status.localeCompare(right.status) * direction
  if (sort.key === 'project') return (left.projectId ?? '').localeCompare(right.projectId ?? '') * direction
  const leftPriority = left.priority ? priorityRank[left.priority] : 99
  const rightPriority = right.priority ? priorityRank[right.priority] : 99
  if (leftPriority !== rightPriority) return (leftPriority - rightPriority) * direction
  return sortByDeadline(left, right)
}

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
  const { run } = useCommandFeedback()
  const { toast } = useToast()

  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string>(ALL)
  const [priorityFilter, setPriorityFilter] = useState<string>(ALL)
  const [sort, setSort] = useState<TaskSort>({ key: 'priority', direction: 'asc' })
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set())
  const [bulkAction, setBulkAction] = useState<BulkAction>('project')
  const [bulkValue, setBulkValue] = useState('')
  const [bulk, setBulk] = useState<{ label: string; total: number; done: number } | null>(null)
  const [bulkError, setBulkError] = useState<string | null>(null)
  const searchRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const valid = new Set(tasks.map((task) => task.id))
    setSelected((current) => {
      const next = new Set([...current].filter((id) => valid.has(id)))
      return next.size === current.size ? current : next
    })
  }, [tasks])

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase()
    return tasks
      .filter((task) => {
        if (needle && !task.title.toLowerCase().includes(needle)) return false
        if (statusFilter !== ALL && task.status !== statusFilter) return false
        if (priorityFilter !== ALL && (task.priority ?? 'none') !== priorityFilter) return false
        return true
      })
      .sort((left, right) => compareTasks(left, right, sort))
  }, [tasks, search, statusFilter, priorityFilter, sort])

  const toggleSelect = useCallback((taskId: string) => {
    setSelected((current) => {
      const next = new Set(current)
      if (next.has(taskId)) next.delete(taskId)
      else next.add(taskId)
      return next
    })
  }, [])

  const toggleAll = useCallback(() => {
    setSelected((current) => {
      const all = filtered.every((task) => current.has(task.id))
      if (all) {
        const next = new Set(current)
        for (const task of filtered) next.delete(task.id)
        return next
      }
      return new Set([...current, ...filtered.map((task) => task.id)])
    })
  }, [filtered])

  const pending =
    command.isPending && command.variables
      ? {
          taskId: command.variables.taskId,
          field:
            command.variables.type === 'change_status'
              ? ('status' as const)
              : command.variables.type === 'assign_project'
                ? ('project' as const)
                : ('priority' as const),
        }
      : null

  const error: ApiError | null = command.disposition === 'rejected' ? command.error : null
  const errorTaskId = error ? command.variables?.taskId ?? null : null
  const errorField =
    error && command.variables
      ? command.variables.type === 'change_status'
        ? ('status' as const)
        : command.variables.type === 'assign_project'
          ? ('project' as const)
          : ('priority' as const)
      : null

  const changeStatus = (task: Task, status: TaskStatus) => {
    void run('Status not changed', () =>
      command.mutateAsync({ type: 'change_status', taskId: task.id, status }),
    )
  }

  const changeProject = (task: Task, projectId: string | null) => {
    void run('Project not changed', () =>
      command.mutateAsync({ type: 'assign_project', taskId: task.id, projectId }),
    )
  }

  const changePriority = (task: Task, priority: TaskPriority | null) => {
    void run('Priority not changed', () =>
      command.mutateAsync({
        type: 'set_details',
        taskId: task.id,
        description: task.description,
        priority,
        labels: task.labels,
      }),
    )
  }

  const toggle = useCallback(
    (taskId: string) => {
      toggleSelect(taskId)
    },
    [toggleSelect],
  )
  useRowKeys({ onToggleSelect: toggle })

  const selectedTasks = useMemo(
    () => filtered.filter((task) => selected.has(task.id)),
    [filtered, selected],
  )

  async function applyBulk() {
    if (selectedTasks.length === 0 || bulkValue.length === 0 || bulk) return
    const label =
      bulkAction === 'status'
        ? 'Status'
        : bulkAction === 'priority'
          ? 'Priority'
          : 'Project'
    if (bulkAction === 'project' && bulkValue === 'none') return

    setBulkError(null)
    setBulk({ label, total: selectedTasks.length, done: 0 })
    let failed = 0
    let firstFailure: unknown = null

    for (const task of selectedTasks) {
      try {
        if (bulkAction === 'status') {
          await command.mutateAsync({
            type: 'change_status',
            taskId: task.id,
            status: bulkValue as TaskStatus,
          })
        } else if (bulkAction === 'priority') {
          await command.mutateAsync({
            type: 'set_details',
            taskId: task.id,
            description: task.description,
            priority: (bulkValue === 'none' ? null : bulkValue) as TaskPriority | null,
            labels: task.labels,
          })
        } else {
          await command.mutateAsync({
            type: 'assign_project',
            taskId: task.id,
            projectId: bulkValue,
          })
        }
      } catch (failure) {
        failed += 1
        if (!firstFailure) firstFailure = failure
      }
      setBulk((current) => (current ? { ...current, done: current.done + 1 } : current))
    }

    setBulk(null)
    const succeeded = selectedTasks.length - failed
    if (failed === 0) {
      toast({
        tone: 'success',
        title: `${label} updated on ${succeeded} task${succeeded === 1 ? '' : 's'}`,
      })
      setSelected(new Set())
      setBulkValue('')
      return
    }

    setBulkError(
      `${failed} of ${selectedTasks.length} task${selectedTasks.length === 1 ? '' : 's'} were not updated. ${userMessage(
        firstFailure,
      )}`,
    )
    toast({
      tone: 'danger',
      title: `${failed} task${failed === 1 ? '' : 's'} not updated`,
      description: userMessage(firstFailure),
    })
  }

  const bulkOptions = useMemo(() => {
    if (bulkAction === 'status') return TASK_STATUSES.map((status) => ({ value: status, label: statusLabels[status] }))
    if (bulkAction === 'priority')
      return [
        { value: 'none', label: 'No priority' },
        ...TASK_PRIORITIES.map((priority) => ({ value: priority, label: priorityLabels[priority] })),
      ]
    return projects.map((project) => ({ value: project.id, label: project.name }))
  }, [bulkAction, projects])

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

      <div className="flex flex-col gap-2.5 xl:flex-row xl:items-center">
        <div className="min-w-0 flex-1">
          <Input
            ref={searchRef}
            data-search-input=""
            type="search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            leadingIcon={<SearchIcon size={15} />}
            placeholder="Search the Inbox"
            aria-label="Search the Inbox"
          />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Select value={statusFilter} onValueChange={setStatusFilter}>
            <SelectTrigger className="h-9 w-[9.5rem] text-sm" aria-label="Filter by status">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL}>Any status</SelectItem>
              {TASK_STATUSES.map((status) => (
                <SelectItem key={status} value={status}>
                  {statusLabels[status]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={priorityFilter} onValueChange={setPriorityFilter}>
            <SelectTrigger className="h-9 w-[9rem] text-sm" aria-label="Filter by priority">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL}>Any priority</SelectItem>
              <SelectItem value="none">No priority</SelectItem>
              {TASK_PRIORITIES.map((priority) => (
                <SelectItem key={priority} value={priority}>
                  {priorityLabels[priority]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={sortOptions.find((option) => sortKeyFor(option.value) === sort.key)?.value ?? 'priority'}
            onValueChange={(value) => setSort({ key: sortKeyFor(value), direction: 'asc' })}
          >
            <SelectTrigger className="h-9 w-[10rem] text-sm" aria-label="Sort the Inbox">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {sortOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  Sort: {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-ink-muted">
        <p aria-live="polite">
          {filtered.length === tasks.length
            ? `${tasks.length} task${tasks.length === 1 ? '' : 's'}`
            : `${filtered.length} of ${tasks.length} tasks`}
          {filtersActive ? (
            <button
              type="button"
              onClick={() => {
                setSearch('')
                setStatusFilter(ALL)
                setPriorityFilter(ALL)
              }}
              className="ml-2 rounded-sm font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
            >
              Clear filters
            </button>
          ) : null}
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
        <section
          aria-label="Bulk actions"
          className="flex flex-wrap items-center gap-2 rounded-md border border-accent/30 bg-accent-soft/45 px-3 py-2"
        >
          <span className="flex items-center gap-2 text-xs font-medium text-accent-soft-ink">
            <Checkbox
              checked
              onCheckedChange={() => setSelected(new Set())}
              aria-label="Clear selection"
            />
            {selectedTasks.length} selected
          </span>

          <div className="ml-auto flex flex-wrap items-center gap-1.5">
            <Select
              value={bulkAction}
              onValueChange={(value) => {
                setBulkAction(value as BulkAction)
                setBulkValue('')
                setBulkError(null)
              }}
              disabled={Boolean(bulk)}
            >
              <SelectTrigger className="h-8 w-[7.5rem] text-xs" aria-label="Bulk action">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="project">Assign project</SelectItem>
                <SelectItem value="status">Set status</SelectItem>
                <SelectItem value="priority">Set priority</SelectItem>
              </SelectContent>
            </Select>

            <Select value={bulkValue} onValueChange={setBulkValue} disabled={Boolean(bulk)}>
              <SelectTrigger className="h-8 w-[11rem] text-xs" aria-label="Bulk value">
                <SelectValue placeholder="Choose a value" />
              </SelectTrigger>
              <SelectContent>
                {bulkOptions.map((option) => (
                  <SelectItem key={option.value} value={option.value}>
                    {option.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>

            <Button
              size="sm"
              loading={Boolean(bulk)}
              disabled={bulkValue.length === 0 || Boolean(bulk)}
              onClick={() => void applyBulk()}
            >
              Apply
            </Button>
            <IconButton
              label="Clear selection"
              icon={<span aria-hidden="true">×</span>}
              size="sm"
              variant="quiet"
              onClick={() => setSelected(new Set())}
              disabled={Boolean(bulk)}
            />
          </div>

          {bulk ? (
            <p className="w-full text-2xs text-accent-soft-ink" role="status" aria-live="polite">
              {bulk.label}: {bulk.done} of {bulk.total} sent…
            </p>
          ) : null}
          {bulkError ? (
            <p role="alert" className="flex w-full items-start gap-1.5 text-2xs leading-4 text-danger-ink">
              <AlertCircleIcon size={12} className="mt-0.5 shrink-0" />
              {bulkError}
            </p>
          ) : null}
        </section>
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
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      setSearch('')
                      setStatusFilter(ALL)
                      setPriorityFilter(ALL)
                    }}
                  >
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
