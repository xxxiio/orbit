import type { ReactElement } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import type { WorkflowViewComponent } from '@/api/adapters'
import { AgentJobsPanel } from '@/features/agents'
import { KnowledgeRefsPanel } from '@/features/knowledge'
import { jsonResponse, snapshotAt, taskRecord, testQueryClient } from '../api/test-utils'

function component(
  overrides: Partial<WorkflowViewComponent> & Pick<WorkflowViewComponent, 'id' | 'type'>,
): WorkflowViewComponent {
  return {
    title: null,
    source: null,
    aggregate: null,
    field: null,
    groupBy: null,
    filter: {},
    sort: null,
    descending: false,
    limit: null,
    columns: [],
    text: null,
    actions: [],
    options: {},
    ...overrides,
  }
}

function renderPanel(panel: ReactElement) {
  return render(
    <MemoryRouter>
      <QueryClientProvider client={testQueryClient()}>{panel}</QueryClientProvider>
    </MemoryRouter>,
  )
}

function stubOverview(overview: unknown[]) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/authoritative/agent-jobs/overview')) return jsonResponse(overview)
      if (url.includes('/authoritative/snapshot')) {
        return jsonResponse(
          snapshotAt(1, {
            tasks: [taskRecord({ id: 't-1', title: 'Ship the release' })],
          }),
        )
      }
      return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )
}

const overviewItem = {
  job_id: 'job-1',
  project_id: 'p-1',
  task_id: 't-1',
  state: 'running',
  updated_at_ms: 1_700_000_000_000,
  handoff_count: 1,
  review_decision: null,
  needs_attention: false,
  next_action: 'none',
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('declared agent_jobs panel', () => {
  it('renders the declared table columns in order', async () => {
    stubOverview([overviewItem])
    renderPanel(
      <AgentJobsPanel
        component={component({
          id: 'jobs',
          type: 'table',
          title: 'Jobs',
          source: 'agent_jobs',
          columns: ['State', 'Task', 'Updated'],
        })}
      />,
    )

    expect(await screen.findByRole('columnheader', { name: 'State' })).toBeInTheDocument()
    expect(screen.getByRole('columnheader', { name: 'Task' })).toBeInTheDocument()
    expect(screen.getByRole('columnheader', { name: 'Updated' })).toBeInTheDocument()
    expect(screen.queryByRole('columnheader', { name: 'Project' })).not.toBeInTheDocument()
    expect(screen.getByText('Ship the release')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Running' })).toHaveAttribute(
      'href',
      '/agents/job-1',
    )
  })

  it('refuses a declared column by name instead of substituting another', async () => {
    stubOverview([overviewItem])
    renderPanel(
      <AgentJobsPanel
        component={component({
          id: 'jobs',
          type: 'table',
          title: 'Jobs',
          source: 'agent_jobs',
          columns: ['State', 'Commit SHA'],
        })}
      />,
    )

    expect(await screen.findByText('Not available in the browser yet')).toBeInTheDocument()
    expect(screen.getByText(/Commit SHA/)).toBeInTheDocument()
  })

  it('uses the declared empty_text when no job is visible', async () => {
    stubOverview([])
    renderPanel(
      <AgentJobsPanel
        component={component({
          id: 'jobs',
          type: 'agent_jobs',
          title: 'Jobs',
          source: 'agent_jobs',
          options: { empty_text: 'No jobs have been started for this workflow.' },
        })}
      />,
    )

    expect(
      await screen.findByText('No jobs have been started for this workflow.'),
    ).toBeInTheDocument()
  })
})

describe('declared knowledge_refs panel', () => {
  it('uses the declared empty_text when the vault has no notes', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input)
        if (url.includes('/authoritative/markdown-inventory')) return jsonResponse([])
        return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
      }),
    )
    renderPanel(
      <KnowledgeRefsPanel
        component={component({
          id: 'references',
          type: 'knowledge_refs',
          title: 'References',
          source: 'knowledge',
          options: { empty_text: 'This project references no notes yet.' },
        })}
      />,
    )

    expect(await screen.findByText('This project references no notes yet.')).toBeInTheDocument()
  })
})
