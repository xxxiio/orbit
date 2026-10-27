import { describe, expect, it } from 'vitest'
import type { ProjectKnowledgeResponse } from '@/api/contracts'
import {
  foreignReferencesOf,
  groupProjectKnowledge,
  inclusionSummary,
  projectReferencesOf,
  unrepresentableColumns,
} from '@/features/knowledge/project-knowledge-model'

const response: ProjectKnowledgeResponse = {
  project_id: 'orbit',
  tasks: ['task-1', 'task-2'],
  notes: [
    {
      id: 'n-project',
      path: 'Work/Orbit.md',
      title: 'Orbit',
      references: [{ kind: 'project', id: 'orbit' }],
    },
    {
      id: 'n-task',
      path: 'Work/Release.md',
      title: 'Release',
      references: [{ kind: 'task', id: 'task-1' }],
    },
    {
      id: 'n-both',
      path: 'Work/Both.md',
      title: 'Both',
      references: [
        { kind: 'project', id: 'orbit' },
        { kind: 'task', id: 'task-2' },
      ],
    },
    {
      id: 'n-odd',
      path: 'Work/Odd.md',
      title: 'Odd',
      references: [{ kind: 'task', id: 'task-9' }],
    },
  ],
}

describe('project knowledge grouping', () => {
  it('separates direct project references from task-driven ones', () => {
    const groups = groupProjectKnowledge(response)
    expect(groups.notes.map((note) => note.id)).toEqual(['n-project', 'n-task', 'n-both', 'n-odd'])
    expect(groups.direct.map((note) => note.id)).toEqual(['n-project', 'n-both'])
    expect(groups.throughTasks.map((note) => note.id)).toEqual(['n-task'])
    expect(groups.taskIds).toEqual(['task-1', 'task-2'])
  })

  it('keeps a note it cannot attribute instead of dropping it', () => {
    const groups = groupProjectKnowledge(response)
    expect(groups.unattributed.map((note) => note.id)).toEqual(['n-odd'])
  })

  it('handles an empty project without inventing notes', () => {
    const groups = groupProjectKnowledge({ project_id: 'orbit', tasks: [], notes: [] })
    expect(groups).toEqual({
      notes: [],
      direct: [],
      throughTasks: [],
      unattributed: [],
      taskIds: [],
    })
  })

  it('lists only the references that belong to this project', () => {
    const both = response.notes[2]
    expect(projectReferencesOf(both, 'orbit', response.tasks)).toEqual([
      { kind: 'project', id: 'orbit' },
      { kind: 'task', id: 'task-2' },
    ])
    expect(foreignReferencesOf(response.notes[3], 'orbit', response.tasks)).toEqual([
      { kind: 'task', id: 'task-9' },
    ])
    expect(projectReferencesOf(response.notes[3], 'orbit', response.tasks)).toEqual([])
  })

  it('summarises why a note is in the list, without inventing a reference', () => {
    expect(inclusionSummary(response.notes[0], 'orbit', response.tasks)).toBe(
      'References project orbit',
    )
    expect(inclusionSummary(response.notes[1], 'orbit', response.tasks)).toBe(
      'References task task-1',
    )
    expect(inclusionSummary(response.notes[3], 'orbit', response.tasks)).toBe(
      'References task:task-9',
    )
  })

  it('names the declared columns the project read cannot represent', () => {
    expect(unrepresentableColumns([])).toEqual([])
    expect(unrepresentableColumns(['Title', 'Path', 'id', 'references'])).toEqual([])
    expect(unrepresentableColumns(['Title', 'Word count', 'Owner'])).toEqual(['Word count', 'Owner'])
  })
})
