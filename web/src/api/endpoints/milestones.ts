/**
 * Milestone commands.
 *
 * One sender per action, plus the generic `sendMilestoneCommand`. The caller
 * supplies the revision fence and the command id: the hook layer reads
 * `expected_revision` from the freshest milestones snapshot at mutation time
 * and allocates the idempotency key once. Nothing here invents either value.
 *
 * Status edges (`planned→in_progress`, …) and the "move needs 0 tasks" rule are
 * decided by the server; this module only shapes the request.
 */
import { api } from '../client'
import { ClientStateError } from '../errors'
import { isTaskPriority } from '../contracts'
import type {
  MilestoneCommand,
  MilestoneResult,
  MilestoneSnapshot,
  MilestoneStatus,
} from '../contracts'

const BASE = '/authoritative'

export function fetchMilestones(signal?: AbortSignal): Promise<MilestoneSnapshot> {
  return api.get<MilestoneSnapshot>(`${BASE}/milestones`, { signal })
}

export function sendMilestoneCommand(command: MilestoneCommand): Promise<MilestoneResult> {
  return api.post<MilestoneResult>(`${BASE}/milestone-commands`, command)
}

/** The editable half of a milestone, matching the rich create/update shape. */
export interface MilestoneDraft {
  name: string
  description: string
  priority: string | null
  owner: string | null
  startAtMs: number | null
  targetAtMs: number | null
}

/**
 * Camel-case inputs accepted by the senders. Ids are client-generated. The
 * rich fields are optional so an older caller (title only) keeps sending the
 * payload the authority has always accepted.
 */
export interface CreateMilestoneInput {
  milestoneId: string
  projectId: string
  name: string
  description?: string
  priority?: string | null
  owner?: string | null
  startAtMs?: number | null
  targetAtMs?: number | null
}

export interface UpdateMilestoneInput extends MilestoneDraft {
  milestoneId: string
}

export interface AssignMilestoneTaskInput {
  milestoneId: string
  taskId: string
}

export interface UnassignMilestoneTaskInput {
  taskId: string
}

export interface AssignMilestoneWorkItemInput {
  milestoneId: string
  itemId: string
}

export interface UnassignMilestoneWorkItemInput {
  itemId: string
}

export interface RenameMilestoneInput {
  milestoneId: string
  name: string
}

export interface ChangeMilestoneStatusInput {
  milestoneId: string
  status: MilestoneStatus
}

export interface SetMilestoneTargetInput {
  milestoneId: string
  targetAtMs: number | null
}

export interface MoveMilestoneProjectInput {
  milestoneId: string
  projectId: string
}

export interface DeleteMilestoneInput {
  milestoneId: string
}

function requireNonEmpty(label: string, value: string): string {
  const trimmed = value.trim()
  if (trimmed.length === 0) {
    throw new ClientStateError(`${label} is required.`)
  }
  return trimmed
}

function requireRevision(expectedRevision: number): number {
  if (!Number.isInteger(expectedRevision) || expectedRevision < 0) {
    throw new ClientStateError('The milestone list has not loaded its version yet. Try again in a moment.')
  }
  return expectedRevision
}

function requireCommandId(commandId: string): string {
  const trimmed = requireNonEmpty('A command identity is required', commandId)
  if (trimmed.length > 128) {
    throw new ClientStateError('That command identity is too long.')
  }
  return trimmed
}

/** The server accepts this enum; refuse anything else before the round trip. */
function optionalPriority(priority: string | null | undefined): string | null {
  if (priority === null || priority === undefined || priority === '') return null
  if (!isTaskPriority(priority)) {
    throw new ClientStateError('Choose a priority of low, medium, high or urgent.')
  }
  return priority
}

/** Timestamps are epoch milliseconds; the domain refuses negatives. */
function optionalInstant(label: string, value: number | null | undefined): number | null {
  if (value === null || value === undefined) return null
  if (!Number.isInteger(value) || value < 0) {
    throw new ClientStateError(`${label} must be a valid date.`)
  }
  return value
}

function checkWindow(startAtMs: number | null, targetAtMs: number | null): void {
  if (startAtMs !== null && targetAtMs !== null && startAtMs > targetAtMs) {
    throw new ClientStateError('The start date must be on or before the target date.')
  }
}

/** Shape the wire command without sending it (also used by the hooks). */
export function buildMilestoneCommand(
  action: MilestoneCommand['action'],
  expectedRevision: number,
  commandId: string,
): MilestoneCommand {
  return {
    command_id: requireCommandId(commandId),
    expected_revision: requireRevision(expectedRevision),
    action,
  }
}

export function createMilestone(
  input: CreateMilestoneInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  const priority = optionalPriority(input.priority)
  const owner = input.owner ?? null
  const startAtMs = optionalInstant('The start date', input.startAtMs)
  const targetAtMs = optionalInstant('The target date', input.targetAtMs)
  checkWindow(startAtMs, targetAtMs)
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'create',
        milestone_id: requireNonEmpty('A milestone identity is required', input.milestoneId),
        project_id: requireNonEmpty('A project is required', input.projectId),
        name: requireNonEmpty('A milestone needs a name', input.name),
        // Only send what the caller supplied: the older payload stays byte
        // identical for a title-only create.
        ...(input.description !== undefined ? { description: input.description } : {}),
        ...(priority !== null ? { priority } : {}),
        ...(owner !== null ? { owner } : {}),
        ...(startAtMs !== null ? { start_at_ms: startAtMs } : {}),
        ...(targetAtMs !== null ? { target_at_ms: targetAtMs } : {}),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function updateMilestone(
  input: UpdateMilestoneInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  const priority = optionalPriority(input.priority)
  const startAtMs = optionalInstant('The start date', input.startAtMs)
  const targetAtMs = optionalInstant('The target date', input.targetAtMs)
  checkWindow(startAtMs, targetAtMs)
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'update',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
        name: requireNonEmpty('A milestone needs a name', input.name),
        description: input.description ?? '',
        priority,
        owner: input.owner ?? null,
        start_at_ms: startAtMs,
        target_at_ms: targetAtMs,
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function assignMilestoneTask(
  input: AssignMilestoneTaskInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'assign_task',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
        task_id: requireNonEmpty('A task is required', input.taskId),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function unassignMilestoneTask(
  input: UnassignMilestoneTaskInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      { type: 'unassign_task', task_id: requireNonEmpty('A task is required', input.taskId) },
      expectedRevision,
      commandId,
    ),
  )
}

export function assignMilestoneWorkItem(
  input: AssignMilestoneWorkItemInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'assign_work_item',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
        item_id: requireNonEmpty('A work item is required', input.itemId),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function unassignMilestoneWorkItem(
  input: UnassignMilestoneWorkItemInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'unassign_work_item',
        item_id: requireNonEmpty('A work item is required', input.itemId),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function renameMilestone(
  input: RenameMilestoneInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'rename',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
        name: requireNonEmpty('A milestone needs a name', input.name),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function changeMilestoneStatus(
  input: ChangeMilestoneStatusInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'change_status',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
        status: input.status,
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function setMilestoneTarget(
  input: SetMilestoneTargetInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'set_target',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
        target_at_ms: input.targetAtMs,
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function moveMilestoneProject(
  input: MoveMilestoneProjectInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'move_project',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
        project_id: requireNonEmpty('A project is required', input.projectId),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function deleteMilestone(
  input: DeleteMilestoneInput,
  expectedRevision: number,
  commandId: string,
): Promise<MilestoneResult> {
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'delete',
        milestone_id: requireNonEmpty('A milestone is required', input.milestoneId),
      },
      expectedRevision,
      commandId,
    ),
  )
}
