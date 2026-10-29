/**
 * The item's capability request history: what was asked for, what state it is
 * in, the executor's receipt (verbatim), and — for the two states nothing has
 * settled — an explicit reconciliation.
 *
 * The panel never re-sends an intent. `pending` waits. An `uncertain` outcome and
 * a `claimed` request with no recorded result are both questions for a person,
 * answered by recording a decision that the server stores; the copy says which of
 * the two it is. Nothing here says the item moved: only the work-item read can
 * say that.
 */
import { useId, useState } from 'react'
import type { CapabilityReceipt, CapabilityRequest } from '@/api/adapters'
import type { CapabilityReconcileDecision } from '@/api/contracts'
import { useCapabilityRequestShow, useReconcileCapabilityAction, userMessage, type ApiError } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Field } from '@/components/ui/field'
import { Textarea } from '@/components/ui/textarea'
import { AlertCircleIcon, LoaderIcon } from '@/components/ui/icons'
import { capabilityLabel, humanizeIdentifier } from '@/lib/humanize'
import { formatMoment } from '@/domain'
import {
  CAPABILITY_RECONCILE_DECISIONS,
  RECONCILE_DETAIL_MAX,
  canReconcile,
  capabilityRefusal,
  isUnsettledRequestState,
  receiptDetailText,
  reconcileCopy,
  requestStateCopy,
  validateReconcile,
} from './capability-model'

export interface CapabilityRequestHistoryProps {
  itemId: string
  /** Oldest first, as the server orders them; rendered newest first. */
  requests: CapabilityRequest[] | undefined
  pending: boolean
  error: ApiError | null
}

export function CapabilityRequestHistory({
  itemId,
  requests,
  pending,
  error,
}: CapabilityRequestHistoryProps) {
  if (pending && !requests) {
    return (
      <p className="flex items-center gap-1.5 text-xs leading-5 text-ink-muted">
        <LoaderIcon size={12} className="animate-spin text-accent" />
        Reading this item’s capability requests…
      </p>
    )
  }
  if (error) {
    return (
      <p
        role="alert"
        data-capability-history-error=""
        className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
      >
        This item’s capability requests could not be read. {userMessage(error)}
      </p>
    )
  }
  const ordered = [...(requests ?? [])].reverse()
  if (ordered.length === 0) {
    return (
      <p className="text-xs leading-5 text-ink-muted">
        No capability request has been recorded for this item. Asking for an action records a
        request; an executor claims and runs it.
      </p>
    )
  }
  return (
    <ul className="flex flex-col gap-2">
      {ordered.map((request, index) => (
        <CapabilityRequestRow
          key={request.commandId}
          request={request}
          itemId={itemId}
          newest={index === 0}
        />
      ))}
    </ul>
  )
}

function CapabilityRequestRow({
  request,
  itemId,
  newest,
}: {
  request: CapabilityRequest
  itemId: string
  newest: boolean
}) {
  const copy = requestStateCopy(request)
  const settled = !isUnsettledRequestState(request.state) && request.state !== 'unknown'
  // Only the newest settled request reads its receipt on its own. An older one
  // waits for "Show receipt": opening a long history must not fire one read per
  // recorded request.
  const [expanded, setExpanded] = useState(newest)
  const show = useCapabilityRequestShow(request.commandId, settled && expanded)
  const actionLabel = humanizeIdentifier(request.actionId)

  return (
    <li
      data-capability-request={request.commandId}
      data-capability-state={request.recordedState}
      data-capability-newest={newest ? '' : undefined}
      className="rounded-md border border-line bg-surface-sunken/25 px-3 py-2.5"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="flex flex-wrap items-center gap-1.5 text-sm font-medium text-ink">
            {actionLabel}
            <Badge tone={copy.tone} dot>
              {copy.label}
            </Badge>
          </p>
          <p className="mt-0.5 text-2xs text-ink-faint">
            <span className="font-mono">{capabilityLabel(request.capabilityId)}</span> ·{' '}
            {humanizeIdentifier(request.from)} → {humanizeIdentifier(request.to)} · recorded{' '}
            {formatMoment(request.createdAtMs)}
          </p>
        </div>
        <span className="font-mono text-2xs text-ink-faint" title={request.commandId}>
          {shortId(request.commandId)}
        </span>
      </div>

      <p className="mt-1.5 text-xs leading-5 text-ink-soft">{copy.description}</p>

      {request.executorDeviceId ? (
        <p className="mt-1 text-2xs leading-4 text-ink-muted">
          Executor <span className="font-mono">{request.executorDeviceId}</span>
        </p>
      ) : request.state === 'pending' ? (
        <p className="mt-1 text-2xs leading-4 text-ink-muted">
          No executor has claimed it yet.
        </p>
      ) : null}

      {settled && expanded ? <ReceiptBlock commandId={request.commandId} show={show} /> : null}

      {settled && !expanded ? (
        <Button
          size="sm"
          variant="quiet"
          data-capability-show-receipt={request.commandId}
          className="mt-1.5 -ml-1.5"
          onClick={() => setExpanded(true)}
        >
          Show receipt
        </Button>
      ) : null}

      {canReconcile(request.state) ? (
        <ReconcileBlock request={request} itemId={itemId} />
      ) : null}
    </li>
  )
}

function ReceiptBlock({
  commandId,
  show,
}: {
  commandId: string
  show: ReturnType<typeof useCapabilityRequestShow>
}) {
  if (show.isPending) {
    return (
      <p className="mt-2 flex items-center gap-1.5 text-2xs leading-4 text-ink-muted">
        <LoaderIcon size={11} className="animate-spin text-accent" />
        Reading the receipt…
      </p>
    )
  }
  if (show.error) {
    return (
      <p role="alert" className="mt-2 text-2xs leading-4 text-danger-ink">
        The receipt could not be read. {userMessage(show.error)}
      </p>
    )
  }
  const receipt = show.data?.receipt
  if (!receipt) {
    return (
      <p className="mt-2 text-2xs leading-4 text-ink-muted">
        The server has recorded no receipt for this request.
      </p>
    )
  }
  return <Receipt receipt={receipt} commandId={commandId} />
}

function Receipt({ receipt, commandId }: { receipt: CapabilityReceipt; commandId: string }) {
  return (
    <div
      data-capability-receipt={commandId}
      className="mt-2 rounded-sm border border-line bg-surface px-2.5 py-2"
    >
      <p className="text-2xs font-semibold tracking-label text-ink-faint">
        Receipt, as reported
      </p>
      <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap break-words font-mono text-2xs leading-4 text-ink-soft">
        {receiptDetailText(receipt.detail)}
      </pre>
      <p className="mt-1.5 text-2xs leading-4 text-ink-muted">
        Outcome <span className="font-medium">{receipt.outcome}</span> · reported by{' '}
        <span className="font-mono">{receipt.reportedBy}</span> ·{' '}
        {formatMoment(receipt.reportedAtMs)}
      </p>
    </div>
  )
}

/**
 * The explicit settlement of a request nothing recorded an outcome for.
 *
 * `uncertain` and `claimed` are separate questions and get separate copy, but the
 * same two decisions: confirm it succeeded, or confirm it failed and say why.
 * `confirm_failed` needs a reason, because "it failed" without one is not
 * something a later reader can act on. Nothing here dispatches the action again,
 * and no retry is offered for either state.
 */
function ReconcileBlock({ request, itemId }: { request: CapabilityRequest; itemId: string }) {
  const detailId = useId()
  const [decision, setDecision] = useState<CapabilityReconcileDecision>('confirm_succeeded')
  const [detail, setDetail] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const reconcile = useReconcileCapabilityAction()
  const pending = reconcile.isPending
  const refusal = reconcile.error ? capabilityRefusal(reconcile.error) : null
  const copy = reconcileCopy(request.state)

  const submit = async () => {
    const validation = validateReconcile(decision, detail)
    setError(validation)
    if (validation) return
    setAnnouncement('Recording the decision…')
    try {
      await reconcile.mutateAsync({
        commandId: request.commandId,
        itemId,
        decision,
        detail: detail.trim(),
      })
      setDetail('')
    } catch {
      setAnnouncement(null)
    }
  }

  // Only the two reconcilable states get here; anything else renders nothing
  // rather than inventing a question.
  if (!copy) return null

  // Once the server has settled it, the form is done: replaying a decision on a
  // settled request is refused, so there is nothing left to offer here.
  if (reconcile.data) {
    return (
      <p
        role="status"
        data-capability-reconciled={request.commandId}
        className="mt-3 rounded-sm border border-success/25 bg-success-soft px-2.5 py-2 text-2xs leading-4 text-success-ink"
      >
        The decision is recorded. The request now reads{' '}
        <span className="font-mono">{reconcile.data.request.recordedState}</span>.
      </p>
    )
  }

  return (
    <form
      data-capability-reconcile={request.commandId}
      data-capability-reconcile-state={request.state}
      className="mt-3 flex flex-col gap-2.5 rounded-sm border border-warning/35 bg-warning-soft/40 px-2.5 py-2.5"
      onSubmit={(event) => {
        event.preventDefault()
        void submit()
      }}
    >
      <div>
        <p className="text-xs font-semibold text-warning-ink">{copy.title}</p>
        <p className="mt-0.5 text-2xs leading-4 text-warning-ink/90">{copy.description}</p>
      </div>
      <fieldset className="flex flex-col gap-1.5">
        <legend className="sr-only">What happened</legend>
        {CAPABILITY_RECONCILE_DECISIONS.map((option) => (
          <label key={option.value} className="flex items-start gap-2 text-xs leading-5 text-ink-soft">
            <input
              type="radio"
              name={`reconcile-${request.commandId}`}
              value={option.value}
              checked={decision === option.value}
              onChange={() => {
                setDecision(option.value)
                setError(null)
              }}
              disabled={pending}
              className="mt-1 h-3.5 w-3.5 shrink-0 accent-[hsl(var(--accent))]"
            />
            <span className="min-w-0">
              <span className="font-medium text-ink">{option.label}</span>
              <span className="block text-2xs leading-4 text-ink-muted">{option.description}</span>
            </span>
          </label>
        ))}
      </fieldset>
      <Field
        id={detailId}
        label="Reason"
        required={decision === 'confirm_failed'}
        hint={`Stored with the decision. ${
          decision === 'confirm_failed'
            ? 'Say what happened: another reader has to act on it.'
            : 'Optional, but useful to the next reader.'
        } Up to ${RECONCILE_DETAIL_MAX} characters.`}
        error={error}
      >
        <Textarea
          value={detail}
          onChange={(event) => {
            setDetail(event.target.value)
            setError(null)
          }}
          rows={2}
          maxLength={RECONCILE_DETAIL_MAX}
          disabled={pending}
          placeholder={
            decision === 'confirm_failed'
              ? 'What the executor did before it stopped.'
              : 'What proves the effect happened.'
          }
        />
      </Field>
      {refusal ? (
        <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
          <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
          <span className="min-w-0">
            {refusal.title} {refusal.detail}
          </span>
        </p>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        <Button type="submit" size="sm" variant="secondary" loading={pending} disabled={pending}>
          Record this decision
        </Button>
        <p role="status" aria-live="polite" className="min-w-0 flex-1 text-2xs leading-4 text-ink-muted">
          {pending ? 'Recording the decision…' : announcement}
        </p>
      </div>
    </form>
  )
}

function shortId(id: string): string {
  return id.length > 16 ? `${id.slice(0, 10)}…${id.slice(-4)}` : id
}
