import type { Milestone, Project, Task } from '@/api/adapters'
import { ProjectTaskList } from '@/features/tasks/project-task-list'
import type { TaskColumnKey } from '@/features/tasks/task-list-state'
import type { TaskSort } from '@/features/tasks/task-list-model'

/** Work that has not entered delivery yet. Sprints are deliberately absent. */
const BACKLOG_STATUSES = new Set(['inbox', 'planned', 'deferred'])

/** Priority, estimate, deadline and dependency information, grouped by milestone. */
const BACKLOG_COLUMNS: readonly TaskColumnKey[] = [
  'title',
  'status',
  'priority',
  'milestone',
  'due',
  'estimate',
  'dependencies',
]

/**
 * The Backlog is the same task list over the not-yet-started statuses. It owns
 * no second task model and no sprint: moving a card to Ready is the same
 * authoritative status control the List uses.
 */
export function BacklogView({
  project,
  tasks,
  milestones,
  defaultSort,
}: {
  project: Project
  tasks: Task[]
  milestones: Milestone[]
  /** The shared control bar's fallback order until the URL names one. */
  defaultSort?: TaskSort
}) {
  const backlog = tasks.filter((task) => BACKLOG_STATUSES.has(task.status))
  const highPriority = backlog.filter(
    (task) => task.priority === 'high' || task.priority === 'urgent',
  ).length
  const unestimated = backlog.filter((task) => task.estimatedMinutes == null).length
  const noMilestone = backlog.filter((task) => task.milestoneId == null).length

  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-lg border border-line bg-surface px-4 py-3">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold text-ink">Backlog</h2>
          <p className="mt-0.5 max-w-2xl text-xs leading-5 text-ink-muted">
            Upcoming work before it is scheduled, grouped by milestone. Move a task to{' '}
            <strong>Ready</strong> from its status control when it should enter delivery.
          </p>
          <p className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-ink-muted">
            <span>
              <span className="tabular-nums text-ink-soft">{backlog.length}</span> items
            </span>
            <span>
              <span className="tabular-nums text-ink-soft">{highPriority}</span> high or urgent
            </span>
            <span>
              <span className="tabular-nums text-ink-soft">{unestimated}</span> unestimated
            </span>
            <span>
              <span className="tabular-nums text-ink-soft">{noMilestone}</span> without a milestone
            </span>
          </p>
        </div>
      </div>

      <ProjectTaskList
        project={project}
        tasks={backlog}
        milestones={milestones}
        defaultColumns={BACKLOG_COLUMNS}
        defaultSort={defaultSort}
        defaultGroup="milestone"
        showFilterBar={false}
        emptyText="The backlog is empty"
      />
      <p className="text-2xs leading-5 text-ink-muted">
        Ordering inside a group comes from the selected sort; the server does not expose a manual
        rank yet.
      </p>
    </div>
  )
}
