import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import type { AcceptanceCriterion, Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import { TASK_STATUSES } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import {
  defaultNowMs,
  newCommandId,
  useMilestones,
  useProjects,
  useTaskCommand,
  useTaskDetail,
  useTaskReadiness,
  useWorkspaceSnapshot,
  userMessage,
  type CommandMutation,
  type ReadinessEntry,
  type TaskCommandInput,
  type TaskMutationOutcome,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Drawer } from '@/components/ui/drawer'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { IconButton } from '@/components/ui/icon-button'
import {
  AlertCircleIcon,
  ArrowDownIcon,
  ArrowUpIcon,
  CalendarIcon,
  CheckCircleIcon,
  ClockIcon,
  LinkIcon,
  LoaderIcon,
  PlusIcon,
} from '@/components/ui/icons'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { cn } from '@/components/ui/utils'
import { useCommandFeedback } from './use-command-feedback'
import { useTaskDrawer } from './use-task-drawer'
import { Markdown } from './markdown'
import {
  formatDateTime,
  formatMoment,
  priorityLabels,
  priorityTones,
  readinessReasonLabel,
  relativeTime,
  statusLabel,
  statusTones,
  TASK_PRIORITIES as PRIORITIES,
} from './task-meta'

type TaskCommand = CommandMutation<TaskMutationOutcome, TaskCommandInput>

/* ------------------------------------------------------------------ */
/* Edit guard: close must never swallow text that was not sent         */
/* ------------------------------------------------------------------ */

interface EditorHandle {
  id: string
  isDirty: () => boolean
  flush: () => void
}

interface EditGuardValue {
  register: (handle: EditorHandle) => () => void
}

const EditGuardContext = createContext<EditGuardValue | null>(null)

function useDirtyGuard(id: string, isDirty: () => boolean, flush: () => void) {
  const guard = useContext(EditGuardContext)
  const dirtyRef = useRef(isDirty)
  dirtyRef.current = isDirty
  const flushRef = useRef(flush)
  flushRef.current = flush

  useEffect(() => {
    if (!guard) return
    return guard.register({
      id,
      isDirty: () => dirtyRef.current(),
      flush: () => flushRef.current(),
    })
  }, [guard, id])
}

/* ------------------------------------------------------------------ */
/* Entry point                                                         */
/* ------------------------------------------------------------------ */

export function TaskDetailDrawer() {
  const { taskId, closeTask } = useTaskDrawer()
  if (!taskId) return null
  return <TaskDrawerPanel key={taskId} taskId={taskId} onClose={closeTask} />
}

/* ------------------------------------------------------------------ */
/* Panel                                                              */
/* ------------------------------------------------------------------ */

function TaskDrawerPanel({ taskId, onClose }: { taskId: string; onClose: () => void }) {
  const detail = useTaskDetail(taskId)
  const projects = useProjects()
  const snapshot = useWorkspaceSnapshot()
  const milestones = useMilestones(detail.task?.projectId ?? undefined)
  const readiness = useTaskReadiness(defaultNowMs())
  const command = useTaskCommand()
  const { run } = useCommandFeedback()

  const editorsRef = useRef(new Map<string, EditorHandle>())
  const register = useCallback((handle: EditorHandle) => {
    editorsRef.current.set(handle.id, handle)
    return () => {
      editorsRef.current.delete(handle.id)
    }
  }, [])
  const guardValue = useMemo<EditGuardValue>(() => ({ register }), [register])

  const [closeNotice, setCloseNotice] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const lastLabel = useRef('Change')
  const detailField = useRef<'description' | 'priority' | 'labels' | null>(null)
  const inFlight = useRef(false)

  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    if (command.disposition === 'accepted') {
      setAnnouncement(`${lastLabel.current} saved.`)
      if (closeNotice) setCloseNotice('Saved. Close the drawer when you are ready.')
    }
    // Reading the transition into `accepted` is the point of this effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [command.disposition])

  useEffect(() => {
    if (!announcement) return
    const timer = window.setTimeout(() => setAnnouncement(null), 5000)
    return () => window.clearTimeout(timer)
  }, [announcement])

  const requestClose = useCallback(() => {
    const dirty = [...editorsRef.current.values()].filter((editor) => editor.isDirty())
    if (dirty.length > 0) {
      setCloseNotice('Your edit is being saved first. Close again when the server answers.')
      for (const editor of dirty) editor.flush()
      return
    }
    if (inFlight.current || command.isPending) {
      setCloseNotice('Waiting for the server to accept the last change. Close again when it settles.')
      return
    }
    setCloseNotice(null)
    onClose()
  }, [command.isPending, onClose])

  const send = useCallback(
    (label: string, input: TaskCommandInput) => {
      lastLabel.current = label
      inFlight.current = true
      return command.mutateAsync(input).finally(() => {
        inFlight.current = false
      })
    },
    [command],
  )

  const error: ApiError | null = command.disposition === 'rejected' ? command.error : null
  const errorType = error ? command.variables?.type : undefined

  const pendingField = (field: 'title' | 'status' | 'project' | 'milestone' | 'criteria' | 'dependencies') => {
    if (!command.isPending || !command.variables) return false
    switch (command.variables.type) {
      case 'rename':
        return field === 'title'
      case 'change_status':
        return field === 'status'
      case 'assign_project':
        return field === 'project'
      case 'assign_milestone':
        return field === 'milestone'
      case 'acceptance_criteria':
        return field === 'criteria'
      case 'add_prerequisite':
      case 'remove_prerequisite':
        return field === 'dependencies'
      default:
        return false
    }
  }

  const pendingDetail = (field: 'description' | 'priority' | 'labels') =>
    command.isPending &&
    command.variables?.type === 'set_details' &&
    detailField.current === field

  const changeStatus = (status: TaskStatus) => {
    void run('Status not changed', () =>
      send('Status', { type: 'change_status', taskId, status }),
    )
  }

  const assignProject = (projectId: string | null) => {
    void run('Project not changed', () =>
      send('Project', { type: 'assign_project', taskId, projectId }),
    )
  }

  const assignMilestone = (milestoneId: string | null) => {
    void run('Milestone not changed', () =>
      send('Milestone', { type: 'assign_milestone', taskId, milestoneId }),
    )
  }

  const setDetails = (
    label: 'Description' | 'Priority' | 'Labels',
    field: 'description' | 'priority' | 'labels',
    patch: { description?: string; priority?: TaskPriority | null; labels?: string[] },
  ) => {
    if (!detail.task) return
    detailField.current = field
    void run(`${label} not saved`, () =>
      send(label, {
        type: 'set_details',
        taskId,
        description: patch.description ?? detail.task!.description,
        priority: patch.priority !== undefined ? patch.priority : detail.task!.priority,
        labels: patch.labels ?? detail.task!.labels,
      }),
    )
  }

  const addCriterion = async (text: string) => {
    const result = await run('Criterion not added', () =>
      send('Checklist', {
        type: 'acceptance_criteria',
        taskId,
        change: { operation: 'add', criterionId: newCommandId(), text },
      }),
    )
    return result !== null
  }

  const task = detail.task

  return (
    <Drawer
      open
      onOpenChange={(next) => {
        if (!next) requestClose()
      }}
      side="right"
      title={task?.title ?? 'Task details'}
      description="Task details, checklist, dependencies and activity."
      closeLabel="Close task details"
      className="w-full max-w-none border-l border-line sm:w-[min(40rem,94vw)]"
    >
      <EditGuardContext.Provider value={guardValue}>
        <div className="flex h-full min-h-0 flex-col">
          {detail.isLoading && !task ? (
            <DrawerSkeleton />
          ) : detail.error && !task ? (
            <div className="flex flex-1 flex-col">
              <div className="flex items-start justify-between gap-3 border-b border-line px-5 py-4 pr-12">
                <p className="text-sm font-medium text-ink">Task {taskId}</p>
              </div>
              <ErrorState
                title="This task could not be loaded"
                description={userMessage(detail.error)}
                onRetry={detail.refetch}
                className="flex-1"
              />
              <div className="border-t border-line px-5 py-3">
                <Button variant="secondary" size="sm" onClick={requestClose}>
                  Close
                </Button>
              </div>
            </div>
          ) : task ? (
            <>
              <header className="border-b border-line px-5 pb-4 pr-12 pt-5">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-mono text-2xs uppercase tracking-[0.08em] text-ink-muted">
                    {task.id.length > 18 ? `${task.id.slice(0, 8)}…${task.id.slice(-4)}` : task.id}
                  </span>
                  <Badge tone={statusTones[task.status]} dot>
                    {statusLabel(task.status)}
                  </Badge>
                  {task.priority ? (
                    <Badge tone={priorityTones[task.priority]}>{priorityLabels[task.priority]}</Badge>
                  ) : null}
                  {milestones.data?.find((milestone) => milestone.id === task.milestoneId) ? (
                    <Badge tone="neutral">
                      {milestones.data.find((milestone) => milestone.id === task.milestoneId)?.name}
                    </Badge>
                  ) : null}
                </div>

                <InlineTitle
                  value={task.title}
                  disabled={pendingField('title')}
                  onCommit={(title) => {
                    void run('Title not changed', () => send('Title', { type: 'rename', taskId, title }))
                  }}
                />
                {errorType === 'rename' && error ? (
                  <p role="alert" className="mt-1.5 flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                    {userMessage(error)}
                  </p>
                ) : null}

                <p className="mt-1.5 text-xs text-ink-muted">
                  {task.projectId
                    ? projects.find((project) => project.id === task.projectId)?.name ?? 'Project'
                    : 'No project · In the Inbox'}
                </p>
                {detail.error ? (
                  <div className="mt-2 flex items-start gap-2 rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5">
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0 text-warning-ink" />
                    <p className="min-w-0 flex-1 text-xs leading-5 text-warning-ink">
                      Some details could not be refreshed. {userMessage(detail.error)}
                    </p>
                    <button
                      type="button"
                      onClick={detail.refetch}
                      className="shrink-0 text-xs font-medium text-warning-ink underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                    >
                      Retry
                    </button>
                  </div>
                ) : null}
              </header>

              <div className="scrollbar-slim flex-1 overflow-y-auto">
                <div className="divide-y divide-line">
                  <PropertiesSection
                    task={task}
                    projects={projects}
                    milestones={milestones.data ?? []}
                    milestonesLoading={milestones.isPending}
                    command={command}
                    error={error}
                    errorType={errorType}
                    pendingField={pendingField}
                    pendingDetail={pendingDetail}
                    onChangeStatus={changeStatus}
                    onAssignProject={assignProject}
                    onAssignMilestone={assignMilestone}
                    onSetPriority={(priority) => setDetails('Priority', 'priority', { priority })}
                    onSetLabels={(labels) => setDetails('Labels', 'labels', { labels })}
                  />

                  <DescriptionEditor
                    description={task.description}
                    pending={pendingDetail('description')}
                    error={errorType === 'set_details' && detailField.current === 'description' ? error : null}
                    onCommit={(description) => setDetails('Description', 'description', { description })}
                  />

                  <CriteriaSection
                    taskId={taskId}
                    criteria={detail.criteria}
                    pending={pendingField('criteria')}
                    error={errorType === 'acceptance_criteria' ? error : null}
                    command={command}
                    onAdd={addCriterion}
                    onChangeText={(criterion, text) =>
                      void run('Criterion not changed', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: { operation: 'set_text', criterionId: criterion.id, text },
                        }),
                      )
                    }
                    onToggle={(criterion) =>
                      void run('Criterion not changed', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: {
                            operation: 'set_completed',
                            criterionId: criterion.id,
                            completed: !criterion.completed,
                          },
                        }),
                      )
                    }
                    onReorder={(criterion, position) =>
                      void run('Criterion not moved', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: { operation: 'reorder', criterionId: criterion.id, position },
                        }),
                      )
                    }
                    onRemove={(criterion) =>
                      void run('Criterion not removed', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: { operation: 'remove', criterionId: criterion.id },
                        }),
                      )
                    }
                  />

                  <DependenciesSection
                    task={task}
                    tasks={snapshot.data?.tasks ?? []}
                    edges={snapshot.data?.taskDependencies ?? []}
                    readiness={readiness.data?.tasks.find((entry) => entry.taskId === task.id) ?? null}
                    readinessLoading={readiness.isPending}
                    readinessError={readiness.error}
                    onRetryReadiness={() => void readiness.refetch()}
                    pending={pendingField('dependencies')}
                    error={
                      errorType === 'add_prerequisite' || errorType === 'remove_prerequisite' ? error : null
                    }
                    onAdd={(prerequisiteId) =>
                      void run('Prerequisite not added', () =>
                        send('Dependencies', { type: 'add_prerequisite', taskId, prerequisiteId }),
                      )
                    }
                    onRemove={(prerequisiteId) =>
                      void run('Prerequisite not removed', () =>
                        send('Dependencies', { type: 'remove_prerequisite', taskId, prerequisiteId }),
                      )
                    }
                  />

                  <ActivitySection
                    entries={detail.activity}
                    loading={detail.isLoading}
                    now={now}
                  />
                </div>
              </div>

              <footer className="flex flex-col gap-1.5 border-t border-line bg-surface px-5 py-3">
                <div className="flex items-center gap-2 text-xs">
                  {command.isPending ? (
                    <>
                      <LoaderIcon size={13} className="animate-spin text-accent" />
                      <span className="text-ink-muted">Saving…</span>
                    </>
                  ) : announcement ? (
                    <>
                      <CheckCircleIcon size={13} className="text-success" />
                      <span className="text-ink-muted" aria-live="polite">
                        {announcement}
                      </span>
                    </>
                  ) : (
                    <span className="text-ink-faint">Changes save when you leave a field.</span>
                  )}
                </div>
                {error ? (
                  <p
                    role="alert"
                    className="flex items-start gap-1.5 rounded-sm border border-danger/30 bg-danger-soft px-2 py-1.5 text-xs leading-5 text-danger-ink"
                  >
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                    <span className="min-w-0">{userMessage(error)}</span>
                  </p>
                ) : null}
                {closeNotice ? (
                  <p
                    role="status"
                    className="rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5 text-xs leading-5 text-warning-ink"
                  >
                    {closeNotice}
                  </p>
                ) : null}
              </footer>
            </>
          ) : null}
        </div>
      </EditGuardContext.Provider>
    </Drawer>
  )
}

function DrawerSkeleton() {
  return (
    <div className="flex h-full flex-col" aria-busy="true" aria-label="Loading task">
      <div className="border-b border-line px-5 pb-4 pr-12 pt-5">
        <Skeleton className="h-2.5 w-24" />
        <Skeleton className="mt-3 h-6 w-4/5" />
        <Skeleton className="mt-2 h-2.5 w-32" />
      </div>
      <div className="flex-1 space-y-6 px-5 py-5">
        <div className="grid gap-3 sm:grid-cols-2">
          {Array.from({ length: 4 }, (_, index) => (
            <div key={index} className="space-y-2">
              <Skeleton className="h-2 w-16" />
              <Skeleton className="h-8 w-full" />
            </div>
          ))}
        </div>
        <div className="space-y-2">
          <Skeleton className="h-2 w-20" />
          <Skeleton className="h-24 w-full" />
        </div>
        <div className="space-y-2">
          <Skeleton className="h-2 w-28" />
          <Skeleton className="h-3 w-full" />
          <Skeleton className="h-3 w-5/6" />
          <Skeleton className="h-3 w-2/3" />
        </div>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ */
/* Header title                                                       */
/* ------------------------------------------------------------------ */

function InlineTitle({
  value,
  disabled,
  onCommit,
}: {
  value: string
  disabled: boolean
  onCommit: (title: string) => void
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(value)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!editing) setDraft(value)
  }, [value, editing])

  const stateRef = useRef({ editing, draft })
  stateRef.current = { editing, draft }
  const valueRef = useRef(value)
  valueRef.current = value

  const commit = useCallback(() => {
    const next = stateRef.current.draft.trim()
    setEditing(false)
    if (next.length === 0) {
      setDraft(valueRef.current)
      return
    }
    if (next !== valueRef.current) onCommit(next)
  }, [onCommit])

  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    'title',
    () => stateRef.current.editing && stateRef.current.draft.trim() !== valueRef.current,
    () => commitRef.current(),
  )

  if (!editing) {
    return (
      <button
        type="button"
        onClick={() => {
          setEditing(true)
          requestAnimationFrame(() => inputRef.current?.select())
        }}
        className="mt-2 block w-full rounded-sm text-left font-display text-xl font-medium leading-7 tracking-tight text-ink transition-colors hover:text-accent-soft-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        aria-label="Edit task title"
        disabled={disabled}
      >
        {value}
      </button>
    )
  }

  return (
    <input
      ref={inputRef}
      autoFocus
      value={draft}
      disabled={disabled}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault()
          commit()
        }
      }}
      aria-label="Task title"
      className="mt-2 w-full rounded-sm border border-accent/50 bg-surface px-1 py-0.5 font-display text-xl font-medium leading-7 tracking-tight text-ink focus:outline-none focus:ring-2 focus:ring-accent/25"
    />
  )
}

/* ------------------------------------------------------------------ */
/* Properties                                                          */
/* ------------------------------------------------------------------ */

function Section({
  title,
  description,
  children,
  aside,
}: {
  title: string
  description?: string
  children: ReactNode
  aside?: ReactNode
}) {
  return (
    <section className="px-5 py-5" aria-label={title}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-ink">{title}</h3>
          {description ? <p className="mt-0.5 text-xs leading-5 text-ink-muted">{description}</p> : null}
        </div>
        {aside}
      </div>
      <div className="mt-3">{children}</div>
    </section>
  )
}

function FieldError({ error }: { error: ApiError | null }) {
  if (!error) return null
  return (
    <p role="alert" className="mt-1.5 flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
      <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
      <span className="min-w-0">{userMessage(error)}</span>
    </p>
  )
}

function SavingNote({ visible }: { visible: boolean }) {
  if (!visible) return null
  return <p className="mt-1 text-2xs text-ink-muted">Saving…</p>
}

const NONE = '__none__'

function PropertiesSection({
  task,
  projects,
  milestones,
  milestonesLoading,
  command,
  error,
  errorType,
  pendingField,
  pendingDetail,
  onChangeStatus,
  onAssignProject,
  onAssignMilestone,
  onSetPriority,
  onSetLabels,
}: {
  task: Task
  projects: Array<{ id: string; name: string }>
  milestones: Array<{ id: string; name: string; projectId: string }>
  milestonesLoading: boolean
  command: TaskCommand
  error: ApiError | null
  errorType: string | undefined
  pendingField: (field: 'title' | 'status' | 'project' | 'milestone' | 'criteria' | 'dependencies') => boolean
  pendingDetail: (field: 'description' | 'priority' | 'labels') => boolean
  onChangeStatus: (status: TaskStatus) => void
  onAssignProject: (projectId: string | null) => void
  onAssignMilestone: (milestoneId: string | null) => void
  onSetPriority: (priority: TaskPriority | null) => void
  onSetLabels: (labels: string[]) => void
}) {
  const propertyError =
    errorType === 'change_status' ||
    errorType === 'assign_project' ||
    errorType === 'assign_milestone' ||
    errorType === 'set_details'
      ? error
      : null

  return (
    <Section
      title="Details"
      description="Status changes are checked by the server; the value here is always the stored one."
    >
      <div className="grid gap-x-4 gap-y-3 sm:grid-cols-2">
        <div className="flex flex-col gap-1">
          <span className="flex items-center gap-1.5 text-xs font-medium text-ink-muted">
            Status
            {pendingField('status') ? <LoaderIcon size={11} className="animate-spin text-accent" /> : null}
          </span>
          <Select value={task.status} onValueChange={(value) => onChangeStatus(value as TaskStatus)} disabled={command.isPending}>
            <SelectTrigger className="h-9 text-sm" aria-label="Task status">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TASK_STATUSES.map((status) => (
                <SelectItem key={status} value={status}>
                  {statusLabel(status)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-1">
          <span className="flex items-center gap-1.5 text-xs font-medium text-ink-muted">
            Priority
            {pendingDetail('priority') ? <LoaderIcon size={11} className="animate-spin text-accent" /> : null}
          </span>
          <Select
            value={task.priority ?? NONE}
            onValueChange={(value) => onSetPriority(value === NONE ? null : (value as TaskPriority))}
            disabled={command.isPending}
          >
            <SelectTrigger className="h-9 text-sm" aria-label="Task priority">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={NONE}>No priority</SelectItem>
              {PRIORITIES.map((priority) => (
                <SelectItem key={priority} value={priority}>
                  {priorityLabels[priority]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-1">
          <span className="flex items-center gap-1.5 text-xs font-medium text-ink-muted">
            Project
            {pendingField('project') ? <LoaderIcon size={11} className="animate-spin text-accent" /> : null}
          </span>
          <Select
            value={task.projectId ?? NONE}
            onValueChange={(value) => onAssignProject(value === NONE ? null : value)}
            disabled={command.isPending}
          >
            <SelectTrigger className="h-9 text-sm" aria-label="Task project">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={NONE}>No project (Inbox)</SelectItem>
              {projects.map((project) => (
                <SelectItem key={project.id} value={project.id}>
                  {project.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-1">
          <span className="flex items-center gap-1.5 text-xs font-medium text-ink-muted">
            Milestone
            {pendingField('milestone') ? <LoaderIcon size={11} className="animate-spin text-accent" /> : null}
          </span>
          <Select
            value={task.milestoneId ?? NONE}
            onValueChange={(value) => onAssignMilestone(value === NONE ? null : value)}
            disabled={command.isPending || !task.projectId || milestonesLoading}
          >
            <SelectTrigger className="h-9 text-sm" aria-label="Task milestone">
              <SelectValue placeholder={milestonesLoading ? 'Loading…' : 'No milestone'} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={NONE}>No milestone</SelectItem>
              {milestones.map((milestone) => (
                <SelectItem key={milestone.id} value={milestone.id}>
                  {milestone.name}
                </SelectItem>
              ))}
              {task.milestoneId && !milestones.some((milestone) => milestone.id === task.milestoneId) ? (
                <SelectItem value={task.milestoneId}>{task.milestoneId}</SelectItem>
              ) : null}
            </SelectContent>
          </Select>
          {!task.projectId ? (
            <span className="text-2xs leading-4 text-ink-muted">
              Assign a project before choosing a milestone.
            </span>
          ) : null}
        </div>
      </div>

      <div className="mt-4">
        <LabelsEditor
          labels={task.labels}
          pending={pendingDetail('labels')}
          disabled={command.isPending}
          onCommit={(labels) => onSetLabels(labels)}
        />
      </div>

      <FieldError error={propertyError} />
      <SavingNote visible={pendingField('status') || pendingField('project') || pendingField('milestone') || pendingDetail('priority') || pendingDetail('labels')} />
      {errorType === 'assign_project' && error && userMessage(error).includes('milestone') ? (
        <p className="mt-1.5 text-2xs leading-4 text-ink-muted">
          Remove the milestone first, then move the task to another project.
        </p>
      ) : null}
    </Section>
  )
}

function LabelsEditor({
  labels,
  pending,
  disabled,
  onCommit,
}: {
  labels: string[]
  pending: boolean
  disabled: boolean
  onCommit: (labels: string[]) => void
}) {
  const [draft, setDraft] = useState(labels.join(', '))
  const [editing, setEditing] = useState(false)

  useEffect(() => {
    if (!editing) setDraft(labels.join(', '))
  }, [labels, editing])

  const draftRef = useRef(draft)
  draftRef.current = draft
  const labelsRef = useRef(labels)
  labelsRef.current = labels

  const parse = (value: string) =>
    value
      .split(',')
      .map((label) => label.trim())
      .filter((label, index, all) => label.length > 0 && all.indexOf(label) === index)

  const commit = useCallback(() => {
    setEditing(false)
    const next = parse(draftRef.current)
    if (next.join('\u0000') === labelsRef.current.join('\u0000')) return
    onCommit(next)
  }, [onCommit])

  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    'labels',
    () => parse(draftRef.current).join('\u0000') !== labelsRef.current.join('\u0000'),
    () => commitRef.current(),
  )

  if (!editing) {
    return (
      <div>
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-medium text-ink-muted">Labels</span>
          {pending ? <span className="text-2xs text-ink-muted">Saving…</span> : null}
        </div>
        <button
          type="button"
          disabled={disabled}
          onClick={() => setEditing(true)}
          className="mt-1 flex min-h-8 w-full flex-wrap items-center gap-1.5 rounded-sm border border-dashed border-line px-2 py-1 text-left transition-colors hover:border-line-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50"
          aria-label="Edit task labels"
        >
          {labels.length === 0 ? (
            <span className="text-xs text-ink-faint">No labels yet</span>
          ) : (
            labels.map((label) => (
              <Badge key={label} tone="neutral" className="lowercase">
                {label}
              </Badge>
            ))
          )}
        </button>
      </div>
    )
  }

  return (
    <div>
      <label className="text-xs font-medium text-ink-muted" htmlFor="task-labels">
        Labels
      </label>
      <input
        id="task-labels"
        autoFocus
        value={draft}
        disabled={disabled}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault()
            commit()
          }
        }}
        placeholder="design, docs"
        className="mt-1 h-8 w-full rounded-sm border border-line bg-surface px-2 text-sm text-ink placeholder:text-ink-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
      />
      <p className="mt-1 text-2xs text-ink-muted">Separate labels with commas.</p>
    </div>
  )
}

/* ------------------------------------------------------------------ */
/* Description                                                         */
/* ------------------------------------------------------------------ */

function DescriptionEditor({
  description,
  pending,
  error,
  onCommit,
}: {
  description: string
  pending: boolean
  error: ApiError | null
  onCommit: (description: string) => void
}) {
  const [draft, setDraft] = useState(description)
  const [mode, setMode] = useState<'write' | 'preview'>('write')

  useEffect(() => {
    setDraft(description)
  }, [description])

  const draftRef = useRef(draft)
  draftRef.current = draft
  const descriptionRef = useRef(description)
  descriptionRef.current = description

  const commit = useCallback(() => {
    if (draftRef.current === descriptionRef.current) return
    onCommit(draftRef.current)
  }, [onCommit])
  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    'description',
    () => draftRef.current !== descriptionRef.current,
    () => commitRef.current(),
  )

  return (
    <Section
      title="Description"
      description="Markdown is rendered as text. Saves when you leave the field."
      aside={
        pending ? (
          <span className="inline-flex items-center gap-1 text-2xs text-ink-muted">
            <LoaderIcon size={11} className="animate-spin" /> Saving
          </span>
        ) : null
      }
    >
      <Tabs value={mode} onValueChange={(value) => setMode(value as 'write' | 'preview')}>
        <TabsList className="gap-4">
          <TabsTrigger value="write" className="pb-1.5 text-xs">
            Write
          </TabsTrigger>
          <TabsTrigger value="preview" className="pb-1.5 text-xs">
            Preview
          </TabsTrigger>
        </TabsList>
        <TabsContent value="write" className="mt-2">
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onBlur={commit}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
                event.preventDefault()
                commit()
              }
            }}
            rows={8}
            placeholder="What does done look like? Markdown is fine."
            aria-label="Task description"
            className="scrollbar-slim min-h-[8rem] w-full resize-y rounded-md border border-line bg-surface px-3 py-2 font-sans text-sm leading-6 text-ink placeholder:text-ink-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
          />
        </TabsContent>
        <TabsContent value="preview" className="mt-2">
          <div className="min-h-[8rem] rounded-md border border-line bg-surface px-3 py-2">
            {draft.trim().length === 0 ? (
              <p className="text-sm italic text-ink-faint">Nothing to preview yet.</p>
            ) : (
              <Markdown source={draft} />
            )}
          </div>
        </TabsContent>
      </Tabs>
      <FieldError error={error} />
    </Section>
  )
}

/* ------------------------------------------------------------------ */
/* Acceptance criteria                                                 */
/* ------------------------------------------------------------------ */

function CriteriaSection({
  taskId,
  criteria,
  pending,
  error,
  command,
  onAdd,
  onChangeText,
  onToggle,
  onReorder,
  onRemove,
}: {
  taskId: string
  criteria: AcceptanceCriterion[]
  pending: boolean
  error: ApiError | null
  command: TaskCommand
  onAdd: (text: string) => Promise<boolean>
  onChangeText: (criterion: AcceptanceCriterion, text: string) => void
  onToggle: (criterion: AcceptanceCriterion) => void
  onReorder: (criterion: AcceptanceCriterion, position: number) => void
  onRemove: (criterion: AcceptanceCriterion) => void
}) {
  const [newText, setNewText] = useState('')
  const [adding, setAdding] = useState(false)
  const newTextRef = useRef(newText)
  newTextRef.current = newText

  const add = useCallback(async () => {
    const text = newTextRef.current.trim()
    if (text.length === 0) return
    setAdding(true)
    const accepted = await onAdd(text)
    setAdding(false)
    if (accepted) setNewText('')
  }, [onAdd])
  const addRef = useRef(add)
  addRef.current = add

  useDirtyGuard('new-criterion', () => newTextRef.current.trim().length > 0, () => void addRef.current())

  const ordered = [...criteria].sort((left, right) => left.position - right.position)
  const completed = ordered.filter((criterion) => criterion.completed).length

  return (
    <Section
      title="Acceptance criteria"
      description="Checking a criterion updates the checklist only — it never changes the task's status."
      aside={
        ordered.length > 0 ? (
          <span className="text-2xs tabular-nums text-ink-muted">
            {completed}/{ordered.length} done
          </span>
        ) : null
      }
    >
      {ordered.length === 0 ? (
        <p className="text-xs text-ink-muted">No criteria yet. Add the first check below.</p>
      ) : (
        <ul className="flex flex-col gap-1" aria-label={`Acceptance criteria for task ${taskId}`}>
          {ordered.map((criterion, index) => (
            <li key={criterion.id} className="group flex items-start gap-2 rounded-sm px-1 py-1 hover:bg-surface-hover/50">
              <span className="mt-0.5">
                <Checkbox
                  checked={criterion.completed}
                  disabled={command.isPending}
                  onCheckedChange={() => onToggle(criterion)}
                  aria-label={`${criterion.completed ? 'Uncheck' : 'Check'} ${criterion.text}`}
                />
              </span>
              <CriterionText
                criterion={criterion}
                disabled={command.isPending}
                onCommit={(text) => {
                  if (text !== criterion.text) onChangeText(criterion, text)
                }}
              />
              <span className="ml-auto flex shrink-0 items-center gap-0.5">
                <IconButton
                  label={`Move ${criterion.text} up`}
                  icon={<ArrowUpIcon size={14} />}
                  size="sm"
                  variant="quiet"
                  disabled={command.isPending || index === 0}
                  onClick={() => onReorder(criterion, index - 1)}
                />
                <IconButton
                  label={`Move ${criterion.text} down`}
                  icon={<ArrowDownIcon size={14} />}
                  size="sm"
                  variant="quiet"
                  disabled={command.isPending || index === ordered.length - 1}
                  onClick={() => onReorder(criterion, index + 1)}
                />
                <IconButton
                  label={`Remove ${criterion.text}`}
                  icon={<span aria-hidden="true">×</span>}
                  size="sm"
                  variant="quiet"
                  disabled={command.isPending}
                  onClick={() => onRemove(criterion)}
                  className="text-ink-muted hover:text-danger-ink"
                />
              </span>
            </li>
          ))}
        </ul>
      )}

      <form
        className="mt-2 flex items-center gap-1.5"
        onSubmit={(event) => {
          event.preventDefault()
          void add()
        }}
      >
        <input
          value={newText}
          onChange={(event) => setNewText(event.target.value)}
          placeholder="Add a criterion"
          aria-label="New acceptance criterion"
          disabled={command.isPending}
          className="h-8 min-w-0 flex-1 rounded-sm border border-line bg-surface px-2 text-sm text-ink placeholder:text-ink-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
        />
        <Button
          type="submit"
          size="sm"
          variant="secondary"
          loading={adding}
          disabled={newText.trim().length === 0 || command.isPending}
          leadingIcon={<PlusIcon size={14} />}
        >
          Add
        </Button>
      </form>

      <SavingNote visible={pending} />
      <FieldError error={error} />
    </Section>
  )
}

function CriterionText({
  criterion,
  disabled,
  onCommit,
}: {
  criterion: AcceptanceCriterion
  disabled: boolean
  onCommit: (text: string) => void
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(criterion.text)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!editing) setDraft(criterion.text)
  }, [criterion.text, editing])

  const stateRef = useRef({ editing, draft })
  stateRef.current = { editing, draft }
  const textRef = useRef(criterion.text)
  textRef.current = criterion.text

  const commit = useCallback(() => {
    const next = stateRef.current.draft.trim()
    setEditing(false)
    if (next.length === 0) {
      setDraft(textRef.current)
      return
    }
    if (next !== textRef.current) onCommit(next)
  }, [onCommit])
  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    `criterion-${criterion.id}`,
    () => stateRef.current.editing && stateRef.current.draft.trim() !== textRef.current,
    () => commitRef.current(),
  )

  if (!editing) {
    return (
      <button
        type="button"
        disabled={disabled}
        onClick={() => {
          setEditing(true)
          requestAnimationFrame(() => inputRef.current?.select())
        }}
        className={cn(
          'min-w-0 flex-1 rounded-sm text-left text-sm leading-5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50',
          criterion.completed ? 'text-ink-muted line-through' : 'text-ink',
        )}
        aria-label={`Edit criterion ${criterion.text}`}
      >
        {criterion.text}
      </button>
    )
  }

  return (
    <input
      ref={inputRef}
      autoFocus
      value={draft}
      disabled={disabled}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault()
          commit()
        }
      }}
      aria-label="Criterion text"
      className="min-w-0 flex-1 rounded-sm border border-accent/50 bg-surface px-1 py-0.5 text-sm text-ink focus:outline-none focus:ring-2 focus:ring-accent/20"
    />
  )
}

/* ------------------------------------------------------------------ */
/* Dependencies                                                        */
/* ------------------------------------------------------------------ */

function DependenciesSection({
  task,
  tasks,
  edges,
  readiness,
  readinessLoading,
  readinessError,
  onRetryReadiness,
  pending,
  error,
  onAdd,
  onRemove,
}: {
  task: Task
  tasks: Task[]
  edges: Array<{ taskId: string; prerequisiteId: string }>
  readiness: ReadinessEntry | null
  readinessLoading: boolean
  readinessError: ApiError | null
  onRetryReadiness: () => void
  pending: boolean
  error: ApiError | null
  onAdd: (prerequisiteId: string) => void
  onRemove: (prerequisiteId: string) => void
}) {
  const [choice, setChoice] = useState('')
  const titles = useMemo(() => new Map(tasks.map((candidate) => [candidate.id, candidate.title])), [tasks])

  const prerequisites = edges.filter((edge) => edge.taskId === task.id).map((edge) => edge.prerequisiteId)
  const dependents = edges.filter((edge) => edge.prerequisiteId === task.id).map((edge) => edge.taskId)

  const candidates = useMemo(
    () =>
      tasks
        .filter(
          (candidate) =>
            candidate.id !== task.id &&
            !prerequisites.includes(candidate.id) &&
            !dependents.includes(candidate.id),
        )
        .sort((left, right) => left.title.localeCompare(right.title)),
    [tasks, prerequisites, dependents, task.id],
  )

  return (
    <Section
      title="Dependencies"
      description="Readiness and blocking are calculated by the server from these links."
    >
      <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2">
        <p className="flex items-center gap-1.5 text-xs font-medium text-ink">
          <LinkIcon size={13} className="text-accent" />
          Readiness
        </p>
        {readinessLoading ? (
          <Skeleton className="mt-1.5 h-3 w-2/3" />
        ) : readinessError ? (
          <div className="mt-1.5 flex items-center gap-2">
            <p className="text-xs text-danger-ink">{userMessage(readinessError)}</p>
            <Button variant="quiet" size="sm" onClick={onRetryReadiness}>
              Retry
            </Button>
          </div>
        ) : readiness ? (
          <>
            <p className="mt-1 text-xs leading-5 text-ink-soft">{readinessReasonLabel(readiness.reason)}</p>
            {readiness.blockingTaskIds.length > 0 ? (
              <p className="mt-1 text-2xs leading-4 text-ink-muted">
                Blocked by{' '}
                {readiness.blockingTaskIds
                  .map((id) => titles.get(id) ?? id)
                  .join(', ')}
              </p>
            ) : null}
            {readiness.earliestStartAtMs ? (
              <p className="mt-1 flex items-center gap-1 text-2xs text-ink-muted">
                <CalendarIcon size={11} />
                Starts {formatMoment(readiness.earliestStartAtMs)}
              </p>
            ) : null}
            {readiness.deadlineAtMs ? (
              <p className="mt-1 flex items-center gap-1 text-2xs text-ink-muted">
                <ClockIcon size={11} />
                Due {formatMoment(readiness.deadlineAtMs)}
              </p>
            ) : null}
          </>
        ) : (
          <p className="mt-1 text-xs text-ink-muted">Readiness was not reported for this task.</p>
        )}
      </div>

      <div className="mt-4">
        <p className="text-xs font-medium text-ink-muted">Prerequisites</p>
        {prerequisites.length === 0 ? (
          <p className="mt-1 text-xs text-ink-muted">No prerequisites. This task does not wait on anything.</p>
        ) : (
          <ul className="mt-1 flex flex-col gap-1">
            {prerequisites.map((id) => (
              <li key={id} className="flex items-center gap-2 rounded-sm px-1 py-1 text-sm text-ink-soft">
                <span className="min-w-0 flex-1 truncate">{titles.get(id) ?? id}</span>
                <Button
                  variant="quiet"
                  size="sm"
                  disabled={pending}
                  onClick={() => onRemove(id)}
                  aria-label={`Remove prerequisite ${titles.get(id) ?? id}`}
                >
                  Remove
                </Button>
              </li>
            ))}
          </ul>
        )}
        <div className="mt-2 flex items-center gap-1.5">
          <Select value={choice} onValueChange={setChoice} disabled={pending || candidates.length === 0}>
            <SelectTrigger className="h-8 min-w-0 flex-1 text-sm" aria-label="Add a prerequisite">
              <SelectValue placeholder={candidates.length === 0 ? 'No other tasks available' : 'Choose a task'} />
            </SelectTrigger>
            <SelectContent>
              {candidates.map((candidate) => (
                <SelectItem key={candidate.id} value={candidate.id}>
                  {candidate.title}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            size="sm"
            variant="secondary"
            loading={pending}
            disabled={choice.length === 0 || pending}
            onClick={() => {
              if (choice.length === 0) return
              onAdd(choice)
              setChoice('')
            }}
          >
            Add
          </Button>
        </div>
      </div>

      <div className="mt-4">
        <p className="text-xs font-medium text-ink-muted">Dependents</p>
        {dependents.length === 0 ? (
          <p className="mt-1 text-xs text-ink-muted">Nothing waits on this task.</p>
        ) : (
          <ul className="mt-1 flex flex-col gap-0.5">
            {dependents.map((id) => (
              <li key={id} className="truncate text-sm text-ink-soft">
                {titles.get(id) ?? id}
              </li>
            ))}
          </ul>
        )}
        <p className="mt-1 text-2xs leading-4 text-ink-muted">
          Dependents are read-only here. Remove the link from the dependent task instead.
        </p>
      </div>

      <FieldError error={error} />
      <SavingNote visible={pending} />
    </Section>
  )
}

/* ------------------------------------------------------------------ */
/* Activity                                                            */
/* ------------------------------------------------------------------ */

function ActivitySection({
  entries,
  loading,
  now,
}: {
  entries: Array<{ id: string; kind: string; origin: string; occurredAtMs: number; summary: string }>
  loading: boolean
  now: number
}) {
  return (
    <Section title="Activity" description="The server's own record for this task.">
      {loading ? (
        <div className="space-y-2">
          <Skeleton className="h-3 w-3/4" />
          <Skeleton className="h-3 w-2/3" />
          <Skeleton className="h-3 w-1/2" />
        </div>
      ) : entries.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          title="No activity yet"
          description="Changes to this task will be recorded here as you make them."
        />
      ) : (
        <ol className="relative flex flex-col gap-3 border-l border-line pl-4">
          {entries.map((entry) => (
            <li key={entry.id} className="relative">
              <span
                aria-hidden="true"
                className="absolute -left-[1.3125rem] top-1.5 h-1.5 w-1.5 rounded-full bg-line-strong"
              />
              <p className="text-xs leading-5 text-ink">{entry.summary}</p>
              <p className="mt-0.5 flex flex-wrap items-center gap-x-2 text-2xs text-ink-muted">
                <span className="font-mono">{entry.kind}</span>
                <span aria-hidden="true">·</span>
                <span className="truncate" title={entry.origin}>
                  {entry.origin}
                </span>
                <span aria-hidden="true">·</span>
                <time dateTime={new Date(entry.occurredAtMs).toISOString()} title={formatDateTime(entry.occurredAtMs)}>
                  {relativeTime(entry.occurredAtMs, now)}
                </time>
              </p>
            </li>
          ))}
        </ol>
      )}
    </Section>
  )
}
