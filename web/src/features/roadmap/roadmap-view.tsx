import { useMemo, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { Epic, Milestone, Task } from '@/api/adapters'
import { defaultNowMs, useTaskGraph } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { ArrowRightIcon, CalendarIcon, LinkIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import {
  DAY_MS,
  SCALE_PX_PER_DAY,
  addMonths,
  barGeometry,
  epicBar,
  formatBarDates,
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
import { epicProgressLabel, formatEpicDate } from '@/features/epics'

const LEFT_WIDTH = 268

interface RoadmapRow {
  key: string
  kind: 'epic' | 'task' | 'milestone' | 'group'
  title: string
  depth: number
  bar: TimelineBar | null
  epic?: Epic
  task?: Task
  milestone?: Milestone
  children?: number
}

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

export function RoadmapView({
  projectName,
  tasks,
  epics,
  milestones,
  epicFilter,
  onOpenTask,
  onOpenEpic,
}: {
  projectName: string
  tasks: Task[]
  epics: Epic[]
  milestones: Milestone[]
  epicFilter: string
  onOpenTask: (taskId: string) => void
  onOpenEpic: (epic: Epic) => void
}) {
  const [searchParams, setSearchParams] = useSearchParams()
  const scaleParam = searchParams.get('scale')
  const scale: RoadmapScale = isRoadmapScale(scaleParam) ? scaleParam : 'month'
  const milestoneFilter = searchParams.get('milestone') ?? 'all'
  const statusFilter = searchParams.get('rmstatus') ?? 'all'
  const nowMs = defaultNowMs()
  const graph = useTaskGraph(nowMs, true)
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const [hoverTask, setHoverTask] = useState<string | null>(null)
  const [editingMilestone, setEditingMilestone] = useState<Milestone | null>(null)

  const commit = (patch: Record<string, string | null>) => {
    setSearchParams((current) => {
      const next = new URLSearchParams(current)
      for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === '' || value === 'all' || value === 'month') next.delete(key)
        else next.set(key, value)
      }
      return next
    })
  }

  const graphByTask = useMemo(
    () => new Map((graph.data?.nodes ?? []).map((node) => [node.taskId, node])),
    [graph.data],
  )
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

  const tasksByEpic = useMemo(() => {
    const assigned = new Map<string, Task[]>()
    for (const epic of epics) assigned.set(epic.id, [])
    const unassigned: Task[] = []
    const epicByTask = new Map<string, string>()
    for (const epic of epics) for (const id of epic.taskIds) epicByTask.set(id, epic.id)
    for (const task of filteredTasks) {
      const epicId = epicByTask.get(task.id)
      if (epicId) assigned.get(epicId)?.push(task)
      else unassigned.push(task)
    }
    return { assigned, unassigned }
  }, [epics, filteredTasks])

  const rows = useMemo<RoadmapRow[]>(() => {
    const result: RoadmapRow[] = []
    for (const epic of epics) {
      if (epicFilter !== 'all' && epicFilter !== 'none' && epic.id !== epicFilter) continue
      if (epicFilter === 'none') continue
      const children = tasksByEpic.assigned.get(epic.id) ?? []
      const bar = epicBar(epic, filteredTasks)
      if (!bar) continue
      result.push({
        key: epic.id,
        kind: 'epic',
        title: epic.title,
        depth: 0,
        bar,
        epic,
        children: children.length,
      })
      if (!collapsed.has(epic.id)) {
        for (const task of children) {
          const childBar = taskBar(task)
          if (!childBar) continue
          result.push({
            key: task.id,
            kind: 'task',
            title: task.title,
            depth: 1,
            bar: childBar,
            task,
            epic,
          })
        }
      }
    }
    if (epicFilter === 'all' || epicFilter === 'none') {
      const unassignedRows = tasksByEpic.unassigned
        .map((task) => ({ task, bar: taskBar(task) }))
        .filter((entry): entry is { task: Task; bar: TimelineBar } => entry.bar !== null)
      if (unassignedRows.length > 0) {
        result.push({
          key: '__none__',
          kind: 'group',
          title: 'No epic',
          depth: 0,
          bar: null,
          children: unassignedRows.length,
        })
        if (!collapsed.has('__none__')) {
          for (const { task, bar } of unassignedRows) {
            result.push({
              key: task.id,
              kind: 'task',
              title: task.title,
              depth: 1,
              bar,
              task,
            })
          }
        }
      }
    }
    for (const milestone of milestones) {
      if (milestoneFilter !== 'all' && milestoneFilter !== milestone.id) continue
      const bar = milestoneBar(milestone)
      if (!bar) continue
      result.push({
        key: milestone.id,
        kind: 'milestone',
        title: milestone.name,
        depth: 0,
        bar,
        milestone,
      })
    }
    return result
  }, [epics, tasksByEpic, filteredTasks, epicFilter, milestoneFilter, milestones, collapsed])

  const bars = rows.map((row) => row.bar).filter((bar): bar is TimelineBar => bar !== null)
  const baseStart = startOfLocalDay(nowMs) - 14 * DAY_MS
  const baseEnd = startOfLocalDay(nowMs) + 90 * DAY_MS
  const startMs = alignStart(
    Math.min(baseStart, ...bars.map((bar) => bar.startMs)),
    scale,
  )
  const endMs = scaleEnd(
    Math.max(baseEnd, ...bars.map((bar) => bar.endMs)),
    scale,
  )
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

  const unscheduledTasks = filteredTasks.filter((task) => taskBar(task) === null)
  const unscheduledMilestones = milestones.filter(
    (milestone) =>
      (milestoneFilter === 'all' || milestoneFilter === milestone.id) &&
      milestoneBar(milestone) === null,
  )

  const toggle = (id: string) => {
    setCollapsed((current) => {
      const next = new Set(current)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <div className="inline-flex rounded-md border border-line bg-surface p-0.5" role="group" aria-label="Roadmap scale">
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
              {['inbox', 'planned', 'ready', 'in_progress', 'waiting', 'blocked', 'review', 'deferred', 'done', 'cancelled'].map(
                (status) => (
                  <SelectItem key={status} value={status}>
                    {status.replace('_', ' ')}
                  </SelectItem>
                ),
              )}
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
        <div className="flex items-center gap-3 text-2xs text-ink-muted">
          <span className="inline-flex items-center gap-1">
            <span className="h-2 w-4 rounded-sm bg-accent/70" aria-hidden="true" /> Planned
          </span>
          <span className="inline-flex items-center gap-1">
            <span className="h-2 w-2 rotate-45 bg-warning" aria-hidden="true" /> Milestone
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
          description="Set a start or target date on an epic, a planned window on a task, or a target date on a milestone. Items without dates stay in the unscheduled list below."
        />
      ) : (
        <div className="scrollbar-slim overflow-x-auto rounded-lg border border-line bg-surface">
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
                    aria-label="Today"
                    className="absolute inset-y-0 z-10 border-l-2 border-danger"
                    style={{ left: nowX }}
                  />
                ) : null}
              </div>
            </div>

            {rows.map((row) => {
              const bar = row.bar
              if (!bar) {
                return (
                  <div key={row.key} className="flex border-b border-line last:border-b-0">
                    <button
                      type="button"
                      onClick={() => toggle(row.key)}
                      className="sticky left-0 z-20 flex shrink-0 items-center gap-1.5 border-r border-line bg-surface px-3 py-2 text-left text-xs font-semibold text-ink"
                      style={{ width: LEFT_WIDTH }}
                    >
                      <span aria-hidden="true" className="text-ink-muted">
                        {collapsed.has(row.key) ? '▸' : '▾'}
                      </span>
                      {row.title}
                      <span className="text-2xs font-normal text-ink-muted">{row.children}</span>
                    </button>
                    <div className="relative h-8 shrink-0 border-b border-line/0" style={{ width }}>
                      {tickLines}
                    </div>
                  </div>
                )
              }
              const geometry = barGeometry(bar, range, pxPerDay)
              const visible = isBarVisible(bar, range)
              const blocked =
                row.task?.status === 'blocked' ||
                (row.task ? (graphByTask.get(row.task.id)?.directPrerequisites.length ?? 0) > 0 : false)
              const highlighted =
                row.task != null &&
                (hoverTask === row.task.id || focusedPrerequisites.has(row.task.id))
              const done =
                row.kind === 'epic'
                  ? row.epic?.status === 'completed'
                  : row.task?.status === 'done'
              return (
                <div
                  key={row.key}
                  data-roadmap-row={row.kind}
                  data-roadmap-id={row.task?.id ?? row.epic?.id ?? row.milestone?.id}
                  className={cn(
                    'flex border-b border-line last:border-b-0',
                    row.kind === 'epic' && 'bg-surface-sunken/30',
                  )}
                >
                  <div
                    className="sticky left-0 z-20 flex shrink-0 items-center gap-1.5 border-r border-line bg-surface px-3 py-1.5"
                    style={{ width: LEFT_WIDTH, paddingLeft: 12 + row.depth * 16 }}
                  >
                    {row.kind === 'epic' || row.kind === 'group' ? (
                      <button
                        type="button"
                        onClick={() => toggle(row.key)}
                        className="inline-flex h-4 w-4 items-center justify-center rounded text-2xs text-ink-muted hover:bg-surface-hover"
                        aria-label={collapsed.has(row.key) ? `Expand ${row.title}` : `Collapse ${row.title}`}
                      >
                        {collapsed.has(row.key) ? '▸' : '▾'}
                      </button>
                    ) : (
                      <span aria-hidden="true" className="w-4" />
                    )}
                    {row.kind === 'epic' && row.epic ? (
                      <button
                        type="button"
                        onClick={() => onOpenEpic(row.epic!)}
                        className="min-w-0 flex-1 truncate text-left text-xs font-semibold text-ink hover:text-accent"
                        title={row.title}
                      >
                        {row.title}
                      </button>
                    ) : row.kind === 'milestone' && row.milestone ? (
                      <button
                        type="button"
                        onClick={() => setEditingMilestone(row.milestone!)}
                        className="min-w-0 flex-1 truncate text-left text-xs font-medium text-ink hover:text-accent"
                        title={row.title}
                      >
                        {row.title}
                      </button>
                    ) : row.task ? (
                      <button
                        type="button"
                        onClick={() => onOpenTask(row.task!.id)}
                        className="min-w-0 flex-1 truncate text-left text-xs text-ink-soft hover:text-accent"
                        title={row.title}
                      >
                        {row.title}
                      </button>
                    ) : (
                      <span className="truncate text-xs text-ink-muted">{row.title}</span>
                    )}
                    {row.kind === 'epic' && row.epic ? (
                      <span className="shrink-0 text-2xs tabular-nums text-ink-muted">
                        {row.epic.progress.percent}%
                      </span>
                    ) : null}
                    {blocked ? (
                      <span title="Has unfinished prerequisites">
                        <LinkIcon size={11} className="shrink-0 text-warning" />
                      </span>
                    ) : null}
                  </div>
                  <div
                    className="relative h-8 shrink-0"
                    style={{ width }}
                    onMouseLeave={() => setHoverTask(null)}
                  >
                    {tickLines}
                    {nowX >= 0 && nowX <= width ? (
                      <span
                        aria-hidden="true"
                        className="absolute inset-y-0 border-l border-danger/40"
                        style={{ left: nowX }}
                      />
                    ) : null}
                    {visible && row.kind === 'milestone' ? (
                      <button
                        type="button"
                        onClick={() => setEditingMilestone(row.milestone!)}
                        className="absolute top-1/2 h-2.5 w-2.5 -translate-y-1/2 rotate-45 rounded-[1px] border border-warning-ink/40 bg-warning hover:bg-warning-ink"
                        style={{ left: Math.max(0, geometry.left - 5) }}
                        aria-label={`${row.title}, milestone on ${formatEpicDate(row.milestone?.targetAtMs)}`}
                        title={`${row.title} · ${formatEpicDate(row.milestone?.targetAtMs)}`}
                      />
                    ) : visible ? (
                      <button
                        type="button"
                        onMouseEnter={() => setHoverTask(row.task?.id ?? null)}
                        onFocus={() => setHoverTask(row.task?.id ?? null)}
                        onClick={() =>
                          row.kind === 'epic' && row.epic
                            ? onOpenEpic(row.epic)
                            : row.task
                              ? onOpenTask(row.task.id)
                              : undefined
                        }
                        aria-label={`${row.title}, ${formatBarDates(bar)}${bar.derived ? ', derived from child dates' : ''}`}
                        title={`${row.title} · ${formatBarDates(bar)}${bar.derived ? ' (derived from children)' : ''}`}
                        className={cn(
                          'absolute top-1/2 -translate-y-1/2 overflow-hidden rounded-sm text-left transition-shadow',
                          row.kind === 'epic'
                            ? 'h-4 bg-accent/75 hover:bg-accent'
                            : done
                              ? 'h-2.5 bg-success/70 hover:bg-success'
                              : 'h-2.5 bg-accent/55 hover:bg-accent',
                          bar.derived && 'border border-dashed border-accent/70 bg-accent/20',
                          highlighted && 'ring-2 ring-accent ring-offset-1',
                          blocked && row.kind !== 'epic' && 'bg-warning/70 hover:bg-warning',
                        )}
                        style={{ left: geometry.left, width: geometry.width }}
                      >
                        {row.kind === 'epic' && row.epic && row.epic.progress.total > 0 ? (
                          <span
                            className="absolute inset-y-0 left-0 bg-accent"
                            style={{ width: `${row.epic.progress.percent}%` }}
                          />
                        ) : null}
                        {geometry.width > 90 ? (
                          <span className="relative px-1 text-[0.625rem] font-medium leading-4 text-accent-ink">
                            {row.kind === 'epic'
                              ? `${epicProgressLabel(row.epic!)} · ${row.epic!.progress.percent}%`
                              : row.task?.status}
                          </span>
                        ) : null}
                      </button>
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
          <h3 className="text-xs font-semibold text-ink">
            Unscheduled{' '}
            <span className="font-normal text-ink-muted">
              ({unscheduledTasks.length} task{unscheduledTasks.length === 1 ? '' : 's'}
              {unscheduledMilestones.length > 0
                ? `, ${unscheduledMilestones.length} milestone${unscheduledMilestones.length === 1 ? '' : 's'}`
                : ''}
              )
            </span>
          </h3>
          <p className="mt-0.5 text-2xs text-ink-muted">
            These items have no authored dates, so they are not given an invented bar. Open one to
            schedule it.
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
                  onClick={() => setEditingMilestone(milestone)}
                >
                  <span className="truncate">{milestone.name}</span>
                  <Badge tone="warning">No target</Badge>
                </Button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      <MilestoneTargetDialog
        milestone={editingMilestone}
        open={editingMilestone !== null}
        onOpenChange={(open) => {
          if (!open) setEditingMilestone(null)
        }}
      />
    </div>
  )
}
