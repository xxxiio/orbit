import { describe, expect, it } from 'vitest'
import type { WorkflowView } from '@/api/adapters'
import { viewHeaderAction } from '@/features/workflows/declarations'
import { projectSections } from '@/features/projects/project-workspace-page'

function tasksView(): WorkflowView {
  return {
    id: 'tasks',
    title: 'Tasks',
    description: '',
    layout: { type: 'vertical', columns: 1 },
    components: [
      {
        id: 'task_table',
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
        columns: [],
        text: null,
        actions: [],
        options: {},
      },
    ],
  }
}

describe('project sections', () => {
  it('synthesizes the Milestones section over milestones, not tasks', () => {
    const sections = projectSections([tasksView()])
    const milestones = sections.find((section) => section.id === 'milestones')
    expect(milestones?.view?.components[0]?.source).toBe('milestones')
    // The header action follows the source: the Milestones tab creates a
    // milestone, not a task.
    expect(viewHeaderAction(milestones!.view!)).toEqual({
      kind: 'new_milestone',
      label: 'New milestone',
    })
    expect(sections.find((section) => section.id === 'tasks')?.view?.components[0]?.source).toBe(
      'tasks',
    )
  })
})
