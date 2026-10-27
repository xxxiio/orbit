import { describe, expect, it } from 'vitest'
import type { WorkflowView } from '@/api/adapters'
import {
  findViewForDisplay,
  projectViewDisplay,
  queryFromSearch,
  savedViewConfigFromLocation,
  savedViewMatchesConfig,
  savedViewNameError,
  searchFromQuery,
} from '@/features/saved-views/model'

function view(id: string, type: string, source: string | null = 'tasks'): WorkflowView {
  return {
    id,
    title: id,
    description: '',
    layout: { type: 'vertical', columns: 1 },
    components: [
      {
        id: `${id}-component`,
        type,
        title: null,
        source,
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
      },
    ],
  }
}

describe('saved view configuration from a route', () => {
  it('keeps the project id, raw query string and display preference', () => {
    expect(
      savedViewConfigFromLocation('/projects/orbit/board', '?status=ready&sort=due', 'board'),
    ).toEqual({
      scope: 'project',
      projectId: 'orbit',
      query: 'status=ready&sort=due',
      display: 'board',
    })
  })

  it('strips only the leading question mark and keeps parameter order', () => {
    expect(queryFromSearch('?q=login&cols=title%2Cstatus')).toBe('q=login&cols=title%2Cstatus')
    expect(queryFromSearch('')).toBe('')
    expect(searchFromQuery('')).toBe('')
    expect(searchFromQuery('q=orbit')).toBe('?q=orbit')
  })

  it('scopes Inbox and Home, and refuses an unrelated route', () => {
    expect(savedViewConfigFromLocation('/inbox', '?priority=high', 'list')).toMatchObject({
      scope: 'inbox',
      projectId: null,
      query: 'priority=high',
    })
    expect(savedViewConfigFromLocation('/', '', 'list')).toMatchObject({
      scope: 'home',
      projectId: null,
    })
    expect(savedViewConfigFromLocation('/knowledge/notes.md', '', 'list')).toBeNull()
  })

  it('matches a view only within its scope and project', () => {
    const config = { scope: 'project' as const, projectId: 'orbit' }
    expect(savedViewMatchesConfig({ scope: 'project', projectId: 'orbit' }, config)).toBe(true)
    expect(savedViewMatchesConfig({ scope: 'project', projectId: 'other' }, config)).toBe(false)
    expect(savedViewMatchesConfig({ scope: 'inbox', projectId: null }, config)).toBe(false)
    expect(
      savedViewMatchesConfig({ scope: 'home', projectId: null }, { scope: 'home', projectId: null }),
    ).toBe(true)
  })
})

describe('workflow view display', () => {
  const views = [
    view('tasks', 'table'),
    view('board', 'kanban'),
    view('overview', 'metric'),
    view('knowledge', 'kanban', 'knowledge'),
  ]

  it('maps only task surfaces to a display preference', () => {
    expect(projectViewDisplay(views[0])).toBe('list')
    expect(projectViewDisplay(views[1])).toBe('board')
    expect(projectViewDisplay(views[2])).toBeNull()
    expect(projectViewDisplay(views[3])).toBeNull()
  })

  it('finds the first view that presents tasks that way', () => {
    expect(findViewForDisplay(views, 'list')?.id).toBe('tasks')
    expect(findViewForDisplay(views, 'board')?.id).toBe('board')
    expect(findViewForDisplay([view('overview', 'metric')], 'board')).toBeUndefined()
  })
})

describe('saved view name validation', () => {
  it('refuses an empty or whitespace-only name before sending', () => {
    expect(savedViewNameError('')).toBe('Give the saved view a name.')
    expect(savedViewNameError('   ')).toBe('Give the saved view a name.')
    expect(savedViewNameError(' Ready work ')).toBeNull()
  })
})
