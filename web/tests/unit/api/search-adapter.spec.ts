import { describe, expect, it } from 'vitest'
import { searchGroupLabel, toSearchView } from '@/api/adapters'
import type { SearchResponse } from '@/api/contracts'

const response: SearchResponse = {
  revision: 42,
  query: 'orbit',
  limit: 10,
  groups: [
    {
      kind: 'project',
      truncated: false,
      results: [
        {
          kind: 'project',
          id: 'orbit',
          title: 'Orbit',
          subtitle: 'active',
          project_id: 'orbit',
          path: null,
        },
      ],
    },
    {
      kind: 'task',
      truncated: true,
      results: [
        {
          kind: 'task',
          id: 'task-1',
          title: 'Fix the login form',
          subtitle: 'in_progress · Orbit',
          project_id: 'orbit',
          path: null,
        },
      ],
    },
    { kind: 'note', truncated: false, results: [] },
  ],
}

describe('search adapters', () => {
  it('keeps the server grouping, order and truncation flags', () => {
    const view = toSearchView(response)

    expect(view.groups.map((group) => group.kind)).toEqual(['project', 'task', 'note'])
    expect(view.groups[1].truncated).toBe(true)
    expect(view.groups[2].results).toEqual([])
    expect(view.groups[0].results[0]).toEqual({
      kind: 'project',
      id: 'orbit',
      title: 'Orbit',
      subtitle: 'active',
      projectId: 'orbit',
      path: null,
    })
  })

  it('counts only the rows the server returned', () => {
    expect(toSearchView(response).total).toBe(2)
  })

  it('names the three groups for a person', () => {
    expect(searchGroupLabel('project')).toBe('Projects')
    expect(searchGroupLabel('task')).toBe('Tasks')
    expect(searchGroupLabel('note')).toBe('Knowledge')
  })
})
