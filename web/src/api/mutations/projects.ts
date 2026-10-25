/**
 * Project commands. Creation is the only project write that exists; both ids
 * come from the dialog's pinned attempt, so a retry replays the receipt.
 */
import { ClientStateError } from '../errors'
import { queryKeys } from '../queries'
import type { ProjectCommand, ProjectResult } from '../contracts'
import { sendProjectCommand } from '../endpoints/projects'
import { toProject, type Project } from '../adapters'
import { acceptLedgerRevision } from '../accepted-write'
import { snapshotRevision, useApiMutation, type CommandMutation } from './core'

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
    affectedKeys: () => [queryKeys.snapshot(), queryKeys.milestones()],
    onAccepted: (outcome, _variables, queryClient) => {
      acceptLedgerRevision(queryClient, outcome.revision)
    },
  })
}
