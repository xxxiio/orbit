import { useCallback, useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import {
  defaultNowMs,
  newCommandId,
  taskCommandAttempt,
  uncertainMessage,
  useCommandFeedback,
  useMilestones,
  useProjects,
  useTaskCommand,
  useTaskDetail,
  useTaskGraph,
  useTaskReadiness,
  useWorkspaceSnapshot,
  userMessage,
  type TaskCommandInput,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Drawer } from '@/components/ui/drawer'
import { ErrorState } from '@/components/ui/error-state'
import { AlertCircleIcon, CheckCircleIcon, LoaderIcon } from '@/components/ui/icons'
import { priorityLabels, priorityTones, statusLabel, statusTones } from '../task-meta'
import { useTaskDrawer } from '../use-task-drawer'
import { writeAcceptedTask } from '../task-cache'
import { ActivitySection } from './activity-section'
import { CriteriaSection } from './criteria-section'
import { DependenciesSection } from './dependencies-section'
import { DescriptionEditor } from './description-editor'
import { PropertiesSection } from './properties-section'
import { DrawerSkeleton } from './skeleton'
import type { TaskCommand } from './task-command'
import { EditGuardProvider, InlineTitle, type EditorHandle } from '@/components/forms'

export function TaskDetailDrawer() {
  const { taskId, closeTask } = useTaskDrawer()
  if (!taskId) return null
  return <TaskDrawerPanel key={taskId} taskId={taskId} onClose={closeTask} />
}

/**
 * The task drawer shell. Section modules own their own markup; this file owns
 * the command in flight, the close guard, and the honest footer.
 */
function TaskDrawerPanel({ taskId, onClose }: { taskId: string; onClose: () => void }) {
  const detail = useTaskDetail(taskId)
  const projects = useProjects()
  const snapshot = useWorkspaceSnapshot()
  const milestones = useMilestones(detail.task?.projectId ?? undefined)
  const readiness = useTaskReadiness(defaultNowMs())
  const graph = useTaskGraph(defaultNowMs())
  const command: TaskCommand = useTaskCommand()
  const queryClient = useQueryClient()
  const { run } = useCommandFeedback()

  const editorsRef = useRef(new Map<string, EditorHandle>())
  const register = useCallback((handle: EditorHandle) => {
    editorsRef.current.set(handle.id, handle)
    return () => {
      editorsRef.current.delete(handle.id)
    }
  }, [])

  const [closeNotice, setCloseNotice] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const lastLabel = useRef('Change')
  const detailField = useRef<'description' | 'priority' | 'labels' | null>(null)
  const inFlight = useRef(false)

  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000)
    return () => window.clearInterval(timer)
  }, [])

  useEffect(() => {
    if (command.disposition === 'accepted') {
      setAnnouncement(`${lastLabel.current} saved.`)
      if (closeNotice) setCloseNotice('Saved. Close the drawer when you are ready.')
    }
    // Reading the transition into `accepted` is the point of this effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [command.disposition])

  useEffect(() => {
    if (!announcement) return
    const timer = window.setTimeout(() => setAnnouncement(null), 5000)
    return () => window.clearTimeout(timer)
  }, [announcement])

  const requestClose = useCallback(() => {
    const dirty = [...editorsRef.current.values()].filter((editor) => editor.isDirty())
    if (dirty.length > 0) {
      setCloseNotice('Your edit is being saved first. Close again when the server answers.')
      for (const editor of dirty) editor.flush()
      return
    }
    if (inFlight.current || command.isPending) {
      setCloseNotice('Waiting for the server to accept the last change. Close again when it settles.')
      return
    }
    setCloseNotice(null)
    onClose()
  }, [command.isPending, onClose])

  const send = useCallback(
    (label: string, input: TaskCommandInput) => {
      lastLabel.current = label
      inFlight.current = true
      return command
        .mutateAsync(taskCommandAttempt(input))
        .then((outcome) => {
          // The drawer builds `set_details` from the record it holds. Write the
          // accepted record back before the next edit can read the old one, so
          // two fast edits cannot revert each other.
          writeAcceptedTask(queryClient, outcome.task)
          return outcome
        })
        .finally(() => {
          inFlight.current = false
        })
    },
    [command, queryClient],
  )

  const uncertain = command.disposition === 'uncertain'
  const error: ApiError | null = command.disposition === 'rejected' ? command.error : null
  const errorType = error ? command.variables?.input.type : undefined

  const pendingField = (field: 'title' | 'status' | 'project' | 'milestone' | 'criteria' | 'dependencies') => {
    if (!command.isPending || !command.variables) return false
    switch (command.variables.input.type) {
      case 'rename':
        return field === 'title'
      case 'change_status':
        return field === 'status'
      case 'assign_project':
        return field === 'project'
      case 'assign_milestone':
        return field === 'milestone'
      case 'acceptance_criteria':
        return field === 'criteria'
      case 'add_prerequisite':
      case 'remove_prerequisite':
        return field === 'dependencies'
      default:
        return false
    }
  }

  const pendingDetail = (field: 'description' | 'priority' | 'labels') =>
    command.isPending &&
    command.variables?.input.type === 'set_details' &&
    detailField.current === field

  const changeStatus = (status: TaskStatus) => {
    void run('Status not changed', () =>
      send('Status', { type: 'change_status', taskId, status }),
    )
  }

  const assignProject = (projectId: string | null) => {
    void run('Project not changed', () =>
      send('Project', { type: 'assign_project', taskId, projectId }),
    )
  }

  const assignMilestone = (milestoneId: string | null) => {
    void run('Milestone not changed', () =>
      send('Milestone', { type: 'assign_milestone', taskId, milestoneId }),
    )
  }

  const setDetails = (
    label: 'Description' | 'Priority' | 'Labels',
    field: 'description' | 'priority' | 'labels',
    patch: { description?: string; priority?: TaskPriority | null; labels?: string[] },
  ) => {
    if (!detail.task) return
    detailField.current = field
    void run(`${label} not saved`, () =>
      send(label, {
        type: 'set_details',
        taskId,
        description: patch.description ?? detail.task!.description,
        priority: patch.priority !== undefined ? patch.priority : detail.task!.priority,
        labels: patch.labels ?? detail.task!.labels,
      }),
    )
  }

  const addCriterion = async (text: string) => {
    const result = await run('Criterion not added', () =>
      send('Checklist', {
        type: 'acceptance_criteria',
        taskId,
        change: { operation: 'add', criterionId: newCommandId(), text },
      }),
    )
    return result !== null
  }

  const task = detail.task

  return (
    <Drawer
      open
      onOpenChange={(next) => {
        if (!next) requestClose()
      }}
      side="right"
      title={task?.title ?? 'Task details'}
      description="Task details, checklist, dependencies and activity."
      closeLabel="Close task details"
      className="w-full max-w-none border-l border-line sm:w-[min(40rem,94vw)]"
    >
      <EditGuardProvider register={register}>
        <div className="flex h-full min-h-0 flex-col">
          {detail.isLoading && !task ? (
            <DrawerSkeleton />
          ) : detail.error && !task ? (
            <div className="flex flex-1 flex-col">
              <div className="flex items-start justify-between gap-3 border-b border-line px-5 py-4 pr-12">
                <p className="text-sm font-medium text-ink">Task {taskId}</p>
              </div>
              <ErrorState
                title="This task could not be loaded"
                description={userMessage(detail.error)}
                onRetry={detail.refetch}
                className="flex-1"
              />
              <div className="border-t border-line px-5 py-3">
                <Button variant="secondary" size="sm" onClick={requestClose}>
                  Close
                </Button>
              </div>
            </div>
          ) : task ? (
            <>
              <header className="border-b border-line px-5 pb-4 pr-12 pt-5">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-mono text-2xs uppercase tracking-[0.08em] text-ink-muted">
                    {task.id.length > 18 ? `${task.id.slice(0, 8)}…${task.id.slice(-4)}` : task.id}
                  </span>
                  <Badge tone={statusTones[task.status]} dot>
                    {statusLabel(task.status)}
                  </Badge>
                  {task.priority ? (
                    <Badge tone={priorityTones[task.priority]}>{priorityLabels[task.priority]}</Badge>
                  ) : null}
                  {milestones.data?.find((milestone) => milestone.id === task.milestoneId) ? (
                    <Badge tone="neutral">
                      {milestones.data.find((milestone) => milestone.id === task.milestoneId)?.name}
                    </Badge>
                  ) : null}
                </div>

                <InlineTitle
                  value={task.title}
                  disabled={command.isPending}
                  onCommit={(title) => {
                    void run('Title not changed', () => send('Title', { type: 'rename', taskId, title }))
                  }}
                />
                {errorType === 'rename' && error ? (
                  <p role="alert" className="mt-1.5 flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                    {userMessage(error)}
                  </p>
                ) : null}

                <p className="mt-1.5 text-xs text-ink-muted">
                  {task.projectId
                    ? projects.find((project) => project.id === task.projectId)?.name ?? 'Project'
                    : 'No project · In the Inbox'}
                </p>
                {detail.error ? (
                  <div className="mt-2 flex items-start gap-2 rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5">
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0 text-warning-ink" />
                    <p className="min-w-0 flex-1 text-xs leading-5 text-warning-ink">
                      Some details could not be refreshed. {userMessage(detail.error)}
                    </p>
                    <button
                      type="button"
                      onClick={detail.refetch}
                      className="shrink-0 text-xs font-medium text-warning-ink underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                    >
                      Retry
                    </button>
                  </div>
                ) : null}
              </header>

              <div className="scrollbar-slim flex-1 overflow-y-auto">
                <div className="divide-y divide-line">
                  <PropertiesSection
                    task={task}
                    projects={projects}
                    milestones={milestones.data ?? []}
                    milestonesLoading={milestones.isPending}
                    command={command}
                    error={error}
                    errorType={errorType}
                    pendingField={pendingField}
                    pendingDetail={pendingDetail}
                    onChangeStatus={changeStatus}
                    onAssignProject={assignProject}
                    onAssignMilestone={assignMilestone}
                    onSetPriority={(priority) => setDetails('Priority', 'priority', { priority })}
                    onSetLabels={(labels) => setDetails('Labels', 'labels', { labels })}
                  />

                  <DescriptionEditor
                    description={task.description}
                    pending={pendingDetail('description')}
                    disabled={command.isPending}
                    error={errorType === 'set_details' && detailField.current === 'description' ? error : null}
                    onCommit={(description) => setDetails('Description', 'description', { description })}
                  />

                  <CriteriaSection
                    taskId={taskId}
                    criteria={detail.criteria}
                    pending={pendingField('criteria')}
                    error={errorType === 'acceptance_criteria' ? error : null}
                    command={command}
                    onAdd={addCriterion}
                    onChangeText={(criterion, text) =>
                      void run('Criterion not changed', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: { operation: 'set_text', criterionId: criterion.id, text },
                        }),
                      )
                    }
                    onToggle={(criterion) =>
                      void run('Criterion not changed', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: {
                            operation: 'set_completed',
                            criterionId: criterion.id,
                            completed: !criterion.completed,
                          },
                        }),
                      )
                    }
                    onReorder={(criterion, position) =>
                      void run('Criterion not moved', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: { operation: 'reorder', criterionId: criterion.id, position },
                        }),
                      )
                    }
                    onRemove={(criterion) =>
                      void run('Criterion not removed', () =>
                        send('Checklist', {
                          type: 'acceptance_criteria',
                          taskId,
                          change: { operation: 'remove', criterionId: criterion.id },
                        }),
                      )
                    }
                  />

                  <DependenciesSection
                    task={task}
                    tasks={snapshot.data?.tasks ?? []}
                    edges={snapshot.data?.taskDependencies ?? []}
                    readiness={readiness.data?.tasks.find((entry) => entry.taskId === task.id) ?? null}
                    readinessLoading={readiness.isPending}
                    readinessError={readiness.error}
                    onRetryReadiness={() => void readiness.refetch()}
                    graphNode={graph.data?.nodes.find((node) => node.taskId === task.id) ?? null}
                    graphLoading={graph.isPending}
                    graphError={graph.error}
                    onRetryGraph={() => void graph.refetch()}
                    pending={pendingField('dependencies')}
                    disabled={command.isPending}
                    error={
                      errorType === 'add_prerequisite' || errorType === 'remove_prerequisite' ? error : null
                    }
                    onAdd={(prerequisiteId) =>
                      void run('Prerequisite not added', () =>
                        send('Dependencies', { type: 'add_prerequisite', taskId, prerequisiteId }),
                      )
                    }
                    onRemove={(prerequisiteId) =>
                      void run('Prerequisite not removed', () =>
                        send('Dependencies', { type: 'remove_prerequisite', taskId, prerequisiteId }),
                      )
                    }
                  />

                  <ActivitySection
                    entries={detail.activity}
                    loading={detail.isLoading}
                    now={now}
                  />
                </div>
              </div>

              <footer className="flex flex-col gap-1.5 border-t border-line bg-surface px-5 py-3">
                <div className="flex items-center gap-2 text-xs">
                  {command.isPending ? (
                    <>
                      <LoaderIcon size={13} className="animate-spin text-accent" />
                      <span className="text-ink-muted">Saving…</span>
                    </>
                  ) : announcement ? (
                    <>
                      <CheckCircleIcon size={13} className="text-success" />
                      <span className="text-ink-muted" aria-live="polite">
                        {announcement}
                      </span>
                    </>
                  ) : (
                    <span className="text-ink-faint">Changes save when you leave a field.</span>
                  )}
                </div>
                {error ? (
                  <p
                    role="alert"
                    className="flex items-start gap-1.5 rounded-sm border border-danger/30 bg-danger-soft px-2 py-1.5 text-xs leading-5 text-danger-ink"
                  >
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                    <span className="min-w-0">{userMessage(error)}</span>
                  </p>
                ) : null}
                {uncertain ? (
                  <p
                    role="alert"
                    className="flex items-start gap-1.5 rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5 text-xs leading-5 text-warning-ink"
                  >
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                    <span className="min-w-0">{uncertainMessage(command.error ?? undefined)}</span>
                  </p>
                ) : null}
                {closeNotice ? (
                  <p
                    role="status"
                    className="rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5 text-xs leading-5 text-warning-ink"
                  >
                    {closeNotice}
                  </p>
                ) : null}
              </footer>
            </>
          ) : null}
        </div>
      </EditGuardProvider>
    </Drawer>
  )
}
