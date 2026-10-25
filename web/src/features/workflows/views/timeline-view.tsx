import { useMemo } from 'react'
import type { ScheduleBlock } from '@/api/adapters'
import { useWorkspaceSnapshot } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { EmptyState } from '@/components/ui/empty-state'
import { CalendarIcon, ClockIcon } from '@/components/ui/icons'
import { SkeletonText } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { formatTimeOfDay, milestoneStatusLabel, isTaskCompleted } from '@/domain'
import { formatDate, formatMinutes } from '@/features/tasks'
import type { ViewRendererProps } from '../view-types'
import {
  componentTypeName,
  emptyTextOption,
  milestoneMatchesDeclaredFilter,
  unsupportedMilestoneFilterKeys,
} from '../view-metadata'
import {
  ComponentPanel,
  DeclaredActionsNote,
  ProvenanceNote,
  UnavailablePanel,
} from './shared'

const STATUS_DOT: Record<string, string> = {
  planned: 'bg-line-strong',
  in_progress: 'bg-accent',
  blocked: 'bg-danger',
  completed: 'bg-success',
  cancelled: 'bg-ink-faint',
}

/**
 * Milestones first: the record has no start or end date, so a fake axis would
 * be a lie. This panel says so and shows what the server does provide — status,
 * sequence and the tasks attached to each milestone.
 */
function MilestoneTimeline({ component, tasks, milestones }: ViewRendererProps) {
  const emptyText = emptyTextOption(component)
  const title = componentTypeName(component)
  const filter = component.filter ?? {}
  const unsupported = unsupportedMilestoneFilterKeys(filter)

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This timeline filters milestones by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  if (component.sort) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This timeline declares a sort by “${component.sort}”. Milestones keep the server's order in this build.`}
      />
    )
  }

  const matching = milestones.filter((milestone) =>
    milestoneMatchesDeclaredFilter(milestone, filter),
  )
  const list =
    component.limit && component.limit > 0 ? matching.slice(0, component.limit) : matching

  return (
    <ComponentPanel
      title={title}
      eyebrow="Milestones"
      description={
        Object.keys(filter).length > 0
          ? `Showing milestones where ${Object.entries(filter)
              .map(([key, value]) => `${key.replace(/_/g, ' ')} is ${value}`)
              .join(', ')}.`
          : undefined
      }
      footer={
        <ProvenanceNote>
          Milestone records carry no dates in this API yet. This is the server&apos;s order and status,
          not a time axis.
          {matching.length > list.length
            ? ` Showing the first ${list.length} of ${matching.length} milestones; the workflow sets this limit.`
            : ''}
          {component.actions.length > 0 ? ' ' : ''}
          <DeclaredActionsNote component={component} />
        </ProvenanceNote>
      }
    >
      {list.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<CalendarIcon size={18} />}
          title={emptyText ?? 'No milestones in this project'}
          description="Create one from the Milestones view; it will appear here with its status and task count."
        />
      ) : (
        <ol className="relative flex flex-col gap-0 border-l border-line pl-4">
          {list.map((milestone) => {
            const assigned = tasks.filter((task) => task.milestoneId === milestone.id)
            const done = assigned.filter((task) => isTaskCompleted(task.status)).length
            return (
              <li key={milestone.id} className="relative py-2.5 first:pt-0 last:pb-0">
                <span
                  aria-hidden="true"
                  className={cn(
                    'absolute -left-[1.32rem] top-3.5 h-2 w-2 rounded-full ring-4 ring-surface',
                    STATUS_DOT[milestone.status] ?? 'bg-line-strong',
                  )}
                />
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-sm font-medium text-ink">{milestone.name}</p>
                  <Badge tone={milestone.status === 'completed' ? 'success' : 'neutral'}>
                    {milestoneStatusLabel(milestone.status)}
                  </Badge>
                </div>
                <p className="mt-1 text-2xs tabular-nums text-ink-muted">
                  {assigned.length === 0
                    ? 'No tasks assigned'
                    : `${done} of ${assigned.length} tasks done`}
                </p>
              </li>
            )
          })}
        </ol>
      )}
    </ComponentPanel>
  )
}

function orderBlocks(blocks: ScheduleBlock[], descending: boolean): ScheduleBlock[] {
  return [...blocks].sort((left, right) =>
    descending ? right.startsAtMs - left.startsAtMs : left.startsAtMs - right.startsAtMs,
  )
}

/** Group already-ordered blocks into consecutive calendar days. */
function groupByDay(blocks: ScheduleBlock[]): Array<{ day: number; blocks: ScheduleBlock[] }> {
  const groups: Array<{ day: number; blocks: ScheduleBlock[] }> = []
  for (const block of blocks) {
    const date = new Date(block.startsAtMs)
    date.setHours(0, 0, 0, 0)
    const day = date.getTime()
    const current = groups.at(-1)
    if (current && current.day === day) current.blocks.push(block)
    else groups.push({ day, blocks: [block] })
  }
  return groups
}

/**
 * Order, limit, then group saved blocks. `descending` reverses the whole
 * sequence (latest day first, latest block first within a day); `limit` slices
 * the ordered list so "the first N" means the N the declaration named.
 */
export function scheduleTimelineGroups(
  blocks: ScheduleBlock[],
  { descending = false, limit = null }: { descending?: boolean; limit?: number | null } = {},
): Array<{ day: number; blocks: ScheduleBlock[] }> {
  const ordered = orderBlocks(blocks, descending)
  const limited = limit !== null && limit > 0 ? ordered.slice(0, limit) : ordered
  return groupByDay(limited)
}

/** Saved schedule blocks, which do have real start and end times. */
const SCHEDULE_SORT_FIELDS = new Set(['starts_at', 'startsat', 'start', 'start_time', 'time'])

function ScheduleTimeline({ component }: ViewRendererProps) {
  const emptyText = emptyTextOption(component)
  const title = componentTypeName(component)
  const snapshot = useWorkspaceSnapshot()
  const filter = component.filter ?? {}
  const filterKeys = Object.keys(filter)
  const sort = component.sort?.trim().toLowerCase() ?? null
  // The declared direction and limit are honoured for saved blocks: the panel
  // orders what the workflow asked for, slices to its limit, then groups by day.
  const groups = useMemo(
    () =>
      scheduleTimelineGroups(snapshot.data?.schedule ?? [], {
        descending: component.descending,
        limit: component.limit,
      }),
    [snapshot.data?.schedule, component.descending, component.limit],
  )
  const shownBlocks = groups.reduce((count, group) => count + group.blocks.length, 0)
  const totalBlocks = snapshot.data?.schedule?.length ?? 0

  if (filterKeys.length > 0) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This schedule timeline filters calendar blocks by ${filterKeys.join(', ')}, which this build cannot evaluate.`}
        hint="Calendar blocks have no task status, priority, milestone or project fields to filter on."
      />
    )
  }

  if (sort && !SCHEDULE_SORT_FIELDS.has(sort)) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This schedule timeline sorts blocks by “${component.sort}”. Saved blocks are ordered by their start time in the declared direction.`}
      />
    )
  }

  return (
    <ComponentPanel
      title={title}
      eyebrow="Schedule"
      footer={
        <ProvenanceNote>
          Every block below is a saved calendar block from the workspace snapshot. Plans that have
          not been reviewed and approved do not appear here.
          {totalBlocks > shownBlocks
            ? ` Showing ${shownBlocks} of ${totalBlocks} blocks; the workflow sets this limit.`
            : ''}
          {component.actions.length > 0 ? ' ' : ''}
          <DeclaredActionsNote component={component} />
        </ProvenanceNote>
      }
    >
      {snapshot.isPending && !snapshot.data ? (
        <div aria-busy="true">
          <SkeletonText lines={2} />
        </div>
      ) : shownBlocks === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<ClockIcon size={18} />}
          title={emptyText ?? 'No schedule blocks yet'}
          description="Approve a schedule plan from the Planning view; approved blocks appear here."
        />
      ) : (
        <div className="flex flex-col gap-4">
          {groups.map((group) => (
            <section key={group.day} aria-label={formatDate(group.day)}>
              <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                {formatDate(group.day)}
              </p>
              <ul className="mt-1.5 flex flex-col gap-1">
                {group.blocks.map((block) => (
                  <li
                    key={block.id}
                    className="flex flex-wrap items-baseline gap-x-3 gap-y-1 rounded-sm border border-line bg-surface-sunken/40 px-2.5 py-1.5"
                  >
                    <span className="font-mono text-2xs tabular-nums text-ink-muted">
                      {formatTimeOfDay(block.startsAtMs)}–{formatTimeOfDay(block.endsAtMs)}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-sm text-ink">{block.title}</span>
                    <span className="text-2xs tabular-nums text-ink-faint">
                      {formatMinutes(block.durationMinutes)}
                    </span>
                    <Badge tone="neutral">{block.flexibility}</Badge>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </ComponentPanel>
  )
}

export function TimelineView(props: ViewRendererProps) {
  const source = props.component.source ?? 'milestones'
  if (source === 'milestones') return <MilestoneTimeline {...props} />
  if (source === 'schedule') return <ScheduleTimeline {...props} />
  return (
    <UnavailablePanel
      title={componentTypeName(props.component)}
      reason={`This timeline is declared against “${source}”, which has no dates the browser can plot.`}
      hint="The panel stays empty rather than drawing an axis with invented dates."
    />
  )
}
