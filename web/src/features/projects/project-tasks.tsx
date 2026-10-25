import { useCallback, useEffect, useMemo, useState } from 'react'
import type { Milestone, Project, Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import { useTaskCommand, userMessage } from '@/hooks'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { EmptyState } from '@/components/ui/empty-state'
import { IconButton } from '@/components/ui/icon-button'
import { Input } from '@/components/ui/input'
import {
  AlertCircleIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  LayersIcon,
  PlusIcon,
  SearchIcon,
} from '@/components/ui/icons'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useToast } from '@/components/ui/toast'
import { useWorkspaceActions } from '@/components/layout/workspace-actions'
import { useTaskDrawer } from '@/features/tasks/use-task-drawer'
import { useCommandFeedback, useRowKeys } from '@/features/tasks/use-command-feedback'
import { TaskTable, type TaskSort } from '@/features/tasks/task-table'
import {
  isTerminalStatus,
  priorityLabels,
  priorityRank,
  sortByDeadline,
  statusLabels,
  TASK_PRIORITIES,
  TASK_STATUSES,
} from '@/features/tasks/task-meta'

const ALL = '__all__'
const PAGE_SIZE = 50

type BulkAction = 'status' | 'priority' | 'project'

function compareTasks(left: Task, right: Task, sort: TaskSort): number {
  const direction = sort.direction === 'asc' ? 1 : -1
  if (sort.key === 'title') return left.title.localeCompare(right.title) * direction
  if (sort.key === 'due') return sortByDeadline(left, right) * direction
  if (sort.key === 'status') return left.status.localeCompare(right.status) * direction
  if (sort.key === 'milestone') return (left.milestoneId ?? '').localeCompare(right.milestoneId ?? '') * direction
  if (sort.key === 'project') return (left.projectId ?? '').localeCompare(right.projectId ?? '') * direction
  const leftPriority = left.priority ? priorityRank[left.priority] : 99
  const rightPriority = right.priority ? priorityRank[right.priority] : 99
  if (leftPriority !== rightPriority) return (leftPriority - rightPriority) * direction
  return sortByDeadline(left, right)
}

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
  const { run } = useCommandFeedback()
  const { toast } = useToast()

  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState(ALL)
  const [priorityFilter, setPriorityFilter] = useState(ALL)
  const [sort, setSort] = useState<TaskSort>({ key: 'due', direction: 'asc' })
  const [page, setPage] = useState(1)
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set())
  const [bulkAction, setBulkAction] = useState<BulkAction>('status')
  const [bulkValue, setBulkValue] = useState('')
  const [bulk, setBulk] = useState<{ label: string; total: number; done: number } | null>(null)
  const [bulkError, setBulkError] = useState<string | null>(null)

  useEffect(() => {
    setPage(1)
  }, [search, statusFilter, priorityFilter, sort])

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

  const needsPaging = filtered.length > 200
  const pageCount = needsPaging ? Math.max(1, Math.ceil(filtered.length / PAGE_SIZE)) : 1
  const safePage = Math.min(page, pageCount)
  const visible = needsPaging
    ? filtered.slice((safePage - 1) * PAGE_SIZE, safePage * PAGE_SIZE)
    : filtered

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
      const all = visible.every((task) => current.has(task.id))
      if (all) {
        const next = new Set(current)
        for (const task of visible) next.delete(task.id)
        return next
      }
      return new Set([...current, ...visible.map((task) => task.id)])
    })
  }, [visible])

  useRowKeys({ onToggleSelect: toggleSelect })

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

  const selectedTasks = useMemo(
    () => filtered.filter((task) => selected.has(task.id)),
    [filtered, selected],
  )

  const bulkOptions = useMemo(() => {
    if (bulkAction === 'status')
      return TASK_STATUSES.map((status) => ({ value: status, label: statusLabels[status] }))
    if (bulkAction === 'priority')
      return [
        { value: 'none', label: 'No priority' },
        ...TASK_PRIORITIES.map((priority) => ({ value: priority, label: priorityLabels[priority] })),
      ]
    return [
      { value: 'none', label: 'No project (Inbox)' },
      ...projects.map((candidate) => ({ value: candidate.id, label: candidate.name })),
    ]
  }, [bulkAction, projects])

  async function applyBulk() {
    if (selectedTasks.length === 0 || bulkValue.length === 0 || bulk) return
    const label = bulkAction === 'status' ? 'Status' : bulkAction === 'priority' ? 'Priority' : 'Project'
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
            projectId: bulkValue === 'none' ? null : bulkValue,
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
      `${failed} of ${selectedTasks.length} were not updated. ${userMessage(firstFailure)}`,
    )
    toast({
      tone: 'danger',
      title: `${failed} task${failed === 1 ? '' : 's'} not updated`,
      description: userMessage(firstFailure),
    })
  }

  const filtersActive = search.trim().length > 0 || statusFilter !== ALL || priorityFilter !== ALL
  const openCount = filtered.filter((task) => !isTerminalStatus(task.status)).length

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <p className="text-xs text-ink-muted">
          <span className="tabular-nums">{filtered.length}</span> task
          {filtered.length === 1 ? '' : 's'} · <span className="tabular-nums">{openCount}</span> open
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

      <div className="flex flex-col gap-2.5 xl:flex-row xl:items-center">
        <div className="min-w-0 flex-1">
          <Input
            data-search-input=""
            type="search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            leadingIcon={<SearchIcon size={15} />}
            placeholder={`Search tasks in ${project.name}`}
            aria-label="Search tasks in this project"
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
            value={
              sort.key === 'title'
                ? 'title'
                : sort.key === 'status'
                  ? 'status'
                  : sort.key === 'priority'
                    ? 'priority'
                    : 'due'
            }
            onValueChange={(value) =>
              setSort({
                key: value === 'title' || value === 'status' || value === 'priority' ? value : 'due',
                direction: 'asc',
              })
            }
          >
            <SelectTrigger className="h-9 w-[10rem] text-sm" aria-label="Sort tasks">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="due">Sort: Due date</SelectItem>
              <SelectItem value="priority">Sort: Priority</SelectItem>
              <SelectItem value="status">Sort: Status</SelectItem>
              <SelectItem value="title">Sort: Title</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      {selectedTasks.length > 0 ? (
        <section
          aria-label="Bulk actions"
          className="flex flex-wrap items-center gap-2 rounded-md border border-accent/30 bg-accent-soft/45 px-3 py-2"
        >
          <span className="flex items-center gap-2 text-xs font-medium text-accent-soft-ink">
            <Checkbox checked onCheckedChange={() => setSelected(new Set())} aria-label="Clear selection" />
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
                <SelectItem value="status">Set status</SelectItem>
                <SelectItem value="priority">Set priority</SelectItem>
                <SelectItem value="project">Assign project</SelectItem>
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
