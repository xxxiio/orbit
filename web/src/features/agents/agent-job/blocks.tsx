/**
 * The small read-only pieces of one agent job: the page skeleton, the refusal
 * for a job this device may not see, the verbatim JSON block, the handoff list
 * and one fact row.
 *
 * None of them read or write: the job page owns the reads, so a second job page
 * cannot drift into a second fetch convention.
 */
import { Link } from 'react-router-dom'
import type { AgentHandoff } from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { AgentsIcon, ClockIcon } from '@/components/ui/icons'
import { Panel } from '@/components/ui/panel'
import { SkeletonText } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { formatMoment } from '@/domain'
import type { ClaimStateCopy } from '../agent-model'

export function JobSkeleton() {
  return (
    <div className="flex flex-col gap-5" aria-busy="true" aria-label="Loading agent job">
      <SkeletonText lines={2} />
      <div className="rounded-lg border border-line p-5">
        <SkeletonText lines={5} />
      </div>
      <div className="rounded-lg border border-line p-5">
        <SkeletonText lines={4} />
      </div>
    </div>
  )
}

export function NotVisiblePanel({
  id,
  unconfirmedLaunch = false,
}: {
  id: string
  /** The dialog navigated here after an unconfirmed launch; say what that means. */
  unconfirmedLaunch?: boolean
}) {
  if (unconfirmedLaunch) {
    return (
      <Panel>
        <EmptyState
          icon={<AgentsIcon size={21} />}
          title="The launch was not confirmed"
          description={
            <>
              Orbit did not get an answer when this browser queued a job, so it opened the id it
              sent (<span className="font-mono text-ink-soft">{id}</span>) instead of sending the
              request again. No job with that id is visible to this device: the launch was probably
              not stored, and nothing was started from here. Queue the task again only after
              checking that no job is listed for it.
            </>
          }
          action={
            <Button asChild variant="secondary" size="sm">
              <Link to="/agents">Back to Agents</Link>
            </Button>
          }
          className="min-h-[20rem]"
        />
      </Panel>
    )
  }
  return (
    <Panel>
      <EmptyState
        icon={<AgentsIcon size={21} />}
        title="This job is not visible to this device"
        description={
          <>
            Agent jobs belong to the device that started them. The server answered 403 for{' '}
            <span className="font-mono text-ink-soft">{id}</span>; it does not confirm whether the
            job exists on another device.
          </>
        }
        action={
          <Button asChild variant="secondary" size="sm">
            <Link to="/agents">Back to Agents</Link>
          </Button>
        }
        className="min-h-[20rem]"
      />
    </Panel>
  )
}

/**
 * What the machine claim means (C13). It is deliberately not a progress bar:
 * until the executor reports, there is no progress to draw.
 */
export function ClaimStateNote({ copy }: { copy: ClaimStateCopy }) {
  return (
    <div
      data-claim-state={copy.kind}
      className={cn(
        'rounded-md border px-3 py-2.5',
        copy.blocksRelaunch
          ? 'border-danger/30 bg-danger-soft'
          : 'border-line bg-surface-sunken/40',
      )}
    >
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={copy.tone} dot>
          {copy.label}
        </Badge>
        <span className="text-2xs font-semibold tracking-label text-ink-faint">
          Machine claim
        </span>
      </div>
      <p
        className={cn(
          'mt-1 text-xs leading-5',
          copy.blocksRelaunch ? 'text-danger-ink' : 'text-ink-soft',
        )}
      >
        {copy.detail}
      </p>
    </div>
  )
}

export function JsonBlock({ label, value }: { label: string; value: unknown }) {
  let text: string
  try {
    text = JSON.stringify(value, null, 2) ?? 'null'
  } catch {
    text = String(value)
  }
  return (
    <div className="min-w-0">
      <p className="text-2xs font-semibold tracking-label text-ink-faint">{label}</p>
      <pre className="scrollbar-slim mt-1 max-h-72 overflow-auto rounded-md border border-line bg-surface-sunken p-3 font-mono text-2xs leading-5 text-ink-soft">
        {text}
      </pre>
    </div>
  )
}

export function HandoffList({ handoffs }: { handoffs: AgentHandoff[] }) {
  if (handoffs.length === 0) {
    return (
      <p className="text-xs leading-5 text-ink-muted">
        This job has no handoffs yet. A review needs a persisted handoff; the executor submits it.
      </p>
    )
  }
  return (
    <ul aria-label="Handoffs" className="flex flex-col gap-2">
      {handoffs.map((handoff, index) => (
        <li key={handoff.handoffId} className="rounded-md border border-line bg-surface-sunken/25 px-3 py-2.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-sm font-medium text-ink">
              {index === handoffs.length - 1 ? 'Latest handoff' : `Handoff ${handoffs.length - index}`}
            </span>
            <span className="font-mono text-2xs text-ink-muted">{handoff.handoffId}</span>
            <span className="ml-auto inline-flex items-center gap-1 text-2xs text-ink-faint">
              <ClockIcon size={11} />
              {formatMoment(handoff.reportedAtMs)}
            </span>
          </div>
          <JsonBlock label="Payload" value={handoff.payload} />
        </li>
      ))}
    </ul>
  )
}

export function Fact({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0">
      <dt className="text-2xs font-semibold tracking-label text-ink-faint">{label}</dt>
      <dd className={cn('break-words text-sm text-ink-soft', mono && 'font-mono text-xs')}>{value}</dd>
    </div>
  )
}

