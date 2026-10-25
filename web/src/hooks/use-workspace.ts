/**
 * Snapshot selectors.
 *
 * `GET /authoritative/tasks` returns the complete task set with no filtering,
 * sorting or pagination, so these selectors are pure presentation over data the
 * server already decided. They never compute readiness, priority or ordering
 * rules; ask `/task-readiness` or `/next-actions` for those.
 */
import { useMemo } from 'react'
import { useWorkspaceSnapshot } from '@/api/queries'
import type { Project, Task, WorkspaceSnapshot } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'

export { useWorkspaceSnapshot }

export interface TaskFilter {
  /** Any of these statuses (empty/absent means all). */
  status?: readonly TaskStatus[]
  priority?: TaskPriority
  milestoneId?: string
  /** Case-insensitive substring of the title. */
  search?: string
}

function matchesFilter(task: Task, filter: TaskFilter | undefined): boolean {
  if (!filter) return true
  if (filter.status && filter.status.length > 0 && !filter.status.includes(task.status)) return false
  if (filter.priority && task.priority !== filter.priority) return false
  if (filter.milestoneId && task.milestoneId !== filter.milestoneId) return false
  if (filter.search && filter.search.trim().length > 0) {
    const needle = filter.search.trim().toLowerCase()
    if (!task.title.toLowerCase().includes(needle)) return false
  }
  return true
}

const EMPTY_PROJECTS: Project[] = []
const EMPTY_TASKS: Task[] = []

export function useProjects(): Project[] {
  const { data } = useWorkspaceSnapshot()
  return data?.projects ?? EMPTY_PROJECTS
}

export function useProject(id: string | undefined): Project | undefined {
  const projects = useProjects()
  return useMemo(() => (id ? projects.find((project) => project.id === id) : undefined), [projects, id])
}

export function useProjectTasks(projectId: string | undefined, filter?: TaskFilter): Task[] {
  const { data } = useWorkspaceSnapshot()
  const tasks = data?.tasks ?? EMPTY_TASKS
  return useMemo(
    () =>
      projectId
        ? tasks.filter((task) => task.projectId === projectId && matchesFilter(task, filter))
        : EMPTY_TASKS,
    [tasks, projectId, filter],
  )
}

/**
 * The Inbox is the set of unassigned tasks: the snapshot has no inbox flag, and
 * "belongs to no project" is the one fact the server gives us.
 */
export function useInboxTasks(filter?: TaskFilter): Task[] {
  const { data } = useWorkspaceSnapshot()
  const tasks = data?.tasks ?? EMPTY_TASKS
  return useMemo(
    () => tasks.filter((task) => task.projectId === null && matchesFilter(task, filter)),
    [tasks, filter],
  )
}

export interface TaskCounts {
  total: number
  unassigned: number
  /** Counts per status; every status key is present. */
  byStatus: Record<TaskStatus, number>
  /** Everything not in a terminal state, as reported by the server's own statuses. */
  open: number
}

const EMPTY_COUNTS: TaskCounts = {
  total: 0,
  unassigned: 0,
  byStatus: {
    inbox: 0,
    planned: 0,
    ready: 0,
    in_progress: 0,
    waiting: 0,
    blocked: 0,
    review: 0,
    deferred: 0,
    done: 0,
    cancelled: 0,
  },
  open: 0,
}

export function useTaskCounts(): TaskCounts {
  const { data } = useWorkspaceSnapshot()
  return useMemo(() => {
    if (!data) return EMPTY_COUNTS
    const byStatus: Record<TaskStatus, number> = { ...EMPTY_COUNTS.byStatus }
    let unassigned = 0
    let open = 0
    for (const task of data.tasks) {
      byStatus[task.status] = (byStatus[task.status] ?? 0) + 1
      if (task.projectId === null) unassigned += 1
      if (task.status !== 'done' && task.status !== 'cancelled') open += 1
    }
    return { total: data.tasks.length, unassigned, byStatus, open }
  }, [data])
}

export type { WorkspaceSnapshot }
