/**
 * Command hooks.
 *
 * Three rules are enforced by construction:
 *
 * 1. **Nothing is shown as saved before the server answered.** There is no
 *    optimistic update and no rollback theatre: a mutation has a per-instance
 *    `isPending`, and `disposition` only becomes `accepted` after a response.
 * 2. **Revisions are read at mutation time, and accepted ones are written back.**
 *    `expected_revision` comes from the query cache inside `mutationFn`, so a
 *    snapshot refresh between render and click cannot produce a stale fence.
 *    After an accepted write the response's revision becomes the cache's new
 *    floor (`Math.max`), so a second command in the same interaction fences on
 *    the revision the server just reported instead of the pre-write one. The
 *    client never decides what a legal transition is; it sends the command and
 *    renders the server's answer.
 * 3. **Idempotency keys are allocated by the caller.** `command_id` (and the
 *    entity id for creates) travel in the mutation variables, minted when the
 *    person acts, so pressing the same button again after an unconfirmed
 *    failure replays the same receipt instead of creating a second entity.
 * 4. **Failures are honest.** A rejected command is `rejected`; a write whose
 *    outcome the server may have applied anyway (lost response, 5xx, knowledge
 *    filesystem publication) is `uncertain`, never silently retried.
 */
import { useMutation, useQueryClient, type QueryClient, type QueryKey } from '@tanstack/react-query'
import {
  ApiError,
  ClientStateError,
  classifyError,
  errorInfo,
  isAmbiguousError,
} from './errors'
import { handleUnauthorized } from './session-store'
import { queryKeys } from './queries'
import {
  toApprovedScheduleReceipt,
  toAttachment,
  toMilestoneResult,
  toProject,
  toTask,
  toWorkItem,
  toWorkflowUpgradeApplied,
  type ApprovedScheduleOutcome,
  type Attachment,
  type MilestoneMutationOutcome,
  type Project,
  type Task,
  type WorkItem,
  type WorkflowUpgradeApplied,
} from './adapters'
import type {
  AcceptanceCriteriaAction,
  ApplyUpgradeBody,
  AttachmentImportResult,
  AttachmentOwnerKind,
  AuthoritySnapshotResponse,
  BlueprintUpgradePreview,
  BlueprintWorkItemRecord,
  CreateWorkItemBody,
  MarkdownCommand,
  MarkdownReceipt,
  MilestoneSnapshot,
  MilestoneStatus,
  PlanReview,
  ProjectCommand,
  ProjectResult,
  ScheduleApprovalRequest,
  TaskAction,
  TaskCommand,
  TaskPriority,
  TaskResult,
  TaskStatus,
  WorkItemPropertiesBody,
  WorkItemTransitionBody,
} from './contracts'
import { newCommandId } from './ids'
import { sendTaskCommand } from './endpoints/tasks'
import { sendProjectCommand } from './endpoints/projects'
import {
  changeMilestoneStatus,
  createMilestone,
  deleteMilestone,
  moveMilestoneProject,
  renameMilestone,
} from './endpoints/milestones'
import {
  applyApprovedPlan,
  reviewPlan,
  type ReviewedSchedulePlan,
} from './endpoints/planning'
import {
  applyUpgrade,
  createWorkItem,
  transitionWorkItem,
  updateWorkItemProperties,
} from './endpoints/workflows'
import { importAttachment } from './endpoints/attachments'
import { sendMarkdownCommand } from './endpoints/knowledge'

export { newCommandId } from './ids'

/* ------------------------------------------------------------------ */
/* Disposition                                                         */
/* ------------------------------------------------------------------ */

/**
 * `idle`      nothing sent yet
 * `pending`   in flight; show a spinner, not a checkmark
 * `accepted`  the server accepted and stored the change
 * `rejected`  the server refused; nothing was written
 * `uncertain` the request failed in a way that may still have applied
 */
export type MutationDisposition = 'idle' | 'pending' | 'accepted' | 'rejected' | 'uncertain'

export interface CommandMutation<TData, TVariables> {
  mutate: (variables: TVariables) => void
  mutateAsync: (variables: TVariables) => Promise<TData>
  isPending: boolean
  error: ApiError | null
  disposition: MutationDisposition
  data: TData | undefined
  /** The variables of the latest attempt; useful for per-row pending states. */
  variables: TVariables | undefined
  /** Present for a 429 when the response body carries a retry window. */
  retryAfterSeconds: number | null
  reset: () => void
}

interface ApiMutationOptions<TData, TVariables> {
  mutationFn: (variables: TVariables, queryClient: QueryClient) => Promise<TData>
  /** Invalidated on acceptance, and refetched on a conflict. */
  affectedKeys?: (variables: TVariables) => readonly QueryKey[]
  /** The server can apply the write and still return an error (imports, knowledge). */
  uncertainOnError?: boolean
  onAccepted?: (data: TData, variables: TVariables, queryClient: QueryClient) => void
}

/** Errors whose meaning is "we do not know whether the write landed". */
function dispositionOf(
  error: ApiError | null,
  isPending: boolean,
  isSuccess: boolean,
  uncertainOnError: boolean,
): MutationDisposition {
  if (isPending) return 'pending'
  if (isSuccess) return 'accepted'
  if (!error) return 'idle'
  if (uncertainOnError && isAmbiguousError(error)) return 'uncertain'
  return 'rejected'
}

export function useApiMutation<TData, TVariables>({
  mutationFn,
  affectedKeys,
  uncertainOnError = false,
  onAccepted,
}: ApiMutationOptions<TData, TVariables>): CommandMutation<TData, TVariables> {
  const queryClient = useQueryClient()

  const invalidate = (keys: readonly QueryKey[] | undefined) => {
    for (const queryKey of keys ?? []) {
      void queryClient.invalidateQueries({ queryKey })
    }
  }

  const mutation = useMutation<TData, ApiError, TVariables>({
    mutationFn: async (variables) => {
      try {
        return await mutationFn(variables, queryClient)
      } catch (error) {
        if (error instanceof ApiError) throw error
        throw new ApiError(0, 'Could not reach the Orbit server.', error)
      }
    },
    onSuccess: (data, variables) => {
      onAccepted?.(data, variables, queryClient)
      invalidate(affectedKeys?.(variables))
    },
    onError: (error, variables) => {
      const kind = classifyError(error)
      if (kind === 'unauthorized') {
        handleUnauthorized(queryClient)
        return
      }
      if (kind === 'conflict') {
        // The server's message explains the conflict; refetch so the next
        // attempt is fenced against the version that actually won.
        invalidate(affectedKeys?.(variables))
      }
    },
  })

  return {
    mutate: mutation.mutate,
    mutateAsync: mutation.mutateAsync,
    isPending: mutation.isPending,
    error: mutation.error ?? null,
    disposition: dispositionOf(mutation.error ?? null, mutation.isPending, mutation.isSuccess, uncertainOnError),
    data: mutation.data,
    variables: mutation.variables,
    retryAfterSeconds: mutation.error ? errorInfo(mutation.error).retryAfterSeconds : null,
    reset: mutation.reset,
  }
}

/* ------------------------------------------------------------------ */
/* Identifiers + revisions                                             */
/* ------------------------------------------------------------------ */

/**
 * Write the revision an accepted response carries back into the snapshot it
 * fences on, never lowering what is already cached: a duplicate receipt carries
 * the revision the *original* command had, which can be older than the tip this
 * browser already knows about.
 *
 * Without this, two commands in one interaction (a bulk loop, status then
 * priority in the drawer) would both fence on the pre-write revision and the
 * server would correctly reject the second as stale.
 */
export function acceptRevision(
  queryClient: QueryClient,
  queryKey: QueryKey,
  revision: number,
): void {
  queryClient.setQueryData<{ revision: number }>(queryKey, (previous) =>
    previous ? { ...previous, revision: Math.max(previous.revision, revision) } : previous,
  )
}

/**
 * The freshest revision this browser knows about. Read inside `mutationFn`,
 * never captured at mount: a snapshot refresh between render and click must
 * fence the command against the new revision.
 */
export function snapshotRevision(queryClient: QueryClient): number {
  const snapshot = queryClient.getQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot())
  if (!snapshot) {
    throw new ClientStateError('The workspace is still loading. Try again in a moment.')
  }
  return snapshot.revision
}

/**
 * Milestones have their own snapshot revision (they are not fenced by the task
 * snapshot). Read it at mutation time so a refreshed milestone list always wins.
 */
export function milestoneRevision(queryClient: QueryClient): number {
  const snapshot = queryClient.getQueryData<MilestoneSnapshot>(queryKeys.milestones())
  if (!snapshot) {
    throw new ClientStateError('The milestone list is still loading. Try again in a moment.')
  }
  return snapshot.revision
}

/** Work items fence on their own `u64` revision, found in the freshest cached list. */
export function workItemRevision(queryClient: QueryClient, id: string): number {
  const lists = queryClient.getQueriesData<BlueprintWorkItemRecord[]>({
    queryKey: queryKeys.workItemsAll(),
  })
  for (const [, items] of lists) {
    const item = items?.find((candidate) => candidate.id === id)
    if (item) return item.revision
  }
  throw new ClientStateError('Reload this project before changing its work items.')
}

/** Revision-derived reads that a domain write makes stale. */
function derivedTaskKeys(): readonly QueryKey[] {
  return [
    queryKeys.snapshot(),
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

/* ------------------------------------------------------------------ */
/* Tasks                                                               */
/* ------------------------------------------------------------------ */

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
      acceptRevision(queryClient, queryKeys.snapshot(), outcome.revision)
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
      acceptRevision(queryClient, queryKeys.snapshot(), outcome.revision)
    },
  })
}

/* ------------------------------------------------------------------ */
/* Projects                                                            */
/* ------------------------------------------------------------------ */

export interface ProjectMutationOutcome {
  revision: number
  duplicate: boolean
  project: Project
}

function toProjectOutcome(result: ProjectResult): ProjectMutationOutcome {
  return { revision: result.revision, duplicate: result.duplicate, project: toProject(result.project) }
}

/**
 * Creation is the only project write that exists. `workflowId` is only read on
 * the first accepted create, where it pins the binding in the same transaction.
 * Both ids come from the dialog's attempt, so a retry replays the receipt.
 */
export interface CreateProjectVariables {
  commandId: string
  projectId: string
  name: string
  workflowId?: string
}

export function useCreateProject(): CommandMutation<ProjectMutationOutcome, CreateProjectVariables> {
  return useApiMutation<ProjectMutationOutcome, CreateProjectVariables>({
    uncertainOnError: true,
    mutationFn: ({ commandId, projectId, name, workflowId }, queryClient) => {
      if (name.trim().length === 0) throw new ClientStateError('A project needs a name.')
      const command: ProjectCommand = {
        command_id: commandId,
        expected_revision: snapshotRevision(queryClient),
        project_id: projectId,
        name,
        ...(workflowId ? { workflow_id: workflowId } : {}),
      }
      return sendProjectCommand(command).then(toProjectOutcome)
    },
    affectedKeys: () => [queryKeys.snapshot()],
    onAccepted: (outcome, _variables, queryClient) => {
      acceptRevision(queryClient, queryKeys.snapshot(), outcome.revision)
    },
  })
}

/* ------------------------------------------------------------------ */
/* Milestones                                                          */
/* ------------------------------------------------------------------ */

export type MilestoneCommandInput =
  | { type: 'create'; projectId: string; name: string; milestoneId: string }
  | { type: 'rename'; milestoneId: string; name: string }
  | { type: 'change_status'; milestoneId: string; status: MilestoneStatus }
  | { type: 'move_project'; milestoneId: string; projectId: string }
  | { type: 'delete'; milestoneId: string }

/** A milestone command plus the idempotency key its caller minted. */
export interface MilestoneCommandAttempt {
  commandId: string
  input: MilestoneCommandInput
}

/**
 * The whole milestone command family as one mutation instance, matching
 * `useTaskCommand`. `expected_revision` is read from the milestones snapshot
 * inside `mutationFn`, the accepted revision is written back, and `command_id`
 * (plus `milestoneId` for creates) is allocated by the caller so a retry
 * replays the receipt. Status edges are server-enforced; this hook only sends
 * the requested status.
 */
export function useMilestoneCommand(): CommandMutation<
  MilestoneMutationOutcome,
  MilestoneCommandAttempt
> & {
  pendingCommand: MilestoneCommandInput['type'] | null
  createMilestone: (input: { projectId: string; name: string; milestoneId?: string }) => void
  renameMilestone: (input: { milestoneId: string; name: string }) => void
  changeMilestoneStatus: (input: { milestoneId: string; status: MilestoneStatus }) => void
  moveMilestoneProject: (input: { milestoneId: string; projectId: string }) => void
  deleteMilestone: (input: { milestoneId: string }) => void
} {
  const mutation = useApiMutation<MilestoneMutationOutcome, MilestoneCommandAttempt>({
    uncertainOnError: true,
    mutationFn: (attempt, queryClient) => {
      const expectedRevision = milestoneRevision(queryClient)
      const { commandId, input } = attempt
      switch (input.type) {
        case 'create':
          return createMilestone(
            {
              milestoneId: input.milestoneId,
              projectId: input.projectId,
              name: input.name,
            },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'rename':
          return renameMilestone(
            { milestoneId: input.milestoneId, name: input.name },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'change_status':
          return changeMilestoneStatus(
            { milestoneId: input.milestoneId, status: input.status },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'move_project':
          return moveMilestoneProject(
            { milestoneId: input.milestoneId, projectId: input.projectId },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'delete':
          return deleteMilestone(
            { milestoneId: input.milestoneId },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
      }
    },
    affectedKeys: () => [queryKeys.milestones(), queryKeys.snapshot()],
    onAccepted: (outcome, _attempt, queryClient) => {
      acceptRevision(queryClient, queryKeys.milestones(), outcome.revision)
      acceptRevision(queryClient, queryKeys.snapshot(), outcome.revision)
    },
  })

  const send = (input: MilestoneCommandInput) =>
    mutation.mutate({ commandId: newCommandId(), input })

  return {
    ...mutation,
    pendingCommand: mutation.isPending ? (mutation.variables?.input.type ?? null) : null,
    createMilestone: (input) =>
      send({
        type: 'create',
        projectId: input.projectId,
        name: input.name,
        milestoneId: input.milestoneId ?? newCommandId(),
      }),
    renameMilestone: (input) => send({ type: 'rename', ...input }),
    changeMilestoneStatus: (input) => send({ type: 'change_status', ...input }),
    moveMilestoneProject: (input) => send({ type: 'move_project', ...input }),
    deleteMilestone: (input) => send({ type: 'delete', ...input }),
  }
}

/* ------------------------------------------------------------------ */
/* Workflows                                                           */
/* ------------------------------------------------------------------ */

/**
 * Confirm an upgrade. The fingerprint is read from the freshest cached preview
 * at mutation time; without a preview nothing is sent, because applying a
 * version the person has not been shown is exactly what the server forbids.
 */
export function useApplyUpgrade(): CommandMutation<
  WorkflowUpgradeApplied,
  { projectId: string; workflowId: string; version: number }
> {
  return useApiMutation<
    WorkflowUpgradeApplied,
    { projectId: string; workflowId: string; version: number }
  >({
    // The version may have been applied even if the response was lost.
    uncertainOnError: true,
    mutationFn: ({ projectId, workflowId, version }, queryClient) => {
      const preview = queryClient.getQueryData<BlueprintUpgradePreview>(
        queryKeys.upgradePreview(projectId, workflowId, version),
      )
      if (!preview) {
        throw new ClientStateError('Open the upgrade preview before applying it.')
      }
      const body: ApplyUpgradeBody = {
        project_id: projectId,
        id: workflowId,
        version,
        confirm: true,
        fingerprint: preview.fingerprint,
      }
      return applyUpgrade(body).then(toWorkflowUpgradeApplied)
    },
    affectedKeys: (variables) => [
      queryKeys.upgradePreview(variables.projectId, variables.workflowId, variables.version),
      queryKeys.upgradePreviewAll(),
      queryKeys.workflowBinding(variables.projectId),
      queryKeys.workItems(variables.projectId),
    ],
  })
}

export function useCreateWorkItem(): CommandMutation<
  WorkItem,
  { projectId: string; kind: string; properties?: Record<string, unknown> }
> {
  return useApiMutation<
    WorkItem,
    { projectId: string; kind: string; properties?: Record<string, unknown> }
  >({
    // The item may exist even when the response never arrived.
    uncertainOnError: true,
    mutationFn: ({ projectId, kind, properties }) => {
      const body: CreateWorkItemBody = {
        id: newCommandId(),
        project_id: projectId,
        kind,
        ...(properties ? { properties } : {}),
      }
      return createWorkItem(body).then(toWorkItem)
    },
    affectedKeys: (variables) => [
      queryKeys.workItems(variables.projectId),
      queryKeys.upgradePreviewAll(),
    ],
  })
}

/** Transition legality, guards and capabilities are decided by the server. */
export function useWorkItemTransition(): CommandMutation<WorkItem, { id: string; to: string }> {
  return useApiMutation<WorkItem, { id: string; to: string }>({
    uncertainOnError: true,
    mutationFn: ({ id, to }, queryClient) => {
      const body: WorkItemTransitionBody = {
        id,
        to,
        expected_revision: workItemRevision(queryClient, id),
      }
      return transitionWorkItem(body).then(toWorkItem)
    },
    affectedKeys: () => [queryKeys.workItemsAll(), queryKeys.upgradePreviewAll()],
  })
}

export function useUpdateWorkItemProperties(): CommandMutation<
  WorkItem,
  { id: string; changes: Record<string, unknown> }
> {
  return useApiMutation<WorkItem, { id: string; changes: Record<string, unknown> }>({
    uncertainOnError: true,
    mutationFn: ({ id, changes }, queryClient) => {
      const body: WorkItemPropertiesBody = {
        id,
        expected_revision: workItemRevision(queryClient, id),
        changes,
      }
      return updateWorkItemProperties(body).then(toWorkItem)
    },
    affectedKeys: () => [queryKeys.workItemsAll(), queryKeys.upgradePreviewAll()],
  })
}

/* ------------------------------------------------------------------ */
/* Attachments                                                         */
/* ------------------------------------------------------------------ */

export interface ImportAttachmentVariables {
  ownerKind: AttachmentOwnerKind | string
  ownerId: string
  file: Blob | ArrayBuffer | Uint8Array
  filename: string
  contentType?: string
  description?: string
}

export interface ImportAttachmentOutcome {
  revision: number
  duplicate: boolean
  removed: boolean
  attachment: Attachment | null
}

/**
 * Raw-body upload. The server may store the file and still answer with an
 * error (the receipt is published after the bytes land), so this mutation is
 * `uncertain` on transport/5xx failures: re-read the attachment list instead of
 * blindly uploading again.
 */
export function useImportAttachment(): CommandMutation<
  ImportAttachmentOutcome,
  ImportAttachmentVariables
> {
  return useApiMutation<ImportAttachmentOutcome, ImportAttachmentVariables>({
    uncertainOnError: true,
    mutationFn: (variables, queryClient) => {
      const metadata = {
        command_id: newCommandId(),
        expected_revision: snapshotRevision(queryClient),
        attachment_id: newCommandId(),
        activity_id: newCommandId(),
        owner_kind: variables.ownerKind,
        owner_id: variables.ownerId,
        filename: variables.filename,
        content_type: variables.contentType,
        description: variables.description,
      }
      return importAttachment(metadata, variables.file).then(
        (result: AttachmentImportResult): ImportAttachmentOutcome => ({
          revision: result.revision,
          duplicate: result.duplicate,
          removed: result.removed,
          attachment: result.attachment ? toAttachment(result.attachment) : null,
        }),
      )
    },
    affectedKeys: (variables) => [
      queryKeys.snapshot(),
      queryKeys.attachments(variables.ownerKind, variables.ownerId),
      ...(variables.ownerKind === 'task' ? [queryKeys.task(variables.ownerId)] : []),
      ...(variables.ownerKind === 'task'
        ? [queryKeys.taskActivity(variables.ownerId)]
        : []),
    ],
    onAccepted: (outcome, _variables, queryClient) => {
      acceptRevision(queryClient, queryKeys.snapshot(), outcome.revision)
    },
  })
}

/* ------------------------------------------------------------------ */
/* Knowledge                                                           */
/* ------------------------------------------------------------------ */

/**
 * Knowledge writes are published to disk before the receipt is accepted, so an
 * error does not prove nothing changed. Treat it as `uncertain` and re-read the
 * document or manifest before retrying with a fresh command id.
 */
export function useMarkdownCommand(): CommandMutation<
  MarkdownReceipt,
  { command: MarkdownCommand }
> {
  return useApiMutation<MarkdownReceipt, { command: MarkdownCommand }>({
    uncertainOnError: true,
    mutationFn: ({ command }) =>
      sendMarkdownCommand({ command_id: newCommandId(), command }),
    affectedKeys: () => [queryKeys.knowledgeAll()],
  })
}

/* ------------------------------------------------------------------ */
/* Explicit plan review + approval                                     */
/* ------------------------------------------------------------------ */

/**
 * The review is a read-only `POST` (it needs the same CSRF/session path), so it
 * deliberately does not use the mutation disposition model: there is no
 * `accepted`, because nothing was written. `review` is the server's `PlanReview`
 * verbatim, including `fingerprint`, `command_id` and
 * `requires_explicit_approval`; `reviewedPlan` keeps that review paired with the
 * exact command it reviewed so it can be applied without being rebuilt.
 */
export interface PlanReviewResult {
  reviewedPlan: ReviewedSchedulePlan | null
  review: PlanReview | null
  fingerprint: string | null
  requiresExplicitApproval: boolean | null
  isPending: boolean
  error: ApiError | null
  reviewPlan: (request: ScheduleApprovalRequest) => void
  reviewPlanAsync: (request: ScheduleApprovalRequest) => Promise<ReviewedSchedulePlan>
  reset: () => void
}

export function usePlanReview(): PlanReviewResult {
  const queryClient = useQueryClient()
  const mutation = useMutation<ReviewedSchedulePlan, ApiError, ScheduleApprovalRequest>({
    mutationFn: async (request) => {
      try {
        const review = await reviewPlan(request)
        return { plan: request, review }
      } catch (error) {
        if (error instanceof ApiError) throw error
        throw new ApiError(0, 'Could not reach the Orbit server.', error)
      }
    },
    onError: (error) => {
      if (classifyError(error) === 'unauthorized') {
        handleUnauthorized(queryClient)
      }
    },
  })

  return {
    reviewedPlan: mutation.data ?? null,
    review: mutation.data?.review ?? null,
    fingerprint: mutation.data?.review.fingerprint ?? null,
    requiresExplicitApproval: mutation.data?.review.requires_explicit_approval ?? null,
    isPending: mutation.isPending,
    error: mutation.error ?? null,
    reviewPlan: (request) => mutation.mutate(request),
    reviewPlanAsync: mutation.mutateAsync,
    reset: mutation.reset,
  }
}

/**
 * Apply exactly the reviewed command. The variables carry the `PlanReview`, so
 * there is no way to apply a plan that was not reviewed in this session; the
 * endpoint refuses locally (typed error) when the review is missing, has no
 * fingerprint, or is not marked `requires_explicit_approval`. The command id is
 * echoed from the review and never regenerated, and a replayed receipt surfaces
 * `replayed: true`.
 */
export function useApplyApprovedPlan(): CommandMutation<
  ApprovedScheduleOutcome,
  ReviewedSchedulePlan | null
> {
  return useApiMutation<ApprovedScheduleOutcome, ReviewedSchedulePlan | null>({
    // The write may have landed even if the response was lost.
    uncertainOnError: true,
    mutationFn: (reviewed) => applyApprovedPlan(reviewed).then(toApprovedScheduleReceipt),
    affectedKeys: () => [
      queryKeys.snapshot(),
      queryKeys.taskReadinessAll(),
      queryKeys.nextActionsAll(),
      queryKeys.taskGraphAll(),
      queryKeys.dayPlanAll(),
      queryKeys.schedulePlanAll(),
      queryKeys.workBriefAll(),
      queryKeys.portfolioCapacityAll(),
      queryKeys.workloadAll(),
    ],
    onAccepted: (outcome, _reviewed, queryClient) => {
      acceptRevision(queryClient, queryKeys.snapshot(), outcome.acceptedRevision)
    },
  })
}
