import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import type { Task, WorkflowViewComponent } from '@/api/adapters'
import type { ViewRendererProps } from '@/features/workflows'
import {
  isViewComponentImplemented,
  renderViewComponent,
  viewComponentRenderers,
} from '@/features/workflows/view-registry'
import { componentTypeNames } from '@/features/workflows/view-metadata'
import { MetricView } from '@/features/workflows/views/metric-view'
import { testQueryClient } from '../api/test-utils'

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

function props(overrides: Partial<WorkflowViewComponent> & Pick<WorkflowViewComponent, 'id' | 'type'>) {
  const value: ViewRendererProps = {
    view: { id: 'view', title: 'View', description: '', layout: { type: 'vertical', columns: 1 }, components: [] },
    component: component(overrides),
    project: { id: 'p-1', name: 'Project', status: 'active' },
    tasks: [],
    milestones: [],
    workItems: [],
    workflow: undefined,
  }
  return value
}

function withQueryClient(children: ReactNode) {
  return <QueryClientProvider client={testQueryClient()}>{children}</QueryClientProvider>
}

function task(overrides: Partial<Task> = {}): Task {
  return {
    id: 't-1',
    title: 'Task',
    status: 'ready',
    projectId: 'p-1',
    milestoneId: null,
    estimatedMinutes: 30,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: '',
    priority: null,
    labels: [],
    ...overrides,
  }
}

describe('workflow view registry', () => {
  it('dispatches on the component type, never on a view id', () => {
    render(renderViewComponent(props({ id: 'notes', type: 'text', text: 'Declared prose.' })))
    expect(screen.getByText('Declared prose.')).toBeInTheDocument()

    // The same component renders the same panel under a different view id.
    render(renderViewComponent(props({ id: 'other', type: 'text', text: 'Declared prose.' })))
    expect(screen.getAllByText('Declared prose.')).toHaveLength(2)
  })

  it('surfaces declared actions on a text panel instead of dropping them', () => {
    render(
      renderViewComponent(
        props({ id: 'notes', type: 'text', text: 'Read me.', actions: ['open', 'create_task'] }),
      ),
    )
    expect(screen.getByText(/also declares the open, create_task actions/)).toBeInTheDocument()
  })

  it('refuses a type it has never heard of by name instead of rendering a blank panel', () => {
    render(renderViewComponent(props({ id: 'future', type: 'future_panel', title: 'Future panel' })))
    expect(screen.getByText('Future panel is not supported')).toBeInTheDocument()
  })

  it('treats prototype keys as unknown manifest types, never as built-in renderers', () => {
    for (const type of ['constructor', 'toString', '__proto__']) {
      expect(isViewComponentImplemented(type)).toBe(false)
    }
    const { unmount } = render(
      renderViewComponent(props({ id: 'proto', type: 'constructor', title: 'Constructor panel' })),
    )
    expect(screen.getByText('Constructor panel is not supported')).toBeInTheDocument()
    unmount()
    render(renderViewComponent(props({ id: 'proto2', type: 'toString', title: 'ToString panel' })))
    expect(screen.getByText('ToString panel is not supported')).toBeInTheDocument()
  })

  it('keeps four named refusals for the declared types this build does not render', () => {
    for (const type of ['habit_tracker', 'metric_history', 'knowledge_refs', 'agent_jobs']) {
      expect(isViewComponentImplemented(type)).toBe(false)
      const { unmount } = render(
        renderViewComponent(props({ id: `${type}-panel`, type, title: componentTypeNames[type] })),
      )
      expect(screen.getByText('Not available in the browser yet')).toBeInTheDocument()
      expect(screen.getByText(componentTypeNames[type])).toBeInTheDocument()
      unmount()
    }
  })

  it('names every registered component type for the refusal and panel chrome', () => {
    for (const type of Object.keys(viewComponentRenderers)) {
      expect(componentTypeNames[type], `${type} needs a human name`).toBeTruthy()
    }
    expect(isViewComponentImplemented('kanban')).toBe(true)
    expect(isViewComponentImplemented('text')).toBe(true)
  })
})

describe('metric declarations in the renderer', () => {
  const tasks = [task({ id: 't-1', estimatedMinutes: 30 }), task({ id: 't-2', estimatedMinutes: 60 })]

  it('renders the declared sum of estimated minutes instead of a task count', () => {
    const viewProps = {
      ...props({
        id: 'estimate',
        type: 'metric',
        title: 'Estimated work',
        source: 'tasks',
        aggregate: 'sum',
        field: 'estimated_minutes',
      }),
      tasks,
    }
    render(withQueryClient(<MetricView {...viewProps} />))
    expect(screen.getByText('Estimated work')).toBeInTheDocument()
    expect(screen.getByText('90')).toBeInTheDocument()
    expect(screen.getByText('minutes')).toBeInTheDocument()
  })

  it('refuses an aggregate it cannot compute with a named panel', () => {
    const viewProps = {
      ...props({
        id: 'estimate',
        type: 'metric',
        title: 'Median estimate',
        source: 'tasks',
        aggregate: 'median',
      }),
      tasks,
    }
    render(withQueryClient(<MetricView {...viewProps} />))
    expect(screen.getByText('Median estimate')).toBeInTheDocument()
    expect(screen.getByText('Not available in the browser yet')).toBeInTheDocument()
    expect(screen.getByText(/cannot compute/)).toBeInTheDocument()
    expect(screen.queryByText('2')).not.toBeInTheDocument()
  })
})
