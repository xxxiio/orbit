import { describe, expect, it } from 'vitest'
import type { Task, WorkflowViewComponent } from '@/api/adapters'
import { taskMatchesDeclaredFilter } from '@/features/workflows/declarations'
import { taskListDeclaration } from '@/features/tasks/tasks-workspace'

function component(filter: Record<string, string>): WorkflowViewComponent {
  return {
    id: 'task_table',
    type: 'table',
    title: 'Tasks',
    source: 'tasks',
    aggregate: null,
    field: null,
    groupBy: null,
    filter,
    sort: null,
    descending: false,
    limit: null,
    columns: [],
    text: null,
    actions: [],
    options: {},
  }
}

function task(overrides: Partial<Task> = {}): Task {
  return {
    id: 't-1',
    title: 'Task',
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

describe('tasks list declaration', () => {
  it('applies a supported filter with the shared task matcher', () => {
    const declaration = taskListDeclaration(component({ status: 'ready', priority: 'high' }))
    expect(declaration.refusal).toBeNull()
    expect(declaration.filter).toEqual({ status: 'ready', priority: 'high' })
    expect(taskMatchesDeclaredFilter(task({ status: 'ready', priority: 'high' }), declaration.filter)).toBe(
      true,
    )
    expect(taskMatchesDeclaredFilter(task({ status: 'planned', priority: 'high' }), declaration.filter)).toBe(
      false,
    )
  })

  it('refuses a filter key the list cannot evaluate', () => {
    const declaration = taskListDeclaration(component({ owner: 'ana' }))
    expect(declaration.refusal?.reason).toContain('owner')
    expect(declaration.refusal?.reason).toContain('cannot evaluate')
  })

  it('refuses actionable instead of silently ignoring it', () => {
    const declaration = taskListDeclaration(component({ actionable: 'true' }))
    expect(declaration.refusal?.reason).toContain('actionable')
    expect(declaration.refusal?.hint).toContain('refused')
  })

  it('has no declaration when the workflow consumes no tasks view', () => {
    expect(taskListDeclaration(undefined)).toEqual({ filter: {}, refusal: null })
  })
})
