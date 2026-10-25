import { describe, expect, it } from 'vitest'
import type { WorkflowViewComponent } from '@/api/adapters'
import {
  DEFAULT_PROJECT_COLUMNS,
  columnsFromComponent,
  readColumns,
  readTaskListUrl,
  windowTasks,
  writeTaskListUrl,
  type TaskColumnKey,
} from '@/features/tasks/task-list-state'
import { ALL, type TaskSort } from '@/features/tasks/task-list-model'

function params(init: Record<string, string> = {}): URLSearchParams {
  return new URLSearchParams(init)
}

describe('task list URL state', () => {
  it('reads defaults without inventing state', () => {
    const state = readTaskListUrl(params())
    expect(state.search).toBe('')
    expect(state.status).toBe(ALL)
    expect(state.priority).toBe(ALL)
    expect(state.sort).toEqual({ key: 'due', direction: 'asc' })
    expect(state.page).toBe(1)
    expect(state.columns).toEqual([...DEFAULT_PROJECT_COLUMNS])
    expect(state.selected).toEqual([])
  })

  it('reads every shareable knob and ignores nonsense', () => {
    const state = readTaskListUrl(
      params({
        q: 'readiness',
        status: 'blocked',
        priority: 'high',
        sort: 'title',
        dir: 'desc',
        page: '3',
        cols: 'title,status,readiness,not-a-column',
        sel: 't-1,t-2,t-1',
        task: 't-9',
      }),
    )
    expect(state).toMatchObject({
      search: 'readiness',
      status: 'blocked',
      priority: 'high',
      sort: { key: 'title', direction: 'desc' },
      page: 3,
    })
    expect(state.columns).toEqual(['title', 'status', 'readiness'])
    expect(state.selected).toEqual(['t-1', 't-2'])

    const nonsense = readTaskListUrl(params({ sort: 'sideways', dir: 'up', page: '-4', cols: 'nope' }))
    expect(nonsense.sort).toEqual({ key: 'due', direction: 'asc' })
    expect(nonsense.page).toBe(1)
    expect(nonsense.columns).toEqual([...DEFAULT_PROJECT_COLUMNS])
  })

  it('always keeps the title column', () => {
    expect(readColumns(params({ cols: 'status,priority' }))).toEqual(['title', 'status', 'priority'])
    expect(readColumns(params({ cols: 'title' }))).toEqual(['title'])
    expect(readColumns(params({}))).toBeNull()
  })

  it('writes a patch without dropping the open task, and clears neutral values', () => {
    const current = params({ task: 't-3', q: 'old', status: 'blocked', page: '4' })
    const next = writeTaskListUrl(current, {
      search: '',
      status: ALL,
      sort: { key: 'due', direction: 'asc' },
      page: 1,
      selected: [],
      columns: ['title', 'status', 'dependencies'],
    })

    expect(next.get('task')).toBe('t-3')
    expect(next.get('q')).toBeNull()
    expect(next.get('status')).toBeNull()
    expect(next.get('page')).toBeNull()
    expect(next.get('sort')).toBeNull()
    expect(next.get('dir')).toBeNull()
    expect(next.get('sel')).toBeNull()
    expect(next.get('cols')).toBe('title,status,dependencies')

    // It does not mutate the caller's parameters.
    expect(current.get('q')).toBe('old')
  })

  it('keeps a chosen sort when the view declares a different fallback', () => {
    const declared: TaskSort = { key: 'title', direction: 'asc' }

    // "Due date" in a view that declares `sort: title` must survive as URL
    // state; clearing it against the global default would silently fall back.
    const chosen = writeTaskListUrl(params(), { sort: { key: 'due', direction: 'asc' } }, declared)
    expect(chosen.get('sort')).toBe('due')
    expect(chosen.get('dir')).toBeNull()
    expect(readTaskListUrl(chosen, DEFAULT_PROJECT_COLUMNS, declared).sort).toEqual({
      key: 'due',
      direction: 'asc',
    })

    // Choosing the declared order again clears the URL and falls back to it.
    const reset = writeTaskListUrl(chosen, { sort: declared }, declared)
    expect(reset.get('sort')).toBeNull()
    expect(reset.get('dir')).toBeNull()
    expect(readTaskListUrl(reset, DEFAULT_PROJECT_COLUMNS, declared).sort).toEqual(declared)

    // A declared descending default also clears only when it is the chosen one.
    const declaredDesc: TaskSort = { key: 'priority', direction: 'desc' }
    const other = writeTaskListUrl(params(), { sort: { key: 'priority', direction: 'asc' } }, declaredDesc)
    expect(other.get('sort')).toBe('priority')
    expect(other.get('dir')).toBeNull()
    expect(readTaskListUrl(other, DEFAULT_PROJECT_COLUMNS, declaredDesc).sort).toEqual({
      key: 'priority',
      direction: 'asc',
    })

    // Untouched, the URL still reads the view's declared order.
    expect(readTaskListUrl(params(), DEFAULT_PROJECT_COLUMNS, declaredDesc).sort).toEqual(declaredDesc)
  })

  it('round-trips a deep link through write and read', () => {
    const written = writeTaskListUrl(params({ task: 't-5' }), {
      search: '  release  ',
      status: 'ready',
      priority: 'none',
      sort: { key: 'priority', direction: 'desc' },
      page: 2,
      columns: ['title', 'milestone', 'estimate'],
      selected: ['t-a', 't-b'],
    })
    const state = readTaskListUrl(written)
    expect(state.search).toBe('release')
    expect(state.status).toBe('ready')
    expect(state.priority).toBe('none')
    expect(state.sort).toEqual({ key: 'priority', direction: 'desc' })
    expect(state.page).toBe(2)
    expect(state.columns).toEqual(['title', 'milestone', 'estimate'])
    expect(state.selected).toEqual(['t-a', 't-b'])
    expect(written.get('task')).toBe('t-5')
  })
})

describe('workflow-declared columns', () => {
  function component(columns: string[]): WorkflowViewComponent {
    return {
      id: 'table',
      type: 'table',
      title: 'Tasks',
      source: 'tasks',
      aggregate: null,
      field: null,
      groupBy: null,
      filter: {},
      sort: null,
      descending: false,
      limit: null,
      columns,
      text: null,
      actions: [],
      options: {},
    }
  }

  it('maps declared column names onto renderable columns, preserving order', () => {
    expect(columnsFromComponent(component(['title', 'status', 'deadline', 'estimate']))).toEqual([
      'title',
      'status',
      'due',
      'estimate',
      'readiness',
      'dependencies',
    ])
    expect(columnsFromComponent(component(['name', 'kind', 'state', 'title']))).toEqual([
      'title',
      'status',
      'readiness',
      'dependencies',
    ])
  })

  it('falls back when the workflow declares nothing usable', () => {
    expect(columnsFromComponent(component([]))).toEqual([...DEFAULT_PROJECT_COLUMNS])
    expect(columnsFromComponent(component(['unknown']), ['title', 'due'])).toEqual(['title', 'due'])
    expect(columnsFromComponent(undefined)).toEqual([...DEFAULT_PROJECT_COLUMNS])
  })
})

describe('task list windowing', () => {
  const rows = Array.from({ length: 250 }, (_, index) => index)

  it('renders short lists whole', () => {
    const short = rows.slice(0, 200)
    const window = windowTasks(short, 4)
    expect(window.windowed).toBe(false)
    expect(window.rows).toHaveLength(200)
    expect(window.page).toBe(1)
    expect(window.pageCount).toBe(1)
  })

  it('renders one page for a long list and clamps the page', () => {
    const first = windowTasks(rows, 1)
    expect(first.windowed).toBe(true)
    expect(first.rows[0]).toBe(0)
    expect(first.rows).toHaveLength(50)
    expect(first.pageCount).toBe(5)

    const third = windowTasks(rows, 3)
    expect(third.page).toBe(3)
    expect(third.rows[0]).toBe(100)
    expect(third.rows.at(-1)).toBe(149)

    const beyond = windowTasks(rows, 99)
    expect(beyond.page).toBe(5)
    expect(beyond.rows[0]).toBe(200)

    const invalid = windowTasks(rows, Number.NaN)
    expect(invalid.page).toBe(1)
  })

  it('can be tuned for tests or denser screens', () => {
    const window = windowTasks(rows, 2, { pageSize: 10, threshold: 5 })
    expect(window.pageCount).toBe(25)
    expect(window.rows).toEqual([10, 11, 12, 13, 14, 15, 16, 17, 18, 19])
  })
})

describe('task column vocabulary', () => {
  it('names every column the picker offers', () => {
    const columns: TaskColumnKey[] = [
      'title',
      'status',
      'priority',
      'project',
      'milestone',
      'due',
      'estimate',
      'labels',
      'readiness',
      'dependencies',
    ]
    expect(columns).toHaveLength(10)
  })
})
