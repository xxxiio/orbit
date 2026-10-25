/**
 * Task commands: one mutation instance for the whole task family, plus create.
 *
 * Every command carries the revision read at mutation time and the command id
 * the caller minted; accepted responses write the task record and the ledger
 * revision back before the invalidation settles.
 */
import type { QueryKey } from '@tanstack/react-query'
import { ClientStateError } from '../errors'
import { queryKeys } from '../queries'
import { newCommandId } from '../ids'
import { acceptLedgerRevision, acceptTaskRecord } from '../accepted-write'
import type {
  AcceptanceCriteriaAction,
  TaskAction,
  TaskCommand,
  TaskPriority,
  TaskResult,
  TaskStatus,
} from '../contracts'
import { sendTaskCommand } from '../endpoints/tasks'
import { toTask, type Task } from '../adapters'
import { snapshotRevision, useApiMutation, type CommandMutation } from './core'

/** Revision-derived reads that a domain write makes stale. */
function derivedTaskKeys(): readonly QueryKey[] {
  return [
    queryKeys.snapshot(),
    // A task write advances the same ledger revision the milestone snapshot
    // fences on, so that read must be refreshed alongside the task reads.
    queryKeys.milestones(),
    queryKeys.taskReadinessAll(),
    queryKeys.nextActionsAll(),
    queryKeys.dayPlanAll(),
    queryKeys.taskGraphAll(),
    queryKeys.projectDependenciesAll(),
    queryKeys.workBriefAll(),
    queryKeys.schedulePlanAll(),
    queryKeys.portfolioCapacityAll(),
    queryKeys.workloadAll(),
  ]
}

export interface AcceptanceCriteriaChangeAdd {
  operation: 'add'
  criterionId: string
  text: string
}
export interface AcceptanceCriteriaChangeSetText {
  operation: 'set_text'
  criterionId: string
  text: string
}
export interface AcceptanceCriteriaChangeSetCompleted {
  operation: 'set_completed'
  criterionId: string
  completed: boolean
}
export interface AcceptanceCriteriaChangeReorder {
  operation: 'reorder'
  criterionId: string
  /** 0-based, matches the server's `usize`. */
  position: number
}
export interface AcceptanceCriteriaChangeRemove {
  operation: 'remove'
  criterionId: string
}

export type AcceptanceCriteriaChange =
  | AcceptanceCriteriaChangeAdd
  | AcceptanceCriteriaChangeSetText
  | AcceptanceCriteriaChangeSetCompleted
  | AcceptanceCriteriaChangeReorder
  | AcceptanceCriteriaChangeRemove

export type TaskCommandInput =
  | { type: 'rename'; taskId: string; title: string }
  | { type: 'change_status'; taskId: string; status: TaskStatus }
  | { type: 'assign_project'; taskId: string; projectId: string | null }
  | { type: 'assign_milestone'; taskId: string; milestoneId: string | null }
  | {
      type: 'set_planning'
      taskId: string
      estimatedMinutes: number | null
      earliestStartAtMs: number | null
      deadlineAtMs: number | null
    }
  | { type: 'add_prerequisite'; taskId: string; prerequisiteId: string }
  | { type: 'remove_prerequisite'; taskId: string; prerequisiteId: string }
  | {
      type: 'set_details'
      taskId: string
      description: string
      priority: TaskPriority | null
      labels: string[]
    }
  | { type: 'acceptance_criteria'; taskId: string; change: AcceptanceCriteriaChange }

function toWireChange(change: AcceptanceCriteriaChange): AcceptanceCriteriaAction {
  if (change.operation === 'reorder') {
    // Shape guard only: the server owns ordering rules.
    if (!Number.isInteger(change.position) || change.position < 0) {
      throw new ClientStateError('That position is not valid. Choose a position in the list.')
    }
  }
  if (change.operation === 'add' || change.operation === 'set_text') {
    if (change.text.trim().length === 0) {
      throw new ClientStateError('An acceptance criterion needs text.')
    }
  }

  switch (change.operation) {
    case 'add':
      return { operation: 'add', criterion_id: change.criterionId, text: change.text }
    case 'set_text':
      return { operation: 'set_text', criterion_id: change.criterionId, text: change.text }
    case 'set_completed':
      return {
        operation: 'set_completed',
        criterion_id: change.criterionId,
        completed: change.completed,
      }
    case 'reorder':
      return { operation: 'reorder', criterion_id: change.criterionId, position: change.position }
    case 'remove':
      return { operation: 'remove', criterion_id: change.criterionId }
  }
}

function toWireTaskAction(input: TaskCommandInput): TaskAction {
  switch (input.type) {
    case 'rename':
      if (input.title.trim().length === 0) throw new ClientStateError('A task needs a title.')
      return { type: 'rename', task_id: input.taskId, title: input.title }
    case 'change_status':
      return { type: 'change_status', task_id: input.taskId, status: input.status }
    case 'assign_project':
      return { type: 'assign_project', task_id: input.taskId, project_id: input.projectId }
    case 'assign_milestone':
      return { type: 'assign_milestone', task_id: input.taskId, milestone_id: input.milestoneId }
    case 'set_planning':
      // The endpoint replaces the whole planning window: every field is sent,
      // and `null` clears (that is the server's stated contract).
      return {
        type: 'set_planning',
        task_id: input.taskId,
        estimated_minutes: input.estimatedMinutes,
        earliest_start_at_ms: input.earliestStartAtMs,
        deadline_at_ms: input.deadlineAtMs,
      }
    case 'add_prerequisite':
      return {
        type: 'add_prerequisite',
        task_id: input.taskId,
        prerequisite_id: input.prerequisiteId,
      }
    case 'remove_prerequisite':
      return {
        type: 'remove_prerequisite',
        task_id: input.taskId,
        prerequisite_id: input.prerequisiteId,
      }
    case 'set_details':
      return {
        type: 'set_details',
        task_id: input.taskId,
        description: input.description,
        priority: input.priority,
        labels: input.labels,
      }
    case 'acceptance_criteria':
      return {
        type: 'acceptance_criteria',
        task_id: input.taskId,
        change: toWireChange(input.change),
      }
  }
}

export interface TaskMutationOutcome {
  revision: number
  duplicate: boolean
  task: Task
}

function toTaskOutcome(result: TaskResult): TaskMutationOutcome {
  return { revision: result.revision, duplicate: result.duplicate, task: toTask(result.task) }
}

/** Invalidation for anything that changes one task's stored record. */
function taskKeys(taskId: string): readonly QueryKey[] {
  return [
    ...derivedTaskKeys(),
    queryKeys.task(taskId),
    queryKeys.taskActivity(taskId),
    queryKeys.taskAcceptanceCriteria(taskId),
  ]
}

/**
 * A task command plus the idempotency key its caller minted. The key is part of
 * the variables (not generated inside `mutationFn`) so a retry of the same
 * intent replays the server's receipt instead of writing twice.
 */
export interface TaskCommandAttempt {
  commandId: string
  input: TaskCommandInput
}

/** Attach a fresh idempotency key to one command intent. */
export function taskCommandAttempt(input: TaskCommandInput): TaskCommandAttempt {
  return { commandId: newCommandId(), input }
}

/**
 * The whole task command family as one mutation instance. `isPending` is shared
 * (only one command can be in flight at a time); use `pendingCommand` to show a
 * spinner on the row that is actually being written.
 */
export function useTaskCommand(): CommandMutation<TaskMutationOutcome, TaskCommandAttempt> & {
  pendingCommand: TaskCommandInput['type'] | null
  renameTask: (input: { taskId: string; title: string }) => void
  changeStatus: (input: { taskId: string; status: TaskStatus }) => void
  assignProject: (input: { taskId: string; projectId: string | null }) => void
  assignMilestone: (input: { taskId: string; milestoneId: string | null }) => void
  setPlanning: (input: {
    taskId: string
    estimatedMinutes: number | null
    earliestStartAtMs: number | null
    deadlineAtMs: number | null
  }) => void
  addPrerequisite: (input: { taskId: string; prerequisiteId: string }) => void
  removePrerequisite: (input: { taskId: string; prerequisiteId: string }) => void
  setDetails: (input: {
    taskId: string
    description: string
    priority: TaskPriority | null
    labels: string[]
  }) => void
  acceptanceCriteria: (input: { taskId: string; change: AcceptanceCriteriaChange }) => void
  addCriterion: (input: { taskId: string; criterionId: string; text: string }) => void
  setCriterionText: (input: { taskId: string; criterionId: string; text: string }) => void
  setCriterionCompleted: (input: {
    taskId: string
    criterionId: string
    completed: boolean
  }) => void
  reorderCriterion: (input: { taskId: string; criterionId: string; position: number }) => void
  removeCriterion: (input: { taskId: string; criterionId: string }) => void
} {
  const mutation = useApiMutation<TaskMutationOutcome, TaskCommandAttempt>({
    // A lost response does not prove the command was not applied.
    uncertainOnError: true,
    mutationFn: (attempt, queryClient) => {
      const command: TaskCommand = {
        command_id: attempt.commandId,
        expected_revision: snapshotRevision(queryClient),
        action: toWireTaskAction(attempt.input),
      }
      return sendTaskCommand(command).then(toTaskOutcome)
    },
    affectedKeys: (attempt) => taskKeys(attempt.input.taskId),
    onAccepted: (outcome, _attempt, queryClient) => {
      acceptTaskRecord(queryClient, outcome.task, outcome.revision)
      acceptLedgerRevision(queryClient, outcome.revision)
    },
  })

  const send = (input: TaskCommandInput) => mutation.mutate(taskCommandAttempt(input))

  return {
    ...mutation,
    pendingCommand: mutation.isPending ? (mutation.variables?.input.type ?? null) : null,
    renameTask: (input) => send({ type: 'rename', ...input }),
    changeStatus: (input) => send({ type: 'change_status', ...input }),
    assignProject: (input) => send({ type: 'assign_project', ...input }),
    assignMilestone: (input) => send({ type: 'assign_milestone', ...input }),
    setPlanning: (input) => send({ type: 'set_planning', ...input }),
    addPrerequisite: (input) => send({ type: 'add_prerequisite', ...input }),
    removePrerequisite: (input) => send({ type: 'remove_prerequisite', ...input }),
    setDetails: (input) => send({ type: 'set_details', ...input }),
    acceptanceCriteria: (input) => send({ type: 'acceptance_criteria', ...input }),
    addCriterion: (input) =>
      send({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'add', criterionId: input.criterionId, text: input.text },
      }),
    setCriterionText: (input) =>
      send({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'set_text', criterionId: input.criterionId, text: input.text },
      }),
    setCriterionCompleted: (input) =>
      send({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: {
          operation: 'set_completed',
          criterionId: input.criterionId,
          completed: input.completed,
        },
      }),
    reorderCriterion: (input) =>
      send({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'reorder', criterionId: input.criterionId, position: input.position },
      }),
    removeCriterion: (input) =>
      send({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'remove', criterionId: input.criterionId },
      }),
  }
}

/**
 * Everything the server needs to create one task, including both ids. The
 * dialog or capture field allocates them once per attempt, so pressing "Create"
 * again after an unconfirmed failure replays the same command instead of
 * producing a second task.
 */
export interface CreateTaskVariables {
  commandId: string
  taskId: string
  title: string
  projectId?: string
}

export function useCreateTask(): CommandMutation<TaskMutationOutcome, CreateTaskVariables> {
  return useApiMutation<TaskMutationOutcome, CreateTaskVariables>({
    uncertainOnError: true,
    mutationFn: ({ commandId, taskId, title, projectId }, queryClient) => {
      if (title.trim().length === 0) throw new ClientStateError('A task needs a title.')
      const command: TaskCommand = {
        command_id: commandId,
        expected_revision: snapshotRevision(queryClient),
        action: {
          type: 'create',
          task_id: taskId,
          title,
          ...(projectId ? { project_id: projectId } : {}),
        },
      }
      return sendTaskCommand(command).then(toTaskOutcome)
    },
    affectedKeys: () => derivedTaskKeys(),
    onAccepted: (outcome, _variables, queryClient) => {
      acceptTaskRecord(queryClient, outcome.task, outcome.revision)
      acceptLedgerRevision(queryClient, outcome.revision)
    },
  })
}
