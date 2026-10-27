/**
 * Enrolled executors, and the one launch control on the Agents page.
 *
 * The read (C10) is the authority's answer, not a client guess: an empty list
 * means no executor is configured, and this panel says exactly that instead of
 * spinning. Launching is offered only while at least one active device exists;
 * the reason it is off is written out, not hidden in a tooltip.
 */
import { formatMoment } from '@/domain'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ErrorState } from '@/components/ui/error-state'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { SkeletonText } from '@/components/ui/skeleton'
import { AgentsIcon, ClockIcon, RefreshIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { executorSummary, hasNeverClaimed, NEVER_CLAIMED_EXPLANATION, NEVER_CLAIMED_LABEL, type ExecutorReadState } from '../launch-model'
import type { ExecutorState } from './use-executor-state'

const BLOCKER_ID = 'agents-launch-blocked'

export function AgentExecutorsPanel({
  onLaunch,
  executors,
}: {
  onLaunch: () => void
  /** The page's executor read, so its Refresh covers this panel too. */
  executors: ExecutorState
}) {
  const { state, blocker, refetch, fetching } = executors
  const launchable = blocker === null

  return (
    <Panel className="overflow-hidden" data-agent-executors="">
      <PanelHeader
        title="Executors"
        description="Devices that can claim a queued job. This page queues work; it never starts a model."
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              leadingIcon={<RefreshIcon size={14} />}
              onClick={refetch}
              loading={fetching}
            >
              Refresh
            </Button>
            <Button
              size="sm"
              data-launch-agent-job=""
              onClick={onLaunch}
              disabled={!launchable}
              leadingIcon={<AgentsIcon size={14} />}
              aria-describedby={blocker ? BLOCKER_ID : undefined}
            >
              Queue a job
            </Button>
          </div>
        }
      />
      <PanelBody>
        {blocker ? (
          <p
            id={BLOCKER_ID}
            data-launch-blocked=""
            className={cn(
              'mb-3 rounded-md border px-3 py-2 text-xs leading-5',
              state.kind === 'loading'
                ? 'border-line bg-surface-sunken/50 text-ink-muted'
                : 'border-warning/40 bg-warning-soft text-warning-ink',
            )}
          >
            {blocker}
          </p>
        ) : null}
        <ExecutorRead state={state} fetching={fetching} refetch={refetch} />
      </PanelBody>
    </Panel>
  )
}

function ExecutorRead({
  state,
  fetching,
  refetch,
}: {
  state: ExecutorReadState
  fetching: boolean
  refetch: () => void
}) {
  if (state.kind === 'loading') {
    return (
      <div aria-busy="true" aria-label="Loading enrolled executors">
        <SkeletonText lines={2} />
      </div>
    )
  }
  if (state.kind === 'failed') {
    return (
      <ErrorState
        compact
        title="The executor list could not be read"
        description={state.message}
        onRetry={refetch}
        retrying={fetching}
      />
    )
  }
  if (state.kind === 'unsupported') {
    return (
      <p data-executor-unsupported="" className="text-xs leading-5 text-ink-muted">
        This server does not provide the executor list, so this page cannot confirm that a device is
        enrolled. Launching is disabled until the server is updated.
      </p>
    )
  }
  if (state.kind === 'empty') {
    return (
      <div data-executor-empty="" className="flex flex-col gap-1">
        <p className="text-sm text-ink">No executor is enrolled</p>
        <p className="text-xs leading-5 text-ink-muted">
          A queued job waits until an enrolled device claims it. With no executor, nothing would
          ever pick it up, so launching is disabled. Enrol a device as an executor, then refresh
          this panel.
        </p>
      </div>
    )
  }

  return (
    <ul aria-label="Enrolled executors" className="divide-y divide-line">
      {state.all.map((executor) => (
        <li
          key={executor.deviceId}
          data-executor={executor.deviceId}
          data-executor-state={executor.active ? 'active' : 'revoked'}
          className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2 first:pt-0 last:pb-0"
        >
          <Badge tone={executor.active ? 'success' : 'neutral'} dot>
            {executor.active ? 'Active' : 'Revoked'}
          </Badge>
          <span className="min-w-0 flex-1 break-all font-mono text-xs text-ink-soft">
            {executor.deviceId}
          </span>
          <span className="text-2xs text-ink-muted">{executorSummary(executor)}</span>
          {executor.lastClaimAtMs === null ? (
            <Badge tone="warning" data-executor-never-claimed="">
              <ClockIcon size={11} />
              {NEVER_CLAIMED_LABEL}
            </Badge>
          ) : (
            <span className="inline-flex items-center gap-1 text-2xs text-ink-faint">
              <ClockIcon size={11} />
              {`last claim ${formatMoment(executor.lastClaimAtMs)}`}
            </span>
          )}
          {hasNeverClaimed(executor) ? (
            <p className="basis-full text-2xs leading-4 text-ink-muted">
              {NEVER_CLAIMED_EXPLANATION}
            </p>
          ) : null}
        </li>
      ))}
    </ul>
  )
}
