/**
 * Milestone commands: one mutation instance for the whole milestone family.
 *
 * `expected_revision` is read from the milestones snapshot inside `mutationFn`
 * and the accepted revision is written back, so a second command in the same
 * interaction fences on what the server just reported. Status edges are
 * server-enforced; this hook only sends the requested status.
 */
import { queryKeys } from '../queries'
import { newCommandId } from '../ids'
import {
  assignMilestoneTask,
  assignMilestoneWorkItem,
  changeMilestoneStatus,
  createMilestone,
  deleteMilestone,
  moveMilestoneProject,
  renameMilestone,
  setMilestoneTarget,
  unassignMilestoneTask,
  unassignMilestoneWorkItem,
  updateMilestone as sendUpdateMilestone,
  type MilestoneDraft,
} from '../endpoints/milestones'
import type { MilestoneStatus } from '../contracts'
import { toMilestoneResult, type MilestoneMutationOutcome } from '../adapters'
import { acceptLedgerRevision } from '../accepted-write'
import { milestoneRevision, useApiMutation, type CommandMutation } from './core'

export type MilestoneCommandInput =
  | {
      type: 'create'
      projectId: string
      name: string
      milestoneId: string
      description?: string
      priority?: string | null
      owner?: string | null
      startAtMs?: number | null
      targetAtMs?: number | null
    }
  | { type: 'update'; milestoneId: string; draft: MilestoneDraft }
  | { type: 'rename'; milestoneId: string; name: string }
  | { type: 'change_status'; milestoneId: string; status: MilestoneStatus }
  | { type: 'set_target'; milestoneId: string; targetAtMs: number | null }
  | { type: 'move_project'; milestoneId: string; projectId: string }
  | { type: 'assign_task'; milestoneId: string; taskId: string }
  // Unassignment names the task/work item and, optionally, the milestone whose
  // row is showing the pending spinner. The authority infers membership.
  | { type: 'unassign_task'; taskId: string; milestoneId?: string }
  | { type: 'assign_work_item'; milestoneId: string; itemId: string }
  | { type: 'unassign_work_item'; itemId: string; milestoneId?: string }
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
  createMilestone: (input: {
    projectId: string
    name: string
    milestoneId?: string
    description?: string
    priority?: string | null
    owner?: string | null
    startAtMs?: number | null
    targetAtMs?: number | null
  }) => void
  updateMilestone: (input: { milestoneId: string; draft: MilestoneDraft }) => void
  renameMilestone: (input: { milestoneId: string; name: string }) => void
  changeMilestoneStatus: (input: { milestoneId: string; status: MilestoneStatus }) => void
  setMilestoneTarget: (input: { milestoneId: string; targetAtMs: number | null }) => void
  moveMilestoneProject: (input: { milestoneId: string; projectId: string }) => void
  assignMilestoneTask: (input: { milestoneId: string; taskId: string }) => void
  unassignMilestoneTask: (input: { taskId: string }) => void
  assignMilestoneWorkItem: (input: { milestoneId: string; itemId: string }) => void
  unassignMilestoneWorkItem: (input: { itemId: string }) => void
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
              description: input.description,
              priority: input.priority,
              owner: input.owner,
              startAtMs: input.startAtMs,
              targetAtMs: input.targetAtMs,
            },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'update':
          return sendUpdateMilestone(
            { milestoneId: input.milestoneId, ...input.draft },
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
        case 'set_target':
          return setMilestoneTarget(
            { milestoneId: input.milestoneId, targetAtMs: input.targetAtMs },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'move_project':
          return moveMilestoneProject(
            { milestoneId: input.milestoneId, projectId: input.projectId },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'assign_task':
          return assignMilestoneTask(
            { milestoneId: input.milestoneId, taskId: input.taskId },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'unassign_task':
          return unassignMilestoneTask(
            { taskId: input.taskId },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'assign_work_item':
          return assignMilestoneWorkItem(
            { milestoneId: input.milestoneId, itemId: input.itemId },
            expectedRevision,
            commandId,
          ).then(toMilestoneResult)
        case 'unassign_work_item':
          return unassignMilestoneWorkItem(
            { itemId: input.itemId },
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
      acceptLedgerRevision(queryClient, outcome.revision)
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
        description: input.description,
        priority: input.priority,
        owner: input.owner,
        startAtMs: input.startAtMs,
        targetAtMs: input.targetAtMs,
      }),
    updateMilestone: (input) => send({ type: 'update', milestoneId: input.milestoneId, draft: input.draft }),
    renameMilestone: (input) => send({ type: 'rename', ...input }),
    changeMilestoneStatus: (input) => send({ type: 'change_status', ...input }),
    setMilestoneTarget: (input) => send({ type: 'set_target', ...input }),
    moveMilestoneProject: (input) => send({ type: 'move_project', ...input }),
    assignMilestoneTask: (input) => send({ type: 'assign_task', ...input }),
    unassignMilestoneTask: (input) => send({ type: 'unassign_task', ...input }),
    assignMilestoneWorkItem: (input) => send({ type: 'assign_work_item', ...input }),
    unassignMilestoneWorkItem: (input) => send({ type: 'unassign_work_item', ...input }),
    deleteMilestone: (input) => send({ type: 'delete', ...input }),
  }
}
