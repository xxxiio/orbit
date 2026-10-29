import { formatTaskTimestamp } from '@/domain'

function TimestampValue({
  value,
  attribute,
}: {
  value: number | null
  attribute: 'created' | 'updated'
}) {
  const formatted = formatTaskTimestamp(value)
  const dataAttribute = attribute === 'created' ? { 'data-task-created': '' } : { 'data-task-updated': '' }

  if (value === null || !formatted) {
    return (
      <span
        {...dataAttribute}
        data-task-timestamp-unknown=""
        className="text-ink-muted"
        title={
          attribute === 'created'
            ? 'No creation history was recorded for this task.'
            : 'No update history was recorded for this task.'
        }
      >
        Unknown
      </span>
    )
  }

  return (
    <time
      {...dataAttribute}
      dateTime={new Date(value).toISOString()}
      title={formatted.full}
      tabIndex={0}
      className="rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
    >
      {formatted.label}
      <span className="sr-only">. Full timestamp: {formatted.full}</span>
    </time>
  )
}

/**
 * Recorded Created / Updated values from the task's own activity history.
 * A missing row stays `Unknown`; the drawer never fills it with "now" or the
 * deadline.
 */
export function TaskTimestamps({
  createdAtMs,
  updatedAtMs,
}: {
  createdAtMs: number | null
  updatedAtMs: number | null
}) {
  return (
    <dl
      data-task-timestamps=""
      className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-2xs text-ink-muted"
    >
      <div className="flex min-w-0 items-center gap-1.5">
        <dt className="font-semibold tracking-label text-ink-faint">Created</dt>
        <dd className="min-w-0 truncate">
          <TimestampValue value={createdAtMs} attribute="created" />
        </dd>
      </div>
      <div className="flex min-w-0 items-center gap-1.5">
        <dt className="font-semibold tracking-label text-ink-faint">Updated</dt>
        <dd className="min-w-0 truncate">
          <TimestampValue value={updatedAtMs} attribute="updated" />
        </dd>
      </div>
    </dl>
  )
}
