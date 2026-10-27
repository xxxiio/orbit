import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import type {
  CapabilityActionRequestRecord,
  CapabilityOfferRecord,
  CapabilityReceiptRecord,
  CapabilityShowResponse,
  CapabilityStatusRecord,
} from '@/api/contracts'
import type { WorkItem, WorkItemKind } from '@/api/adapters'
import { toCapabilityOffer, toCapabilityReceipt, toCapabilityRequest } from '@/api/adapters'
import { queryKeys } from '@/api/queries'
import { CapabilityPanel } from '@/features/workflows/capability-panel'
import { jsonResponse, testQueryClient } from '../api/test-utils'

/**
 * The capability panel against a fake server: an unavailable action has no
 * control, an available one records a fenced request after an explicit
 * confirmation, and the two states nothing settled — `uncertain` and `claimed` —
 * offer a reconcile, never a retry.
 *
 * The cache always holds the wire shape, because that is what `queryFn` returns
 * and `select` adapts on read.
 */

const CAPABILITY = 'orbit.reference.work-item-note'

const item: WorkItem = {
  id: 'wi-1',
  projectId: 'p-1',
  kind: 'ticket',
  state: 'open',
  properties: {},
  revision: 3,
}

const kind: WorkItemKind = {
  id: 'ticket',
  name: 'Ticket',
  initialState: 'open',
  states: ['open', 'closed'],
  fields: [],
  transitions: [
    {
      from: 'open',
      to: 'closed',
      requiredCapability: null,
      guards: [],
      actions: [
        {
          id: 'append_note',
          capability: CAPABILITY,
          description: 'Append a durable note.',
          parameters: { note: '' },
        },
      ],
    },
  ],
}

function statusRecord(overrides: Partial<CapabilityStatusRecord> = {}): CapabilityStatusRecord {
  return {
    capability_id: CAPABILITY,
    name: 'Reference notes',
    permission: 'mutate',
    requires: [],
    installed: true,
    granted: true,
    executors: [{ device_id: 'exec-1', announced_at_ms: 1_760_000_000_000 }],
    actions: [],
    ...overrides,
  }
}

const inputSchema = {
  type: 'object',
  properties: {
    note: {
      type: 'string',
      minLength: 1,
      maxLength: 2000,
      description: 'Note body (1..2000 characters)',
    },
    tag: { type: 'string', description: 'Optional label for the note' },
  },
  required: ['note'],
}

function offer(overrides: Partial<CapabilityOfferRecord> = {}): CapabilityOfferRecord {
  return {
    action_id: 'append_note',
    capability_id: CAPABILITY,
    from: 'open',
    to: 'closed',
    description: 'Append a durable note to this work item',
    input_schema: inputSchema,
    external_effect: false,
    available: true,
    code: null,
    reason: null,
    ...overrides,
  }
}

function requestRecord(
  overrides: Partial<CapabilityActionRequestRecord> = {},
): CapabilityActionRequestRecord {
  return {
    command_id: 'cap-1',
    item_id: item.id,
    item_revision: item.revision,
    action_id: 'append_note',
    capability_id: CAPABILITY,
    from: 'open',
    to: 'closed',
    state: 'pending',
    inputs: { note: 'Hello' },
    created_at_ms: 1_760_000_000_000,
    executor_device_id: null,
    ...overrides,
  }
}

function receiptRecord(overrides: Partial<CapabilityReceiptRecord> = {}): CapabilityReceiptRecord {
  return {
    outcome: 'uncertain',
    detail: { note: 'Hello', reason: 'the process died before writing' },
    reported_at_ms: 1_760_000_600_000,
    reported_by: 'exec-1',
    ...overrides,
  }
}

interface MockOptions {
  offers?: CapabilityOfferRecord[]
  /** `null` makes the status read answer 404. */
  capabilities?: CapabilityStatusRecord[] | null
  requestResponse?: CapabilityActionRequestRecord
  /** Make the request write refuse, as the server does for an unavailable action. */
  requestFailure?: { error: string; status: number }
  reconcileResponse?: { request: CapabilityActionRequestRecord; receipt: CapabilityReceiptRecord }
}

/** A router for the four reads and the two writes the panel may make. */
function mockServer(options: MockOptions = {}) {
  const request = options.requestResponse ?? requestRecord()
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      const method = (init?.method ?? 'GET').toUpperCase()
      if (url.includes('/capability-actions/request') && method === 'POST') {
        if (options.requestFailure) {
          return jsonResponse({ error: options.requestFailure.error }, options.requestFailure.status)
        }
        return jsonResponse({ request })
      }
      if (url.includes('/capability-actions/reconcile') && method === 'POST') {
        return jsonResponse(
          options.reconcileResponse ?? { request, receipt: receiptRecord() },
        )
      }
      if (url.includes('/capability-actions/show')) {
        return jsonResponse({ request, receipt: null })
      }
      if (url.includes('/capability-actions/list')) {
        return jsonResponse({ requests: [] })
      }
      if (url.includes('/capability-actions')) {
        return jsonResponse({
          item_id: item.id,
          item_revision: item.revision,
          offers: options.offers ?? [offer()],
        })
      }
      if (url.includes('/capabilities')) {
        if (options.capabilities === null) {
          return jsonResponse({ error: 'not found' }, 404)
        }
        return jsonResponse({ capabilities: options.capabilities ?? [statusRecord()] })
      }
      return jsonResponse({})
    }),
  )
}

interface RenderOptions extends MockOptions {
  /** Seed the request-history read (wire shape). */
  history?: CapabilityActionRequestRecord[]
  /** Seed the show read for one command (wire shape). */
  show?: CapabilityShowResponse
  item?: WorkItem
}

function renderPanel(options: RenderOptions = {}) {
  const client = testQueryClient()
  const target = options.item ?? item
  const capabilities =
    options.capabilities === null ? undefined : (options.capabilities ?? [statusRecord()])
  if (capabilities) {
    // The status read selects on the wire shape, so the cache holds the wire shape.
    client.setQueryData(queryKeys.capabilityStatus(target.projectId), { capabilities })
  }
  // The offers and requests reads adapt in their `queryFn`, so their cache holds
  // the adapted shape.
  client.setQueryData(queryKeys.capabilityOffers(target.id, target.revision), {
    itemId: target.id,
    itemRevision: target.revision,
    offers: (options.offers ?? [offer()]).map(toCapabilityOffer),
  })
  client.setQueryData(
    queryKeys.capabilityRequests(target.id),
    (options.history ?? []).map(toCapabilityRequest),
  )
  if (options.show) {
    client.setQueryData(queryKeys.capabilityRequest(options.show.request.command_id), {
      request: toCapabilityRequest(options.show.request),
      receipt: options.show.receipt ? toCapabilityReceipt(options.show.receipt) : null,
    })
  }
  render(
    <QueryClientProvider client={client}>
      <CapabilityPanel item={target} kind={kind} />
    </QueryClientProvider>,
  )
  return { client }
}

function sentBodies(path: string): Array<Record<string, unknown>> {
  return vi
    .mocked(fetch)
    .mock.calls.filter(([url, init]) => {
      const method = ((init as RequestInit | undefined)?.method ?? 'GET').toUpperCase()
      return String(url).includes(path) && method === 'POST'
    })
    .map(([, init]) => JSON.parse((init as RequestInit).body as string))
}

function actionCard(): HTMLElement {
  const card = document.querySelector<HTMLElement>('[data-capability-offer="append_note"]')
  if (!card) throw new Error('action card is not rendered')
  return card
}

describe('capability panel availability', () => {
  beforeEach(() => {
    mockServer()
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('describes an executor_missing offer as nobody having announced it, with no control', () => {
    const unavailable = offer({
      available: false,
      code: 'executor_missing',
      reason: `no active enrolled executor announced ${CAPABILITY}`,
    })
    mockServer({ offers: [unavailable] })
    renderPanel({ offers: [unavailable] })

    const card = actionCard()
    expect(card).toHaveAttribute('data-capability-code', 'executor_missing')
    expect(card).toHaveAttribute('data-capability-available', 'false')
    expect(within(card).getByText('executor_missing')).toBeInTheDocument()
    expect(
      within(card).getByText('No executor has announced this capability.'),
    ).toBeInTheDocument()
    expect(within(card).getByText(/request would wait in the queue/i)).toBeInTheDocument()
    // The server's own reason is quoted, not paraphrased.
    expect(
      within(card).getByText(
        (_, element) =>
          element?.tagName === 'P' &&
          (element.textContent ?? '').includes(
            `The server says: “no active enrolled executor announced ${CAPABILITY}”`,
          ),
      ),
    ).toBeInTheDocument()
    // An unavailable action is described; it is never a disabled-but-clickable control.
    expect(within(card).queryByRole('button')).toBeNull()
    expect(screen.queryByRole('button', { name: /Request this action/i })).toBeNull()
  })

  it('keeps not_authorized distinct from a missing executor', () => {
    const unavailable = offer({
      available: false,
      code: 'not_authorized',
      reason: 'capability x is not authorized for project p-1',
    })
    mockServer({ offers: [unavailable] })
    renderPanel({ offers: [unavailable] })

    const card = actionCard()
    expect(
      within(card).getByText('This project has not been granted this capability.'),
    ).toBeInTheDocument()
    expect(within(card).queryByText(/No executor has announced/)).toBeNull()
    expect(within(card).queryByRole('button')).toBeNull()
  })

  it('shows the durable status of the declared capability', () => {
    renderPanel()
    const row = document.querySelector<HTMLElement>(`[data-capability-status="${CAPABILITY}"]`)
    expect(row).not.toBeNull()
    expect(row).toHaveAttribute('data-capability-installed', 'true')
    expect(row).toHaveAttribute('data-capability-granted', 'true')
    expect(row).toHaveAttribute('data-capability-executors', '1')
    expect(within(row as HTMLElement).getByText('Installed')).toBeInTheDocument()
    expect(within(row as HTMLElement).getByText('Granted for this project')).toBeInTheDocument()
    expect(within(row as HTMLElement).getByText('Executor announced')).toBeInTheDocument()
    expect(within(row as HTMLElement).getByText('1 executor: exec-1')).toBeInTheDocument()
    expect(within(row as HTMLElement).getByText('Requires no other capability.')).toBeInTheDocument()
  })

  it('says plainly when this server has no capability status read', async () => {
    mockServer({ capabilities: null })
    renderPanel({ capabilities: null })
    await waitFor(() =>
      expect(document.querySelector('[data-capability-status-error]')).not.toBeNull(),
    )
    expect(document.querySelector('[data-capability-status-error]')).toHaveAttribute(
      'data-capability-status-error',
      'notFound',
    )
    expect(screen.getByText(/This server does not expose capability status/)).toBeInTheDocument()
  })
})

describe('capability requests', () => {
  beforeEach(() => {
    mockServer()
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('validates inputs before the confirmation step and sends nothing', async () => {
    const user = userEvent.setup()
    renderPanel()

    await user.click(screen.getByRole('button', { name: 'Request this action' }))
    const form = document.querySelector<HTMLElement>('[data-capability-request-form="append_note"]')
    expect(form).not.toBeNull()

    await user.click(within(form as HTMLElement).getByRole('button', { name: 'Review the request' }))
    expect(within(form as HTMLElement).getByText('Note is required.')).toBeInTheDocument()
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(sentBodies('/capability-actions/request')).toHaveLength(0)
  })

  it('records a mutating request only after a confirmation that names the executor', async () => {
    const user = userEvent.setup()
    mockServer({ requestResponse: requestRecord({ inputs: { note: 'Hello', tag: 'release' } }) })
    renderPanel()

    await user.click(screen.getByRole('button', { name: 'Request this action' }))
    const form = document.querySelector<HTMLElement>('[data-capability-request-form="append_note"]')
    await user.type(within(form as HTMLElement).getByLabelText(/^Note/), 'Hello')
    await user.type(within(form as HTMLElement).getByLabelText(/^Tag/), 'release')
    await user.click(within(form as HTMLElement).getByRole('button', { name: 'Review the request' }))

    const dialog = await screen.findByRole('dialog')
    expect(
      within(dialog).getByText('Append Note will be recorded as a request.'),
    ).toBeInTheDocument()
    expect(within(dialog).getByText(/waits for an executor/i)).toBeInTheDocument()
    expect(within(dialog).getByText(/This browser does not run the action/)).toBeInTheDocument()
    // Nothing is sent until the confirmation is confirmed.
    expect(sentBodies('/capability-actions/request')).toHaveLength(0)

    await user.click(within(dialog).getByRole('button', { name: 'Record Append Note' }))

    await waitFor(() => expect(sentBodies('/capability-actions/request')).toHaveLength(1))
    const [body] = sentBodies('/capability-actions/request')
    expect(body).toMatchObject({
      item_id: 'wi-1',
      expected_revision: 3,
      action_id: 'append_note',
      inputs: { note: 'Hello', tag: 'release' },
      confirm: true,
    })
    expect(typeof body.command_id).toBe('string')
    expect(String(body.command_id).length).toBeGreaterThan(8)

    await screen.findByText(/is recorded as a request/)
    // The form is closed: one intent was recorded, not left ready to send again.
    expect(document.querySelector('[data-capability-request-form]')).toBeNull()
  })

  it('does not ask for confirmation when the capability only reads', async () => {
    const user = userEvent.setup()
    const readStatus = statusRecord({ permission: 'read' })
    mockServer({ capabilities: [readStatus] })
    renderPanel({ capabilities: [readStatus] })

    await user.click(screen.getByRole('button', { name: 'Request this action' }))
    const form = document.querySelector<HTMLElement>('[data-capability-request-form="append_note"]')
    await user.type(within(form as HTMLElement).getByLabelText(/^Note/), 'Hello')
    await user.click(within(form as HTMLElement).getByRole('button', { name: 'Request Append Note' }))

    await waitFor(() => expect(sentBodies('/capability-actions/request')).toHaveLength(1))
    expect(sentBodies('/capability-actions/request')[0]).toMatchObject({ confirm: false })
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('shows the receipt verbatim and offers a reconcile for both unsettled outcomes', async () => {
    const uncertain = requestRecord({ state: 'uncertain' })
    mockServer()
    renderPanel({
      history: [
        requestRecord({
          command_id: 'cap-2',
          state: 'claimed',
          executor_device_id: 'exec-1',
        }),
        uncertain,
      ],
      show: { request: uncertain, receipt: receiptRecord() },
    })

    const row = document.querySelector<HTMLElement>('[data-capability-request="cap-1"]')
    expect(row).not.toBeNull()
    expect(row).toHaveAttribute('data-capability-state', 'uncertain')
    expect(within(row as HTMLElement).getByText('Uncertain')).toBeInTheDocument()

    // The receipt is the stored JSON document, with its reporter and time.
    await waitFor(() =>
      expect(document.querySelector('[data-capability-receipt="cap-1"]')).not.toBeNull(),
    )
    const receiptNode = document.querySelector<HTMLElement>('[data-capability-receipt="cap-1"]')
    expect(receiptNode?.querySelector('pre')?.textContent).toContain(
      '"reason": "the process died before writing"',
    )
    expect(receiptNode?.textContent).toContain('exec-1')

    // Both rows with no recorded outcome can be reconciled: `uncertain` is an
    // executor admitting it cannot tell, and `claimed` is silence after a claim
    // (the lost-acknowledgement case the server also settles).
    const uncertainReconcile = document.querySelector<HTMLElement>(
      '[data-capability-reconcile="cap-1"]',
    )
    expect(uncertainReconcile).not.toBeNull()
    expect(uncertainReconcile).toHaveAttribute('data-capability-reconcile-state', 'uncertain')
    expect(uncertainReconcile?.textContent).toContain(
      'The executor could not tell whether its effect landed.',
    )
    const claimedReconcile = document.querySelector<HTMLElement>(
      '[data-capability-reconcile="cap-2"]',
    )
    expect(claimedReconcile).not.toBeNull()
    expect(claimedReconcile).toHaveAttribute('data-capability-reconcile-state', 'claimed')
    expect(claimedReconcile?.textContent).toContain(
      'An executor took this request, and no result was recorded.',
    )
    expect(claimedReconcile?.textContent).toContain('answer may have been lost')
    expect(claimedReconcile?.textContent).not.toContain(
      'could not tell whether its effect landed',
    )
    const claimedRow = document.querySelector<HTMLElement>('[data-capability-request="cap-2"]')
    expect(
      within(claimedRow as HTMLElement).getByText(/An executor has taken it; no result yet/),
    ).toBeInTheDocument()
    // Never a retry control, anywhere in the panel.
    expect(screen.queryByRole('button', { name: /retry|send again|run again/i })).toBeNull()
  })

  it('reads a receipt on its own only for the newest request, and on request for older ones', async () => {
    const user = userEvent.setup()
    const oldest = requestRecord({ command_id: 'cap-3', state: 'succeeded' })
    const newest = requestRecord({ command_id: 'cap-4', state: 'failed' })
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input)
        if (url.includes('/capability-actions/show')) {
          const commandId = new URL(url, 'http://localhost').searchParams.get('command_id')
          return jsonResponse({
            request: commandId === 'cap-3' ? oldest : newest,
            receipt: receiptRecord({ outcome: 'failed' }),
          })
        }
        return jsonResponse({})
      }),
    )
    const client = testQueryClient()
    client.setQueryData(queryKeys.capabilityStatus(item.projectId), {
      capabilities: [statusRecord()],
    })
    client.setQueryData(queryKeys.capabilityOffers(item.id, item.revision), {
      itemId: item.id,
      itemRevision: item.revision,
      offers: [toCapabilityOffer(offer())],
    })
    client.setQueryData(
      queryKeys.capabilityRequests(item.id),
      [oldest, newest].map(toCapabilityRequest),
    )
    render(
      <QueryClientProvider client={client}>
        <CapabilityPanel item={item} kind={kind} />
      </QueryClientProvider>,
    )

    // The newest settled request reads its receipt without being asked.
    await waitFor(() =>
      expect(document.querySelector('[data-capability-receipt="cap-4"]')).not.toBeNull(),
    )
    expect(vi.mocked(fetch).mock.calls.map(([url]) => String(url)).join(' ')).not.toContain(
      'command_id=cap-3',
    )
    expect(document.querySelector('[data-capability-receipt="cap-3"]')).toBeNull()

    const showButton = document.querySelector<HTMLElement>(
      '[data-capability-show-receipt="cap-3"]',
    )
    expect(showButton).not.toBeNull()
    await user.click(showButton as HTMLElement)
    await waitFor(() =>
      expect(document.querySelector('[data-capability-receipt="cap-3"]')).not.toBeNull(),
    )
  })

  it('requires a reason to confirm a failure and records the decision', async () => {
    const user = userEvent.setup()
    const uncertain = requestRecord({ state: 'uncertain' })
    mockServer({
      reconcileResponse: {
        request: requestRecord({ state: 'failed' }),
        receipt: receiptRecord({ outcome: 'failed' }),
      },
    })
    renderPanel({
      history: [uncertain],
      show: { request: uncertain, receipt: receiptRecord() },
    })

    const reconcile = document.querySelector<HTMLElement>('[data-capability-reconcile="cap-1"]')
    expect(reconcile).not.toBeNull()
    await user.click(within(reconcile as HTMLElement).getByLabelText(/Confirm it failed/))
    await user.click(
      within(reconcile as HTMLElement).getByRole('button', { name: 'Record this decision' }),
    )
    expect(
      within(reconcile as HTMLElement).getByText(
        'Say what happened before confirming that it failed.',
      ),
    ).toBeInTheDocument()
    expect(sentBodies('/capability-actions/reconcile')).toHaveLength(0)

    await user.type(
      within(reconcile as HTMLElement).getByLabelText(/^Reason/),
      'the process died before writing',
    )
    await user.click(
      within(reconcile as HTMLElement).getByRole('button', { name: 'Record this decision' }),
    )

    await waitFor(() => expect(sentBodies('/capability-actions/reconcile')).toHaveLength(1))
    expect(sentBodies('/capability-actions/reconcile')[0]).toEqual({
      command_id: 'cap-1',
      decision: 'confirm_failed',
      detail: 'the process died before writing',
    })
  })

  it('offers no reconcile while a request is still pending', () => {
    renderPanel({ history: [requestRecord()] })
    const row = document.querySelector<HTMLElement>('[data-capability-request="cap-1"]')
    expect(row).toHaveAttribute('data-capability-state', 'pending')
    expect(
      within(row as HTMLElement).getByText(/Waiting for an executor to claim it/),
    ).toBeInTheDocument()
    expect(document.querySelector('[data-capability-reconcile]')).toBeNull()
  })

  it('shows the recorded request instead of offering the action again on the same revision', async () => {
    const settled = requestRecord({ state: 'succeeded', command_id: 'cap-settled' })
    mockServer()
    renderPanel({ history: [settled] })

    const card = actionCard()
    // The server still reports the action as available; the panel does not
    // pretend otherwise and does not change the availability contract.
    expect(card).toHaveAttribute('data-capability-available', 'true')
    expect(card).toHaveAttribute('data-capability-recorded-request', 'cap-settled')
    // …but a second request on this item revision would be refused, so it is not
    // offered. The recorded state is named, not hidden.
    expect(within(card).queryByRole('button', { name: 'Request this action' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Request this action' })).toBeNull()
    expect(within(card).getByText('Requested · Succeeded')).toBeInTheDocument()
    const recorded = card.querySelector<HTMLElement>('[data-capability-recorded="append_note"]')
    expect(recorded).toHaveAttribute('data-capability-recorded-state', 'succeeded')
    expect(recorded?.textContent).toContain('One action is requested once per item revision')
    // The link goes to the durable history, which still lists the request.
    const link = recorded?.querySelector<HTMLAnchorElement>('a[href="#capability-request-history"]')
    expect(link).not.toBeNull()
    expect(document.getElementById('capability-request-history')).not.toBeNull()
    expect(document.querySelector('[data-capability-request="cap-settled"]')).not.toBeNull()
    // History is rendered even while it is read: nothing was requested.
    await waitFor(() =>
      expect(document.querySelector('[data-capability-request="cap-settled"]')).not.toBeNull(),
    )
  })

  it('still offers the action when the recorded request belongs to an older revision', () => {
    mockServer()
    renderPanel({
      history: [requestRecord({ command_id: 'cap-old', item_revision: item.revision - 1 })],
    })

    const card = actionCard()
    expect(within(card).getByRole('button', { name: 'Request this action' })).toBeInTheDocument()
    expect(card.querySelector('[data-capability-recorded]')).toBeNull()
    expect(within(card).getByText('Available to request')).toBeInTheDocument()
  })
})

describe('capability request refusals', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('quotes the server when it refuses the request', async () => {
    const user = userEvent.setup()
    const readStatus = statusRecord({ permission: 'read' })
    mockServer({
      capabilities: [readStatus],
      requestFailure: {
        error: `capability action unavailable: executor_missing: no active enrolled executor announced ${CAPABILITY}`,
        status: 400,
      },
    })
    renderPanel({ capabilities: [readStatus] })

    await user.click(screen.getByRole('button', { name: 'Request this action' }))
    const form = document.querySelector<HTMLElement>('[data-capability-request-form="append_note"]')
    await user.type(within(form as HTMLElement).getByLabelText(/^Note/), 'Hello')
    await user.click(within(form as HTMLElement).getByRole('button', { name: 'Request Append Note' }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('No executor has announced this capability.')
    expect(alert.textContent).toContain('capability action unavailable: executor_missing')
    // The form stays open with its inputs, so nothing has to be retyped.
    expect(document.querySelector('[data-capability-request-form="append_note"]')).not.toBeNull()
  })

  it('describes the action as unavailable when the refusal is a durable fact', async () => {
    const user = userEvent.setup()
    const readStatus = statusRecord({ permission: 'read' })
    const unavailable = offer({
      available: false,
      code: 'not_authorized',
      reason: 'capability x is not authorized for project p-1',
    })
    // The server refuses the write and now reports the action as unavailable.
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input)
        const method = (init?.method ?? 'GET').toUpperCase()
        if (url.includes('/capability-actions/request') && method === 'POST') {
          return jsonResponse(
            { error: 'capability action unavailable: not_authorized: not granted' },
            400,
          )
        }
        if (url.includes('/capability-actions/list')) return jsonResponse({ requests: [] })
        if (url.includes('/capability-actions/show')) {
          return jsonResponse({ request: requestRecord(), receipt: null })
        }
        if (url.includes('/capability-actions')) {
          return jsonResponse({
            item_id: item.id,
            item_revision: item.revision,
            offers: [unavailable],
          })
        }
        return jsonResponse({ capabilities: [readStatus] })
      }),
    )
    const client = testQueryClient()
    client.setQueryData(queryKeys.capabilityStatus(item.projectId), { capabilities: [readStatus] })
    client.setQueryData(queryKeys.capabilityOffers(item.id, item.revision), {
      itemId: item.id,
      itemRevision: item.revision,
      offers: [toCapabilityOffer(offer())],
    })
    client.setQueryData(queryKeys.capabilityRequests(item.id), [])
    render(
      <QueryClientProvider client={client}>
        <CapabilityPanel item={item} kind={kind} />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Request this action' }))
    const form = document.querySelector<HTMLElement>('[data-capability-request-form="append_note"]')
    await user.type(within(form as HTMLElement).getByLabelText(/^Note/), 'Hello')
    await user.click(within(form as HTMLElement).getByRole('button', { name: 'Request Append Note' }))

    // The refusal is a durable fact now: the card explains it and offers no control.
    await waitFor(() =>
      expect(document.querySelector('[data-capability-unavailable="not_authorized"]')).not.toBeNull(),
    )
    expect(document.querySelector('[data-capability-request-form]')).toBeNull()
    expect(
      screen.getByText('This project has not been granted this capability.'),
    ).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Request this action' })).toBeNull()
  })
})
