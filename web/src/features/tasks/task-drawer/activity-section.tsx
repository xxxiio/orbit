import { EmptyState } from '@/components/ui/empty-state'
import { Skeleton } from '@/components/ui/skeleton'
import { humanizeIdentifier } from '@/lib/humanize'
import { formatDateTime, relativeTime } from '../task-meta'
import { Section } from './section'

/** `agent:<id>` and `external:<device>` name a source, never a raw identifier. */
function originLabel(origin: string): string {
  if (origin === 'human') return 'You'
  if (origin === 'system') return 'System'
  if (origin.startsWith('agent:')) return 'An agent'
  if (origin.startsWith('external:')) return 'Another device'
  return humanizeIdentifier(origin)
}

export function ActivitySection({
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
                <span className="text-ink-muted">{humanizeIdentifier(entry.kind)}</span>
                <span aria-hidden="true">·</span>
                <span className="truncate" title={originLabel(entry.origin)}>
                  {originLabel(entry.origin)}
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
