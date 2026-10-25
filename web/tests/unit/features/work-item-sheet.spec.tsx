import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import type { WorkItem, Workflow } from '@/api/adapters'
import { queryKeys } from '@/api/queries'
import { WorkItemSheet } from '@/features/workflows/work-item-sheet'
import { jsonResponse, testQueryClient } from '../api/test-utils'

const workflow: Workflow = {
  id: 'guarded',
  version: 1,
  name: 'Guarded release',
  description: '',
  requiredCapabilities: [],
  templates: [],
  kinds: [
    {
      id: 'ticket',
      name: 'Ticket',
      initialState: 'open',
      states: ['open', 'approved'],
      fields: [
        {
          id: 'priority',
          kind: { type: 'choice', options: ['low', 'high'] },
          required: true,
          default: 'low',
        },
        { id: 'note', kind: { type: 'text' }, required: false, default: null },
        { id: 'minutes', kind: { type: 'integer' }, required: false, default: null },
      ],
      transitions: [
        {
          from: 'open',
          to: 'approved',
          requiredCapability: 'orbit.release.approve',
          guards: [{ type: 'property_equals', field: 'priority', value: 'high' }],
          actions: [
            {
              id: 'append_note',
              capability: 'orbit.release.note',
              description: 'Append a durable note to this ticket.',
              parameters: { note: '', visible: true },
            },
          ],
        },
      ],
    },
  ],
  views: [],
}

function renderSheet(item: WorkItem, onRefresh?: () => void) {
  const onClose = vi.fn()
  const client = testQueryClient()
  // Every command fences on the cached item revision.
  client.setQueryData(queryKeys.workItems(item.projectId), [
    {
      id: item.id,
      project_id: item.projectId,
      kind: item.kind,
      state: item.state,
      properties: item.properties,
      revision: item.revision,
    },
  ])
  // The sheet mounts the shared attachment list. Seed an empty list so these
  // sheet-focused tests do not spend their mocked responses on its read.
  client.setQueryData(queryKeys.attachments('work_item', item.id), { attachments: [] })
  render(
    <QueryClientProvider client={client}>
      <WorkItemSheet item={item} workflow={workflow} onClose={onClose} onRefresh={onRefresh} />
    </QueryClientProvider>,
  )
  return { onClose }
}

const openTicket: WorkItem = {
  id: 'wi-open',
  projectId: 'p-1',
  kind: 'ticket',
  state: 'open',
  properties: { priority: 'low', minutes: 30 },
  revision: 3,
}

function sentBodies(): Array<Record<string, unknown>> {
  return vi
    .mocked(fetch)
    .mock.calls.map(([, init]) => JSON.parse((init as RequestInit).body as string))
}

describe('work-item sheet declarations', () => {
  it('renders guards as sentences and the required capability as a chip', () => {
    renderSheet(openTicket)
    const moves = screen.getByRole('region', { name: 'Moves' })
    expect(within(moves).getByText(/Requires Priority to be High\./)).toBeInTheDocument()
    expect(
      within(moves).getByText('Requires capability Orbit release approve'),
    ).toBeInTheDocument()
    expect(
      within(moves).getByText(/Browser sessions are granted no capabilities/),
    ).toBeInTheDocument()
    expect(
      within(moves).getByRole('button', { name: 'Move work item to Approved' }),
    ).toBeInTheDocument()
  })

  it('lists declared actions with capability and parameters, and no run control', () => {
    renderSheet(openTicket)
    const actions = screen.getByRole('region', { name: 'Actions' })

    expect(within(actions).getByText('Append Note')).toBeInTheDocument()
    expect(
      within(actions).getByText('Append a durable note to this ticket.'),
    ).toBeInTheDocument()
    expect(
      within(actions).getByText('Requires capability Orbit release note'),
    ).toBeInTheDocument()
    expect(within(actions).getByText('note')).toBeInTheDocument()
    expect(within(actions).getByText('string')).toBeInTheDocument()
    expect(within(actions).getByText('boolean')).toBeInTheDocument()
    expect(
      within(actions).getByText('Execution requires the local Orbit authority.'),
    ).toBeInTheDocument()
    expect(
      within(actions).getByText('Capability status is not exposed by this server.'),
    ).toBeInTheDocument()
    // The only buttons on the sheet are the declared move and the close control;
    // there is no "run" affordance that would pretend to execute a capability.
    expect(within(actions).queryByRole('button')).toBeNull()
    expect(screen.queryByRole('button', { name: /run|execute/i })).toBeNull()
  })

  it('says plainly when a state declares no actions', () => {
    renderSheet({ ...openTicket, state: 'approved' })
    const actions = screen.getByRole('region', { name: 'Actions' })
    expect(within(actions).getByText(/This state declares no actions/)).toBeInTheDocument()
    expect(
      within(actions).getByText('Capability status is not exposed by this server.'),
    ).toBeInTheDocument()
  })

  it('shows the revision and the stored property values', () => {
    renderSheet(openTicket)
    expect(screen.getAllByText('Revision 3').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Low').length).toBeGreaterThan(0)
  })
})

describe('work-item sheet closes honestly', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reverts a cleared integer and closes without sending a clear command', async () => {
    const user = userEvent.setup()
    const { onClose } = renderSheet(openTicket)

    const minutes = screen.getByLabelText(/Minutes/)
    await user.clear(minutes)
    await user.click(screen.getByRole('button', { name: 'Close work item details' }))

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1))
    // The API has no clear-property command, so clearing must not send anything.
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
    expect(screen.getByText(/Clearing a stored number is not supported/)).toBeInTheDocument()
    expect(minutes).toHaveValue(30)
  })

  it('saves a fresh draft after an earlier close already flushed another one', async () => {
    vi.mocked(fetch).mockResolvedValue(
      jsonResponse({
        id: 'wi-open',
        project_id: 'p-1',
        kind: 'ticket',
        state: 'open',
        properties: { priority: 'low', minutes: 45 },
        revision: 4,
      }),
    )
    const user = userEvent.setup()
    const { onClose } = renderSheet(openTicket)

    const minutes = screen.getByLabelText(/Minutes/)
    // Clear → Escape: the commit reverts (no clear command exists), so nothing
    // is sent and the sheet asks for another close.
    await user.clear(minutes)
    await user.keyboard('{Escape}')
    await screen.findByText(/Your edit is being saved first/)
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
    expect(onClose).not.toHaveBeenCalled()

    // A new draft typed after that flush must be saved, never discarded by the
    // earlier flush's bookkeeping.
    await user.clear(minutes)
    await user.type(minutes, '45')
    await user.keyboard('{Escape}')

    await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1))
    expect(sentBodies()[0]?.changes).toEqual({ minutes: 45 })
    // The sheet stays open for the answer: a close with zero requests is the
    // silent data loss this guards against.
    expect(onClose).not.toHaveBeenCalled()
  })

  it('sends a typed value once when the sheet is closed in the same interaction', async () => {
    vi.mocked(fetch).mockResolvedValue(
      jsonResponse({
        id: 'wi-open',
        project_id: 'p-1',
        kind: 'ticket',
        state: 'open',
        properties: { priority: 'low', note: 'Edited once', minutes: 30 },
        revision: 4,
      }),
    )
    const user = userEvent.setup()
    renderSheet(openTicket)

    const note = screen.getByPlaceholderText('Add note')
    await user.type(note, 'Edited once')
    expect(note).toHaveValue('Edited once')
    await user.click(screen.getByRole('button', { name: 'Close work item details' }))

    // Blur submits the draft; close must not flush the same draft a second time.
    await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1))
    expect(sentBodies()[0]?.changes).toEqual({ note: 'Edited once' })
    expect(screen.queryByText(/The server refused the change/)).toBeNull()
  })

  it('offers a refresh action when the server refuses a stale write', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse({ error: 'stale revision' }, 400))
    const onRefresh = vi.fn()
    const user = userEvent.setup()
    renderSheet(openTicket, onRefresh)

    await user.click(screen.getByRole('button', { name: 'Move work item to Approved' }))

    await screen.findByText(/The server refused the change/)
    await user.click(screen.getByRole('button', { name: 'Refresh the item and try again' }))
    expect(onRefresh).toHaveBeenCalledTimes(1)
  })
})
