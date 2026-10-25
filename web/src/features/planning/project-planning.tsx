import { useCallback, useEffect, useMemo, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import type { Milestone, Project, Task } from '@/api/adapters'
import {
  defaultNowMs,
  isAmbiguousError,
  newCommandId,
  uncertainMessage,
  useApplyApprovedPlan,
  useDayPlan,
  usePlanReview,
  useSchedulePlan,
  userMessage,
  type ApiError,
  type PlannedTaskEntry,
  type SchedulePlanView,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { Field } from '@/components/ui/field'
import {
  AlertCircleIcon,
  AlertTriangleIcon,
  CalendarIcon,
  CheckCircleIcon,
  ClockIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { Input } from '@/components/ui/input'
import { Panel, PanelBody, PanelFooter, PanelHeader } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { cn } from '@/components/ui/utils'
import { formatDateTime, useTaskDrawer } from '@/features/tasks'
import {
  DISPOSITION_LABELS,
  DISPOSITION_TONES,
  buildScheduleApprovalRequest,
  defaultWindow,
  formatDuration,
  parseWindow,
  toDateInput,
} from './planning-utils'

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="bg-surface px-3 py-2.5" title={hint}>
      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-muted">{label}</dt>
      <dd className="mt-1 whitespace-nowrap font-display text-lg tabular-nums leading-none text-ink">
        {value}
      </dd>
    </div>
  )
}

function WarningList({ warnings }: { warnings: string[] }) {
  if (warnings.length === 0) return null
  return (
    <ul className="flex flex-col gap-1 rounded-md border border-warning/35 bg-warning-soft/70 px-3 py-2">
      {warnings.map((warning, index) => (
        <li key={`${index}-${warning}`} className="flex items-start gap-1.5 text-2xs leading-4 text-warning-ink">
          <AlertTriangleIcon size={12} className="mt-0.5 shrink-0" />
          <span className="min-w-0">{warning}</span>
        </li>
      ))}
    </ul>
  )
}

function ReadFailure({ label, error, onRetry, retrying }: { label: string; error: ApiError; onRetry: () => void; retrying: boolean }) {
  return (
    <ErrorState
      compact
      title={`${label} could not be read`}
      description={userMessage(error)}
      onRetry={onRetry}
      retrying={retrying}
    />
  )
}

/* ------------------------------------------------------------------ */
/* Day plan                                                           */
/* ------------------------------------------------------------------ */

function DayPlanRow({ entry, onOpen }: { entry: PlannedTaskEntry; onOpen: (id: string) => void }) {
  return (
    <li className="flex flex-wrap items-start justify-between gap-3 px-3.5 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => onOpen(entry.taskId)}
            className="min-w-0 rounded-sm text-left text-sm font-medium text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          >
            {entry.title}
          </button>
          <Badge tone={DISPOSITION_TONES[entry.disposition]}>
            {DISPOSITION_LABELS[entry.disposition]}
          </Badge>
          <span className="font-mono text-2xs text-ink-faint">{entry.disposition}</span>
        </div>
        {entry.explanation.length > 0 ? (
          <ul className="mt-1 flex flex-col gap-0.5">
            {entry.explanation.map((line, index) => (
              <li key={index} className="text-2xs leading-4 text-ink-muted">
                {line}
              </li>
            ))}
          </ul>
        ) : (
          <p className="mt-1 text-2xs text-ink-faint">The server gave no explanation for this row.</p>
        )}
      </div>
      <div className="shrink-0 text-right text-2xs tabular-nums text-ink-muted">
        <p>{entry.allocatedMinutes > 0 ? formatDuration(entry.allocatedMinutes) : '—'}</p>
        <p className="text-ink-faint">
          {entry.estimatedMinutes === null
            ? 'not estimated'
            : `${formatDuration(entry.estimatedMinutes)} estimate`}
        </p>
        {entry.startOffsetMinutes !== null ? (
          <p className="text-ink-faint">starts +{formatDuration(entry.startOffsetMinutes)}</p>
        ) : null}
      </div>
    </li>
  )
}

function DayPlanPanel({
  project,
  capacity,
  onCommitCapacity,
}: {
  project: Project
  capacity: number
  onCommitCapacity: (minutes: number) => void
}) {
  const nowMs = defaultNowMs()
  const [draft, setDraft] = useState(capacity > 0 ? String(capacity) : '')
  const [localError, setLocalError] = useState<string | null>(null)
  const [showUnplanned, setShowUnplanned] = useState(false)
  const { openTask } = useTaskDrawer()

  useEffect(() => {
    setDraft(capacity > 0 ? String(capacity) : '')
  }, [capacity])

  const plan = useDayPlan(capacity > 0 ? { nowMs, availableMinutes: capacity, projectId: project.id } : null)

  const commit = () => {
    const parsed = Number(draft)
    if (!Number.isInteger(parsed) || parsed <= 0) {
      setLocalError('Enter the minutes you can actually spend, as a whole number.')
      return
    }
    if (parsed > 24 * 60) {
      setLocalError('A day plan is limited to 24 hours. Split longer work into several days.')
      return
    }
    setLocalError(null)
    onCommitCapacity(parsed)
  }

  const error = localError

  const planned = (plan.data?.tasks ?? []).filter(
    (entry) => entry.disposition !== 'not_actionable' && entry.disposition !== 'outside_project',
  )
  const unplanned = (plan.data?.tasks ?? []).filter(
    (entry) => entry.disposition === 'not_actionable' || entry.disposition === 'outside_project',
  )

  return (
    <Panel className="flex flex-col">
      <PanelHeader
        title="Day plan"
        eyebrow="Capacity"
        description="How much time you have decides what the server proposes. Nothing is scheduled here."
        actions={
          plan.data ? (
            <Button
              variant="quiet"
              size="sm"
              leadingIcon={<RefreshIcon size={14} className={plan.isFetching ? 'animate-spin' : undefined} />}
              onClick={() => void plan.refetch()}
              disabled={plan.isFetching}
            >
              Refresh
            </Button>
          ) : null
        }
      />
      <PanelBody className="flex flex-col gap-4">
        <div className="flex flex-wrap items-end gap-2">
          <Field label="Available minutes today" error={error ?? undefined} className="w-52">
            <Input
              type="number"
              min={1}
              max={1440}
              step={15}
              inputMode="numeric"
              value={draft}
              onChange={(event) => {
                setDraft(event.target.value)
                if (localError) setLocalError(null)
              }}
              onBlur={commit}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault()
                  commit()
                }
              }}
              placeholder="e.g. 120"
              aria-invalid={error ? true : undefined}
            />
          </Field>
          <Button variant="secondary" size="md" onClick={commit}>
            Build day plan
          </Button>
        </div>

        {capacity <= 0 ? (
          <EmptyState
            compact
            headingLevel="p"
            icon={<ClockIcon size={18} />}
            title="Tell the server how much time you have"
            description="A day plan needs an authored capacity. Enter the minutes and press Build day plan; no guess is made for you."
          />
        ) : plan.error ? (
          <ReadFailure
            label="Day plan"
            error={plan.error}
            onRetry={() => void plan.refetch()}
            retrying={plan.isFetching}
          />
        ) : plan.isPending || !plan.data ? (
          <div className="flex flex-col gap-2" aria-busy="true">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-3 w-2/3" />
            <Skeleton className="h-3 w-1/2" />
          </div>
        ) : (
          <>
            <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
              <Stat label="Capacity" value={formatDuration(plan.data.availableMinutes)} />
              <Stat label="Allocated" value={formatDuration(plan.data.allocatedMinutes)} />
              <Stat label="Remaining" value={formatDuration(plan.data.remainingMinutes)} />
              <Stat
                label="Selected"
                value={`${plan.data.selectedCount}`}
                hint={`${plan.data.actionableCount} actionable · ${plan.data.blockedCount} blocked in this project`}
              />
            </dl>

            <p className="text-2xs text-ink-muted">
              {plan.data.actionableCount} actionable and {plan.data.blockedCount} blocked tasks were
              evaluated. Dispositions and explanations below are the server&apos;s.
            </p>

            <WarningList warnings={plan.data.warnings} />

            {plan.data.tasks.length === 0 ? (
              <EmptyState
                compact
                headingLevel="p"
                icon={<CalendarIcon size={18} />}
                title="The server placed nothing in this day"
                description="There is no actionable, estimated work in this project for that capacity."
              />
            ) : (
              <>
                {planned.length === 0 ? (
                  <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-muted">
                    Nothing in this project can be placed at that capacity. The server&apos;s reasons
                    are listed below.
                  </p>
                ) : (
                  <ul className="divide-y divide-line rounded-lg border border-line">
                    {planned.map((entry) => (
                      <DayPlanRow key={entry.taskId} entry={entry} onOpen={openTask} />
                    ))}
                  </ul>
                )}
                {unplanned.length > 0 ? (
                  <div>
                    <Button
                      variant="quiet"
                      size="sm"
                      aria-expanded={showUnplanned}
                      onClick={() => setShowUnplanned((current) => !current)}
                    >
                      {showUnplanned ? 'Hide' : 'Show'} {unplanned.length} task
                      {unplanned.length === 1 ? '' : 's'} the server could not plan
                    </Button>
                    {showUnplanned ? (
                      <ul className="scrollbar-slim mt-2 max-h-80 divide-y divide-line overflow-y-auto rounded-lg border border-line">
                        {unplanned.map((entry) => (
                          <DayPlanRow key={entry.taskId} entry={entry} onOpen={openTask} />
                        ))}
                      </ul>
                    ) : null}
                  </div>
                ) : null}
              </>
            )}
          </>
        )}
      </PanelBody>
      <PanelFooter>
        <p className="text-2xs leading-4 text-ink-muted">
          A day plan is a proposal from the server. It never books calendar time by itself.
        </p>
      </PanelFooter>
    </Panel>
  )
}

/* ------------------------------------------------------------------ */
/* Schedule window, review and apply                                  */
/* ------------------------------------------------------------------ */

function FreeWindows({ plan }: { plan: SchedulePlanView }) {
  if (plan.freeWindows.length === 0) {
    return <p className="text-2xs text-ink-muted">The whole window is already occupied.</p>
  }
  return (
    <ul className="flex flex-wrap gap-1.5">
      {plan.freeWindows.map((window) => (
        <li
          key={`${window.startsAtMs}-${window.endsAtMs}`}
          className="rounded-full border border-line bg-surface-sunken px-2 py-0.5 text-2xs tabular-nums text-ink-muted"
        >
          {time(window.startsAtMs)}–{time(window.endsAtMs)} · {formatDuration(window.minutes)}
        </li>
      ))}
    </ul>
  )
}

const timeFormatter = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' })

function time(ms: number): string {
  return timeFormatter.format(new Date(ms))
}

function SchedulePanel({
  window,
  taskTitles,
  onWindowChange,
  plan,
  review,
  apply,
  receipt,
  onReview,
  onApply,
  approvalChecked,
  onApprovalChange,
}: {
  window: { date: string; from: string; to: string; error: string | null }
  taskTitles: ReadonlyMap<string, string>
  onWindowChange: (patch: Partial<{ date: string; from: string; to: string }>) => void
  plan: ReturnType<typeof useSchedulePlan>
  review: ReturnType<typeof usePlanReview>
  apply: ReturnType<typeof useApplyApprovedPlan>
  receipt: {
    fingerprint: string
    commandId: string
    acceptedRevision: number
    replayed: boolean
    blockIds: string[]
  } | null
  onReview: () => void
  onApply: () => void
  approvalChecked: boolean
  onApprovalChange: (checked: boolean) => void
}) {
  const data = plan.data
  const reviewData = review.review
  const reviewed = review.reviewedPlan

  return (
    <Panel className="flex flex-col">
      <PanelHeader
        title="Schedule a window"
        eyebrow="Calendar"
        description="The preview covers exactly the window below. Nothing is booked until you review and approve it."
        actions={
          data ? (
            <Button
              variant="quiet"
              size="sm"
              leadingIcon={<RefreshIcon size={14} className={plan.isFetching ? 'animate-spin' : undefined} />}
              onClick={() => void plan.refetch()}
              disabled={plan.isFetching}
            >
              Refresh
            </Button>
          ) : null
        }
      />
      <PanelBody className="flex flex-col gap-4">
        <div className="flex flex-wrap items-end gap-2">
          <Field label="Date" className="w-40">
            <Input
              type="date"
              value={window.date}
              onChange={(event) => onWindowChange({ date: event.target.value })}
              aria-label="Schedule window date"
            />
          </Field>
          <Field label="From" className="w-28">
            <Input
              type="time"
              value={window.from}
              onChange={(event) => onWindowChange({ from: event.target.value })}
              aria-label="Schedule window start time"
            />
          </Field>
          <Field label="To" className="w-28">
            <Input
              type="time"
              value={window.to}
              onChange={(event) => onWindowChange({ to: event.target.value })}
              aria-label="Schedule window end time"
            />
          </Field>
        </div>

        {window.error ? (
          <p role="alert" className="flex items-center gap-1.5 text-xs text-danger-ink">
            <AlertCircleIcon size={13} className="shrink-0" />
            {window.error}
          </p>
        ) : null}

        {!window.error && plan.error ? (
          <ReadFailure
            label="Schedule plan"
            error={plan.error}
            onRetry={() => void plan.refetch()}
            retrying={plan.isFetching}
          />
        ) : !window.error && (plan.isPending || !data) ? (
          <div className="flex flex-col gap-2" aria-busy="true">
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-3 w-2/3" />
          </div>
        ) : !window.error && data ? (
          <>
            <dl className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border border-line bg-line sm:grid-cols-4">
              <Stat label="Window" value={formatDuration(data.availableMinutes)} />
              <Stat label="Placed" value={formatDuration(data.allocatedMinutes)} />
              <Stat label="Occupied" value={formatDuration(data.occupiedMinutes)} />
              <Stat
                label="Free"
                value={`${data.freeWindows.length}`}
                hint="Number of free windows in this planning window"
              />
            </dl>

            <div className="flex flex-col gap-1.5">
              <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">Free windows</p>
              <FreeWindows plan={data} />
            </div>

            <WarningList warnings={data.warnings} />

            {data.slots.length === 0 ? (
              <EmptyState
                compact
                headingLevel="p"
                icon={<CalendarIcon size={18} />}
                title="The server placed nothing in this window"
                description="Widen the window, or check whether the project's tasks are actionable and estimated."
              />
            ) : (
              <div className="flex flex-col gap-1.5">
                <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Proposed blocks
                </p>
                <ul className="divide-y divide-line rounded-lg border border-line">
                  {data.slots.map((slot) => (
                    <li key={slot.taskId} data-schedule-slot={slot.taskId} className="px-3.5 py-2.5">
                      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                        <p className="min-w-0 flex-1 truncate text-sm font-medium text-ink">{slot.title}</p>
                        <p className="whitespace-nowrap font-mono text-2xs tabular-nums text-ink-muted">
                          {time(slot.startsAtMs)}–{time(slot.endsAtMs)} · {formatDuration(slot.minutes)}
                        </p>
                      </div>
                      <p className="mt-0.5 text-2xs text-ink-faint">Server rank {slot.ranking}</p>
                      {slot.explanation.length > 0 ? (
                        <ul className="mt-1 flex flex-col gap-0.5">
                          {slot.explanation.map((line, index) => (
                            <li key={index} className="text-2xs leading-4 text-ink-muted">
                              {line}
                            </li>
                          ))}
                        </ul>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {data.unplaced.length > 0 ? (
              <div className="flex flex-col gap-1.5">
                <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Not placed
                </p>
                <ul className="flex flex-col gap-1">
                  {data.unplaced.map((entry) => (
                    <li
                      key={entry.taskId}
                      data-unplaced-task={entry.taskId}
                      className="flex flex-wrap items-baseline gap-x-2 text-2xs"
                    >
                      <span className="font-medium text-ink-soft">
                        {taskTitles.get(entry.taskId) ?? 'A task outside this project'}
                      </span>
                      <span className="text-ink-muted">{entry.reason}</span>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            <div className="flex flex-wrap items-center gap-2 border-t border-line pt-3">
              <Button
                size="sm"
                variant="secondary"
                onClick={onReview}
                loading={review.isPending}
                disabled={data.slots.length === 0 || review.isPending || apply.isPending}
              >
                Review this plan
              </Button>
              <p className="text-2xs text-ink-muted">
                {data.slots.length === 0
                  ? 'There is nothing to review until the server places at least one task.'
                  : 'Review is read-only: it fingerprints the exact command but writes nothing.'}
              </p>
            </div>
            {review.error ? (
              <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
                <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                <span className="min-w-0 flex-1">
                  Review failed. {userMessage(review.error)}
                </span>
              </p>
            ) : null}
          </>
        ) : null}
      </PanelBody>

      {reviewData && reviewed ? (
        <PanelBody className="border-t border-line bg-surface-sunken/30" data-plan-review="">
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone="accent" dot>
                Reviewed · not applied
              </Badge>
              <p className="text-xs text-ink-muted">
                {reviewData.scheduled_slots.length} block
                {reviewData.scheduled_slots.length === 1 ? '' : 's'} ·{' '}
                {formatDuration(reviewData.total_reserved_minutes)} reserved
              </p>
            </div>

            <dl className="grid gap-2 text-2xs sm:grid-cols-2">
              <div className="min-w-0 rounded-md border border-line bg-surface px-2.5 py-2">
                <dt className="font-semibold uppercase tracking-label text-ink-faint">Fingerprint</dt>
                <dd data-review-fingerprint="" className="mt-0.5 break-all font-mono text-ink-soft">
                  {reviewData.fingerprint}
                </dd>
              </div>
              <div className="min-w-0 rounded-md border border-line bg-surface px-2.5 py-2">
                <dt className="font-semibold uppercase tracking-label text-ink-faint">Command</dt>
                <dd data-review-command="" className="mt-0.5 break-all font-mono text-ink-soft">
                  {reviewData.command_id}
                </dd>
              </div>
            </dl>

            <p className="text-2xs leading-4 text-ink-muted">
              Window {formatDateTime(reviewed.plan.window_start_ms)} →{' '}
              {formatDateTime(reviewed.plan.window_end_ms)} against accepted revision{' '}
              {reviewData.expected_revision}. The server marked this plan as
              requiring explicit approval, and applying it creates calendar blocks.
            </p>

            <label className="flex items-start gap-2 text-xs leading-5 text-ink-soft">
              <Checkbox
                checked={approvalChecked}
                onCheckedChange={(checked) => onApprovalChange(checked === true)}
                aria-label="Approve the reviewed schedule"
                className="mt-0.5"
              />
              <span className="min-w-0">
                I reviewed these {reviewData.scheduled_slots.length} block
                {reviewData.scheduled_slots.length === 1 ? '' : 's'} and approve booking them.
              </span>
            </label>

            <div className="flex flex-wrap items-center gap-2">
              <Button
                size="sm"
                onClick={onApply}
                loading={apply.isPending}
                disabled={!approvalChecked || apply.isPending || apply.disposition === 'accepted'}
              >
                Apply approved plan
              </Button>
              <Button
                size="sm"
                variant="quiet"
                onClick={onReview}
                disabled={review.isPending || apply.isPending}
              >
                Review again
              </Button>
              {!approvalChecked ? (
                <p className="text-2xs text-ink-muted">Tick the approval box to enable Apply.</p>
              ) : null}
            </div>

            {apply.error ? (
              <p
                role="alert"
                className={cn(
                  'flex items-start gap-1.5 text-xs leading-5',
                  isAmbiguousError(apply.error) ? 'text-warning-ink' : 'text-danger-ink',
                )}
              >
                <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                <span className="min-w-0 flex-1">
                  {isAmbiguousError(apply.error)
                    ? uncertainMessage(apply.error)
                    : `The server refused to apply this plan. ${userMessage(apply.error)}`}
                </span>
              </p>
            ) : null}
          </div>
        </PanelBody>
      ) : null}

      {receipt ? (
        <PanelBody className="border-t border-line bg-success-soft/40" data-plan-receipt="">
          <div className="flex flex-col gap-2">
            <div className="flex flex-wrap items-center gap-2">
              <CheckCircleIcon size={16} className="text-success" />
              <p className="text-sm font-medium text-success-ink">
                Plan applied at revision {receipt.acceptedRevision}
                {receipt.replayed ? ' (replayed receipt)' : ''}
              </p>
            </div>
            <dl className="grid gap-2 text-2xs sm:grid-cols-2">
              <div className="min-w-0 rounded-md border border-line bg-surface px-2.5 py-2">
                <dt className="font-semibold uppercase tracking-label text-ink-faint">Receipt fingerprint</dt>
                <dd data-receipt-fingerprint="" className="mt-0.5 break-all font-mono text-ink-soft">
                  {receipt.fingerprint}
                </dd>
              </div>
              <div className="min-w-0 rounded-md border border-line bg-surface px-2.5 py-2">
                <dt className="font-semibold uppercase tracking-label text-ink-faint">Command id</dt>
                <dd data-receipt-command="" className="mt-0.5 break-all font-mono text-ink-soft">
                  {receipt.commandId}
                </dd>
              </div>
            </dl>
            <p className="text-2xs text-ink-muted">
              {receipt.blockIds.length} calendar block
              {receipt.blockIds.length === 1 ? '' : 's'} stored:{' '}
              <span className="break-all font-mono">{receipt.blockIds.join(', ')}</span>. Reload the
              Timeline view to see them in the saved schedule.
            </p>
          </div>
        </PanelBody>
      ) : null}
      <PanelFooter>
        <p className="text-2xs leading-4 text-ink-muted">
          Review → explicit approval → apply. The fingerprint and command id above are the receipt the
          server bound this write to.
        </p>
      </PanelFooter>
    </Panel>
  )
}

/* ------------------------------------------------------------------ */
/* Surface                                                            */
/* ------------------------------------------------------------------ */

export function ProjectPlanning({
  project,
  tasks,
  milestones,
}: {
  project: Project
  /** Accepted once so the panels and the rest of the workspace agree. */
  tasks: Task[]
  milestones: Milestone[]
}) {
  const [searchParams, setSearchParams] = useSearchParams()
  const nowMs = useMemo(() => defaultNowMs(), [])
  const defaults = useMemo(() => defaultWindow(nowMs), [nowMs])
  const { toast } = useToast()

  const capacityValue = Number(searchParams.get('capacity'))
  const capacity = Number.isInteger(capacityValue) && capacityValue > 0 ? capacityValue : 0

  const date = searchParams.get('on') ?? defaults.date
  const from = searchParams.get('from') ?? defaults.from
  const to = searchParams.get('to') ?? defaults.to
  const parsed = parseWindow({ date, from, to })

  const setParam = useCallback(
    (key: string, value: string | null) => {
      setSearchParams(
        (current) => {
          const next = new URLSearchParams(current)
          if (value === null || value === '') next.delete(key)
          else next.set(key, value)
          return next
        },
        { replace: true },
      )
    },
    [setSearchParams],
  )

  const windowParams =
    parsed && parsed.error === null
      ? { startsAtMs: parsed.startsAtMs, endsAtMs: parsed.endsAtMs, projectId: project.id }
      : null
  const schedulePlan = useSchedulePlan(windowParams)

  const taskTitles = useMemo(
    () => new Map(tasks.map((task) => [task.id, task.title])),
    [tasks],
  )

  const review = usePlanReview()
  const apply = useApplyApprovedPlan()
  const [approvalChecked, setApprovalChecked] = useState(false)
  const [receipt, setReceipt] = useState<{
    fingerprint: string
    commandId: string
    acceptedRevision: number
    replayed: boolean
    blockIds: string[]
  } | null>(null)

  // A new window or a changed plan invalidates the review: its fingerprint is
  // bound to one window, one revision and one set of identities.
  useEffect(() => {
    setApprovalChecked(false)
    setReceipt(null)
    review.reset()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [date, from, to])

  const planRevision = schedulePlan.data?.scheduleRevision
  useEffect(() => {
    if (!review.review) return
    if (planRevision === undefined) return
    if (review.review.expected_revision === planRevision) return
    review.reset()
    setApprovalChecked(false)
  }, [planRevision, review])

  const handleReview = () => {
    const plan = schedulePlan.data
    if (!plan || plan.slots.length === 0) return
    setApprovalChecked(false)
    setReceipt(null)
    apply.reset()
    review.reviewPlan(buildScheduleApprovalRequest(plan, newCommandId(), newCommandId))
  }

  const handleApply = async () => {
    if (!review.reviewedPlan || !review.review) return
    try {
      const outcome = await apply.mutateAsync(review.reviewedPlan)
      setReceipt({
        fingerprint: review.review.fingerprint,
        commandId: review.review.command_id,
        acceptedRevision: outcome.acceptedRevision,
        replayed: outcome.replayed,
        blockIds: outcome.scheduleBlockIds,
      })
      toast({ tone: 'success', title: 'Plan applied', description: `${outcome.scheduleBlockIds.length} calendar blocks stored.` })
    } catch {
      // `apply.error` is rendered next to the button with the server's message.
    }
  }

  const header = (
    <div className="flex flex-col gap-1">
      <p className="section-label">Planning</p>
      <h2 className="font-display text-2xl font-medium tracking-tight text-ink">
        Plan {project.name}
      </h2>
      <p className="max-w-2xl text-pretty text-sm leading-6 text-ink-soft">
        Everything on this page is a server proposal until you approve a reviewed schedule. Capacity,
        dispositions, windows, slots and blockers come from the API.
      </p>
    </div>
  )

  return (
    <div className="flex flex-col gap-5">
      {header}
      <div className="grid items-start gap-5 xl:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)]">
        <DayPlanPanel
          project={project}
          capacity={capacity}
          onCommitCapacity={(minutes) => setParam('capacity', String(minutes))}
        />
        <div className="flex flex-col gap-5">
          <SchedulePanel
            taskTitles={taskTitles}
            window={{ date, from, to, error: parsed?.error ?? null }}
            onWindowChange={(patch) => {
              if (patch.date !== undefined) setParam('on', patch.date || toDateInput(nowMs))
              if (patch.from !== undefined) setParam('from', patch.from)
              if (patch.to !== undefined) setParam('to', patch.to)
            }}
            plan={schedulePlan}
            review={review}
            apply={apply}
            receipt={receipt}
            onReview={handleReview}
            onApply={() => void handleApply()}
            approvalChecked={approvalChecked}
            onApprovalChange={setApprovalChecked}
          />
          <Panel>
            <PanelBody className="flex flex-col gap-2">
              <p className="flex items-center gap-1.5 text-xs font-semibold text-ink">
                <ClockIcon size={13} className="text-accent" />
                What is not on this page
              </p>
              <p className="text-2xs leading-4 text-ink-muted">
                Unplaced tasks keep the reason the server gave. Workload, portfolio capacity and
                scenario comparisons are read over the API but have no screen in this build yet;
                nothing is estimated here on their behalf.
              </p>
              <p className="text-2xs leading-4 text-ink-faint">
                {tasks.length} tasks and {milestones.length} milestones in this project.
              </p>
            </PanelBody>
          </Panel>
        </div>
      </div>
    </div>
  )
}
