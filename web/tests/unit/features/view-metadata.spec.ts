import { describe, expect, it } from 'vitest'
import type { Milestone, Task, WorkItem, WorkflowView, WorkflowViewComponent } from '@/api/adapters'
import {
  componentFamily,
  componentSpansFullWidth,
  componentTypeName,
  describeFilter,
  milestoneMatchesDeclaredFilter,
  taskMatchesDeclaredFilter,
  unsupportedTaskFilterKeys,
  viewBadge,
  viewHeaderAction,
  viewLayoutClassName,
  workItemMatchesDeclaredFilter,
} from '@/features/workflows/view-metadata'

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

function view(
  id: string,
  components: WorkflowViewComponent[],
  layout: WorkflowView['layout'] = { type: 'vertical', columns: 1 },
): WorkflowView {
  return { id, title: id, description: '', layout, components }
}

const metric = component({ id: 'm', type: 'metric', source: 'tasks' })
const table = component({ id: 't', type: 'table', source: 'tasks' })
const board = component({ id: 'b', type: 'kanban', source: 'tasks' })
const milestoneTimeline = component({ id: 'mt', type: 'timeline', source: 'milestones' })
const progress = component({ id: 'mp', type: 'milestone_progress', source: 'milestones' })
const scheduleTimeline = component({ id: 'st', type: 'timeline', source: 'schedule' })

function task(overrides: Partial<Task> = {}): Task {
  return {
    id: 'task-1',
    title: 'Write the readiness notes',
    status: 'ready',
    projectId: 'p-1',
    milestoneId: 'm-1',
    estimatedMinutes: 30,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: '',
    priority: 'high',
    labels: [],
    ...overrides,
  }
}

function milestone(overrides: Partial<Milestone> = {}): Milestone {
  return { id: 'm-1', projectId: 'p-1', name: 'Checkpoint', status: 'planned', ...overrides }
}

function workItem(overrides: Partial<WorkItem> = {}): WorkItem {
  return {
    id: 'w-1',
    projectId: 'p-1',
    kind: 'lesson',
    state: 'draft',
    properties: {},
    revision: 1,
    ...overrides,
  }
}

describe('workflow view metadata', () => {
  it('identifies a component family from its type and declared source', () => {
    expect(componentFamily(table)).toBe('tasks')
    expect(componentFamily(board)).toBe('board')
    expect(componentFamily(milestoneTimeline)).toBe('milestones')
    expect(componentFamily(progress)).toBe('milestones')
    expect(componentFamily(scheduleTimeline)).toBe('schedule')
    expect(componentFamily(component({ id: 'w', type: 'kanban', source: 'work_items' }))).toBe('other')
    expect(componentFamily(component({ id: 'k', type: 'knowledge_refs', source: 'knowledge' }))).toBe('other')
  })

  it('derives the header action from the primary surface, with a task fallback', () => {
    expect(viewHeaderAction(view('tasks', [table]))).toEqual({ kind: 'new_task', label: 'New task' })
    expect(viewHeaderAction(view('board', [board]))).toEqual({ kind: 'new_task', label: 'New task' })
    expect(viewHeaderAction(view('milestones', [milestoneTimeline]))).toEqual({
      kind: 'new_milestone',
      label: 'New milestone',
    })
    // An overview whose first panel is a metric still offers New task because a
    // task list is declared somewhere in the view.
    expect(viewHeaderAction(view('overview', [metric, table]))).toEqual({
      kind: 'new_task',
      label: 'New task',
    })
    // A view with no task or milestone surface offers nothing.
    expect(viewHeaderAction(view('agents', [component({ id: 'j', type: 'agent_jobs', source: 'agent_jobs' })]))).toBeNull()
    expect(viewHeaderAction(view('notes', [component({ id: 'x', type: 'text' })]))).toBeNull()
  })

  it('badges only the primary list surface', () => {
    expect(viewBadge(view('tasks', [table]))).toEqual({ kind: 'tasks', noun: 'tasks' })
    expect(viewBadge(view('board', [board]))).toEqual({ kind: 'tasks', noun: 'tasks' })
    expect(viewBadge(view('milestones', [milestoneTimeline, progress]))).toEqual({
      kind: 'milestones',
      noun: 'milestones',
    })
    // Mixed overviews and schedule-led timelines get no count badge.
    expect(viewBadge(view('overview', [metric, table]))).toBeNull()
    expect(viewBadge(view('timeline', [scheduleTimeline]))).toBeNull()
    expect(viewBadge(view('empty', []))).toBeNull()
  })

  it('maps the declared layout to static, responsive classes', () => {
    expect(viewLayoutClassName(view('v', [table], { type: 'vertical', columns: 1 }))).toBe('grid gap-5')
    expect(viewLayoutClassName(view('h', [table], { type: 'horizontal', columns: 2 }))).toBe(
      'grid gap-5 md:grid-cols-2',
    )
    expect(viewLayoutClassName(view('g2', [table], { type: 'grid', columns: 2 }))).toBe(
      'grid gap-5 md:grid-cols-2',
    )
    expect(viewLayoutClassName(view('g3', [table], { type: 'grid', columns: 3 }))).toContain(
      'xl:grid-cols-3',
    )
    // Declared columns are clamped to a readable maximum.
    expect(viewLayoutClassName(view('g9', [table], { type: 'grid', columns: 9 }))).toContain(
      'xl:grid-cols-4',
    )
  })

  it('gives wide work the whole row in a grid, and everything the row when vertical', () => {
    expect(componentSpansFullWidth(table, { type: 'grid', columns: 2 })).toBe(true)
    expect(componentSpansFullWidth(board, { type: 'horizontal', columns: 2 })).toBe(true)
    expect(componentSpansFullWidth(metric, { type: 'grid', columns: 2 })).toBe(false)
    expect(
      componentSpansFullWidth(component({ id: 'tl', type: 'task_list', source: 'tasks' }), {
        type: 'grid',
        columns: 2,
      }),
    ).toBe(true)
    expect(
      componentSpansFullWidth(
        component({ id: 'tl', type: 'task_list', source: 'tasks', limit: 8 }),
        { type: 'grid', columns: 2 },
      ),
    ).toBe(false)
    expect(componentSpansFullWidth(metric, { type: 'vertical', columns: 1 })).toBe(true)
  })

  it('projects workflow-authored titles onto the product vocabulary', () => {
    const legacy = ['blue', 'print'].join('')
    expect(componentTypeName(component({ id: 'x', type: 'text', title: `${legacy} notes` }))).toBe(
      'workflow notes',
    )
    const capitalised = `${legacy.charAt(0).toUpperCase()}${legacy.slice(1)} notes`
    expect(componentTypeName(component({ id: 'z', type: 'text', title: capitalised }))).toBe(
      'Workflow notes',
    )
    // Real published copy: "Authorized task and blueprint workflow transitions."
    expect(
      componentTypeName(
        component({
          id: 'w',
          type: 'text',
          title: `Authorized task and ${legacy} workflow transitions.`,
        }),
      ),
    ).toBe('Authorized task and workflow transitions.')
    expect(componentTypeName(component({ id: 'y', type: 'text', title: null }))).toBe('Text')
  })
})

describe('declared component filters', () => {
  it('matches tasks on the declared keys only', () => {
    expect(taskMatchesDeclaredFilter(task(), { status: 'ready' })).toBe(true)
    expect(taskMatchesDeclaredFilter(task(), { status: 'done' })).toBe(false)
    expect(taskMatchesDeclaredFilter(task(), { priority: 'high' })).toBe(true)
    expect(taskMatchesDeclaredFilter(task(), { priority: 'none' })).toBe(false)
    expect(taskMatchesDeclaredFilter(task({ priority: null }), { priority: 'none' })).toBe(true)
    expect(taskMatchesDeclaredFilter(task(), { milestone_id: 'm-1' })).toBe(true)
  })

  it('never claims a task is actionable without the server report', () => {
    const readiness = {
      taskId: 'task-1',
      title: 'Write the readiness notes',
      status: 'ready' as const,
      actionable: true,
      reason: 'ready',
      blockingTaskIds: [],
      prerequisiteIds: [],
      earliestStartAtMs: null,
      deadlineAtMs: null,
      estimatedMinutes: 30,
      projectId: 'p-1',
    }
    expect(taskMatchesDeclaredFilter(task(), { actionable: 'true' })).toBe(false)
    expect(taskMatchesDeclaredFilter(task(), { actionable: 'true' }, readiness)).toBe(true)
    expect(
      taskMatchesDeclaredFilter(task(), { actionable: 'false' }, { ...readiness, actionable: false }),
    ).toBe(true)
  })

  it('matches milestones and work items on their own vocabularies', () => {
    expect(milestoneMatchesDeclaredFilter(milestone(), { status: 'planned' })).toBe(true)
    expect(milestoneMatchesDeclaredFilter(milestone(), { status: 'completed' })).toBe(false)
    expect(workItemMatchesDeclaredFilter(workItem(), { kind: 'lesson', state: 'draft' })).toBe(true)
    expect(workItemMatchesDeclaredFilter(workItem(), { kind: 'assessment' })).toBe(false)
  })

  it('names filter keys it cannot evaluate instead of ignoring them', () => {
    expect(unsupportedTaskFilterKeys({ status: 'ready', priority: 'high' })).toEqual([])
    expect(unsupportedTaskFilterKeys({ status: 'ready', owner: 'me' })).toEqual(['owner'])
    expect(describeFilter({ status: 'ready', milestone_id: 'm-1' })).toBe(
      'status is ready, milestone id is m-1',
    )
    expect(describeFilter({})).toBeNull()
  })
})
