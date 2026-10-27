import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import type { Task } from '@/api/adapters'
import { TaskAgentJobSection } from '@/features/agents/launch/task-agent-job-section'
import { jsonResponse, snapshotAt, testQueryClient } from '../api/test-utils'

/**
 * The drawer's launch control explains eligibility but never decides it: the
 * server re-checks the status, the prerequisites and the active-job rule.
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

function stubApi(jobs: unknown[] = []) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/authoritative/agent-executors')) {
        return jsonResponse({
          executors: [
            {
              device_id: 'dev-1',
              active: true,
              last_claim_at_ms: null,
              claimed_jobs: 0,
              preparing_jobs: 0,
            },
          ],
        })
      }
      if (url.includes('/authoritative/agent-jobs/overview')) return jsonResponse(jobs)
      if (url.includes('/authoritative/snapshot')) return jsonResponse(snapshotAt(1))
      return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )
}

function renderSection(overrides: Partial<Task> = {}) {
  render(
    <MemoryRouter>
      <QueryClientProvider client={testQueryClient()}>
        <TaskAgentJobSection task={{ ...task, ...overrides }} />
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('task drawer agent section', () => {
  it('opens the launch dialog for a ready task with an enrolled executor', async () => {
    stubApi()
    renderSection()
    const user = userEvent.setup()

    const button = await screen.findByRole('button', { name: 'Queue an agent job' })
    await waitFor(() => expect(button).toBeEnabled())
    await user.click(button)

    expect(await screen.findByRole('dialog')).toBeInTheDocument()
    expect(
      screen.getByRole('heading', { name: 'Queue an agent job' }),
    ).toBeInTheDocument()
  })

  it('explains a status the server would refuse instead of offering the launch', async () => {
    stubApi()
    renderSection({ status: 'waiting' })

    expect(await screen.findByText(/Only a task that is ready, in progress or in review/)).toBeInTheDocument()
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Queue an agent job' })).toBeDisabled(),
    )
  })

  it('points at the existing job and offers no second launch for the same task', async () => {
    stubApi([
      {
        job_id: 'job-1',
        project_id: 'p-1',
        task_id: 't-1',
        state: 'running',
        updated_at_ms: 1,
        handoff_count: 0,
        review_decision: null,
        needs_attention: false,
        next_action: 'respond_to_agent',
      },
    ])
    renderSection()

    const link = await screen.findByRole('link', { name: /Open job/ })
    expect(link).toHaveAttribute('href', '/agents/job-1')
    expect(screen.getByText(/This task has an active job in running/)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Queue an agent job' })).not.toBeInTheDocument()
  })
})
