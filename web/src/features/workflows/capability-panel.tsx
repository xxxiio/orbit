/**
 * The capability surface of a work item.
 *
 * Three server reads, three questions a person can answer here:
 *
 * 1. **Status** — for every capability the workflow declares for this state, is
 *    it installed, is this project granted it, and has any executor announced
 *    it? These are durable facts; none of them is inferred from a provider being
 *    loaded in some process.
 * 2. **Offers** — the actions the server offers for the item's current state,
 *    each with its availability code and the server's own reason. An action the
 *    server refuses is described, never offered as a disabled control.
 * 3. **Requests** — what has been asked for, what an executor reported, and (for
 *    an uncertain outcome only) an explicit reconciliation.
 *
 * The browser records an intent. It never claims an action, never reports a
 * result, and never says the item moved unless the work-item read says so.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import type {
  CapabilityOffer,
  CapabilityRequest,
  CapabilityStatus,
  WorkItem,
  WorkItemKind,
} from '@/api/adapters'
import {
  classifyError,
  useCapabilityOffers,
  useCapabilityRequests,
  useCapabilityStatus,
  userMessage,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { LoaderIcon } from '@/components/ui/icons'
import { capabilityLabel, humanizeIdentifier } from '@/lib/humanize'
import { queryKeys } from '@/api/queries'
import {
  actionsForState,
  parameterNames,
  parameterType,
  stateLabel,
} from './work-item-model'
import {
  availabilityCopy,
  declaredCapabilitiesForState,
  executorSummary,
  offersMatchRevision,
  permissionLabel,
  recordedActionCopy,
  requestForActionRevision,
  requestStateCopy,
  type DeclaredCapability,
} from './capability-model'
import { CapabilityRequestForm } from './capability-request-form'
import { CapabilityRequestHistory } from './capability-request-history'

export interface CapabilityPanelProps {
  item: WorkItem
  kind: WorkItemKind | undefined
}

export function CapabilityPanel({ item, kind }: CapabilityPanelProps) {
  const status = useCapabilityStatus(item.projectId)
  const offers = useCapabilityOffers(item.id, item.revision)
  const requests = useCapabilityRequests(item.id)
  const queryClient = useQueryClient()

  const [openAction, setOpenAction] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  /**
   * Where focus goes when a request form closes. Opening a form removes the
   * control that had focus, and recording one removes the form itself; without
   * this, a keyboard user would land on the page body.
   */
  const triggerRefs = useRef(new Map<string, HTMLButtonElement>())
  const [focusAction, setFocusAction] = useState<string | null>(null)

  useEffect(() => {
    if (openAction !== null || focusAction === null) return
    triggerRefs.current.get(focusAction)?.focus({ preventScroll: true })
    setFocusAction(null)
  }, [openAction, focusAction])

  /**
   * A settled receipt can change the item — an executor may have moved it or
   * written a property. The panel therefore re-reads the item once, and only
   * once, when a request acquires an outcome it did not have on the previous
   * read. It still says nothing about what changed: the item read is the only
   * thing that can say that.
   */
  const settledFingerprint = useRef<string | null>(null)
  useEffect(() => {
    if (!requests.data) return
    const fingerprint = requests.data
      .filter((request) => request.state === 'succeeded' || request.state === 'failed')
      .map((request) => `${request.commandId}:${request.state}`)
      .join('|')
    // The first read establishes the baseline; opening a sheet is not news.
    if (settledFingerprint.current === null || settledFingerprint.current === fingerprint) {
      settledFingerprint.current = fingerprint
      return
    }
    settledFingerprint.current = fingerprint
    void queryClient.invalidateQueries({ queryKey: queryKeys.workItemsAll() })
  }, [requests.data, queryClient])

  const declared = useMemo(
    () => declaredCapabilitiesForState(kind, item.state),
    [kind, item.state],
  )
  const stateActions = useMemo(
    () => (kind ? actionsForState(kind, item.state) : []),
    [kind, item.state],
  )
  const statusById = useMemo(() => {
    const map = new Map<string, CapabilityStatus>()
    for (const entry of status.data ?? []) map.set(entry.capabilityId, entry)
    return map
  }, [status.data])
  const offerFor = (capabilityId: string, actionId: string): CapabilityOffer | undefined =>
    offers.data?.offers.find(
      (offer) => offer.capabilityId === capabilityId && offer.actionId === actionId,
    )

  const recorded = (message: string) => {
    setFocusAction(openAction)
    setOpenAction(null)
    setAnnouncement(message)
  }
  const cancelForm = () => {
    setFocusAction(openAction)
    setOpenAction(null)
    setAnnouncement(null)
  }

  return (
    <div data-capability-panel="" className="flex flex-col gap-4">
      {declared.length > 0 ? (
        <CapabilityStatusList declared={declared} statusById={statusById} state={status} />
      ) : null}

      <div className="flex flex-col gap-2">
        <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
          Actions offered for {stateLabel(item.state)}
        </p>

        {offers.isPending && !offers.data ? (
          <p className="flex items-center gap-1.5 text-xs leading-5 text-ink-muted">
            <LoaderIcon size={12} className="animate-spin text-accent" />
            Reading the actions this server offers…
          </p>
        ) : null}

        {offers.error ? (
          <p
            role="alert"
            data-capability-offers-error=""
            className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
          >
            The offered actions could not be read. {userMessage(offers.error)}
          </p>
        ) : null}

        {offers.data && !offersMatchRevision(offers.data.itemRevision, item.revision) ? (
          <p
            data-capability-offers-stale=""
            className="rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5 text-2xs leading-4 text-warning-ink"
          >
            These offers were computed for revision {offers.data.itemRevision}, and this item is now
            at revision {item.revision}. Reload the item before requesting anything.
          </p>
        ) : null}

        {!kind ? (
          <p className="text-xs leading-5 text-ink-muted">
            The pinned workflow does not declare the kind “{item.kind}”, so its actions cannot be
            named or requested here. Any request already recorded for this item is listed below.
          </p>
        ) : null}

        {kind && stateActions.length === 0 ? (
          <p className="text-xs leading-5 text-ink-muted">
            This state declares no actions. The workflow declares no move out of{' '}
            {stateLabel(item.state)}, so the server offers no capability action here.
          </p>
        ) : null}

        {stateActions.length > 0 ? (
          <ul className="flex flex-col gap-2">
            {stateActions.map(({ transition: edge, action }) => {
              const capability = statusById.get(action.capability)
              const offer = offerFor(action.capability, action.id)
              const key = `${action.capability}#${action.id}`
              // One action, one request per item revision: the server refuses a
              // repeat as a conflict, so the panel shows the recorded request
              // instead of offering the same intent again.
              const recordedRequest = requestForActionRevision(
                requests.data,
                action.id,
                item.revision,
              )
              return (
                <li
                  key={`${edge.from}->${edge.to}#${action.id}`}
                  data-work-item-action={action.id}
                  data-capability-offer={action.id}
                  data-capability-available={offer ? String(offer.available) : 'false'}
                  data-capability-code={offer ? (offer.code ?? 'available') : 'not_offered'}
                  data-capability-recorded-request={recordedRequest?.commandId}
                  className="rounded-md border border-line bg-surface px-3 py-2.5"
                >
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div className="min-w-0">
                      <p className="flex flex-wrap items-center gap-1.5 text-sm font-medium text-ink">
                        {humanizeIdentifier(action.id)}
                        <OfferBadge
                          offer={offer}
                          loading={offers.isPending && !offers.data}
                          recorded={recordedRequest}
                        />
                      </p>
                      <p className="mt-0.5 text-xs leading-5 text-ink-muted">
                        {(offer?.description ?? action.description).trim() ||
                          'The workflow gives this action no description.'}
                      </p>
                      <p className="mt-1 text-2xs text-ink-faint">
                        Declared on {stateLabel(edge.from)} → {stateLabel(edge.to)}
                      </p>
                    </div>
                  </div>

                  <div className="mt-2 flex flex-wrap items-center gap-1.5">
                    <Badge tone="info">Requires capability {capabilityLabel(action.capability)}</Badge>
                    {capability ? (
                      <Badge tone="neutral">{permissionLabel(capability.permission)}</Badge>
                    ) : null}
                    {edge.requiredCapability && edge.requiredCapability !== action.capability ? (
                      <Badge tone="warning">
                        Move requires {capabilityLabel(edge.requiredCapability)}
                      </Badge>
                    ) : null}
                  </div>

                  {parameterNames(action).length > 0 ? (
                    <div className="mt-2 border-t border-line pt-2">
                      <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                        Declared parameters
                      </p>
                      <ul className="mt-1 flex flex-wrap gap-1.5">
                        {parameterNames(action).map((name) => (
                          <li
                            key={name}
                            className="rounded-sm border border-line bg-surface-sunken px-1.5 py-0.5 font-mono text-2xs text-ink-soft"
                          >
                            {name}
                            <span className="ml-1 text-ink-faint">
                              {parameterType(action.parameters[name])}
                            </span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : null}

                  {offer && !offer.available ? (
                    <UnavailableAction offer={offer} />
                  ) : null}

                  {offer && offer.available && recordedRequest ? (
                    <RecordedAction request={recordedRequest} actionId={action.id} />
                  ) : null}

                  {offer && offer.available && !recordedRequest ? (
                    openAction === key ? (
                      <CapabilityRequestForm
                        item={item}
                        offer={offer}
                        permission={capability?.permission ?? ''}
                        itemRevision={item.revision}
                        onCancel={cancelForm}
                        onRecorded={recorded}
                      />
                    ) : (
                      <div className="mt-2.5 flex flex-wrap items-center gap-2 border-t border-line pt-2.5">
                        <Button
                          ref={(node) => {
                            if (node) triggerRefs.current.set(key, node)
                            else triggerRefs.current.delete(key)
                          }}
                          size="sm"
                          variant="secondary"
                          data-capability-request-open={action.id}
                          onClick={() => {
                            setAnnouncement(null)
                            setFocusAction(null)
                            setOpenAction(key)
                          }}
                        >
                          Request this action
                        </Button>
                        <span className="text-2xs leading-4 text-ink-muted">
                          An executor claims and runs it. This browser only records the request.
                        </span>
                      </div>
                    )
                  ) : null}

                  {!offer && !offers.isPending && !offers.error ? (
                    <p
                      data-capability-not-offered={action.id}
                      className="mt-2 text-xs leading-5 text-ink-muted"
                    >
                      The server does not offer this action for the item’s current state.
                    </p>
                  ) : null}
                </li>
              )
            })}
          </ul>
        ) : null}
      </div>

      <div
        className="flex flex-col gap-2"
        id="capability-request-history"
        data-capability-history=""
        role="region"
        aria-label="Capability request history"
        tabIndex={-1}
      >
        <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
          Requests for this item
        </p>
        <CapabilityRequestHistory
          itemId={item.id}
          requests={requests.data}
          pending={requests.isPending}
          error={requests.error}
        />
      </div>

      <p role="status" aria-live="polite" className="min-h-4 text-2xs leading-4 text-ink-muted">
        {announcement}
      </p>
    </div>
  )
}

function OfferBadge({
  offer,
  loading,
  recorded,
}: {
  offer: CapabilityOffer | undefined
  loading: boolean
  recorded: CapabilityRequest | undefined
}) {
  if (loading) return <Badge tone="neutral">Checking availability…</Badge>
  if (!offer) return <Badge tone="neutral">Not offered for this state</Badge>
  // The offer is still the server's answer; a recorded request is the reason
  // this panel has nothing left to offer for it.
  if (offer.available && recorded) {
    const state = requestStateCopy(recorded)
    return <Badge tone={state.tone}>Requested · {state.label}</Badge>
  }
  if (offer.available) return <Badge tone="success">Available to request</Badge>
  return (
    <Badge tone="warning" className="font-mono">
      {offer.code ?? 'unavailable'}
    </Badge>
  )
}

/**
 * What stands in place of the request control once this action is already
 * recorded for the item revision, with a link to the durable history rather
 * than a second request the server would refuse.
 */
function RecordedAction({
  request,
  actionId,
}: {
  request: CapabilityRequest
  actionId: string
}) {
  const copy = recordedActionCopy(request)
  return (
    <div
      data-capability-recorded={actionId}
      data-capability-recorded-state={request.recordedState}
      className="mt-2.5 rounded-md border border-line bg-surface-sunken/50 px-3 py-2 text-xs leading-5 text-ink-soft"
    >
      <p className="font-medium text-ink">{copy.title}</p>
      <p className="mt-0.5">{copy.description}</p>
      <p className="mt-1.5 text-2xs leading-4">
        <a
          href="#capability-request-history"
          data-capability-recorded-link={request.commandId}
          className="rounded-sm font-medium text-accent underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        >
          Show the request history for this item
        </a>{' '}
        <span className="text-ink-muted">
          (request <span className="break-all font-mono">{request.commandId}</span>)
        </span>
      </p>
    </div>
  )
}

function UnavailableAction({ offer }: { offer: CapabilityOffer }) {
  const copy = availabilityCopy(offer.code, offer.reason)
  return (
    <div
      data-capability-unavailable={offer.code ?? 'unnamed'}
      className="mt-2.5 rounded-md border border-warning/35 bg-warning-soft/50 px-3 py-2 text-xs leading-5 text-warning-ink"
    >
      <p className="font-medium">{copy.title}</p>
      <p className="mt-0.5">{copy.hint}</p>
      {copy.serverReason ? (
        <p className="mt-1.5 break-words font-mono text-2xs leading-4 text-warning-ink/90">
          The server says: “{copy.serverReason}”
        </p>
      ) : null}
    </div>
  )
}

function CapabilityStatusList({
  declared,
  statusById,
  state,
}: {
  declared: DeclaredCapability[]
  statusById: Map<string, CapabilityStatus>
  state: ReturnType<typeof useCapabilityStatus>
}) {
  return (
    <div className="flex flex-col gap-2">
      <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
        Capability status
      </p>

      {state.isPending && !state.data ? (
        <p className="flex items-center gap-1.5 text-xs leading-5 text-ink-muted">
          <LoaderIcon size={12} className="animate-spin text-accent" />
          Reading capability status…
        </p>
      ) : null}

      {state.error ? (
        <p
          role="alert"
          data-capability-status-error={classifyError(state.error)}
          className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-soft"
        >
          {classifyError(state.error) === 'notFound'
            ? 'This server does not expose capability status, so availability cannot be shown here. Nothing can be requested from this panel until it does.'
            : `Capability status could not be read. ${userMessage(state.error)}`}
        </p>
      ) : null}

      <ul className="flex flex-col gap-2">
        {declared.map((entry) => (
          <CapabilityStatusRow
            key={entry.capabilityId}
            declared={entry}
            capability={statusById.get(entry.capabilityId)}
            statusRead={Boolean(state.data)}
          />
        ))}
      </ul>
    </div>
  )
}

function CapabilityStatusRow({
  declared,
  capability,
  statusRead,
}: {
  declared: DeclaredCapability
  capability: CapabilityStatus | undefined
  statusRead: boolean
}) {
  const executors = capability?.executors.length ?? 0
  return (
    <li
      data-capability-status={declared.capabilityId}
      data-capability-installed={capability ? String(capability.installed) : 'unknown'}
      data-capability-granted={capability ? String(capability.granted) : 'unknown'}
      data-capability-executors={capability ? String(executors) : 'unknown'}
      className="rounded-md border border-line bg-surface-sunken/25 px-3 py-2"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-sm font-medium text-ink">
            {capability?.name ?? capabilityLabel(declared.capabilityId)}
          </p>
          <p className="mt-0.5 break-words font-mono text-2xs text-ink-faint">
            {declared.capabilityId}
          </p>
        </div>
        {capability ? (
          <div className="flex flex-wrap items-center gap-1.5">
            <Badge tone={capability.installed ? 'success' : 'warning'}>
              {capability.installed ? 'Installed' : 'Not installed'}
            </Badge>
            <Badge tone={capability.granted ? 'success' : 'warning'}>
              {capability.granted ? 'Granted for this project' : 'Not granted'}
            </Badge>
            <Badge tone={executors > 0 ? 'success' : 'warning'} dot>
              {executors > 0 ? 'Executor announced' : 'No executor'}
            </Badge>
            <Badge tone="neutral">{permissionLabel(capability.permission)}</Badge>
          </div>
        ) : (
          <Badge tone={statusRead ? 'warning' : 'neutral'}>
            {statusRead ? 'Not in the server catalog' : 'Status unknown'}
          </Badge>
        )}
      </div>

      <p className="mt-1.5 text-2xs leading-4 text-ink-muted">
        {capability ? executorSummary(capability.executors) : 'No executor information was read.'}
      </p>

      <p className="mt-0.5 text-2xs leading-4 text-ink-muted">
        {capability
          ? capability.requires.length > 0
            ? `Requires ${capability.requires.map((required) => capabilityLabel(required)).join(', ')}.`
            : 'Requires no other capability.'
          : 'The server did not describe this capability.'}
      </p>

      {declared.requiredByTransition ? (
        <p className="mt-0.5 text-2xs leading-4 text-ink-faint">
          A move out of this state also requires it.
        </p>
      ) : null}

      {declared.actionIds.length > 0 ? (
        <p className="mt-0.5 text-2xs leading-4 text-ink-faint">
          Declared for{' '}
          {declared.actionIds.map((actionId) => humanizeIdentifier(actionId)).join(', ')}.
        </p>
      ) : null}
    </li>
  )
}
