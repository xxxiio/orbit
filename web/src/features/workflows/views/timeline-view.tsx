import { useMemo } from 'react'
import type { ScheduleBlock } from '@/api/adapters'
import { useWorkspaceSnapshot } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { EmptyState } from '@/components/ui/empty-state'
import { CalendarIcon, ClockIcon } from '@/components/ui/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { milestoneStatusLabel } from '@/features/projects/project-status'
import { isTaskCompleted } from '@/features/projects/project-progress'
import { formatDate, formatMinutes } from '@/features/tasks'
import type { ViewRendererProps } from '../view-types'
import { componentTypeName } from '../view-metadata'
import { ComponentPanel, ProvenanceNote, UnavailablePanel } from './shared'

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
  const title = componentTypeName(component)

  return (
    <ComponentPanel
      title={title}
      eyebrow="Milestones"
      footer={
        <ProvenanceNote>
          Milestone records carry no dates in this API yet. This is the server&apos;s order and status,
          not a time axis.
        </ProvenanceNote>
      }
    >
      {milestones.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<CalendarIcon size={18} />}
          title="No milestones in this project"
          description="Create one from the Milestones view; it will appear here with its status and task count."
        />
      ) : (
        <ol className="relative flex flex-col gap-0 border-l border-line pl-4">
          {milestones.map((milestone) => {
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

function groupByDay(blocks: ScheduleBlock[]): Array<{ day: number; blocks: ScheduleBlock[] }> {
  const sorted = [...blocks].sort((left, right) => left.startsAtMs - right.startsAtMs)
  const groups: Array<{ day: number; blocks: ScheduleBlock[] }> = []
  for (const block of sorted) {
    const date = new Date(block.startsAtMs)
    date.setHours(0, 0, 0, 0)
    const day = date.getTime()
    const current = groups.at(-1)
    if (current && current.day === day) current.blocks.push(block)
    else groups.push({ day, blocks: [block] })
  }
  return groups
}

/** Saved schedule blocks, which do have real start and end times. */
function ScheduleTimeline({ component }: ViewRendererProps) {
  const title = componentTypeName(component)
  const snapshot = useWorkspaceSnapshot()
  const blocks = snapshot.data?.schedule ?? []
  const groups = useMemo(() => groupByDay(blocks), [blocks])

  return (
    <ComponentPanel
      title={title}
      eyebrow="Schedule"
      footer={
        <ProvenanceNote>
          Every block below is a saved calendar block from the workspace snapshot. Plans that have
          not been reviewed and approved do not appear here.
        </ProvenanceNote>
      }
    >
      {snapshot.isPending && !snapshot.data ? (
        <div className="flex flex-col gap-2" aria-busy="true">
          <Skeleton className="h-3 w-2/3" />
          <Skeleton className="h-3 w-1/2" />
        </div>
      ) : blocks.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<ClockIcon size={18} />}
          title="No schedule blocks yet"
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
                      {formatTime(block.startsAtMs)}–{formatTime(block.endsAtMs)}
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

const timeFormatter = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' })

function formatTime(ms: number): string {
  return timeFormatter.format(new Date(ms))
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
