import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import type { Workflow } from '@/api/adapters'
import { NewWorkItemDialog } from '@/features/workflows/work-item-create-dialog'
import { ToastProvider } from '@/components/ui/toast'
import { jsonResponse, testQueryClient } from '../api/test-utils'

/**
 * The create dialog mints one attempt (command id + item id) per submitted
 * payload. An unconfirmed failure therefore leaves a *retry* — same ids — while
 * a changed payload is not offered at all, because it would be a second create
 * while the first one may already be stored.
 */
const workflow: Workflow = {
  id: 'development',
  version: 1,
  name: 'Development',
  description: '',
  requiredCapabilities: [],
  templates: [],
  kinds: [
    {
      id: 'feature',
      name: 'Feature',
      initialState: 'backlog',
      states: ['backlog', 'ready'],
      fields: [
        { id: 'summary', kind: { type: 'text' }, required: true, default: null },
      ],
      transitions: [],
    },
  ],
  views: [],
}

const created = {
  id: 'wi-made',
  project_id: 'p-1',
  kind: 'feature',
  state: 'backlog',
  properties: { summary: 'Retry me' },
  revision: 0,
}

function renderDialog(onClose = vi.fn()) {
  render(
    <QueryClientProvider client={testQueryClient()}>
      <ToastProvider>
        <NewWorkItemDialog
          projectId="p-1"
          workflow={workflow}
          pinnedKind={null}
          onClose={onClose}
        />
      </ToastProvider>
    </QueryClientProvider>,
  )
  return onClose
}

function bodies(): Array<Record<string, unknown>> {
  return vi
    .mocked(fetch)
    .mock.calls.map(([, init]) => JSON.parse((init as RequestInit).body as string))
}

describe('work item create dialog idempotency', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('retries an unconfirmed create with the same item id', async () => {
    vi.mocked(fetch)
      .mockRejectedValueOnce(new TypeError('network down'))
      .mockResolvedValueOnce(jsonResponse(created))

    const onClose = renderDialog()
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Summary/), 'Retry me')
    await user.click(screen.getByRole('button', { name: 'Create work item' }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(/may or may not have been saved/i)

    // The unchanged payload is still submittable: it is the same request.
    const retry = screen.getByRole('button', { name: 'Retry the same request' })
    expect(retry).toBeEnabled()
    await user.click(retry)

    await waitFor(() => expect(onClose).toHaveBeenCalled())

    const sent = bodies()
    expect(sent).toHaveLength(2)
    expect(sent[0].id).toEqual(expect.any(String))
    expect(sent[1].id).toBe(sent[0].id)
    expect(sent[1].project_id).toBe('p-1')
    expect(sent[0].kind).toBe('feature')
    expect(sent[0].properties).toEqual({ summary: 'Retry me' })
    expect(sent[1].properties).toEqual(sent[0].properties)
  })

  it('refuses a changed payload while the earlier create is unconfirmed', async () => {
    vi.mocked(fetch).mockRejectedValueOnce(new TypeError('network down'))

    renderDialog()
    const user = userEvent.setup()
    const summary = screen.getByLabelText(/Summary/)

    await user.type(summary, 'Once')
    await user.click(screen.getByRole('button', { name: 'Create work item' }))
    await screen.findByRole('alert')

    await user.type(summary, ' edited')
    expect(screen.getByRole('button', { name: 'Create work item' })).toBeDisabled()
    expect(screen.getByText(/no longer match the request whose outcome is unknown/)).toBeInTheDocument()

    // Reverting to the submitted payload restores the same-id retry.
    await user.clear(summary)
    await user.type(summary, 'Once')
    expect(screen.getByRole('button', { name: 'Retry the same request' })).toBeEnabled()
    expect(bodies()).toHaveLength(1)
  })
})
