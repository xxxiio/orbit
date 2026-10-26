import { useEffect, useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import type { AgentJob, ReadinessEntry, Task } from '@/api/adapters'
import type { TaskStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import {
  defaultNowMs,
  useAgentAttention,
  useProjects,
  useTaskCommand,
  useTaskCounts,
  useTaskReadiness,
  useWorkspaceSnapshot,
  userMessage,
} from '@/hooks'
import { useDocumentTitle } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { IconButton } from '@/components/ui/icon-button'
import {
  AlertCircleIcon,
  AgentsIcon,
  ArrowRightIcon,
  CalendarIcon,
  CheckCircleIcon,
  ClockIcon,
  FolderIcon,
  InboxIcon,
  PlusIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { Panel } from '@/components/ui/panel'
import { Skeleton, SkeletonText } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import {
  formatLongDay,
  formatMoment,
  formatWeekday,
  freshnessLabel,
  isDueToday,
  isOverdue,
  readinessReasonLabel,
  sortByDeadline,
  TaskControlError,
  TaskProjectControl,
  TaskStatusControl,
  useTaskCommandState,
  useTaskDrawer,
} from '@/features/tasks'
import { useRecentProjectIds } from '@/lib/recent-projects'
import { isTaskCompleted, isTaskOpen } from '@/domain'
import {
  agentStateLabel,
  agentStateTone,
  shortAgentId,
} from '@/features/agents'

interface SectionTask {
  task: Task
  readiness?: ReadinessEntry
}

const SECTION_LIMIT = 5

/** The four expandable sections on Home, keyed by id rather than by label. */
type HomeSectionKey = 'overdue' | 'dueToday' | 'ready' | 'blocked'

function HomeSkeleton() {
  return (
    <div className="flex flex-col gap-7" aria-busy="true" aria-label="Loading today">
      <div className="flex flex-col gap-4 border-b border-line pb-6 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <Skeleton className="h-2.5 w-14" />
          <Skeleton className="mt-3 h-9 w-72" />
          <Skeleton className="mt-3 h-3 w-64" />
        </div>
        <Skeleton className="h-9 w-32" />
      </div>
      <div className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
        {Array.from({ length: 4 }, (_, index) => (
          <div key={index} className="bg-surface px-4 py-3">
            <Skeleton className="h-2 w-16" />
            <Skeleton className="mt-2 h-6 w-10" />
          </div>
        ))}
      </div>
      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]">
        <div className="flex flex-col gap-4">
          {Array.from({ length: 3 }, (_, index) => (
            <div key={index} className="rounded-lg border border-line bg-surface">
              <div className="border-b border-line px-4 py-3">
                <Skeleton className="h-3 w-32" />
              </div>
              {Array.from({ length: 3 }, (_, row) => (
                <div key={row} className="flex items-center gap-3 border-b border-line px-4 py-3 last:border-b-0">
                  <Skeleton className="h-3 flex-1" />
                  <Skeleton className="h-6 w-24 rounded-sm" />
                </div>
              ))}
            </div>
          ))}
        </div>
        <div className="flex flex-col gap-4">
          <Skeleton className="h-48 rounded-lg" />
          <Skeleton className="h-64 rounded-lg" />
        </div>
      </div>
    </div>
  )
}

function StatCell({ label, value, hint }: { label: string; value: number; hint?: string }) {
  return (
    <div className="bg-surface px-4 py-3" title={hint}>
      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">{label}</dt>
      <dd className="mt-1 font-display text-2xl tabular-nums leading-none text-ink">{value}</dd>
    </div>
  )
}

function HomeSection({
  title,
  description,
  count,
  tone,
  empty,
  error,
  onRetry,
  retrying,
  loading = false,
  children,
  expanded,
  onToggleExpand,
}: {
  title: string
  description: string
  count: number
  tone: 'danger' | 'warning' | 'accent' | 'neutral'
  empty: string
  error?: ApiError | null
  onRetry?: () => void
  retrying?: boolean
  loading?: boolean
  children: React.ReactNode
  expanded: boolean
  onToggleExpand: () => void
}) {
  const dot =
    tone === 'danger'
      ? 'bg-danger'
      : tone === 'warning'
        ? 'bg-warning'
        : tone === 'accent'
          ? 'bg-accent'
          : 'bg-line-strong'

  return (
    <Panel className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-4 py-3">
        <span aria-hidden="true" className={cn('h-1.5 w-1.5 rounded-full', dot)} />
        <h2 className="text-sm font-semibold text-ink">{title}</h2>
        <span className="rounded-full bg-surface-sunken px-1.5 py-0.5 text-2xs font-semibold tabular-nums text-ink-muted">
          {count}
        </span>
        <p className="hidden text-xs text-ink-muted sm:block">{description}</p>
        {count > SECTION_LIMIT ? (
          <button
            type="button"
            onClick={onToggleExpand}
            className="ml-auto rounded-sm text-2xs font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          >
            {expanded ? 'Show less' : `Show all ${count}`}
          </button>
        ) : null}
      </div>

      {error ? (
        <div className="flex items-center gap-2 px-4 py-3">
          <p className="min-w-0 flex-1 text-xs text-danger-ink">{userMessage(error)}</p>
          {onRetry ? (
            <Button variant="quiet" size="sm" loading={retrying} onClick={onRetry}>
              Retry
            </Button>
          ) : null}
        </div>
      ) : loading && count === 0 ? (
        <div className="px-4 py-3" aria-busy="true">
          <SkeletonText lines={2} />
        </div>
      ) : count === 0 ? (
        <p className="px-4 py-4 text-xs leading-5 text-ink-muted">{empty}</p>
      ) : (
        children
      )}
    </Panel>
  )
}

function AgentAttentionPanel({
  jobs,
  projectName,
  taskTitle,
  loading,
  error,
  onRetry,
  retrying,
}: {
  jobs: AgentJob[]
  projectName: (id: string) => string | null
  taskTitle: (id: string) => string | null
  loading: boolean
  error: ApiError | null
  onRetry: () => void
  retrying: boolean
}) {
  return (
    <Panel className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-4 py-3">
        <AgentsIcon size={15} className="text-ink-muted" />
        <h2 className="text-sm font-semibold text-ink">Agent jobs needing attention</h2>
        <span className="rounded-full bg-surface-sunken px-1.5 py-0.5 text-2xs font-semibold tabular-nums text-ink-muted">
          {jobs.length}
        </span>
        <Link
          to="/agents"
          className="ml-auto rounded-sm text-2xs font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        >
          All agents
        </Link>
      </div>
      {error ? (
        <div className="flex items-center gap-2 px-4 py-3">
          <p className="min-w-0 flex-1 text-xs text-danger-ink">{userMessage(error)}</p>
          <Button variant="quiet" size="sm" loading={retrying} onClick={onRetry}>
            Retry
          </Button>
        </div>
      ) : loading && jobs.length === 0 ? (
        <div className="px-4 py-3" aria-busy="true">
          <SkeletonText lines={2} />
        </div>
      ) : jobs.length === 0 ? (
        <p className="px-4 py-4 text-xs leading-5 text-ink-muted">
          No agent job needs attention. Reviews and resumes are explicit actions on the Agents page.
        </p>
      ) : (
        <ul aria-label="Agent jobs needing attention">
          {jobs.map((job) => (
            <li key={job.id} className="border-b border-line last:border-b-0">
              <Link
                to={`/agents/${encodeURIComponent(job.id)}`}
                className="flex flex-col gap-1 px-4 py-3 transition-colors hover:bg-surface-hover/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/50"
              >
                <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">
                    {taskTitle(job.taskId) ?? shortAgentId(job.id)}
                  </span>
                  <Badge tone={agentStateTone(job.state)} dot>
                    {agentStateLabel(job.state)}
                  </Badge>
                </span>
                <span className="flex flex-wrap items-center gap-x-2 text-2xs text-ink-muted">
                  <span>{projectName(job.projectId) ?? job.projectId}</span>
                  <span aria-hidden="true">·</span>
                  <span className="font-mono">{shortAgentId(job.id)}</span>
                  <span aria-hidden="true">·</span>
                  <span>{formatMoment(job.updatedAtMs)}</span>
                  <ArrowRightIcon size={11} className="ml-auto shrink-0 text-ink-faint" />
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
      {jobs.length > 0 ? (
        <p className="border-t border-line px-4 py-2 text-2xs leading-4 text-ink-faint">
          Review and resume are explicit actions on the job page. Opening one never launches an
          agent.
        </p>
      ) : null}
    </Panel>
  )
}

function HomeTaskRow({
  entry,
  projectName,
  blockedBy,
  compact = false,
  onOpen,
  onStatusChange,
  onProjectChange,
  onChangeProject,
  projects,
  pending,
  error,
  errorField,
}: {
  entry: SectionTask
  projectName: string | null
  blockedBy?: string
  compact?: boolean
  onOpen: () => void
  onStatusChange: (status: TaskStatus) => void
  onProjectChange: (projectId: string | null) => void
  onChangeProject: boolean
  projects: Array<{ id: string; name: string }>
  pending: { taskId: string; field: 'status' | 'project' | 'priority' } | null
  error: ApiError | null
  errorField: 'status' | 'project' | 'priority' | null
}) {
  const { task, readiness } = entry
  const overdue = isOverdue(task, Date.now())

  return (
    <li
      className={cn(
        'flex flex-col gap-2 border-b border-line px-4 py-3 last:border-b-0 hover:bg-surface-hover/40',
        !compact && 'sm:flex-row sm:items-center',
      )}
    >
      <div className="min-w-0 flex-1">
        <button
          type="button"
          onClick={onOpen}
          className="block max-w-full truncate rounded-sm text-left text-sm font-medium text-ink hover:text-accent-soft-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        >
          {task.title}
        </button>
        <p className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-2xs text-ink-muted">
          {projectName ? (
            <span className="inline-flex items-center gap-1">
              <FolderIcon size={11} className="opacity-70" />
              {projectName}
            </span>
          ) : (
            <span className="inline-flex items-center gap-1">
              <InboxIcon size={11} className="opacity-70" />
              Inbox
            </span>
          )}
          {task.deadlineAtMs !== null ? (
            <span className={cn('inline-flex items-center gap-1', overdue && 'font-medium text-danger-ink')}>
              <CalendarIcon size={11} />
              {formatMoment(task.deadlineAtMs)}
            </span>
          ) : null}
          {blockedBy ? (
            <span className="inline-flex items-center gap-1 text-warning-ink">
              <AlertCircleIcon size={11} />
              Waiting on {blockedBy}
            </span>
          ) : readiness && !readiness.actionable ? (
            <span
              className={cn(
                'inline-flex items-center gap-1',
                readiness.reason === 'past_deadline' && 'text-danger-ink',
              )}
            >
              {readinessReasonLabel(readiness.reason)}
            </span>
          ) : null}
        </p>
        {error ? <TaskControlError error={error} field={errorField} taskStatus={task.status} /> : null}
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <TaskStatusControl
          task={task}
          pending={pending?.taskId === task.id && pending.field === 'status'}
          invalid={Boolean(error)}
          onChange={onStatusChange}
        />
        {onChangeProject ? (
          <TaskProjectControl
            task={task}
            projects={projects}
            pending={pending?.taskId === task.id && pending.field === 'project'}
            invalid={Boolean(error)}
            onChange={onProjectChange}
          />
        ) : null}
      </div>
    </li>
  )
}

export function HomePage() {
  useDocumentTitle('Today')
  const { openNewTask } = useWorkspaceActions()
  const { openTask } = useTaskDrawer()
  const snapshot = useWorkspaceSnapshot()
  const projects = useProjects()
  const counts = useTaskCounts()
  const readiness = useTaskReadiness(defaultNowMs())
  const command = useTaskCommand()
  const recentIds = useRecentProjectIds()
  const agentAttention = useAgentAttention()

  const [now, setNow] = useState(() => Date.now())
  const [expanded, setExpanded] = useState<Record<string, boolean>>({})

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [])

  const tasks = snapshot.data?.tasks ?? []
  const taskById = useMemo(() => new Map(tasks.map((task) => [task.id, task])), [tasks])
  const projectById = useMemo(() => new Map(projects.map((project) => [project.id, project])), [projects])

  const readinessById = useMemo(() => {
    const map = new Map<string, ReadinessEntry>()
    for (const entry of readiness.data?.tasks ?? []) map.set(entry.taskId, entry)
    return map
  }, [readiness.data])

  const overview = useMemo(() => {
    const overdue: SectionTask[] = tasks
      .filter((task) => isOverdue(task, now))
      .sort(sortByDeadline)
      .map((task) => ({ task, readiness: readinessById.get(task.id) }))

    const overdueIds = new Set(overdue.map((entry) => entry.task.id))
    const dueToday: SectionTask[] = tasks
      .filter((task) => !overdueIds.has(task.id) && isDueToday(task, now))
      .sort(sortByDeadline)
      .map((task) => ({ task, readiness: readinessById.get(task.id) }))

    const dueTodayIds = new Set(dueToday.map((entry) => entry.task.id))
    const ready: SectionTask[] = (readiness.data?.tasks ?? [])
      .filter((entry) => entry.actionable)
      .map((entry) => taskById.get(entry.taskId))
      .filter((task): task is Task => Boolean(task))
      .filter((task) => !overdueIds.has(task.id) && !dueTodayIds.has(task.id))
      .sort(sortByDeadline)
      .map((task) => ({ task, readiness: readinessById.get(task.id) }))

    const blocked: SectionTask[] = (readiness.data?.tasks ?? [])
      .filter(
        (entry) =>
          entry.blockingTaskIds.length > 0 || entry.status === 'blocked' || entry.status === 'waiting',
      )
      .map((entry) => taskById.get(entry.taskId))
      .filter((task): task is Task => Boolean(task))
      .sort(sortByDeadline)
      .map((task) => ({ task, readiness: readinessById.get(task.id) }))

    return { overdue, dueToday, ready, blocked }
  }, [tasks, readiness.data, readinessById, taskById, now])

  const orderedProjects = useMemo(() => {
    const rank = new Map(recentIds.map((id, index) => [id, index]))
    return [...projects].sort((left, right) => {
      const leftRank = rank.get(left.id)
      const rightRank = rank.get(right.id)
      if (leftRank !== undefined && rightRank !== undefined) return leftRank - rightRank
      if (leftRank !== undefined) return -1
      if (rightRank !== undefined) return 1
      return left.name.localeCompare(right.name)
    })
  }, [projects, recentIds])

  const projectStats = useMemo(() => {
    const stats = new Map<string, { total: number; open: number; done: number }>()
    for (const project of projects) stats.set(project.id, { total: 0, open: 0, done: 0 })
    for (const task of tasks) {
      if (!task.projectId) continue
      const entry = stats.get(task.projectId)
      if (!entry) continue
      entry.total += 1
      if (isTaskCompleted(task.status)) entry.done += 1
      else if (isTaskOpen(task.status)) entry.open += 1
    }
    return stats
  }, [projects, tasks])

  const { changeStatus, changeProject, pending, error, errorTaskId, errorField } =
    useTaskCommandState(command)

  /**
   * Expansion keys are stable section ids (`overdue`, `dueToday`, `ready`,
   * `blocked`). The accessible name of the list is a separate string: using the
   * label as the key made the toggle write `expanded.blocked` while the renderer
   * read `expanded['Blocked tasks']`, so "Show all" only relabelled itself.
   */
  const section = (key: HomeSectionKey, entries: SectionTask[]) =>
    expanded[key] ? entries : entries.slice(0, SECTION_LIMIT)
  const toggleExpanded = (key: string) =>
    setExpanded((current) => ({ ...current, [key]: !current[key] }))

  const blockedLabel = (entry: SectionTask) => {
    const ids = entry.readiness?.blockingTaskIds ?? []
    if (ids.length === 0) return undefined
    const names = ids.map((id) => taskById.get(id)?.title ?? 'another task')
    return names.length > 2 ? `${names.slice(0, 2).join(', ')} +${names.length - 2}` : names.join(', ')
  }

  const renderRows = (entries: SectionTask[], key: HomeSectionKey, label: string, compact = false) => (
    <ul aria-label={label}>
      {section(key, entries).map((entry) => (
        <HomeTaskRow
          key={entry.task.id}
          entry={entry}
          compact={compact}
          projectName={
            entry.task.projectId ? projectById.get(entry.task.projectId)?.name ?? 'Project' : null
          }
          blockedBy={blockedLabel(entry)}
          onOpen={() => openTask(entry.task.id)}
          onStatusChange={(status) => changeStatus(entry.task, status)}
          onProjectChange={(projectId) => changeProject(entry.task, projectId)}
          onChangeProject
          projects={projects}
          pending={pending}
          error={errorTaskId === entry.task.id ? error : null}
          errorField={errorField}
        />
      ))}
    </ul>
  )

  if (snapshot.isPending && !snapshot.data) return <HomeSkeleton />

  if (snapshot.error && !snapshot.data) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="font-display text-3xl font-medium tracking-tight text-ink">Today</h1>
        <Panel>
          <ErrorState
            title="Today could not be loaded"
            description={userMessage(snapshot.error)}
            onRetry={() => void snapshot.refetch()}
            retrying={snapshot.isFetching}
          />
        </Panel>
      </div>
    )
  }

  const recentProjects = orderedProjects.filter((project) => recentIds.includes(project.id)).slice(0, 4)
  const projectsForRail = recentProjects.length > 0 ? recentProjects : orderedProjects.slice(0, 4)
  const readyCount = readiness.data?.actionableCount ?? overview.ready.length

  return (
    <div className="flex flex-col gap-7">
      <header className="flex flex-col gap-5 border-b border-line pb-6 lg:flex-row lg:items-end lg:justify-between">
        <div className="min-w-0">
          <p className="section-label" title="Local time">
            {formatWeekday(now)}
          </p>
          <h1 className="mt-1 font-display text-3xl font-medium tracking-tight text-ink md:text-4xl">
            {formatLongDay(now)}
          </h1>
          <p className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-ink-soft">
            <span>
              <span className="tabular-nums">{counts.open}</span> open
            </span>
            <span aria-hidden="true" className="text-ink-faint">
              ·
            </span>
            <span>
              <span className="tabular-nums">{counts.unassigned}</span> unassigned
            </span>
            <span aria-hidden="true" className="text-ink-faint">
              ·
            </span>
            <span>
              <span className="tabular-nums">{counts.byStatus.done}</span> done
            </span>
          </p>
          <div className="mt-3 flex items-center gap-2 text-2xs text-ink-muted">
            <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-success/80" />
            <span aria-live="polite">{freshnessLabel(snapshot.dataUpdatedAt, now)}</span>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <IconButton
            label="Refresh today"
            tooltip="Read the workspace again"
            icon={<RefreshIcon size={16} />}
            variant="secondary"
            onClick={() => {
              void snapshot.refetch()
              void readiness.refetch()
              void agentAttention.refetch()
            }}
            disabled={snapshot.isFetching || readiness.isFetching}
            className={snapshot.isFetching || readiness.isFetching ? 'opacity-70' : undefined}
          />
          <Button leadingIcon={<PlusIcon size={16} />} onClick={() => openNewTask()}>
            New task
          </Button>
        </div>
      </header>

      <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
        <StatCell label="Open" value={counts.open} hint="Tasks not done or cancelled" />
        <StatCell
          label="Ready now"
          value={readyCount}
          hint="The server says these have no unfinished prerequisites and are ready; overdue and due-today work is listed above"
        />
        <StatCell
          label="Blocked"
          value={overview.blocked.length}
          hint="Waiting on prerequisites, a blocker or another team"
        />
        <StatCell label="Inbox" value={counts.unassigned} hint="Tasks without a project" />
      </dl>

      {counts.total === 0 ? (
        <div className="flex flex-col gap-4">
          <section className="rounded-lg border border-dashed border-line-strong/70 bg-surface/45">
            <EmptyState
              icon={<CheckCircleIcon size={21} />}
              title="Nothing planned yet"
              description="Capture your first task with the button below, or press c anywhere in Orbit. Estimates, deadlines, priorities and projects can come later."
              action={
                <Button leadingIcon={<PlusIcon size={15} />} onClick={() => openNewTask()}>
                  Create the first task
                </Button>
              }
              secondaryAction={
                <Button asChild variant="ghost" size="sm">
                  <Link to="/projects">Open projects</Link>
                </Button>
              }
              className="min-h-[20rem]"
            />
          </section>
          {/* Agent jobs outlive the task list they reference; the attention rail
              is a device-wide read and must not disappear with an empty task list. */}
          <AgentAttentionPanel
            jobs={agentAttention.data ?? []}
            projectName={(id) => projectById.get(id)?.name ?? null}
            taskTitle={(id) => taskById.get(id)?.title ?? null}
            loading={agentAttention.isPending}
            error={agentAttention.error && !agentAttention.data ? agentAttention.error : null}
            onRetry={() => void agentAttention.refetch()}
            retrying={agentAttention.isFetching}
          />
        </div>
      ) : (
        <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_22rem]">
          <div className="flex min-w-0 flex-col gap-4">
            <HomeSection
              title="Overdue"
              description="Past their deadline and still open"
              count={overview.overdue.length}
              tone="danger"
              empty="Nothing is overdue."
              expanded={Boolean(expanded.overdue)}
              onToggleExpand={() => toggleExpanded('overdue')}
            >
              {renderRows(overview.overdue, 'overdue', 'Overdue tasks')}
            </HomeSection>

            <HomeSection
              title="Due today"
              description="Deadlines later today"
              count={overview.dueToday.length}
              tone="warning"
              empty="Nothing is due later today."
              expanded={Boolean(expanded.dueToday)}
              onToggleExpand={() => toggleExpanded('dueToday')}
            >
              {renderRows(overview.dueToday, 'dueToday', 'Tasks due today')}
            </HomeSection>

            <HomeSection
              title="Ready to start"
              description="Server readiness; overdue and due-today work is listed above"
              count={overview.ready.length}
              tone="accent"
              empty="No tasks are marked Ready. Move a planned task to Ready when you can start it."
              error={readiness.error && !readiness.data ? readiness.error : null}
              onRetry={() => void readiness.refetch()}
              retrying={readiness.isFetching}
              loading={readiness.isPending}
              expanded={Boolean(expanded.ready)}
              onToggleExpand={() => toggleExpanded('ready')}
            >
              {renderRows(overview.ready, 'ready', 'Tasks ready to start')}
            </HomeSection>
          </div>

          <aside className="flex min-w-0 flex-col gap-4">
            <HomeSection
              title="Blocked"
              description="Blocked or waiting, with the server's reason"
              count={overview.blocked.length}
              tone="neutral"
              empty="Nothing is blocked or waiting."
              error={readiness.error && !readiness.data ? readiness.error : null}
              onRetry={() => void readiness.refetch()}
              retrying={readiness.isFetching}
              loading={readiness.isPending}
              expanded={Boolean(expanded.blocked)}
              onToggleExpand={() => toggleExpanded('blocked')}
            >
              {renderRows(overview.blocked, 'blocked', 'Blocked tasks', true)}
            </HomeSection>

            <AgentAttentionPanel
              jobs={agentAttention.data ?? []}
              projectName={(id) => projectById.get(id)?.name ?? null}
              taskTitle={(id) => taskById.get(id)?.title ?? null}
              loading={agentAttention.isPending}
              error={agentAttention.error && !agentAttention.data ? agentAttention.error : null}
              onRetry={() => void agentAttention.refetch()}
              retrying={agentAttention.isFetching}
            />

            <Panel className="overflow-hidden">
              <div className="flex items-center justify-between gap-2 border-b border-line px-4 py-3">
                <div>
                  <h2 className="text-sm font-semibold text-ink">
                    {recentProjects.length > 0 ? 'Recently opened' : 'Projects'}
                  </h2>
                  <p className="mt-0.5 text-2xs text-ink-muted">
                    {recentProjects.length > 0
                      ? 'In this browser, newest first'
                      : 'Alphabetical until you open one'}
                  </p>
                </div>
                <Link
                  to="/projects"
                  className="rounded-sm text-2xs font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                >
                  All projects
                </Link>
              </div>
              {projectsForRail.length === 0 ? (
                <EmptyState
                  compact
                  headingLevel="p"
                  title="No projects yet"
                  description="Create one to group tasks and pin a workflow."
                  icon={<FolderIcon size={16} />}
                  action={
                    <Button variant="secondary" size="sm" asChild>
                      <Link to="/projects">Open projects</Link>
                    </Button>
                  }
                />
              ) : (
                <ul aria-label="Projects">
                  {projectsForRail.map((project) => {
                    const stats = projectStats.get(project.id) ?? { total: 0, open: 0, done: 0 }
                    const doneWidth = stats.total > 0 ? (stats.done / stats.total) * 100 : 0
                    return (
                      <li key={project.id} className="border-b border-line last:border-b-0">
                        <Link
                          to={`/projects/${project.id}`}
                          className="flex flex-col gap-1.5 px-4 py-3 transition-colors hover:bg-surface-hover/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/50"
                        >
                          <span className="flex items-center gap-2">
                            <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">
                              {project.name}
                            </span>
                            <span className="shrink-0 text-2xs tabular-nums text-ink-muted">
                              {stats.open} open
                            </span>
                            <ArrowRightIcon size={13} className="shrink-0 text-ink-faint" />
                          </span>
                          <span className="flex items-center gap-2">
                            <span className="h-1 flex-1 overflow-hidden rounded-full bg-surface-sunken">
                              <span
                                aria-hidden="true"
                                className="block h-full rounded-full bg-success/70"
                                style={{ width: `${doneWidth}%` }}
                              />
                            </span>
                            <span className="shrink-0 text-2xs tabular-nums text-ink-faint">
                              {stats.done}/{stats.total}
                            </span>
                          </span>
                        </Link>
                      </li>
                    )
                  })}
                </ul>
              )}
            </Panel>

            <p className="px-1 text-2xs leading-4 text-ink-faint">
              <ClockIcon size={11} className="mr-1 inline align-[-1px]" />
              Readiness, blockers and deadlines come from the server; section grouping and counts
              here are display-only.
            </p>
          </aside>
        </div>
      )}
    </div>
  )
}
