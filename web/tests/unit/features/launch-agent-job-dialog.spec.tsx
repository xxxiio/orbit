import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter, useLocation } from 'react-router-dom'
import type { AgentExecutor, Task } from '@/api/adapters'
import type { AgentJobView } from '@/api/contracts'
import { LaunchAgentJobDialog } from '@/features/agents/launch/launch-agent-job-dialog'
import { jsonResponse, snapshotAt, testQueryClient } from '../api/test-utils'

/**
 * The launch dialog queues a job; it never claims one. These tests hold it to
 * three promises: it cannot be submitted without an enrolled executor, the
 * confirmation says what queueing does not do, and a retry re-sends the same
 * job id instead of minting a second one.
 */
const task: Task = {
  id: 't-1',
  title: 'Ship the release',
  status: 'ready',
  projectId: 'p-1',
  milestoneId: null,
  estimatedMinutes: null,
  earliestStartAtMs: null,
  deadlineAtMs: null,
  description: '',
  priority: null,
  labels: [],
  createdAtMs: null,
  updatedAtMs: null,
}

const executor: AgentExecutor = {
  deviceId: 'dev-1',
  active: true,
  lastClaimAtMs: null,
  claimedJobs: 0,
  preparingJobs: 0,
}

const startedJob: AgentJobView = {
  id: 'job-new',
  request_id: 'job-new',
  project_id: 'p-1',
  task_id: 't-1',
  state: 'preparing',
  revision: 1,
  agent_kind: 'codex',
  machine_ref: 'dev-1',
  runtime_kind: 'herdr',
  repository_ref: 'orbit',
  runtime_session_ref: null,
  created_at_ms: 1_761_432_000_000,
  updated_at_ms: 1_761_432_000_000,
  claim_state: null,
}

function LocationProbe() {
  const location = useLocation()
  return (
    <p data-testid="location">
      {location.pathname} {JSON.stringify(location.state)}
    </p>
  )
}

type StartResponder = (init: RequestInit | undefined) => Response | Promise<Response>

function stubApi({
  executors = [executor],
  start,
}: {
  executors?: AgentExecutor[]
  start?: StartResponder
} = {}) {
  const startResponder: StartResponder = start ?? (() => jsonResponse(startedJob))
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.includes('/authoritative/agent-executors')) {
        return jsonResponse({
          executors: executors.map((entry) => ({
            device_id: entry.deviceId,
            active: entry.active,
            last_claim_at_ms: entry.lastClaimAtMs,
            claimed_jobs: entry.claimedJobs,
            preparing_jobs: entry.preparingJobs,
          })),
        })
      }
      if (url.includes('/authoritative/agent-jobs/overview')) return jsonResponse([])
      if (url.includes('/authoritative/agent-jobs/start')) return startResponder(init)
      if (url.includes('/authoritative/snapshot')) return jsonResponse(snapshotAt(1))
      return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )
}

function renderDialog() {
  const onClose = vi.fn()
  render(
    <MemoryRouter initialEntries={['/agents']}>
      <QueryClientProvider client={testQueryClient()}>
        <LaunchAgentJobDialog fixedTask={task} onClose={onClose} />
        <LocationProbe />
      </QueryClientProvider>
    </MemoryRouter>,
  )
  return onClose
}

function startBodies(): Array<Record<string, unknown>> {
  return vi
    .mocked(fetch)
    .mock.calls.filter(
      ([input, init]) =>
        String(input).includes('/agent-jobs/start') &&
        (init as RequestInit | undefined)?.method === 'POST',
    )
    .map(([, init]) => JSON.parse((init as RequestInit).body as string) as Record<string, unknown>)
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('launch agent job dialog', () => {
  it('disables launching and names the missing executor when none is enrolled', async () => {
    stubApi({ executors: [] })
    renderDialog()
    const user = userEvent.setup()

    const review = screen.getByRole('button', { name: 'Review and queue' })
    await waitFor(() => expect(review).toBeDisabled())
    expect(await screen.findByText(/No executor is enrolled/)).toBeInTheDocument()
    expect(screen.getByLabelText(/Executor/)).toBeDisabled()

    await user.click(review)
    // Nothing was sent, and the confirm step was never reached.
    expect(screen.queryByText(/Queueing this job does not start it/)).not.toBeInTheDocument()
    expect(startBodies()).toHaveLength(0)
  })

  it('requires a repository reference and ties the error to the input', async () => {
    stubApi()
    renderDialog()
    const user = userEvent.setup()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Review and queue' })).toBeEnabled(),
    )
    await user.click(screen.getByRole('button', { name: 'Review and queue' }))

    const repository = screen.getByLabelText(/Repository/)
    expect(repository).toHaveAttribute('aria-invalid', 'true')
    expect(repository).toHaveAttribute(
      'aria-describedby',
      expect.stringContaining('agent-launch-repository-error'),
    )
    expect(screen.getByText(/Enter the repository reference/)).toBeInTheDocument()
    expect(screen.queryByText(/Queueing this job does not start it/)).not.toBeInTheDocument()
  })

  it('confirms in plain words, then opens the job the server created', async () => {
    stubApi()
    const onClose = renderDialog()
    const user = userEvent.setup()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Review and queue' })).toBeEnabled(),
    )
    await user.type(screen.getByLabelText(/Repository/), 'orbit')
    await user.type(
      screen.getByLabelText(/Objective/),
      'Fix the login redirect and add a regression test.',
    )
    await user.click(screen.getByRole('button', { name: 'Review and queue' }))

    // The confirmation says what queueing does and does not do.
    const confirm = document.querySelector('[data-launch-confirm]') as HTMLElement
    expect(confirm).not.toBeNull()
    expect(within(confirm).getByText(/Queueing this job does not start it/)).toBeInTheDocument()
    expect(within(confirm).getByText(/must claim it before/)).toBeInTheDocument()
    expect(within(confirm).getByText(/does not start a model/)).toBeInTheDocument()
    expect(within(confirm).getByText(/does not complete the task/)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Queue the job' }))

    await waitFor(() => expect(onClose).toHaveBeenCalled())
    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/agents/job-new'),
    )

    const sent = startBodies()
    expect(sent).toHaveLength(1)
    expect(sent[0].task_id).toBe('t-1')
    expect(sent[0].machine_ref).toBe('dev-1')
    expect(sent[0].agent_kind).toBe('codex')
    expect(sent[0].repository_ref).toBe('orbit')
    expect(sent[0].worktree).toBe(false)
    expect(sent[0].objective).toBe('Fix the login redirect and add a regression test.')
    expect(sent[0].job_id).toEqual(expect.any(String))
    expect(sent[0].at_ms).toEqual(expect.any(Number))
  })

  it('flags a device that has never claimed beside the picker and repeats it on the confirmation', async () => {
    stubApi()
    renderDialog()
    const user = userEvent.setup()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Review and queue' })).toBeEnabled(),
    )
    // The flag is visible next to the control, with the reason in one line.
    const formWarning = document.querySelector('[data-executor-never-claimed]')
    expect(formWarning).not.toBeNull()
    expect(formWarning).toHaveTextContent('dev-1 has never claimed a job.')
    expect(formWarning).toHaveTextContent(/waits at Preparing until something on that device claims it/)

    await user.type(screen.getByLabelText(/Repository/), 'orbit')
    await user.click(screen.getByRole('button', { name: 'Review and queue' }))

    // It is repeated at the step above the button, and it does not disable it.
    const confirmation = document.querySelector('[data-launch-never-claimed]')
    expect(confirmation).not.toBeNull()
    expect(confirmation).toHaveTextContent('This device has never claimed a job.')
    expect(confirmation).toHaveTextContent('dev-1 has not claimed a job before')
    expect(confirmation).toHaveTextContent(/waits at Preparing indefinitely/)
    expect(confirmation).toHaveTextContent(/Preparing does not mean work has started/)
    const queue = screen.getByRole('button', { name: 'Queue the job' })
    expect(queue).toBeEnabled()
    expect(queue).toHaveAttribute('aria-describedby', 'agent-launch-never-claimed')

    await user.click(queue)
    await waitFor(() => expect(startBodies()).toHaveLength(1))
  })

  it('says nothing about claims for a device that has claimed before', async () => {
    stubApi({
      executors: [
        {
          ...executor,
          lastClaimAtMs: 1_761_432_000_000,
          claimedJobs: 2,
          preparingJobs: 1,
        },
      ],
    })
    renderDialog()
    const user = userEvent.setup()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Review and queue' })).toBeEnabled(),
    )
    expect(document.querySelector('[data-executor-never-claimed]')).toBeNull()
    expect(screen.queryByText(/no claim yet/)).not.toBeInTheDocument()

    await user.type(screen.getByLabelText(/Repository/), 'orbit')
    await user.click(screen.getByRole('button', { name: 'Review and queue' }))

    expect(document.querySelector('[data-launch-never-claimed]')).toBeNull()
    expect(screen.getByRole('button', { name: 'Queue the job' })).not.toHaveAttribute(
      'aria-describedby',
    )
  })

  it('quotes the server refusal and retries with the same job id', async () => {
    stubApi({
      start: () => jsonResponse({ error: 'task not eligible for connected start' }, 400),
    })
    renderDialog()
    const user = userEvent.setup()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Review and queue' })).toBeEnabled(),
    )
    await user.type(screen.getByLabelText(/Repository/), 'orbit')
    await user.click(screen.getByRole('button', { name: 'Review and queue' }))
    await user.click(screen.getByRole('button', { name: 'Queue the job' }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      /task not eligible for connected start/,
    )

    await user.click(screen.getByRole('button', { name: 'Retry the same launch' }))

    await waitFor(() => expect(startBodies()).toHaveLength(2))
    const [first, retry] = startBodies()
    expect(retry.job_id).toBe(first.job_id)
    expect(retry.at_ms).toBe(first.at_ms)
  })

  it('reads the pinned job instead of re-posting when the answer never arrives', async () => {
    stubApi({ start: () => Promise.reject(new TypeError('network down')) })
    const onClose = renderDialog()
    const user = userEvent.setup()

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Review and queue' })).toBeEnabled(),
    )
    await user.type(screen.getByLabelText(/Repository/), 'orbit')
    await user.click(screen.getByRole('button', { name: 'Review and queue' }))
    await user.click(screen.getByRole('button', { name: 'Queue the job' }))

    await waitFor(() => expect(onClose).toHaveBeenCalled())
    const location = screen.getByTestId('location')
    expect(location).toHaveTextContent('/agents/')
    expect(location).toHaveTextContent('launchUnconfirmed')
    expect(startBodies()).toHaveLength(1)
  })
})
