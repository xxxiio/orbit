import { describe, expect, it } from 'vitest'
import type {
  Milestone,
  Task,
  WorkItem,
  Workflow,
  WorkflowView,
  WorkflowViewComponent,
} from '@/api/adapters'
import { actionableFilterNote, actionableFilterRefusal, describeFilter, milestoneMatchesDeclaredFilter, parseActionable, taskMatchesDeclaredFilter, unsupportedTaskFilterKeys, workItemKindFilterRefusal, workItemMatchesDeclaredFilter } from '@/features/workflows/declarations/filters'
import { aggregateMetric, metricDeclaration, metricDisplayValue, numericFieldsForSource, taskMetricRows, workItemMetricRows } from '@/features/workflows/declarations/metrics'
import { columnWidthNote, columnWidthOption } from '@/features/workflows/declarations/columns'
import { taskSortDeclaration } from '@/features/workflows/declarations/sorts'
import { componentFamily, componentSpansFullWidth, componentTypeName, viewBadge, viewBadgeCount, viewHeaderAction, viewLayoutClassName } from '@/features/workflows/declarations/views'
import { declaredWorkspaceViews, isReservedViewId, reservedDeclaredViews, workspaceViewTabs } from '@/features/workflows/declarations/reserved'
import { taskSortDescriptor } from '@/features/workflows/declarations'
import { filterSortLimit } from '@/components/surface'

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
    expect(componentFamily(component({ id: 'w', type: 'kanban', source: 'work_items' }))).toBe('work_items')
    expect(componentFamily(component({ id: 'wt', type: 'table', source: 'work_items' }))).toBe('work_items')
    expect(componentFamily(component({ id: 'wl', type: 'list', source: 'work_items' }))).toBe('work_items')
    expect(componentFamily(component({ id: 'wd', type: 'detail', source: 'work_items' }))).toBe('work_items')
    expect(componentFamily(component({ id: 'k', type: 'knowledge_refs', source: 'knowledge' }))).toBe('other')
  })

  it('derives the header action from the primary surface, with a task fallback', () => {
    expect(viewHeaderAction(view('tasks', [table]))).toEqual({ kind: 'new_task', label: 'New task' })
    expect(viewHeaderAction(view('board', [board]))).toEqual({ kind: 'new_task', label: 'New task' })
    expect(viewHeaderAction(view('milestones', [milestoneTimeline]))).toEqual({
      kind: 'new_milestone',
      label: 'New milestone',
    })
    // Work items are a family of their own: I3's create action has a name now.
    expect(
      viewHeaderAction(view('items', [component({ id: 'wi', type: 'kanban', source: 'work_items' })])),
    ).toEqual({ kind: 'new_work_item', label: 'New work item' })
    // A view whose first panel is a declared work-item list still gets a create.
    expect(
      viewHeaderAction(view('items', [component({ id: 'wi', type: 'list', source: 'work_items' })])),
    ).toEqual({ kind: 'new_work_item', label: 'New work item' })
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
    // A work-item view badges its own read: the work-item family is not the
    // milestone family, and an unread list never produces a number.
    expect(
      viewBadge(view('items', [component({ id: 'wi', type: 'kanban', source: 'work_items' })])),
    ).toEqual({ kind: 'work_items', noun: 'work items' })
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

  it('names a clamped board column width instead of applying it silently', () => {
    const wide = component({ id: 'b', type: 'kanban', options: { column_width: 900 } })
    expect(columnWidthOption(wide).value).toBe(420)
    expect(columnWidthOption(wide).clamped).toBe(true)
    expect(columnWidthNote(wide)).toMatch(/clamped to 420 pixels/)
    const narrow = component({ id: 'b2', type: 'kanban', options: { column_width: 100 } })
    expect(columnWidthOption(narrow).value).toBe(180)
    expect(columnWidthNote(narrow)).toMatch(/clamped to 180 pixels/)
    const exact = component({ id: 'b3', type: 'kanban', options: { column_width: 300 } })
    expect(columnWidthOption(exact).clamped).toBe(false)
    expect(columnWidthNote(exact)).toBeNull()
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

  it('refuses an undeclared work-item kind consistently across surfaces', () => {
    expect(workItemKindFilterRefusal({}, workflowWithKinds)).toBeNull()
    expect(workItemKindFilterRefusal({ kind: 'lesson' }, workflowWithKinds)).toBeNull()
    expect(workItemKindFilterRefusal({ kind: 'LESSON' }, workflowWithKinds)).toBeNull()
    const refusal = workItemKindFilterRefusal({ kind: 'missing' }, workflowWithKinds)
    expect(refusal).toMatch(/“missing”/)
    expect(refusal).toMatch(/does not declare/)
    // Without a manifest every named kind is undeclared, never a silent match.
    expect(workItemKindFilterRefusal({ kind: 'lesson' }, undefined)).toMatch(/does not declare/)
  })

  it('names filter keys it cannot evaluate instead of ignoring them', () => {
    expect(unsupportedTaskFilterKeys({ status: 'ready', priority: 'high' })).toEqual([])
    expect(unsupportedTaskFilterKeys({ status: 'ready', owner: 'me' })).toEqual(['owner'])
    expect(describeFilter({ status: 'ready', milestone_id: 'm-1' })).toBe(
      'status is ready, milestone id is m-1',
    )
    expect(describeFilter({})).toBeNull()
  })

  it('refuses an actionable value that is not a boolean', () => {
    expect(actionableFilterRefusal({})).toBeNull()
    expect(actionableFilterRefusal({ status: 'ready' })).toBeNull()
    expect(actionableFilterRefusal({ actionable: 'true' })).toBeNull()
    expect(actionableFilterRefusal({ actionable: ' FALSE ' })).toBeNull()
    expect(actionableFilterRefusal({ actionable: 'yes' })).toMatch(/only true or false/)
    // The old behaviour read anything but "true" as false; the refusal names it.
    expect(actionableFilterRefusal({ actionable: 'actionable' })).toBe(
      'This panel filters tasks by actionable = “actionable”; only true or false are supported.',
    )

    // The board's note follows the value it was given.
    expect(actionableFilterNote({})).toBeNull()
    expect(actionableFilterNote({ actionable: 'true' })).toMatch(/^Filtered to tasks the server marks actionable/)
    expect(actionableFilterNote({ actionable: 'FALSE' })).toMatch(
      /^Filtered to tasks the server does not mark actionable/,
    )
    expect(actionableFilterNote({ actionable: 'maybe' })).toBeNull()
  })

  it('reads a padded actionable value the same way in the guard, the note and the filter', () => {
    // The manifest validator only bounds the string, so " true " is valid input.
    expect(parseActionable(' true ')).toBe(true)
    expect(parseActionable(' FALSE ')).toBe(false)
    expect(parseActionable('yes')).toBeNull()
    expect(parseActionable(undefined)).toBeNull()

    const filter = { actionable: ' true ' }
    expect(actionableFilterRefusal(filter)).toBeNull()
    expect(actionableFilterNote(filter)).toMatch(/^Filtered to tasks the server marks actionable/)

    const readiness = (actionable: boolean) => ({
      taskId: 't-1',
      title: 'Task',
      status: 'ready' as const,
      actionable,
      reason: actionable ? 'ready' : 'unfinished_prerequisites',
      blockingTaskIds: [],
      prerequisiteIds: [],
      earliestStartAtMs: null,
      deadlineAtMs: null,
      estimatedMinutes: null,
      projectId: null,
    })
    expect(taskMatchesDeclaredFilter(task({ status: 'ready' }), filter, readiness(true))).toBe(true)
    expect(taskMatchesDeclaredFilter(task({ status: 'ready' }), filter, readiness(false))).toBe(false)
    expect(
      taskMatchesDeclaredFilter(task({ status: 'ready' }), { actionable: ' false ' }, readiness(false)),
    ).toBe(true)
  })
})

const workflowWithKinds: Workflow = {
  id: 'wf',
  version: 1,
  name: 'Workflow',
  description: '',
  kinds: [
    {
      id: 'lesson',
      name: 'Lesson',
      initialState: 'draft',
      states: ['draft'],
      fields: [
        { id: 'minutes', kind: { type: 'integer' }, required: false, default: 0 },
        { id: 'summary', kind: { type: 'text' }, required: false, default: '' },
      ],
      transitions: [],
    },
    {
      id: 'assessment',
      name: 'Assessment',
      initialState: 'draft',
      states: ['draft'],
      fields: [{ id: 'score', kind: { type: 'integer' }, required: false, default: 0 }],
      transitions: [],
    },
  ],
  requiredCapabilities: [],
  templates: [],
  views: [],
}

describe('declared metric aggregates', () => {
  it('honours count, sum, average, min and max over estimated minutes', () => {
    const rows = [30, null, 60, 120]
    const declaration = (aggregate: string) =>
      metricDeclaration(
        component({ id: 'm', type: 'metric', source: 'tasks', aggregate, field: 'estimated_minutes' }),
        undefined,
      )

    // A count of a declared field counts the non-null values; without a field
    // it counts rows.
    expect(aggregateMetric(declaration('count'), rows)).toBe(3)
    expect(
      aggregateMetric(
        metricDeclaration(component({ id: 'm', type: 'metric', source: 'tasks' }), undefined),
        rows,
      ),
    ).toBe(4)
    expect(aggregateMetric(declaration('sum'), rows)).toBe(210)
    expect(aggregateMetric(declaration('average'), rows)).toBe(70)
    expect(aggregateMetric(declaration('min'), rows)).toBe(30)
    expect(aggregateMetric(declaration('max'), rows)).toBe(120)
    // `estimated_minutes` is the only numeric field tasks carry.
    expect(taskMetricRows([task(), task({ estimatedMinutes: null })], 'estimated_minutes')).toEqual([30, null])
    expect(numericFieldsForSource('tasks', undefined)).toEqual(new Set(['estimated_minutes']))
  })

  it('refuses an aggregate or field it cannot honour instead of counting under it', () => {
    const unsupportedAggregate = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'tasks', aggregate: 'median' }),
      workflowWithKinds,
    )
    expect(unsupportedAggregate.refusal).toMatch(/cannot compute/)

    const unsupportedField = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'tasks', aggregate: 'sum', field: 'title' }),
      workflowWithKinds,
    )
    expect(unsupportedField.refusal).toMatch(/not a numeric field/)

    const missingField = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'tasks', aggregate: 'sum' }),
      workflowWithKinds,
    )
    expect(missingField.refusal).toMatch(/needs the numeric field/)

    const milestones = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'milestones', aggregate: 'sum', field: 'x' }),
      workflowWithKinds,
    )
    expect(milestones.refusal).toMatch(/has no numeric field/)
  })

  it('aggregates integer work-item properties and refuses non-integer ones', () => {
    expect(numericFieldsForSource('work_items', workflowWithKinds)).toEqual(new Set(['minutes', 'score']))
    expect(numericFieldsForSource('work_items', workflowWithKinds, { kind: 'lesson' })).toEqual(
      new Set(['minutes']),
    )
    const sum = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'work_items', aggregate: 'sum', field: 'minutes' }),
      workflowWithKinds,
    )
    expect(sum.refusal).toBeNull()
    expect(aggregateMetric(sum, workItemMetricRows([
      workItem({ properties: { minutes: 45 } }),
      workItem({ properties: { minutes: '15' } }),
    ], 'minutes'))).toBe(60)

    const text = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'work_items', aggregate: 'sum', field: 'summary' }),
      workflowWithKinds,
    )
    expect(text.refusal).toMatch(/not a numeric field/)
  })

  it('honours a declared unit and refuses an unlabelled non-minutes field', () => {
    // `score` is numeric but its unit is unknown: showing it as minutes would
    // be a mislabel, so the declaration is refused until it names a unit.
    const undeclared = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'work_items', aggregate: 'sum', field: 'score' }),
      workflowWithKinds,
    )
    expect(undeclared.refusal).toMatch(/no declared unit/)
    expect(undeclared.unit).toBeNull()

    const declared = metricDeclaration(
      component({
        id: 'm',
        type: 'metric',
        source: 'work_items',
        aggregate: 'sum',
        field: 'score',
        options: { unit: ' points ' },
      }),
      workflowWithKinds,
    )
    expect(declared.refusal).toBeNull()
    expect(declared.unit).toBe('points')

    // Fields whose name states the unit keep it without an option.
    const minutes = metricDeclaration(
      component({
        id: 'm',
        type: 'metric',
        source: 'tasks',
        aggregate: 'sum',
        field: 'estimated_minutes',
      }),
      workflowWithKinds,
    )
    expect(minutes.refusal).toBeNull()
    expect(minutes.unit).toBe('minutes')

    // A count is a number of rows, not a unit.
    const count = metricDeclaration(
      component({ id: 'm', type: 'metric', source: 'tasks' }),
      workflowWithKinds,
    )
    expect(count.unit).toBeNull()
  })

  it('converts a known unit or refuses a mismatch instead of relabelling the number', () => {
    const hours = metricDeclaration(
      component({
        id: 'm',
        type: 'metric',
        source: 'tasks',
        aggregate: 'sum',
        field: 'estimated_minutes',
        options: { unit: 'hours' },
      }),
      workflowWithKinds,
    )
    expect(hours.refusal).toBeNull()
    expect(hours.unit).toBe('hours')
    expect(hours.factor).toBeCloseTo(1 / 60)
    expect(metricDisplayValue(hours, 120)).toBe(2)

    const seconds = metricDeclaration(
      component({
        id: 'm',
        type: 'metric',
        source: 'tasks',
        aggregate: 'sum',
        field: 'estimated_minutes',
        options: { unit: 'seconds' },
      }),
      workflowWithKinds,
    )
    expect(seconds.unit).toBe('seconds')
    expect(metricDisplayValue(seconds, 2)).toBe(120)

    const unknown = metricDeclaration(
      component({
        id: 'm',
        type: 'metric',
        source: 'tasks',
        aggregate: 'sum',
        field: 'estimated_minutes',
        options: { unit: 'furlongs' },
      }),
      workflowWithKinds,
    )
    expect(unknown.refusal).toMatch(/furlongs/)
    expect(unknown.refusal).toMatch(/minutes, hours or seconds/)
  })
})

describe('declared task sorts', () => {
  it('maps supported fields and descending onto the shared comparators', () => {
    expect(taskSortDeclaration(component({ id: 'l', type: 'list', source: 'tasks', sort: 'title' }))).toEqual({
      sort: { key: 'title', direction: 'asc' },
      refusal: null,
    })
    expect(
      taskSortDeclaration(
        component({ id: 'l', type: 'list', source: 'tasks', sort: 'deadline', descending: true }),
      ),
    ).toEqual({ sort: { key: 'due', direction: 'desc' }, refusal: null })
  })

  /**
   * The live path: `list-view.tsx` and `board-view.tsx` both hand
   * `taskSortDescriptor(component)` to `filterSortLimit`. The declaration's
   * direction must reach the rows exactly once, so the copy ("descending") and
   * the order the person sees agree.
   */
  it('applies a declared direction exactly once through the surface pipeline', () => {
    const tasks = [
      task({ id: 'a', title: 'Alpha' }),
      task({ id: 'b', title: 'Beta' }),
      task({ id: 'c', title: 'Gamma' }),
    ]

    const ascending = component({ id: 'l', type: 'list', source: 'tasks', sort: 'title' })
    const descending = component({
      id: 'l',
      type: 'list',
      source: 'tasks',
      sort: 'title',
      descending: true,
    })

    expect(
      filterSortLimit(tasks, { sort: taskSortDescriptor(ascending) }).limited.map((entry) => entry.title),
    ).toEqual(['Alpha', 'Beta', 'Gamma'])
    expect(
      filterSortLimit(tasks, { sort: taskSortDescriptor(descending) }).limited.map((entry) => entry.title),
    ).toEqual(['Gamma', 'Beta', 'Alpha'])

    // The footer copy comes from the same declaration, so it cannot disagree.
    expect(taskSortDeclaration(descending).sort?.direction).toBe('desc')
    expect(taskSortDeclaration(ascending).sort?.direction).toBe('asc')
  })

  it('still honours a limit after sorting in the declared direction', () => {
    const tasks = [
      task({ id: 'a', title: 'Alpha' }),
      task({ id: 'b', title: 'Beta' }),
      task({ id: 'c', title: 'Gamma' }),
    ]
    const { limited } = filterSortLimit(tasks, {
      sort: taskSortDescriptor(
        component({ id: 'l', type: 'list', source: 'tasks', sort: 'title', descending: true, limit: 2 }),
      ),
      limit: 2,
    })
    expect(limited.map((entry) => entry.title)).toEqual(['Gamma', 'Beta'])
  })

  it('refuses a field the adapter does not carry, notably `updated`', () => {
    for (const field of ['updated', 'constructor', 'toString']) {
      const declaration = taskSortDeclaration(
        component({ id: 'l', type: 'list', source: 'tasks', sort: field }),
      )
      expect(declaration.sort, field).toBeNull()
      expect(declaration.refusal, field).toMatch(/does not carry/)
    }
  })
})

describe('reserved view ids and tab counts', () => {
  it('distinguishes no workflow from a workflow that declares no views', () => {
    const fallback = [view('overview', [table])]
    expect(declaredWorkspaceViews(undefined, fallback)).toEqual(fallback)
    const empty = { ...workflowWithKinds, views: [] }
    expect(declaredWorkspaceViews(empty, fallback)).toEqual([])
  })

  it('reserves the tool routes so a declared view cannot shadow them', () => {
    const declared = [
      view('planning', [table]),
      view('tasks', [table]),
      view('settings', [table]),
    ]
    expect(workspaceViewTabs(declared).map((entry) => entry.id)).toEqual(['tasks'])
    expect(reservedDeclaredViews(declared).map((entry) => entry.id)).toEqual([
      'planning',
      'settings',
    ])

    expect(isReservedViewId('planning')).toBe(true)
    expect(isReservedViewId('settings')).toBe(true)
    expect(isReservedViewId('overview')).toBe(false)
  })

  it('counts the same filtered and limited set the primary panel renders', () => {
    const badgeView = view('tasks', [
      component({
        id: 'list',
        type: 'task_list',
        source: 'tasks',
        filter: { status: 'ready' },
        limit: 2,
      }),
    ])
    const tasks = [
      task({ id: 'a', status: 'ready' }),
      task({ id: 'b', status: 'ready' }),
      task({ id: 'c', status: 'ready' }),
      task({ id: 'd', status: 'done' }),
    ]
    expect(viewBadgeCount(badgeView, { tasks, milestones: [] })).toBe(2)

    const milestoneView = view('goals', [
      component({
        id: 'progress',
        type: 'milestone_progress',
        source: 'milestones',
        filter: { status: 'planned' },
      }),
    ])
    expect(
      viewBadgeCount(milestoneView, {
        tasks,
        milestones: [
          milestone({ id: 'm-1', status: 'planned' }),
          milestone({ id: 'm-2', status: 'completed' }),
        ],
      }),
    ).toBe(1)

    // Work items count their own family: a board counts the resolved kind, and
    // a list counts every kind its filter selects. An unread list is null.
    const itemBoard = view('items', [
      component({ id: 'wb', type: 'kanban', source: 'work_items', limit: 2 }),
    ])
    const items = [
      workItem({ id: 'w-1' }),
      workItem({ id: 'w-2' }),
      workItem({ id: 'w-3' }),
      workItem({ id: 'w-4', kind: 'assessment' }),
    ]
    expect(
      viewBadgeCount(itemBoard, {
        tasks: [],
        milestones: [],
        workItems: items,
        workflow: workflowWithKinds,
      }),
    ).toBe(2)
    expect(viewBadgeCount(itemBoard, { tasks: [], milestones: [] })).toBeNull()

    const itemList = view('items', [
      component({ id: 'wl', type: 'list', source: 'work_items' }),
    ])
    expect(
      viewBadgeCount(itemList, {
        tasks: [],
        milestones: [],
        workItems: items,
        workflow: workflowWithKinds,
      }),
    ).toBe(4)
  })
})

describe('prototype keys are data, not built-ins', () => {
  it('never resolves a manifest type through Object.prototype', () => {
    expect(componentTypeName(component({ id: 'x', type: 'constructor' }))).toBe('View panel')
    expect(componentTypeName(component({ id: 'x', type: 'toString' }))).toBe('View panel')
  })
})
