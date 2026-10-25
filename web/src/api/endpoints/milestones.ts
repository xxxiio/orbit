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

/** Camel-case inputs accepted by the senders. Ids are client-generated. */
export interface CreateMilestoneInput {
  milestoneId: string
  projectId: string
  name: string
}

export interface RenameMilestoneInput {
  milestoneId: string
  name: string
}

export interface ChangeMilestoneStatusInput {
  milestoneId: string
  status: MilestoneStatus
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
  return sendMilestoneCommand(
    buildMilestoneCommand(
      {
        type: 'create',
        milestone_id: requireNonEmpty('A milestone identity is required', input.milestoneId),
        project_id: requireNonEmpty('A project is required', input.projectId),
        name: requireNonEmpty('A milestone needs a name', input.name),
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
