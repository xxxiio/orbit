/**
 * Epic reads and commands.
 *
 * Validation here is shape-only: ids, required titles and date windows the
 * server also validates. Hierarchy rules, project matching and progress stay
 * authoritative on the server.
 */
import { api } from '../client'
import { ClientStateError } from '../errors'
import type { EpicAction, EpicCommand, EpicResult, EpicSnapshot, EpicStatus } from '../contracts'

const BASE = '/authoritative'

export function fetchEpics(signal?: AbortSignal): Promise<EpicSnapshot> {
  return api.get<EpicSnapshot>(`${BASE}/epics`, { signal })
}

export function sendEpicCommand(command: EpicCommand): Promise<EpicResult> {
  return api.post<EpicResult>(`${BASE}/epic-commands`, command)
}

export interface EpicDraft {
  title: string
  description: string
  priority: string | null
  owner: string | null
  startAtMs: number | null
  targetAtMs: number | null
}

function requireText(label: string, value: string): string {
  const trimmed = value.trim()
  if (!trimmed) throw new ClientStateError(`${label} is required.`)
  return trimmed
}

function requireRevision(expectedRevision: number): number {
  if (!Number.isInteger(expectedRevision) || expectedRevision < 0) {
    throw new ClientStateError('The epic list has not loaded its version yet. Try again in a moment.')
  }
  return expectedRevision
}

function requireCommandId(commandId: string): string {
  const id = requireText('A command identity is required', commandId)
  if (id.length > 128) throw new ClientStateError('That command identity is too long.')
  return id
}

function checkWindow(startAtMs: number | null, targetAtMs: number | null): void {
  if (startAtMs !== null && targetAtMs !== null && startAtMs > targetAtMs) {
    throw new ClientStateError('The start date must be on or before the target date.')
  }
}

export function buildEpicCommand(
  action: EpicAction,
  expectedRevision: number,
  commandId: string,
): EpicCommand {
  return {
    command_id: requireCommandId(commandId),
    expected_revision: requireRevision(expectedRevision),
    action,
  }
}

export function createEpic(
  epicId: string,
  projectId: string,
  draft: EpicDraft,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  checkWindow(draft.startAtMs, draft.targetAtMs)
  return sendEpicCommand(
    buildEpicCommand(
      {
        type: 'create',
        epic_id: requireText('An epic identity is required', epicId),
        project_id: requireText('A project is required', projectId),
        title: requireText('An epic needs a title', draft.title),
        description: draft.description,
        priority: draft.priority,
        owner: draft.owner,
        start_at_ms: draft.startAtMs,
        target_at_ms: draft.targetAtMs,
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function updateEpic(
  epicId: string,
  draft: EpicDraft,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  checkWindow(draft.startAtMs, draft.targetAtMs)
  return sendEpicCommand(
    buildEpicCommand(
      {
        type: 'update',
        epic_id: requireText('An epic is required', epicId),
        title: requireText('An epic needs a title', draft.title),
        description: draft.description,
        priority: draft.priority,
        owner: draft.owner,
        start_at_ms: draft.startAtMs,
        target_at_ms: draft.targetAtMs,
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function changeEpicStatus(
  epicId: string,
  status: EpicStatus,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  return sendEpicCommand(
    buildEpicCommand(
      { type: 'change_status', epic_id: requireText('An epic is required', epicId), status },
      expectedRevision,
      commandId,
    ),
  )
}

export function assignTaskToEpic(
  epicId: string,
  taskId: string,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  return sendEpicCommand(
    buildEpicCommand(
      {
        type: 'assign_task',
        epic_id: requireText('An epic is required', epicId),
        task_id: requireText('A task is required', taskId),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function unassignTaskFromEpic(
  taskId: string,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  return sendEpicCommand(
    buildEpicCommand(
      { type: 'unassign_task', task_id: requireText('A task is required', taskId) },
      expectedRevision,
      commandId,
    ),
  )
}

export function assignWorkItemToEpic(
  epicId: string,
  itemId: string,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  return sendEpicCommand(
    buildEpicCommand(
      {
        type: 'assign_work_item',
        epic_id: requireText('An epic is required', epicId),
        item_id: requireText('A work item is required', itemId),
      },
      expectedRevision,
      commandId,
    ),
  )
}

export function unassignWorkItemFromEpic(
  itemId: string,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  return sendEpicCommand(
    buildEpicCommand(
      { type: 'unassign_work_item', item_id: requireText('A work item is required', itemId) },
      expectedRevision,
      commandId,
    ),
  )
}

export function deleteEpic(
  epicId: string,
  expectedRevision: number,
  commandId: string,
): Promise<EpicResult> {
  return sendEpicCommand(
    buildEpicCommand(
      { type: 'delete', epic_id: requireText('An epic is required', epicId) },
      expectedRevision,
      commandId,
    ),
  )
}
