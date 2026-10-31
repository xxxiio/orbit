import { describe, expect, it } from 'vitest'
import {
  DEFAULT_TASK_SORT,
  TASKS_VIEW_OPTIONS,
  legacyTasksView,
  readTaskListUrl,
  readTasksView,
  writeTaskListUrl,
  writeTasksView,
  type TasksView,
} from '@/features/tasks/task-list-state'
import { ALL } from '@/features/tasks/task-list-model'

function params(init: Record<string, string> = {}): URLSearchParams {
  return new URLSearchParams(init)
}

describe('tasks view switcher URL', () => {
  it('reads list by default and accepts only the three declared views', () => {
    expect(readTasksView(params())).toBe('list')
    expect(readTasksView(params({ view: 'list' }))).toBe('list')
    expect(readTasksView(params({ view: 'board' }))).toBe('board')
    expect(readTasksView(params({ view: 'backlog' }))).toBe('backlog')
    expect(readTasksView(params({ view: 'calendar' }))).toBe('list')
    expect(readTasksView(params({ view: '' }))).toBe('list')
    expect(TASKS_VIEW_OPTIONS.map((option) => option.value)).toEqual<TasksView[]>([
      'list',
      'board',
      'backlog',
    ])
  })

  it('writes the view and keeps every shared filter and the open task', () => {
    const current = params({
      q: 'release',
      status: 'ready',
      priority: 'high',
      milestone: 'm-1',
      sort: 'priority',
      dir: 'desc',
      group: 'milestone',
      cols: 'title,status,due',
      sel: 't-1,t-2',
      task: 't-9',
    })

    const board = writeTasksView(current, 'board')
    expect(board.get('view')).toBe('board')
    for (const [key, value] of current.entries()) {
      expect(board.get(key), key).toBe(value)
    }
    // It does not mutate the caller's parameters.
    expect(current.get('view')).toBeNull()

    expect(writeTasksView(board, 'backlog').get('view')).toBe('backlog')
    // List is the default: choosing it clears the parameter again.
    expect(writeTasksView(board, 'list').get('view')).toBeNull()
  })

  it('keeps the shared filter state readable after a view switch', () => {
    const switched = writeTasksView(
      params({ q: 'release', status: 'blocked', milestone: 'm-2', group: 'milestone' }),
      'backlog',
    )
    const state = readTaskListUrl(switched)
    expect(state.search).toBe('release')
    expect(state.status).toBe('blocked')
    expect(state.milestone).toBe('m-2')
    expect(state.group).toBe('milestone')
    expect(state.sort).toEqual(DEFAULT_TASK_SORT)
  })

  it('switching the view never drops a chosen sort', () => {
    const switched = writeTasksView(
      writeTaskListUrl(params(), { sort: { key: 'priority', direction: 'desc' } }),
      'board',
    )
    const state = readTaskListUrl(switched)
    expect(readTasksView(switched)).toBe('board')
    expect(state.sort).toEqual({ key: 'priority', direction: 'desc' })
  })

  it('maps the legacy routes onto views without rewriting filters', () => {
    expect(legacyTasksView('list')).toBe('list')
    expect(legacyTasksView('board')).toBe('board')
    expect(legacyTasksView('backlog')).toBe('backlog')
    expect(legacyTasksView('overview')).toBeNull()
    expect(legacyTasksView('tasks')).toBeNull()
    expect(legacyTasksView(undefined)).toBeNull()

    const legacy = writeTasksView(params({ status: 'inbox', task: 't-4' }), 'backlog')
    expect(readTasksView(legacy)).toBe('backlog')
    expect(legacy.get('status')).toBe('inbox')
    expect(legacy.get('task')).toBe('t-4')
  })
})

describe('milestone filter URL state', () => {
  it('round-trips a milestone filter and clears it with the neutral value', () => {
    const filtered = writeTaskListUrl(params(), { milestone: 'm-3' })
    expect(filtered.get('milestone')).toBe('m-3')
    expect(readTaskListUrl(filtered).milestone).toBe('m-3')

    expect(readTaskListUrl(params()).milestone).toBe(ALL)

    const cleared = writeTaskListUrl(filtered, { milestone: ALL })
    expect(cleared.get('milestone')).toBeNull()
  })

  it('distinguishes "no milestone" from "any milestone"', () => {
    const none = writeTaskListUrl(params(), { milestone: 'none' })
    expect(readTaskListUrl(none).milestone).toBe('none')
    expect(readTaskListUrl(params()).milestone).toBe(ALL)
  })
})
