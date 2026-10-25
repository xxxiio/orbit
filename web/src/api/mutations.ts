/**
 * Command hooks.
 *
 * Three rules are enforced by construction:
 *
 * 1. **Nothing is shown as saved before the server answered.** There is no
 *    optimistic update and no rollback theatre: a mutation has a per-instance
 *    `isPending`, and `disposition` only becomes `accepted` after a response.
 * 2. **Revisions are read at mutation time.** `expected_revision` comes from the
 *    query cache inside `mutationFn`, so a snapshot refresh between render and
 *    click cannot produce a stale fence. The client never decides what a legal
 *    transition is; it sends the command and renders the server's answer.
 * 3. **Failures are honest.** A rejected command is `rejected`; a write whose
 *    outcome the server may have applied anyway (lost response, 5xx, knowledge
 *    filesystem publication) is `uncertain`, never silently retried.
 */
import { useMutation, useQueryClient, type QueryClient, type QueryKey } from '@tanstack/react-query'
import { ApiError, ClientStateError, classifyError, errorInfo } from './errors'
import { handleUnauthorized } from './session-store'
import { queryKeys } from './queries'
import {
  toAttachment,
  toProject,
  toTask,
  toWorkItem,
  toWorkflowBinding,
  toWorkflowUpgradeApplied,
  type Attachment,
  type Project,
  type Task,
  type WorkItem,
  type WorkflowBinding,
  type WorkflowUpgradeApplied,
} from './adapters'
import type {
  AcceptanceCriteriaAction,
  ApplyUpgradeBody,
  AttachmentImportResult,
  AttachmentOwnerKind,
  AuthoritySnapshotResponse,
  BindWorkflowBody,
  BlueprintUpgradePreview,
  BlueprintWorkItemRecord,
  CreateWorkItemBody,
  MarkdownCommand,
  MarkdownReceipt,
  ProjectCommand,
  ProjectResult,
  TaskAction,
  TaskCommand,
  TaskPriority,
  TaskResult,
  TaskStatus,
  WorkItemPropertiesBody,
  WorkItemTransitionBody,
} from './contracts'
import { sendTaskCommand } from './endpoints/tasks'
import { sendProjectCommand } from './endpoints/projects'
import {
  applyUpgrade,
  bindWorkflow,
  createWorkItem,
  transitionWorkItem,
  updateWorkItemProperties,
} from './endpoints/workflows'
import { importAttachment } from './endpoints/attachments'
import { sendMarkdownCommand } from './endpoints/knowledge'

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
const AMBIGUOUS_KINDS = new Set(['offline', 'server', 'rateLimited', 'conflict'])

function dispositionOf(
  error: ApiError | null,
  isPending: boolean,
  isSuccess: boolean,
  uncertainOnError: boolean,
): MutationDisposition {
  if (isPending) return 'pending'
  if (isSuccess) return 'accepted'
  if (!error) return 'idle'
  if (uncertainOnError && AMBIGUOUS_KINDS.has(classifyError(error))) return 'uncertain'
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

/** Idempotency key: 1–128 characters. `crypto.randomUUID()` is 36. */
export function newCommandId(): string {
  const cryptoApi = globalThis.crypto
  if (cryptoApi && typeof cryptoApi.randomUUID === 'function') {
    return cryptoApi.randomUUID()
  }
  return `cmd-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
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
 * The whole task command family as one mutation instance. `isPending` is shared
 * (only one command can be in flight at a time); use `pendingCommand` to show a
 * spinner on the row that is actually being written.
 */
export function useTaskCommand(): CommandMutation<TaskMutationOutcome, TaskCommandInput> & {
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
  const mutation = useApiMutation<TaskMutationOutcome, TaskCommandInput>({
    mutationFn: (input, queryClient) => {
      const command: TaskCommand = {
        command_id: newCommandId(),
        expected_revision: snapshotRevision(queryClient),
        action: toWireTaskAction(input),
      }
      return sendTaskCommand(command).then(toTaskOutcome)
    },
    affectedKeys: (input) => taskKeys(input.taskId),
  })

  return {
    ...mutation,
    pendingCommand: mutation.isPending ? (mutation.variables?.type ?? null) : null,
    renameTask: (input) => mutation.mutate({ type: 'rename', ...input }),
    changeStatus: (input) => mutation.mutate({ type: 'change_status', ...input }),
    assignProject: (input) => mutation.mutate({ type: 'assign_project', ...input }),
    assignMilestone: (input) => mutation.mutate({ type: 'assign_milestone', ...input }),
    setPlanning: (input) => mutation.mutate({ type: 'set_planning', ...input }),
    addPrerequisite: (input) => mutation.mutate({ type: 'add_prerequisite', ...input }),
    removePrerequisite: (input) => mutation.mutate({ type: 'remove_prerequisite', ...input }),
    setDetails: (input) => mutation.mutate({ type: 'set_details', ...input }),
    acceptanceCriteria: (input) => mutation.mutate({ type: 'acceptance_criteria', ...input }),
    addCriterion: (input) =>
      mutation.mutate({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'add', criterionId: input.criterionId, text: input.text },
      }),
    setCriterionText: (input) =>
      mutation.mutate({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'set_text', criterionId: input.criterionId, text: input.text },
      }),
    setCriterionCompleted: (input) =>
      mutation.mutate({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: {
          operation: 'set_completed',
          criterionId: input.criterionId,
          completed: input.completed,
        },
      }),
    reorderCriterion: (input) =>
      mutation.mutate({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'reorder', criterionId: input.criterionId, position: input.position },
      }),
    removeCriterion: (input) =>
      mutation.mutate({
        type: 'acceptance_criteria',
        taskId: input.taskId,
        change: { operation: 'remove', criterionId: input.criterionId },
      }),
  }
}

/** The task id is allocated here because the server requires one in the command. */
export function useCreateTask(): CommandMutation<
  TaskMutationOutcome,
  { title: string; projectId?: string }
> {
  return useApiMutation<TaskMutationOutcome, { title: string; projectId?: string }>({
    mutationFn: ({ title, projectId }, queryClient) => {
      if (title.trim().length === 0) throw new ClientStateError('A task needs a title.')
      const command: TaskCommand = {
        command_id: newCommandId(),
        expected_revision: snapshotRevision(queryClient),
        action: {
          type: 'create',
          task_id: newCommandId(),
          title,
          ...(projectId ? { project_id: projectId } : {}),
        },
      }
      return sendTaskCommand(command).then(toTaskOutcome)
    },
    affectedKeys: () => derivedTaskKeys(),
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
 */
export function useCreateProject(): CommandMutation<
  ProjectMutationOutcome,
  { name: string; workflowId?: string }
> {
  return useApiMutation<ProjectMutationOutcome, { name: string; workflowId?: string }>({
    mutationFn: ({ name, workflowId }, queryClient) => {
      if (name.trim().length === 0) throw new ClientStateError('A project needs a name.')
      const projectId = newCommandId()
      const command: ProjectCommand = {
        command_id: newCommandId(),
        expected_revision: snapshotRevision(queryClient),
        project_id: projectId,
        name,
        ...(workflowId ? { workflow_id: workflowId } : {}),
      }
      return sendProjectCommand(command).then(toProjectOutcome)
    },
    affectedKeys: () => [queryKeys.snapshot()],
  })
}

/* ------------------------------------------------------------------ */
/* Workflows                                                           */
/* ------------------------------------------------------------------ */

/** Re-pin a project to a workflow version (the "apply" step). */
export function useApplyWorkflow(): CommandMutation<
  WorkflowBinding,
  { projectId: string; workflowId: string; version: number }
> {
  return useApiMutation<WorkflowBinding, { projectId: string; workflowId: string; version: number }>({
    mutationFn: ({ projectId, workflowId, version }) => {
      const body: BindWorkflowBody = { project_id: projectId, id: workflowId, version }
      return bindWorkflow(body).then(toWorkflowBinding)
    },
    affectedKeys: (variables) => [
      queryKeys.workflowBinding(variables.projectId),
      queryKeys.workItems(variables.projectId),
      queryKeys.upgradePreviewAll(),
    ],
  })
}

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
