import { describe, expect, it } from 'vitest'
import type { Task } from '@/api/adapters'
import {
  ALL,
  bulkValueOptions,
  filterAndSortTasks,
  inboxSortKey,
  projectSortKey,
  projectSortOptions,
  projectSortValue,
  type TaskSort,
} from '@/features/tasks/task-list-model'

function task(overrides: Partial<Task> & { id: string }): Task {
  return {
    title: overrides.id,
    status: 'ready',
    projectId: null,
    milestoneId: null,
    estimatedMinutes: null,
    earliestStartAtMs: null,
    deadlineAtMs: null,
    description: '',
    priority: null,
    labels: [],
    createdAtMs: null,
    updatedAtMs: null,
    ...overrides,
  }
}

const tasks: Task[] = [
  task({ id: 'a', title: 'Write the release notes', status: 'planned', priority: 'low' }),
  task({ id: 'b', title: 'Release the build', status: 'ready', priority: 'urgent' }),
  task({ id: 'c', title: 'Review the changes', status: 'ready', priority: null }),
]

const noFilters = { search: '', status: ALL, priority: ALL }
const priorityAsc: TaskSort = { key: 'priority', direction: 'asc' }

describe('task list model', () => {
  it('filters by title search, status and priority in one pipeline', () => {
    expect(filterAndSortTasks(tasks, { ...noFilters, search: 'release' }, priorityAsc).map((t) => t.id)).toEqual([
      'b',
      'a',
    ])
    // A URL query keeps its whitespace; matching trims it so the result is the
    // same as typing the words without padding.
    expect(
      filterAndSortTasks(tasks, { ...noFilters, search: '  release  ' }, priorityAsc).map(
        (t) => t.id,
      ),
    ).toEqual(['b', 'a'])
    expect(filterAndSortTasks(tasks, { ...noFilters, status: 'ready' }, priorityAsc).map((t) => t.id)).toEqual([
      'b',
      'c',
    ])
    // `none` matches the unprioritised task, not a literal priority value.
    expect(filterAndSortTasks(tasks, { ...noFilters, priority: 'none' }, priorityAsc).map((t) => t.id)).toEqual([
      'c',
    ])
    // Priority sorts unprioritised tasks last, and direction reverses the order.
    expect(filterAndSortTasks(tasks, noFilters, priorityAsc).map((t) => t.id)).toEqual(['b', 'a', 'c'])
    expect(
      filterAndSortTasks(tasks, noFilters, { key: 'priority', direction: 'desc' }).map((t) => t.id),
    ).toEqual(['c', 'a', 'b'])
  })

  it('maps each page select value to the sort key it applies and offers the right bulk values', () => {
    const projects = [{ id: 'p-1', name: 'Orbit' }]
    expect(inboxSortKey('status')).toBe('priority')
    expect(inboxSortKey('due')).toBe('due')
    expect(projectSortKey('status')).toBe('status')
    expect(projectSortKey('milestone')).toBe('milestone')
    expect(projectSortKey('project')).toBe('project')
    // The Tasks control reports a `?sort=project` honestly instead of falling
    // back to "Due date".
    expect(projectSortOptions.map((option) => option.value)).toContain('project')
    expect(projectSortValue({ key: 'project', direction: 'asc' })).toBe('project')
    expect(bulkValueOptions('project', projects).map((option) => option.value)).toEqual(['p-1'])
    expect(bulkValueOptions('project', projects, { includeInbox: true }).map((option) => option.value)).toEqual([
      'none',
      'p-1',
    ])
    expect(bulkValueOptions('status', projects)[0]).toEqual({ value: 'inbox', label: 'Inbox' })
    expect(bulkValueOptions('priority', projects)[0]).toEqual({ value: 'none', label: 'No priority' })
  })
})
