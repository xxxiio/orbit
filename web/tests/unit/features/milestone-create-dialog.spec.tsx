import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import type { MilestoneCommand } from '@/api/contracts'
import { queryKeys } from '@/api/queries'
import { ToastProvider } from '@/components/ui/toast'
import { MilestoneCreateDialog } from '@/features/milestones'
import { jsonResponse, testQueryClient } from '../api/test-utils'

const created = {
  id: 'm-made',
  project_id: 'p-1',
  name: 'First review',
  description: 'Ship the beta to the review group.',
  status: 'planned',
  priority: null,
  owner: 'Alex',
  start_at_ms: new Date(2026, 9, 1).getTime(),
  target_at_ms: new Date(2026, 9, 20).getTime(),
  created_at_ms: 1,
  updated_at_ms: 1,
  task_ids: [],
  work_item_ids: [],
  progress: { total: 0, completed: 0, blocked: 0, percent: 0 },
  activities: [],
}

function renderDialog(onOpenChange = vi.fn()) {
  const client = testQueryClient()
  // The command fences on the cached milestone ledger revision.
  client.setQueryData(queryKeys.milestones(), { revision: 3, milestones: [] })
  render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <MilestoneCreateDialog
          open
          onOpenChange={onOpenChange}
          projectId="p-1"
          projectName="Orbit"
        />
      </ToastProvider>
    </QueryClientProvider>,
  )
  return onOpenChange
}

function commands(): MilestoneCommand[] {
  return vi
    .mocked(fetch)
    .mock.calls.filter(([, init]) => Boolean((init as RequestInit | undefined)?.body))
    .map(([, init]) => JSON.parse((init as RequestInit).body as string) as MilestoneCommand)
}

describe('milestone create dialog', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('sends the rich fields in one create and replays the same attempt on retry', async () => {
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ error: 'storage error' }, 500))
      .mockResolvedValueOnce(jsonResponse({ revision: 4, duplicate: false, milestone: created }))
    const onOpenChange = renderDialog()
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Name/), 'First review')
    await user.type(
      screen.getByLabelText(/Description/),
      'Ship the beta to the review group.',
    )
    await user.type(screen.getByLabelText(/Owner/), 'Alex')
    fireEvent.change(screen.getByLabelText(/Start date/), {
      target: { value: '2026-10-01' },
    })
    fireEvent.change(screen.getByLabelText(/Target date/), {
      target: { value: '2026-10-20' },
    })
    // A new milestone is offered only `planned`, and it is not editable.
    expect(screen.getByLabelText('Milestone status')).toBeDisabled()

    await user.click(screen.getByRole('button', { name: 'Create milestone' }))
    await waitFor(() => expect(commands()).toHaveLength(1))

    const first = commands()[0]
    expect(first.expected_revision).toBe(3)
    expect(first.action).toMatchObject({
      type: 'create',
      project_id: 'p-1',
      name: 'First review',
      description: 'Ship the beta to the review group.',
      owner: 'Alex',
      start_at_ms: new Date(2026, 9, 1).getTime(),
      target_at_ms: new Date(2026, 9, 20).getTime(),
    })

    // The unanswered attempt is retried with the same command and milestone
    // identities, so the server's receipt replay cannot create a duplicate.
    await user.click(screen.getByRole('button', { name: 'Create milestone' }))
    await waitFor(() => expect(commands()).toHaveLength(2))
    expect(commands()[1].command_id).toBe(first.command_id)
    expect((commands()[1].action as { milestone_id: string }).milestone_id).toBe(
      (first.action as { milestone_id: string }).milestone_id,
    )
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))
  })

  it('refuses a start date after the target without sending a command', async () => {
    renderDialog()
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Name/), 'First review')
    fireEvent.change(screen.getByLabelText(/Start date/), {
      target: { value: '2026-10-20' },
    })
    fireEvent.change(screen.getByLabelText(/Target date/), {
      target: { value: '2026-10-01' },
    })
    await user.click(screen.getByRole('button', { name: 'Create milestone' }))

    expect(screen.getByText('The start date must be on or before the target date.')).toBeInTheDocument()
    expect(fetch).not.toHaveBeenCalled()
  })
})
