import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import type { Project, Task, WorkflowViewComponent } from '@/api/adapters'
import { ApiError } from '@/api/errors'
import { ToastProvider } from '@/components/ui/toast'

/**
 * The board consumes a workflow `board` declaration. These tests pin the three
 * things a declaration can change: its filter (including readiness), its limit
 * and the refusal for a declaration the board cannot honour.
 */

const harness = vi.hoisted(() => ({
  readiness: {
    data: undefined as unknown,
    isPending: false,
    error: null as unknown,
    isFetching: false,
    refetch: () => undefined,
  },
}))

vi.mock('@/hooks', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/hooks')>()
  return {
    ...actual,
    defaultNowMs: () => 0,
    useTaskReadiness: () => harness.readiness,
    useTaskCommand: () => ({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isPending: false,
      error: null,
      disposition: 'idle' as const,
      data: undefined,
      variables: undefined,
      retryAfterSeconds: null,
      reset: vi.fn(),
    }),
    useTaskTransitionEdges: () => ({ edges: new Map(), failed: false, isPending: false }),
  }
})

import { TaskBoard, boardDeclarationIssue } from '@/features/tasks/task-board'

const project = { id: 'p-1', name: 'Orbit', status: 'active' } as Project

function task(overrides: Partial<Task> & { id: string }): Task {
  return {
    title: overrides.id,
    status: 'ready',
    projectId: 'p-1',
    milestoneId: null,
    estimatedMinutes: null,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: '',
    priority: null,
    labels: [],
    createdAtMs: 0,
    updatedAtMs: 0,
    ...overrides,
  } as Task
}

function component(overrides: Partial<WorkflowViewComponent> = {}): WorkflowViewComponent {
  return {
    id: 'board',
    type: 'kanban',
    title: 'Board',
    source: 'tasks',
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

function renderBoard(overrides: {
  tasks?: Task[]
  component?: WorkflowViewComponent
} = {}) {
  const tasks = overrides.tasks ?? [task({ id: 't-1', title: 'First' })]
  return render(
    <MemoryRouter initialEntries={['/projects/p-1/tasks?view=board']}>
      <ToastProvider>
        <TaskBoard
          project={project}
          tasks={tasks}
          milestones={[]}
          component={overrides.component ?? component()}
        />
      </ToastProvider>
    </MemoryRouter>,
  )
}

beforeEach(() => {
  harness.readiness = {
    data: undefined,
    isPending: false,
    error: null,
    isFetching: false,
    refetch: () => undefined,
  }
})

describe('task board declaration refusals', () => {
  it('names an unsupported filter key instead of ignoring it', () => {
    const issue = boardDeclarationIssue(component({ filter: { please: 'work' } }))
    expect(issue?.reason).toContain('please')
    expect(issue?.reason).toContain('cannot evaluate')
  })

  it('refuses an actionable spelling the board cannot evaluate', () => {
    const issue = boardDeclarationIssue(component({ filter: { actionable: 'maybe' } }))
    expect(issue?.reason).toContain('only true or false')
  })

  it('only accepts status grouping', () => {
    expect(boardDeclarationIssue(component({ groupBy: 'priority' }))?.reason).toContain(
      'only group by status',
    )
    expect(boardDeclarationIssue(component({ groupBy: 'status' }))).toBeNull()
    expect(boardDeclarationIssue(component({ groupBy: null }))).toBeNull()
  })

  it('renders the refusal panel for a non-status grouping', () => {
    renderBoard({ component: component({ groupBy: 'priority' }) })
    expect(screen.getByText(/only group by status/)).toBeInTheDocument()
  })
})

describe('task board readiness and limit', () => {
  it('shows a pending skeleton instead of an empty board while readiness is read', () => {
    harness.readiness = {
      data: undefined,
      isPending: true,
      error: null,
      isFetching: true,
      refetch: () => undefined,
    }
    renderBoard({ component: component({ filter: { actionable: 'true' } }) })

    expect(screen.getByLabelText('Reading task readiness')).toBeInTheDocument()
    expect(screen.queryByText('No tasks match this board')).toBeNull()
  })

  it('shows an inline retry error instead of an empty board when readiness fails', () => {
    const refetch = vi.fn()
    harness.readiness = {
      data: undefined,
      isPending: false,
      error: new ApiError(500, 'readiness unavailable'),
      isFetching: false,
      refetch,
    }
    renderBoard({ component: component({ filter: { actionable: 'true' } }) })

    expect(screen.getByText('Task readiness could not be read')).toBeInTheDocument()
    expect(screen.queryByText('No tasks match this board')).toBeNull()
    screen.getByRole('button', { name: 'Try again' }).click()
    expect(refetch).toHaveBeenCalled()
  })

  it('reports the declared limit when matching cards are hidden', () => {
    renderBoard({
      tasks: [
        task({ id: 't-1', title: 'First' }),
        task({ id: 't-2', title: 'Second' }),
        task({ id: 't-3', title: 'Third' }),
      ],
      component: component({ limit: 2 }),
    })

    expect(screen.getByText('First')).toBeInTheDocument()
    expect(screen.getByText('Second')).toBeInTheDocument()
    expect(screen.queryByText('Third')).toBeNull()
    expect(screen.getByText(/Showing the first 2 of 3/)).toBeInTheDocument()
  })
})
