/**
 * Project commands. Create and update share one route; each attempt carries the
 * caller-minted `command_id` and reads its revision fence at mutation time.
 */
import type { ProjectStatus } from '../contracts'
import { ClientStateError } from '../errors'
import { queryKeys } from '../queries'
import type { ProjectCommand, ProjectResult } from '../contracts'
import { sendProjectCommand } from '../endpoints/projects'
import { acceptLedgerRevision, acceptProjectRecord } from '../accepted-write'
import { toProject, type Project } from '../adapters'
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
 * Creation. `workflowId` is only read on the first accepted create, where it
 * pins the binding in the same transaction. Both ids come from the dialog's
 * attempt, so a retry replays the receipt.
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
    affectedKeys: () => [queryKeys.snapshot(), queryKeys.milestones(), queryKeys.searchAll()],
    onAccepted: (outcome, _variables, queryClient) => {
      acceptLedgerRevision(queryClient, outcome.revision)
    },
  })
}

/**
 * Rename and/or status. An empty `name` means "keep the stored name"; a missing
 * `status` means "keep the stored status". At least one must change, checked
 * before anything is sent.
 */
export interface UpdateProjectVariables {
  commandId: string
  projectId: string
  name: string
  status?: ProjectStatus
}

export function useUpdateProject(): CommandMutation<ProjectMutationOutcome, UpdateProjectVariables> {
  return useApiMutation<ProjectMutationOutcome, UpdateProjectVariables>({
    // A lost response may have applied the update: the next attempt replays the
    // same receipt instead of writing a second activity row.
    uncertainOnError: true,
    mutationFn: ({ commandId, projectId, name, status }, queryClient) => {
      if (name.trim().length === 0 && status === undefined) {
        throw new ClientStateError('Choose a new name or status before saving.')
      }
      const command: ProjectCommand = {
        command_id: commandId,
        expected_revision: snapshotRevision(queryClient),
        action: 'update',
        project_id: projectId,
        name,
        ...(status ? { status } : {}),
      }
      return sendProjectCommand(command).then(toProjectOutcome)
    },
    affectedKeys: () => [
      queryKeys.snapshot(),
      queryKeys.milestones(),
      queryKeys.searchAll(),
    ],
    onAccepted: (outcome, _variables, queryClient) => {
      // Raise the fence and write the record the server returned so every
      // surface reading the snapshot (sidebar, header, project list, recent
      // destinations) updates before the refetch settles.
      acceptLedgerRevision(queryClient, outcome.revision)
      acceptProjectRecord(queryClient, outcome.project, outcome.revision)
    },
  })
}
