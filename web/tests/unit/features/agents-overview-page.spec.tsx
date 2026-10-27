import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { AgentsOverviewPage } from '@/features/agents/agents-overview-page'
import { jsonResponse, snapshotAt, testQueryClient } from '../api/test-utils'

/**
 * The overview must show a queued job that no executor has claimed. It is not
 * `needs_attention`, so it cannot live only in the attention list, and it must
 * not read as "nothing to do" either.
 */
const awaitingJob = {
  job_id: 'job-awaiting',
  project_id: 'p-1',
  task_id: 't-1',
  state: 'preparing',
  updated_at_ms: 1_761_432_000_000,
  handoff_count: 0,
  review_decision: null,
  needs_attention: false,
  next_action: 'await_executor',
}

function stubApi({
  executors,
  executorStatus = 200,
}: {
  executors: unknown[]
  executorStatus?: number
}) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/authoritative/agent-executors')) {
        return executorStatus === 200
          ? jsonResponse({ executors })
          : jsonResponse({ error: 'not found' }, executorStatus)
      }
      if (url.includes('/authoritative/agent-jobs/overview')) return jsonResponse([awaitingJob])
      if (url.includes('/authoritative/snapshot')) return jsonResponse(snapshotAt(1))
      return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )
}

function renderPage() {
  render(
    <MemoryRouter>
      <QueryClientProvider client={testQueryClient()}>
        <AgentsOverviewPage />
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('agents overview', () => {
  it('lists a job with no claim as awaiting an executor, not as nothing to do', async () => {
    stubApi({ executors: [] })
    renderPage()

    const list = await screen.findByRole('list', { name: 'Awaiting an executor' })
    expect(within(list).getByText('Preparing')).toBeInTheDocument()
    expect(within(list).getAllByText('Awaiting an executor').length).toBeGreaterThan(0)
    // The queued job is not in the attention list.
    expect(screen.queryByRole('list', { name: 'Needs attention' })).not.toBeInTheDocument()
  })

  it('says no executor is enrolled and disables the launch control', async () => {
    stubApi({ executors: [] })
    renderPage()

    expect(await screen.findByText('No executor is enrolled')).toBeInTheDocument()
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Queue a job' })).toBeDisabled(),
    )
    expect(screen.getByText(/Enrol a device as an executor first/)).toBeInTheDocument()
  })

  it('names an older server instead of pretending the list is empty', async () => {
    stubApi({ executors: [], executorStatus: 404 })
    renderPage()

    expect(
      await screen.findByText(/does not provide the executor list/),
    ).toBeInTheDocument()
    expect(screen.queryByText('No executor is enrolled')).not.toBeInTheDocument()
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Queue a job' })).toBeDisabled(),
    )
  })

  it('offers the launch control when an executor is enrolled', async () => {
    stubApi({
      executors: [
        {
          device_id: 'dev-1',
          active: true,
          last_claim_at_ms: null,
          claimed_jobs: 0,
          preparing_jobs: 1,
        },
      ],
    })
    renderPage()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Queue a job' })).toBeEnabled(),
    )
    expect(screen.getByText('dev-1')).toBeInTheDocument()
    // A device with no claim on record is flagged, not hidden or disabled.
    expect(screen.getByText('has never claimed a job')).toBeInTheDocument()
    expect(
      screen.getByText(/waits at Preparing until something on that device claims it/),
    ).toBeInTheDocument()
    expect(
      document.querySelector('[data-executor="dev-1"] [data-executor-never-claimed]'),
    ).not.toBeNull()
  })

  it('gives the last claim for a device that has claimed before', async () => {
    stubApi({
      executors: [
        {
          device_id: 'dev-1',
          active: true,
          last_claim_at_ms: 1_761_432_000_000,
          claimed_jobs: 2,
          preparing_jobs: 1,
        },
      ],
    })
    renderPage()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Queue a job' })).toBeEnabled(),
    )
    expect(screen.getByText(/last claim/)).toBeInTheDocument()
    expect(screen.queryByText('has never claimed a job')).not.toBeInTheDocument()
    expect(document.querySelector('[data-executor-never-claimed]')).toBeNull()
  })
})
