/**
 * Epic commands: one mutation instance for the whole family.
 *
 * `expected_revision` is read from the epic snapshot at mutation time and the
 * accepted revision is written back into every ledger read, because an epic
 * command advances the same global fence as tasks and milestones. A refused
 * hierarchy change (cross-project assignment, invalid status edge) never lands
 * on screen.
 */
import { newCommandId } from '../ids'
import { queryKeys } from '../queries'
import { acceptLedgerRevision } from '../accepted-write'
import {
  assignTaskToEpic,
  assignWorkItemToEpic,
  changeEpicStatus,
  createEpic,
  deleteEpic,
  unassignTaskFromEpic,
  unassignWorkItemFromEpic,
  updateEpic,
  type EpicDraft,
} from '../endpoints/epics'
import { toEpicResult, type EpicMutationOutcome } from '../adapters'
import type { EpicStatus } from '../contracts'
import { epicRevision, useApiMutation, type CommandMutation } from './core'

export type EpicCommandInput =
  | { type: 'create'; projectId: string; epicId: string; draft: EpicDraft }
  | { type: 'update'; epicId: string; draft: EpicDraft }
  | { type: 'change_status'; epicId: string; status: EpicStatus }
  | { type: 'assign_task'; epicId: string; taskId: string }
  | { type: 'unassign_task'; taskId: string }
  | { type: 'assign_work_item'; epicId: string; itemId: string }
  | { type: 'unassign_work_item'; itemId: string }
  | { type: 'delete'; epicId: string }

export interface EpicCommandAttempt {
  commandId: string
  input: EpicCommandInput
}

export function useEpicCommand(): CommandMutation<EpicMutationOutcome, EpicCommandAttempt> & {
  pendingCommand: EpicCommandInput['type'] | null
} {
  const mutation = useApiMutation<EpicMutationOutcome, EpicCommandAttempt>({
    uncertainOnError: true,
    mutationFn: (attempt, queryClient) => {
      const revision = epicRevision(queryClient)
      const { commandId, input } = attempt
      switch (input.type) {
        case 'create':
          return createEpic(
            input.epicId,
            input.projectId,
            input.draft,
            revision,
            commandId,
          ).then(toEpicResult)
        case 'update':
          return updateEpic(input.epicId, input.draft, revision, commandId).then(toEpicResult)
        case 'change_status':
          return changeEpicStatus(input.epicId, input.status, revision, commandId).then(
            toEpicResult,
          )
        case 'assign_task':
          return assignTaskToEpic(input.epicId, input.taskId, revision, commandId).then(
            toEpicResult,
          )
        case 'unassign_task':
          return unassignTaskFromEpic(input.taskId, revision, commandId).then(toEpicResult)
        case 'assign_work_item':
          return assignWorkItemToEpic(input.epicId, input.itemId, revision, commandId).then(
            toEpicResult,
          )
        case 'unassign_work_item':
          return unassignWorkItemFromEpic(input.itemId, revision, commandId).then(toEpicResult)
        case 'delete':
          return deleteEpic(input.epicId, revision, commandId).then(toEpicResult)
      }
    },
    affectedKeys: () => [queryKeys.epics(), queryKeys.snapshot(), queryKeys.milestones()],
    onAccepted: (outcome, _attempt, queryClient) => {
      acceptLedgerRevision(queryClient, outcome.revision)
    },
  })

  return {
    ...mutation,
    pendingCommand: mutation.isPending ? (mutation.variables?.input.type ?? null) : null,
  }
}

/** Convenience for surface-level create buttons. */
export function newEpicCommand(input: EpicCommandInput): EpicCommandAttempt {
  return { commandId: newCommandId(), input }
}
