import { describe, expect, it } from 'vitest'
import type { WorkItem, Workflow, WorkflowViewComponent } from '@/api/adapters'
import {
  actionsForState,
  fieldValueText,
  guardSentence,
  pinnedKindForView,
  resolveBoardKind,
  stateLabel,
  workItemColumnPlan,
  workItemSortDeclaration,
  workItemTitle,
} from '@/features/workflows/work-item-model'
import type { SortDeclaration } from '@/lib/declarations'
import { filterSortLimit } from '@/components/surface'

const workflow: Workflow = {
  id: 'guarded',
  version: 1,
  name: 'Guarded release',
  description: '',
  requiredCapabilities: [],
  templates: [],
  kinds: [
    {
      id: 'ticket',
      name: 'Ticket',
      initialState: 'open',
      states: ['open', 'approved', 'rejected'],
      fields: [
        {
          id: 'priority',
          kind: { type: 'choice', options: ['low', 'high'] },
          required: true,
          default: 'low',
        },
        { id: 'note', kind: { type: 'text' }, required: false, default: null },
        { id: 'minutes', kind: { type: 'integer' }, required: false, default: null },
        { id: 'urgent', kind: { type: 'boolean' }, required: false, default: false },
      ],
      transitions: [
        {
          from: 'open',
          to: 'approved',
          requiredCapability: 'orbit.release.approve',
          guards: [{ type: 'property_equals', field: 'priority', value: 'high' }],
          actions: [
            {
              id: 'append_note',
              capability: 'orbit.release.note',
              description: 'Append a durable note',
              parameters: { note: '' },
            },
          ],
        },
        {
          from: 'open',
          to: 'rejected',
          requiredCapability: null,
          guards: [
            { type: 'property_present', field: 'note' },
            { type: 'all_prerequisites_in_state', state: 'approved' },
          ],
          actions: [],
        },
      ],
    },
    {
      id: 'task',
      name: 'Task',
      initialState: 'todo',
      states: ['todo', 'done'],
      fields: [{ id: 'title', kind: { type: 'text' }, required: true, default: null }],
      transitions: [],
    },
  ],
  views: [],
}

function component(overrides: Partial<WorkflowViewComponent> = {}): WorkflowViewComponent {
  return {
    id: 'board',
    type: 'kanban',
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

function item(overrides: Partial<WorkItem> = {}): WorkItem {
  return {
    id: 'wi-1',
    projectId: 'p-1',
    kind: 'ticket',
    state: 'open',
    properties: { priority: 'high', note: 'Ready', minutes: 30, urgent: true },
    revision: 2,
    ...overrides,
  }
}

describe('work-item titles and values', () => {
  it('uses the first text field, and never promotes a classification such as priority', () => {
    expect(workItemTitle(item(), workflow.kinds[0])).toBe('Ready')
    // `priority` is a classification, not a name: a card must not read “High”.
    expect(
      workItemTitle(item({ properties: { priority: 'high' } }), workflow.kinds[0]),
    ).toBe('Ticket')
    expect(workItemTitle(item({ properties: {} }), workflow.kinds[0])).toBe('Ticket')
  })

  it('prefers a title-like text field over an earlier free-text field', () => {
    const kind = {
      ...workflow.kinds[0],
      fields: [
        { id: 'note', kind: { type: 'text' as const }, required: false, default: null },
        { id: 'title', kind: { type: 'text' as const }, required: true, default: null },
      ],
    }
    expect(
      workItemTitle(
        item({ properties: { note: 'Internal note', title: 'Readable name' } }),
        kind,
      ),
    ).toBe('Readable name')
  })

  it('falls back to the kind name when the only declared field is priority', () => {
    const kind = {
      ...workflow.kinds[0],
      fields: [
        {
          id: 'priority',
          kind: { type: 'choice' as const, options: ['low', 'high'] },
          required: true,
          default: 'low',
        },
      ],
    }
    expect(workItemTitle(item({ properties: { priority: 'medium' } }), kind)).toBe('Ticket')
  })

  it('humanizes state and choice values but leaves free text alone', () => {
    expect(stateLabel('in_progress')).toBe('In Progress')
    expect(fieldValueText(workflow.kinds[0].fields[0], 'high')).toBe('High')
    expect(fieldValueText(workflow.kinds[0].fields[2], 30)).toBe('30')
    expect(fieldValueText(workflow.kinds[0].fields[3], false)).toBe('No')
    expect(fieldValueText(workflow.kinds[0].fields[1], 'snake_case text')).toBe('snake_case text')
  })
})

describe('work-item declarations', () => {
  it('turns each declared guard into a sentence', () => {
    const kind = workflow.kinds[0]
    expect(guardSentence(kind.transitions[0].guards[0], kind)).toBe(
      'Requires Priority to be High.',
    )
    expect(guardSentence(kind.transitions[1].guards[0], kind)).toBe('Requires Note to be set.')
    expect(guardSentence(kind.transitions[1].guards[1], kind)).toBe(
      'All prerequisites must be in Approved.',
    )
  })

  it('lists the actions declared on transitions out of the current state only', () => {
    const offers = actionsForState(workflow.kinds[0], 'open')
    expect(offers).toHaveLength(1)
    expect(offers[0]?.action.id).toBe('append_note')
    expect(offers[0]?.transition.to).toBe('approved')
    expect(actionsForState(workflow.kinds[0], 'approved')).toHaveLength(0)
  })

  it('resolves a pinned board kind and names an unknown one', () => {
    expect(resolveBoardKind(component({ filter: { kind: 'ticket' } }), workflow)).toMatchObject({
      kind: { id: 'ticket' },
      needsChoice: false,
      refusal: null,
    })
    expect(
      resolveBoardKind(component({ filter: { kind: 'missing' } }), workflow).refusal,
    ).toContain('missing')
    expect(resolveBoardKind(component(), workflow)).toMatchObject({
      kind: { id: 'ticket' },
      needsChoice: true,
    })
  })

  it('reads the pinned kind from the first work-item component', () => {
    expect(
      pinnedKindForView({
        id: 'v',
        title: 'View',
        description: '',
        layout: { type: 'vertical', columns: 1 },
        components: [
          component({ filter: {} }),
          component({ id: 'other', filter: { kind: 'ticket' } }),
        ],
      }),
    ).toBe('ticket')
  })
})

describe('work-item columns and order', () => {
  it('maps declared columns onto virtual columns and kind fields', () => {
    const plan = workItemColumnPlan(
      component({ columns: ['title', 'kind', 'state', 'priority', 'note'] }),
      workflow,
    )
    expect(plan.columns.map((column) => column.label)).toEqual([
      'Work item',
      'Kind',
      'State',
      'Priority',
      'Note',
    ])
    expect(plan.unsupported).toEqual([])
  })

  it('always keeps the title column and reports unknown declared columns', () => {
    const plan = workItemColumnPlan(component({ columns: ['priority', 'mystery'] }), workflow)
    expect(plan.columns[0]?.key).toBe('title')
    expect(plan.unsupported).toEqual(['mystery'])
    expect(plan.columns.some((column) => column.field?.id === 'priority')).toBe(true)
  })

  it('sorts by a declared field, state or the title each row displays', () => {
    // The same pipeline the work-item list, table, board and chooser run:
    // `workItemSortDeclaration` supplies an ascending comparator and the
    // declaration supplies the direction, which `filterSortLimit` applies once.
    const sortedIds = (sort: string, descending: boolean, rows: WorkItem[]) => {
      const declaration = workItemSortDeclaration(component({ sort }), workflow)
      const sortDeclaration: SortDeclaration<WorkItem> | null = declaration.compare
        ? {
            key: sort,
            direction: descending ? 'desc' : 'asc',
            compare: declaration.compare,
            refusal: null,
          }
        : null
      return filterSortLimit(rows, { sort: sortDeclaration }).limited.map((entry) => entry.id)
    }

    const low = item({ id: 'low', properties: { priority: 'low' } })
    const high = item({ id: 'high', properties: { priority: 'high' } })
    // Textual comparison: "high" sorts before "low" ascending.
    expect(sortedIds('priority', false, [low, high])).toEqual(['high', 'low'])
    expect(sortedIds('priority', true, [high, low])).toEqual(['low', 'high'])

    const a = item({ id: 'a', kind: 'task', properties: { title: 'Alpha' } })
    const b = item({ id: 'b', kind: 'task', properties: { title: 'Beta' } })
    expect(sortedIds('title', false, [b, a])).toEqual(['a', 'b'])
    expect(sortedIds('title', true, [a, b])).toEqual(['b', 'a'])
  })

  it('refuses a declared sort field that no kind defines', () => {
    const declaration = workItemSortDeclaration(component({ sort: 'not_a_field' }), workflow)
    expect(declaration.compare).toBeNull()
    expect(declaration.refusal).toContain('not_a_field')
  })
})
