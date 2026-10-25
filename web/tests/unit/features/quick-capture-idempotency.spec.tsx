import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import { queryKeys } from '@/api/queries'
import { QuickCapture } from '@/features/inbox/quick-capture'
import { jsonResponse, snapshotAt, taskRecord, testQueryClient } from '../api/test-utils'

/**
 * Pressing capture again after an unconfirmed failure is a *retry*, not a new
 * task: the ids travel with the intent (minted at submit) and are reused while
 * the text is unchanged, so the server replays its receipt.
 */
describe('quick capture idempotency', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reuses the command id and task id when the same capture is retried after an uncertain failure', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(7))

    vi.mocked(fetch)
      .mockRejectedValueOnce(new TypeError('network down'))
      .mockResolvedValueOnce(
        jsonResponse({ revision: 8, duplicate: false, task: taskRecord({ id: 't-captured' }) }),
      )

    const user = userEvent.setup()
    render(
      <QueryClientProvider client={client}>
        <QuickCapture />
      </QueryClientProvider>,
    )

    const input = screen.getByLabelText('Capture a task for the Inbox')
    await user.type(input, 'Retry the capture')
    await user.keyboard('{Enter}')

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(/may or may not have been saved/i)
    // The honest message is not the rejection copy.
    expect(alert).not.toHaveTextContent(/was not captured/i)
    // The text is kept so the person can retry with one keystroke.
    expect(input).toHaveValue('Retry the capture')

    await user.click(screen.getByRole('button', { name: /^capture$/i }))

    await waitFor(() => expect(input).toHaveValue(''))

    const bodies = vi
      .mocked(fetch)
      .mock.calls.map(([, init]) => JSON.parse((init as RequestInit).body as string))
    expect(bodies).toHaveLength(2)
    expect(bodies[1].command_id).toBe(bodies[0].command_id)
    expect(bodies[1].action.task_id).toBe(bodies[0].action.task_id)
    expect(bodies[1].action.title).toBe('Retry the capture')
  })
})
