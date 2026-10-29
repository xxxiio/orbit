import type { Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import { PropertySelect } from '@/components/forms'
import { useEpics, useTaskTransitions, userMessage } from '@/hooks'
import type { ApiError } from '@/api/errors'
import { NO_PRIORITY, prioritySelectOptions, statusOptionsFor } from '../task-controls'
import { LabelsEditor } from './labels-editor'
import { EpicAssignmentField, epicIdForTask } from '@/features/epics'
import { FieldError, SavingNote, Section } from './section'
import type { TaskCommand } from './task-command'

const NONE = '__none__'

/** Status, priority, project and milestone, always showing the stored value. */
export function PropertiesSection({
  task,
  projects,
  milestones,
  milestonesLoading,
  command,
  error,
  errorType,
  detailField,
  onRefresh,
  pendingField,
  pendingDetail,
  onChangeStatus,
  onAssignProject,
  onAssignMilestone,
  onSetPriority,
  onSetLabels,
}: {
  task: Task
  projects: Array<{ id: string; name: string }>
  milestones: Array<{ id: string; name: string; projectId: string }>
  milestonesLoading: boolean
  command: TaskCommand
  error: ApiError | null
  errorType: string | undefined
  /**
   * Which `set_details` field the rejected command targeted. Only the fields
   * this section renders (priority/labels) may quote that error; the
   * description owns its own surface beside the editor.
   */
  detailField: 'description' | 'priority' | 'labels' | null
  /** Offered beside a conflict so the person can recover without leaving. */
  onRefresh?: () => void
  pendingField: (field: 'title' | 'status' | 'project' | 'milestone' | 'criteria' | 'dependencies') => boolean
  pendingDetail: (field: 'description' | 'priority' | 'labels') => boolean
  onChangeStatus: (status: TaskStatus) => void
  onAssignProject: (projectId: string | null) => void
  onAssignMilestone: (milestoneId: string | null) => void
  onSetPriority: (priority: TaskPriority | null) => void
  onSetLabels: (labels: string[]) => void
}) {
  const propertyError =
    errorType === 'change_status' ||
    errorType === 'assign_project' ||
    errorType === 'assign_milestone' ||
    (errorType === 'set_details' && (detailField === 'priority' || detailField === 'labels'))
      ? error
      : null

  const epics = useEpics(task.projectId ?? undefined)
  const epicId = epicIdForTask(epics.data ?? [], task.id)
  const transitions = useTaskTransitions(task.status)
  const allowedNext = transitions.edges.get(task.status) ?? null
  const options = statusOptionsFor(task.status, allowedNext)
  const statusLocked = allowedNext !== null && allowedNext.length === 0

  const projectOptions = [
    { value: NONE, label: 'No project (Inbox)' },
    ...projects.map((project) => ({ value: project.id, label: project.name })),
  ]

  const milestoneOptions = [
    { value: NONE, label: 'No milestone' },
    ...milestones.map((milestone) => ({ value: milestone.id, label: milestone.name })),
    // A reference we cannot resolve must never be rendered as a raw id.
    ...(task.milestoneId && !milestones.some((milestone) => milestone.id === task.milestoneId)
      ? [{ value: task.milestoneId, label: 'Unknown milestone' }]
      : []),
  ]

  return (
    <Section
      title="Details"
      description="Status changes are checked by the server; the value here is always the stored one."
    >
      <div className="grid gap-x-4 gap-y-3 sm:grid-cols-2">
        <PropertySelect
          label="Status"
          ariaLabel="Task status"
          value={task.status}
          onValueChange={(value) => onChangeStatus(value as TaskStatus)}
          options={options}
          pending={pendingField('status')}
          disabled={command.isPending || statusLocked}
          hint={
            <span className="text-2xs leading-4 text-ink-muted">
              Only the next states this task&apos;s workflow allows are offered. A refusal keeps
              the stored value and quotes the server&apos;s reason below.
              {transitions.failed
                ? ' The allowed moves could not be read, so every status is offered; the server still refuses invalid changes.'
                : null}
            </span>
          }
        />

        <PropertySelect
          label="Priority"
          ariaLabel="Task priority"
          value={task.priority ?? NO_PRIORITY}
          onValueChange={(value) => onSetPriority(value === NO_PRIORITY ? null : (value as TaskPriority))}
          options={prioritySelectOptions}
          pending={pendingDetail('priority')}
          disabled={command.isPending}
        />

        <EpicAssignmentField
          epics={epics.data ?? []}
          selectedEpicId={epicId}
          taskId={task.id}
          disabled={!task.projectId || epics.isPending}
        />

        <PropertySelect
          label="Project"
          ariaLabel="Task project"
          value={task.projectId ?? NONE}
          onValueChange={(value) => onAssignProject(value === NONE ? null : value)}
          options={projectOptions}
          pending={pendingField('project')}
          disabled={command.isPending}
        />

        <PropertySelect
          label="Milestone"
          ariaLabel="Task milestone"
          value={task.milestoneId ?? NONE}
          onValueChange={(value) => onAssignMilestone(value === NONE ? null : value)}
          options={milestoneOptions}
          pending={pendingField('milestone')}
          disabled={command.isPending || !task.projectId || milestonesLoading}
          placeholder={milestonesLoading ? 'Loading…' : 'No milestone'}
          hint={
            !task.projectId ? (
              <span className="text-2xs leading-4 text-ink-muted">
                Assign a project before choosing a milestone.
              </span>
            ) : undefined
          }
        />
      </div>

      <div className="mt-4">
        <LabelsEditor
          labels={task.labels}
          pending={pendingDetail('labels')}
          disabled={command.isPending}
          onCommit={(labels) => onSetLabels(labels)}
        />
      </div>

      <FieldError error={propertyError} onRefresh={onRefresh} />
      <SavingNote visible={pendingField('status') || pendingField('project') || pendingField('milestone') || pendingDetail('priority') || pendingDetail('labels')} />
      {errorType === 'assign_project' && error && userMessage(error).includes('milestone') ? (
        <p className="mt-1.5 text-2xs leading-4 text-ink-muted">
          Remove the milestone first, then move the task to another project.
        </p>
      ) : null}
    </Section>
  )
}
