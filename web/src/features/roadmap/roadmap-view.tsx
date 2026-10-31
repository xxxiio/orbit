import { useMemo, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { Milestone, Task } from '@/api/adapters'
import { defaultNowMs, useTaskGraph } from '@/hooks'
import { humanizeIdentifier } from '@/lib/humanize'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  AlertTriangleIcon,
  ArrowRightIcon,
  CalendarIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  LinkIcon,
} from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import {
  formatMilestoneDate,
  milestoneDateSummary,
  milestoneProgressLabel,
} from '@/features/milestones'
import {
  DAY_MS,
  SCALE_PX_PER_DAY,
  addMonths,
  barGeometry,
  compareMilestones,
  compareTasks,
  formatBarDates,
  groupTasksByMilestone,
  isBarVisible,
  isRoadmapScale,
  milestoneBar,
  startOfLocalDay,
  startOfLocalMonth,
  startOfLocalQuarter,
  startOfLocalWeek,
  taskBar,
  ticksForRange,
  todayOffset,
  type RoadmapScale,
  type TimelineBar,
} from './roadmap-model'
import { MilestoneTargetDialog } from './milestone-target-dialog'

const LEFT_WIDTH = 280

const STATUS_FILTER_OPTIONS = [
  'inbox',
  'planned',
  'ready',
  'in_progress',
  'waiting',
  'blocked',
  'review',
  'deferred',
  'done',
  'cancelled',
] as const

interface MilestoneRow {
  kind: 'milestone'
  key: string
  milestone: Milestone
  bar: TimelineBar | null
  children: number
}

interface TaskRow {
  kind: 'task'
  key: string
  task: Task
  bar: TimelineBar
}

interface GroupRow {
  kind: 'group'
  key: string
  title: string
  children: number
}

type RoadmapRow = MilestoneRow | TaskRow | GroupRow

function alignStart(ms: number, scale: RoadmapScale): number {
  if (scale === 'week') return startOfLocalWeek(ms)
  if (scale === 'month') return startOfLocalMonth(ms)
  return startOfLocalQuarter(ms)
}

function scaleEnd(ms: number, scale: RoadmapScale): number {
  if (scale === 'week') return ms + 7 * DAY_MS
  if (scale === 'month') return addMonths(ms, 1)
  return addMonths(ms, 3)
}

/**
 * The project roadmap, grouped by milestone.
 *
 * Every milestone is a parent row; its tasks sit underneath in authored-date
 * order, and tasks that belong to no milestone follow as "Unassigned". Dates
 * are only ever read from the milestone and task records, so an item without a
 * usable date is listed as unscheduled instead of being drawn with an invented
 * span.
 */
export interface RoadmapViewProps {
  projectName: string
  tasks: Task[]
  milestones: Milestone[]
  onOpenTask: (taskId: string) => void
  onOpenMilestone: (milestone: Milestone) => void
}

export function RoadmapView({
  projectName,
  tasks,
  milestones,
  onOpenTask,
  onOpenMilestone,
}: RoadmapViewProps) {
  const [searchParams, setSearchParams] = useSearchParams()
  const scaleParam = searchParams.get('scale')
  const scale: RoadmapScale = isRoadmapScale(scaleParam) ? scaleParam : 'month'
  const milestoneFilter = searchParams.get('milestone') ?? 'all'
  const statusFilter = searchParams.get('rmstatus') ?? 'all'
  const nowMs = defaultNowMs()
  const graph = useTaskGraph(nowMs, true)
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const [hoverTask, setHoverTask] = useState<string | null>(null)
  const [editingTarget, setEditingTarget] = useState<Milestone | null>(null)

  const commit = (patch: Record<string, string | null>) => {
    setSearchParams((current) => {
      const next = new URLSearchParams(current)
      for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === '' || value === 'all' || (key === 'scale' && value === 'month')) {
          next.delete(key)
        } else {
          next.set(key, value)
        }
      }
      return next
    })
  }

  const graphByTask = useMemo(
    () => new Map((graph.data?.nodes ?? []).map((node) => [node.taskId, node])),
    [graph.data],
  )
  // Hovering or focusing a task highlights what it is waiting on, so a bar can
  // be traced back to the work that gates it.
  const focusedPrerequisites = useMemo(() => {
    if (!hoverTask) return new Set<string>()
    return new Set(graphByTask.get(hoverTask)?.directPrerequisites ?? [])
  }, [hoverTask, graphByTask])

  const filteredTasks = useMemo(
    () =>
      tasks.filter((task) => {
        if (statusFilter !== 'all' && task.status !== statusFilter) return false
        if (milestoneFilter !== 'all') {
          if (milestoneFilter === 'none' ? task.milestoneId != null : task.milestoneId !== milestoneFilter) {
            return false
          }
        }
        return true
      }),
    [tasks, statusFilter, milestoneFilter],
  )

  const visibleMilestones = useMemo(
    () =>
      milestones
        .filter((milestone) => milestoneFilter === 'all' || milestoneFilter === milestone.id)
        .slice()
        .sort(compareMilestones),
    [milestones, milestoneFilter],
  )

  const groups = useMemo(
    () => groupTasksByMilestone(filteredTasks, visibleMilestones),
    [filteredTasks, visibleMilestones],
  )

  const rows = useMemo<RoadmapRow[]>(() => {
    const result: RoadmapRow[] = []
    for (const milestone of visibleMilestones) {
      const children = (groups.byMilestone.get(milestone.id) ?? []).slice().sort(compareTasks)
      const scheduled = children
        .map((task) => ({ task, bar: taskBar(task) }))
        .filter((entry): entry is { task: Task; bar: TimelineBar } => entry.bar !== null)
      result.push({
        kind: 'milestone',
        key: milestone.id,
        milestone,
        bar: milestoneBar(milestone),
        children: scheduled.length,
      })
      if (collapsed.has(milestone.id)) continue
      for (const { task, bar } of scheduled) {
        result.push({ kind: 'task', key: task.id, task, bar })
      }
    }
    if (milestoneFilter === 'all' || milestoneFilter === 'none') {
      const unassigned = groups.unassigned
        .map((task) => ({ task, bar: taskBar(task) }))
        .filter((entry): entry is { task: Task; bar: TimelineBar } => entry.bar !== null)
        .sort((a, b) => compareTasks(a.task, b.task))
      if (unassigned.length > 0) {
        result.push({
          kind: 'group',
          key: '__unassigned__',
          title: 'Unassigned',
          children: unassigned.length,
        })
        if (!collapsed.has('__unassigned__')) {
          for (const { task, bar } of unassigned) {
            result.push({ kind: 'task', key: task.id, task, bar })
          }
        }
      }
    }
    return result
  }, [visibleMilestones, groups, milestoneFilter, collapsed])

  const bars = rows
    .map((row) => (row.kind === 'task' ? row.bar : row.kind === 'milestone' ? row.bar : null))
    .filter((bar): bar is TimelineBar => bar !== null)
  const baseStart = startOfLocalDay(nowMs) - 14 * DAY_MS
  const baseEnd = startOfLocalDay(nowMs) + 90 * DAY_MS
  const startMs = alignStart(Math.min(baseStart, ...bars.map((bar) => bar.startMs)), scale)
  const endMs = scaleEnd(Math.max(baseEnd, ...bars.map((bar) => bar.endMs)), scale)
  const range = { startMs, endMs }
  const pxPerDay = SCALE_PX_PER_DAY[scale]
  const width = Math.max(640, ((endMs - startMs) / DAY_MS) * pxPerDay)
  const ticks = ticksForRange(range, scale)
  const nowX = todayOffset(nowMs, range, pxPerDay)
  const tickLines = ticks.map((tick) => (
    <span
      key={tick.key}
      aria-hidden="true"
      className="absolute inset-y-0 border-l border-line/70"
      style={{ left: ((tick.startMs - startMs) / DAY_MS) * pxPerDay }}
    />
  ))

  const unscheduledTasks = filteredTasks
    .filter((task) => taskBar(task) === null)
    .slice()
    .sort((a, b) => a.title.localeCompare(b.title))
  const unscheduledMilestones = visibleMilestones.filter((milestone) => milestoneBar(milestone) === null)

  const toggle = (id: string) => {
    setCollapsed((current) => {
      const next = new Set(current)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const isBlockedTask = (task: Task) =>
    task.status === 'blocked' || (graphByTask.get(task.id)?.unfinishedPrerequisites.length ?? 0) > 0

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <div
            className="inline-flex rounded-md border border-line bg-surface p-0.5"
            role="group"
            aria-label="Roadmap scale"
          >
            {(['week', 'month', 'quarter'] as const).map((option) => (
              <button
                key={option}
                type="button"
                onClick={() => commit({ scale: option })}
                aria-pressed={scale === option}
                className={cn(
                  'rounded px-2.5 py-1 text-xs font-medium capitalize transition-colors',
                  scale === option
                    ? 'bg-accent-soft text-accent-soft-ink'
                    : 'text-ink-muted hover:bg-surface-hover hover:text-ink',
                )}
              >
                {option}
              </button>
            ))}
          </div>
          <Select value={statusFilter} onValueChange={(value) => commit({ rmstatus: value })}>
            <SelectTrigger className="h-8 w-[9.5rem] text-xs" aria-label="Filter roadmap by status">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All statuses</SelectItem>
              {STATUS_FILTER_OPTIONS.map((status) => (
                <SelectItem key={status} value={status}>
                  {humanizeIdentifier(status)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={milestoneFilter} onValueChange={(value) => commit({ milestone: value })}>
            <SelectTrigger className="h-8 w-[11rem] text-xs" aria-label="Filter roadmap by milestone">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All milestones</SelectItem>
              <SelectItem value="none">No milestone</SelectItem>
              {milestones.map((milestone) => (
                <SelectItem key={milestone.id} value={milestone.id}>
                  {milestone.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="flex flex-wrap items-center gap-3 text-2xs text-ink-muted">
          <span className="inline-flex items-center gap-1">
            <span className="h-2 w-4 rounded-sm bg-accent/70" aria-hidden="true" /> Duration
          </span>
          <span className="inline-flex items-center gap-1">
            <span className="h-2 w-2 rotate-45 bg-warning" aria-hidden="true" /> Checkpoint
          </span>
          <span className="inline-flex items-center gap-1">
            <span className="h-3 border-l-2 border-danger" aria-hidden="true" /> Today
          </span>
        </div>
      </div>

      {rows.length === 0 ? (
        <EmptyState
          icon={<CalendarIcon size={20} />}
          title="Nothing scheduled in this range"
          description="Give a milestone a start or target date, or plan a window on a task. Items without dates stay in the unscheduled list below."
        />
      ) : (
        <div
          className="scrollbar-slim overflow-x-auto rounded-lg border border-line bg-surface"
          role="group"
          aria-label={`${projectName} roadmap timeline`}
        >
          <div className="relative" style={{ width: LEFT_WIDTH + width }}>
            <div className="sticky top-0 z-30 flex border-b border-line bg-surface">
              <div
                className="sticky left-0 z-40 shrink-0 border-r border-line bg-surface px-3 py-2 text-2xs font-semibold text-ink-muted"
                style={{ width: LEFT_WIDTH }}
              >
                {projectName} roadmap
              </div>
              <div className="relative h-9 shrink-0" style={{ width }}>
                {ticks.map((tick) => (
                  <span
                    key={tick.key}
                    className="absolute inset-y-0 border-l border-line px-1.5 py-2 text-2xs text-ink-muted"
                    style={{
                      left: ((tick.startMs - startMs) / DAY_MS) * pxPerDay,
                      width: ((tick.endMs - tick.startMs) / DAY_MS) * pxPerDay,
                    }}
                  >
                    <span className="sticky left-2 whitespace-nowrap">{tick.label}</span>
                  </span>
                ))}
                {nowX >= 0 && nowX <= width ? (
                  <span
                    role="separator"
                    aria-label="Today"
                    className="absolute inset-y-0 z-10 border-l-2 border-danger"
                    style={{ left: nowX }}
                  />
                ) : null}
              </div>
            </div>

            {rows.map((row) => {
              if (row.kind === 'task') {
                const bar = row.bar
                const geometry = barGeometry(bar, range, pxPerDay)
                const visible = isBarVisible(bar, range)
                const blocked = isBlockedTask(row.task)
                const highlighted =
                  hoverTask === row.task.id || focusedPrerequisites.has(row.task.id)
                const done = row.task.status === 'done'
                return (
                  <div
                    key={row.key}
                    data-roadmap-row="task"
                    data-roadmap-id={row.task.id}
                    onMouseLeave={() => setHoverTask(null)}
                    className="flex border-b border-line last:border-b-0"
                  >
                    <div
                      className="sticky left-0 z-20 flex h-8 shrink-0 items-center gap-1.5 border-r border-line bg-surface py-1.5 pr-3"
                      style={{ width: LEFT_WIDTH, paddingLeft: 32 }}
                    >
                      <button
                        type="button"
                        onClick={() => onOpenTask(row.task.id)}
                        onMouseEnter={() => setHoverTask(row.task.id)}
                        onFocus={() => setHoverTask(row.task.id)}
                        className={cn(
                          'min-w-0 flex-1 truncate rounded-sm text-left text-xs transition-colors',
                          hoverTask === row.task.id ? 'text-accent' : 'text-ink-soft hover:text-accent',
                        )}
                        title={row.task.title}
                        aria-label={`Open task ${row.task.title}`}
                      >
                        {row.task.title}
                      </button>
                      <span className="shrink-0 text-2xs text-ink-faint">
                        {humanizeIdentifier(row.task.status)}
                      </span>
                      {blocked ? (
                        <span title="Waiting on unfinished prerequisites">
                          <LinkIcon size={11} className="shrink-0 text-warning" />
                        </span>
                      ) : null}
                    </div>
                    <div className="relative h-8 shrink-0" style={{ width }}>
                      {tickLines}
                      {nowX >= 0 && nowX <= width ? (
                        <span
                          aria-hidden="true"
                          className="absolute inset-y-0 border-l border-danger/40"
                          style={{ left: nowX }}
                        />
                      ) : null}
                      {visible ? (
                        <button
                          type="button"
                          onMouseEnter={() => setHoverTask(row.task.id)}
                          onFocus={() => setHoverTask(row.task.id)}
                          onClick={() => onOpenTask(row.task.id)}
                          aria-label={`${row.task.title}, ${formatBarDates(bar)}${blocked ? ', blocked' : ''}`}
                          title={`${row.task.title} · ${formatBarDates(bar)}${blocked ? ' · blocked' : ''}`}
                          className={cn(
                            'absolute top-1/2 -translate-y-1/2 transition-shadow',
                            bar.shape === 'checkpoint'
                              ? 'flex h-4 items-center justify-center'
                              : 'h-2 rounded-sm',
                            bar.shape === 'duration' &&
                              (done
                                ? 'bg-success/60 hover:bg-success'
                                : blocked
                                  ? 'bg-warning/70 hover:bg-warning'
                                  : 'bg-accent/55 hover:bg-accent'),
                            highlighted && 'ring-2 ring-accent ring-offset-1',
                          )}
                          style={
                            bar.shape === 'checkpoint'
                              ? {
                                  left:
                                    geometry.left +
                                    geometry.width / 2 -
                                    Math.max(10, geometry.width) / 2,
                                  width: Math.max(10, geometry.width),
                                }
                              : { left: geometry.left, width: geometry.width }
                          }
                        >
                          {bar.shape === 'checkpoint' ? (
                            <span
                              aria-hidden="true"
                              className={cn(
                                'h-2 w-2 rotate-45 rounded-[1px]',
                                done ? 'bg-success' : blocked ? 'bg-warning' : 'bg-accent/80',
                              )}
                            />
                          ) : null}
                        </button>
                      ) : null}
                    </div>
                  </div>
                )
              }

              const bar = row.kind === 'milestone' ? row.bar : null
              const geometry = bar ? barGeometry(bar, range, pxPerDay) : null
              const visible = bar ? isBarVisible(bar, range) : false
              const progress = row.kind === 'milestone' ? row.milestone.progress : null
              const blockedCount = progress?.blocked ?? 0
              const expanded = !collapsed.has(row.key)
              return (
                <div
                  key={row.key}
                  data-roadmap-row={row.kind}
                  data-roadmap-id={row.kind === 'milestone' ? row.milestone.id : undefined}
                  onMouseLeave={() => setHoverTask(null)}
                  className={cn(
                    'flex border-b border-line last:border-b-0',
                    row.kind === 'milestone' ? 'bg-surface-sunken/40' : 'bg-surface-sunken/20',
                  )}
                >
                  <div
                    className="sticky left-0 z-20 flex shrink-0 flex-col justify-center border-r border-line bg-surface px-3 py-1.5"
                    style={{ width: LEFT_WIDTH }}
                  >
                    <div className="flex items-center gap-1">
                      <button
                        type="button"
                        onClick={() => toggle(row.key)}
                        aria-expanded={expanded}
                        aria-label={`${expanded ? 'Collapse' : 'Expand'} ${row.kind === 'milestone' ? row.milestone.name : row.title}`}
                        className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded text-ink-muted hover:bg-surface-hover hover:text-ink"
                      >
                        {expanded ? <ChevronDownIcon size={12} /> : <ChevronRightIcon size={12} />}
                      </button>
                      {row.kind === 'milestone' ? (
                        <button
                          type="button"
                          onClick={() => onOpenMilestone(row.milestone)}
                          className="min-w-0 flex-1 truncate rounded-sm text-left text-xs font-semibold text-ink hover:text-accent"
                          title={row.milestone.name}
                          aria-label={`Open milestone ${row.milestone.name}`}
                        >
                          {row.milestone.name}
                        </button>
                      ) : (
                        <span className="min-w-0 flex-1 truncate text-xs font-medium text-ink-muted">
                          {row.title}
                        </span>
                      )}
                      <span
                        className="shrink-0 text-2xs font-normal tabular-nums text-ink-muted"
                        title={`${row.children} scheduled ${row.children === 1 ? 'item' : 'items'}`}
                      >
                        {row.children}
                      </span>
                      {progress ? (
                        <span
                          className="shrink-0 text-2xs tabular-nums text-ink-muted"
                          title={`${progress.completed} of ${progress.total} items complete`}
                        >
                          {progress.percent}%
                        </span>
                      ) : null}
                      {blockedCount > 0 ? (
                        <span
                          className="inline-flex shrink-0 items-center gap-0.5 text-2xs font-medium text-warning-ink"
                          title={`${blockedCount} blocked ${blockedCount === 1 ? 'task' : 'tasks'}`}
                        >
                          <AlertTriangleIcon size={11} />
                          {blockedCount}
                        </span>
                      ) : null}
                    </div>
                    {row.kind === 'milestone' ? (
                      <p className="mt-0.5 flex items-center gap-1 truncate pl-8 text-2xs text-ink-muted">
                        <CalendarIcon size={10} className="shrink-0" />
                        <span className="truncate">{milestoneDateSummary(row.milestone)}</span>
                        {row.milestone.owner ? (
                          <>
                            <span aria-hidden="true">·</span>
                            <span className="truncate">{row.milestone.owner}</span>
                          </>
                        ) : null}
                      </p>
                    ) : null}
                  </div>
                  <div className="relative min-h-8 shrink-0" style={{ width }}>
                    {tickLines}
                    {nowX >= 0 && nowX <= width ? (
                      <span
                        aria-hidden="true"
                        className="absolute inset-y-0 border-l border-danger/40"
                        style={{ left: nowX }}
                      />
                    ) : null}
                    {row.kind === 'milestone' && bar && geometry && visible ? (
                      <>
                        {bar.shape === 'checkpoint' ? (
                          <button
                            type="button"
                            onClick={() => setEditingTarget(row.milestone)}
                            className="group absolute top-1/2 flex -translate-y-1/2 items-center gap-1.5 rounded-sm pr-1"
                            style={{ left: Math.max(0, geometry.left - 5) }}
                            aria-label={`${row.milestone.name}, milestone on ${formatBarDates(bar)}, change target date`}
                            title={`${row.milestone.name} · target ${formatBarDates(bar)}`}
                          >
                            <span
                              aria-hidden="true"
                              className="h-2.5 w-2.5 rotate-45 rounded-[1px] border border-warning-ink/50 bg-warning group-hover:bg-warning-ink"
                            />
                            <span className="whitespace-nowrap text-2xs text-ink-muted group-hover:text-ink">
                              {formatBarDates(bar)}
                            </span>
                          </button>
                        ) : (
                          <>
                            <button
                              type="button"
                              onClick={() => onOpenMilestone(row.milestone)}
                              className={cn(
                                'absolute top-1/2 h-4 -translate-y-1/2 overflow-hidden rounded-sm ring-1 ring-inset transition-colors',
                                row.milestone.status === 'completed'
                                  ? 'bg-success/20 ring-success/40 hover:bg-success/25'
                                  : row.milestone.status === 'blocked'
                                    ? 'bg-danger/15 ring-danger/40 hover:bg-danger/20'
                                    : 'bg-accent/15 ring-accent/35 hover:bg-accent/20',
                              )}
                              style={{ left: geometry.left, width: geometry.width }}
                              aria-label={`${row.milestone.name}, ${formatBarDates(bar)}, ${milestoneProgressLabel(row.milestone)} (${row.milestone.progress.percent}%), open milestone`}
                              title={`${row.milestone.name} · ${formatBarDates(bar)} · ${milestoneProgressLabel(row.milestone)}`}
                            >
                              <span
                                aria-hidden="true"
                                className={cn(
                                  'absolute inset-y-0 left-0',
                                  row.milestone.status === 'completed' ? 'bg-success/70' : 'bg-accent/70',
                                )}
                                style={{ width: `${row.milestone.progress.percent}%` }}
                              />
                            </button>
                            <button
                              type="button"
                              onClick={() => setEditingTarget(row.milestone)}
                              className="group absolute top-1/2 -translate-y-1/2 rounded-sm"
                              style={{ left: Math.max(0, geometry.left + geometry.width - 5) }}
                              aria-label={`${row.milestone.name}, target ${formatMilestoneDate(row.milestone.targetAtMs)}, change target date`}
                              title={`Target ${formatMilestoneDate(row.milestone.targetAtMs)}`}
                            >
                              <span
                                aria-hidden="true"
                                className="block h-2.5 w-2.5 rotate-45 rounded-[1px] border border-warning-ink/60 bg-warning group-hover:bg-warning-ink"
                              />
                            </button>
                          </>
                        )}
                      </>
                    ) : row.kind === 'milestone' ? (
                      <span
                        className="absolute top-1/2 left-2 -translate-y-1/2 rounded-sm border border-dashed border-line-strong px-1.5 py-0.5 text-2xs text-ink-muted"
                        title="Set a target date to place this milestone on the timeline"
                      >
                        No target
                      </span>
                    ) : null}
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {unscheduledTasks.length > 0 || unscheduledMilestones.length > 0 ? (
        <section className="rounded-lg border border-dashed border-line-strong/70 bg-surface/60 p-3">
          <h2 className="text-xs font-semibold text-ink">
            Unscheduled{' '}
            <span className="font-normal text-ink-muted">
              ({unscheduledTasks.length} task{unscheduledTasks.length === 1 ? '' : 's'}
              {unscheduledMilestones.length > 0
                ? `, ${unscheduledMilestones.length} milestone${unscheduledMilestones.length === 1 ? '' : 's'}`
                : ''}
              )
            </span>
          </h2>
          <p className="mt-0.5 text-2xs text-ink-muted">
            These items have no authored dates they can be placed by, so they are not given an
            invented bar. Open one to schedule it.
          </p>
          <ul className="mt-2 flex flex-wrap gap-1.5">
            {unscheduledTasks.map((task) => (
              <li key={task.id}>
                <Button
                  variant="secondary"
                  size="sm"
                  className="h-7 max-w-[18rem] text-xs"
                  onClick={() => onOpenTask(task.id)}
                  title="Open to set planned dates"
                >
                  <span className="truncate">{task.title}</span>
                  <ArrowRightIcon size={12} />
                </Button>
              </li>
            ))}
            {unscheduledMilestones.map((milestone) => (
              <li key={milestone.id}>
                <Button
                  variant="secondary"
                  size="sm"
                  className="h-7 text-xs"
                  data-unscheduled-milestone={milestone.id}
                  onClick={() => setEditingTarget(milestone)}
                  title={milestoneDateSummary(milestone)}
                >
                  <span className="truncate">{milestone.name}</span>
                  <Badge tone="warning">
                    {milestone.targetAtMs == null && milestone.startAtMs != null
                      ? 'No target'
                      : 'No dates'}
                  </Badge>
                </Button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {editingTarget ? (
        <MilestoneTargetDialog
          milestone={editingTarget}
          open
          onOpenChange={(open) => {
            if (!open) setEditingTarget(null)
          }}
        />
      ) : null}
    </div>
  )
}
