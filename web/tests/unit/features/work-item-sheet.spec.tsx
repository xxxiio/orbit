import { describe, expect, it } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import type { WorkItem, Workflow } from '@/api/adapters'
import { WorkItemSheet } from '@/features/workflows/work-item-sheet'
import { testQueryClient } from '../api/test-utils'

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

function renderSheet(item: WorkItem) {
  render(
    <QueryClientProvider client={testQueryClient()}>
      <WorkItemSheet item={item} workflow={workflow} onClose={() => {}} />
    </QueryClientProvider>,
  )
}

const openTicket: WorkItem = {
  id: 'wi-open',
  projectId: 'p-1',
  kind: 'ticket',
  state: 'open',
  properties: { priority: 'low' },
  revision: 3,
}

describe('work-item sheet declarations', () => {
  it('renders guards as sentences and the required capability as a chip', () => {
    renderSheet(openTicket)
    const moves = screen.getByRole('region', { name: 'Moves' })
    expect(within(moves).getByText(/Requires Priority to be High\./)).toBeInTheDocument()
    expect(
      within(moves).getByText('Requires capability orbit.release.approve'),
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
      within(actions).getByText('Requires capability orbit.release.note'),
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
    expect(screen.getByText('Revision 3')).toBeInTheDocument()
    expect(screen.getAllByText('Low').length).toBeGreaterThan(0)
  })
})
