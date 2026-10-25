import { describe, expect, it } from 'vitest'
import type { ReactNode } from 'react'
import { render, screen, within } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import type { WorkItem, Workflow, WorkflowViewComponent } from '@/api/adapters'
import {
  WorkItemDetailView,
  WorkItemListView,
  WorkItemTableView,
} from '@/features/workflows/views/work-item-views'
import type { ViewRendererProps } from '@/features/workflows/view-types'
import { testQueryClient } from '../api/test-utils'

function component(
  overrides: Partial<WorkflowViewComponent> & Pick<WorkflowViewComponent, 'id' | 'type'>,
): WorkflowViewComponent {
  return {
    title: null,
    source: 'work_items',
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

const workflow: Workflow = {
  id: 'research',
  version: 1,
  name: 'Research',
  description: '',
  requiredCapabilities: [],
  templates: [],
  kinds: [
    {
      id: 'hypothesis',
      name: 'Hypothesis',
      initialState: 'proposed',
      states: ['proposed', 'investigating'],
      fields: [
        { id: 'title', kind: { type: 'text' }, required: true, default: null },
        { id: 'priority', kind: { type: 'choice', options: ['low', 'high'] }, required: false, default: null },
        { id: 'note', kind: { type: 'text' }, required: false, default: null },
      ],
      transitions: [],
    },
    {
      id: 'experiment',
      name: 'Experiment',
      initialState: 'designed',
      states: ['designed'],
      // The same id as the hypothesis kind, deliberately declared as free text:
      // a row must be formatted by its own kind, not the merged column's first.
      fields: [
        { id: 'title', kind: { type: 'text' }, required: true, default: null },
        { id: 'priority', kind: { type: 'text' }, required: false, default: null },
      ],
      transitions: [],
    },
  ],
  views: [],
}

function item(overrides: Partial<WorkItem> = {}): WorkItem {
  return {
    id: 'wi-1',
    projectId: 'p-1',
    kind: 'hypothesis',
    state: 'proposed',
    properties: { title: 'Fence stale revisions', priority: 'high', note: 'A note' },
    revision: 4,
    ...overrides,
  }
}

function props(overrides: Partial<ViewRendererProps> = {}): ViewRendererProps {
  return {
    view: {
      id: 'view',
      title: 'View',
      description: '',
      layout: { type: 'vertical', columns: 1 },
      components: [],
    },
    component: component({ id: 'panel', type: 'list' }),
    project: { id: 'p-1', name: 'Project', status: 'active' },
    tasks: [],
    milestones: [],
    workItems: [],
    workflow,
    ...overrides,
  }
}

function renderView(children: ReactNode) {
  render(
    <QueryClientProvider client={testQueryClient()}>
      <MemoryRouter initialEntries={['/']}>{children}</MemoryRouter>
    </QueryClientProvider>,
  )
}

describe('work-item list declarations', () => {
  it('shows a declared revision and names columns the compact budget omits', () => {
    renderView(
      <WorkItemListView
        {...props({
          component: component({
            id: 'list',
            type: 'list',
            columns: ['title', 'kind', 'state', 'revision', 'priority', 'note'],
            options: { show_status: false },
          }),
          workItems: [item()],
        })}
      />,
    )

    expect(screen.getByText('Revision')).toBeInTheDocument()
    expect(screen.getByText('4')).toBeInTheDocument()
    // show_status=false is an explicit declaration, so state is not rendered…
    expect(screen.queryByText('Proposed')).toBeNull()
    // …but a column pushed past the four-column budget is named, not dropped.
    expect(screen.getByText(/“Note” is not shown here/)).toBeInTheDocument()
  })

  it('refuses a filter kind the pinned workflow does not declare', () => {
    renderView(
      <WorkItemListView
        {...props({
          component: component({ id: 'list', type: 'list', filter: { kind: 'missing' } }),
          workItems: [item()],
        })}
      />,
    )
    expect(screen.getByText(/Not available in the browser yet/)).toBeInTheDocument()
    expect(screen.getByText(/“missing”/)).toBeInTheDocument()
  })
})

describe('work-item table declarations', () => {
  it('formats a merged field by the row kind that declares it', () => {
    renderView(
      <WorkItemTableView
        {...props({
          component: component({ id: 'table', type: 'table', columns: ['title', 'priority'] }),
          workItems: [
            item({ id: 'wi-h', kind: 'hypothesis', properties: { title: 'Choice row', priority: 'high' } }),
            item({ id: 'wi-e', kind: 'experiment', properties: { title: 'Text row', priority: 'high' } }),
          ],
        })}
      />,
    )

    // The hypothesis kind declares a choice (humanized); the experiment kind
    // declares text (verbatim), so the same stored value renders differently.
    expect(screen.getByText('High')).toBeInTheDocument()
    expect(screen.getByText('high')).toBeInTheDocument()
  })
})

describe('work-item detail declarations', () => {
  it('applies a declared sort and limit and names the omitted action', () => {
    renderView(
      <WorkItemDetailView
        {...props({
          component: component({
            id: 'detail',
            type: 'detail',
            sort: 'priority',
            limit: 1,
            actions: ['create_work_item'],
          }),
          workItems: [
            item({ id: 'wi-low', properties: { title: 'Low item', priority: 'low' } }),
            item({ id: 'wi-high', properties: { title: 'High item', priority: 'high' } }),
          ],
        })}
      />,
    )

    const chooser = screen.getByRole('list')
    const rows = within(chooser).getAllByRole('listitem')
    expect(rows).toHaveLength(1)
    expect(within(rows[0]).getByText('High item')).toBeInTheDocument()
    expect(screen.getByText(/Showing the first 1 of 2\. The workflow sets this limit\./)).toBeInTheDocument()
    expect(screen.getByText(/also declares the Create Work Item action/)).toBeInTheDocument()
  })

  it('explains the fallback chooser cap when no limit is declared', () => {
    const many = Array.from({ length: 14 }, (_, index) =>
      item({ id: `wi-${index}`, properties: { title: `Item ${index}`, priority: 'high' } }),
    )
    renderView(
      <WorkItemDetailView
        {...props({ component: component({ id: 'detail', type: 'detail' }), workItems: many })}
      />,
    )
    expect(screen.getByText(/This panel declares no limit, so the chooser stops at 12/)).toBeInTheDocument()
  })

  it('refuses a declared sort it cannot apply', () => {
    renderView(
      <WorkItemDetailView
        {...props({
          component: component({ id: 'detail', type: 'detail', sort: 'not_a_field' }),
          workItems: [item()],
        })}
      />,
    )
    expect(screen.getByText(/not a property of its kinds/)).toBeInTheDocument()
  })
})
