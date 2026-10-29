import type { Epic, Milestone, Project, Task } from '@/api/adapters'
import { Button } from '@/components/ui/button'
import { PlusIcon } from '@/components/ui/icons'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { ProjectTaskList } from '@/features/tasks'
import { TASK_COLUMN_LABELS, type TaskColumnKey } from '@/features/tasks/task-list-state'
import type { TaskSort } from '@/features/tasks/task-list-model'

/** Work that has not entered delivery yet. Sprints are deliberately absent. */
const BACKLOG_STATUSES = new Set(['inbox', 'planned', 'deferred'])

const BACKLOG_COLUMNS: readonly TaskColumnKey[] = [
  'title',
  'status',
  'priority',
  'epic',
  'milestone',
  'due',
  'estimate',
  'dependencies',
]

const BACKLOG_SORT: TaskSort = { key: 'priority', direction: 'desc' }

export function BacklogView({
  project,
  tasks,
  milestones,
  epics,
}: {
  project: Project
  tasks: Task[]
  milestones: Milestone[]
  epics: Epic[]
}) {
  const { openNewTask } = useWorkspaceActions()
  const backlog = tasks.filter((task) => BACKLOG_STATUSES.has(task.status))
  const highPriority = backlog.filter(
    (task) => task.priority === 'high' || task.priority === 'urgent',
  ).length
  const unestimated = backlog.filter((task) => task.estimatedMinutes == null).length
  const noMilestone = backlog.filter((task) => task.milestoneId == null).length

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3 rounded-lg border border-line bg-surface px-4 py-3">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold text-ink">Backlog</h2>
          <p className="mt-0.5 max-w-2xl text-xs leading-5 text-ink-muted">
            Upcoming work before it is scheduled. Move a task to <strong>Ready</strong> from its
            status control when it should enter delivery. Grouped by epic; nothing here is a sprint.
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
        <Button
          size="sm"
          leadingIcon={<PlusIcon size={15} />}
          onClick={() => openNewTask(project.id)}
        >
          New task
        </Button>
      </div>

      <ProjectTaskList
        project={project}
        tasks={backlog}
        milestones={milestones}
        epics={epics}
        defaultColumns={BACKLOG_COLUMNS}
        defaultSort={BACKLOG_SORT}
        defaultGroup="epic"
        emptyText="The backlog is empty"
      />
      <p className="text-2xs leading-5 text-ink-muted">
        Columns: {BACKLOG_COLUMNS.map((column) => TASK_COLUMN_LABELS[column]).join(', ')}. Ordering
        inside a group comes from the selected sort; the server does not expose a manual rank yet.
      </p>
    </div>
  )
}
