/**
 * Saved-view commands. These never touch the authority revision or a workflow
 * binding: the store is a separate personal table, and the server says so in
 * its response shape.
 */
import type { SavedViewCommand, SavedViewDisplay, SavedViewScope } from '../contracts'
import { queryKeys } from '../queries'
import { sendSavedViewCommand } from '../endpoints/saved-views'
import { toSavedViewOutcome, type SavedViewOutcome } from '../adapters'
import { acceptSavedView, removeSavedView } from '../accepted-write'
import { useApiMutation, type CommandMutation } from './core'

export interface SaveSavedViewVariables {
  commandId: string
  viewId: string
  name: string
  scope: SavedViewScope
  projectId: string | null
  query: string
  display: SavedViewDisplay
}

/**
 * A stale version is answered with 400 `saved view changed on the server`, not
 * with a 409 the mutation frame would recognise as a conflict. So every
 * definitive refusal must refresh the list: the control tells the person to
 * choose again "from the refreshed list", and an `already exists` save only
 * makes sense once the live row is on screen.
 */
export function useSaveSavedView(): CommandMutation<SavedViewOutcome, SaveSavedViewVariables> {
  return useApiMutation<SavedViewOutcome, SaveSavedViewVariables>({
    // A lost response may have created the row. Replaying the same command id
    // returns the stored receipt instead of creating a second view.
    uncertainOnError: true,
    invalidateOnRejection: true,
    mutationFn: (variables) => {
      const command: SavedViewCommand = {
        type: 'save',
        command_id: variables.commandId,
        view_id: variables.viewId,
        expected_version: 0,
        name: variables.name,
        scope: variables.scope,
        project_id: variables.projectId,
        query: variables.query,
        display: variables.display,
      }
      return sendSavedViewCommand(command).then(toSavedViewOutcome)
    },
    affectedKeys: () => [queryKeys.savedViews()],
    onAccepted: (outcome, _variables, queryClient) => {
      if (outcome.view) acceptSavedView(queryClient, outcome.view)
    },
  })
}

export interface RenameSavedViewVariables {
  commandId: string
  viewId: string
  expectedVersion: number
  name: string
}

export function useRenameSavedView(): CommandMutation<SavedViewOutcome, RenameSavedViewVariables> {
  return useApiMutation<SavedViewOutcome, RenameSavedViewVariables>({
    uncertainOnError: true,
    // A stale version is a 400: refresh the list on the refusal itself.
    invalidateOnRejection: true,
    mutationFn: (variables) => {
      const command: SavedViewCommand = {
        type: 'rename',
        command_id: variables.commandId,
        view_id: variables.viewId,
        expected_version: variables.expectedVersion,
        name: variables.name,
      }
      return sendSavedViewCommand(command).then(toSavedViewOutcome)
    },
    affectedKeys: () => [queryKeys.savedViews()],
    onAccepted: (outcome, _variables, queryClient) => {
      if (outcome.view) acceptSavedView(queryClient, outcome.view)
    },
  })
}

export interface DeleteSavedViewVariables {
  commandId: string
  viewId: string
  expectedVersion: number
}

export function useDeleteSavedView(): CommandMutation<SavedViewOutcome, DeleteSavedViewVariables> {
  return useApiMutation<SavedViewOutcome, DeleteSavedViewVariables>({
    uncertainOnError: true,
    // A stale version is a 400: refresh the list on the refusal itself.
    invalidateOnRejection: true,
    mutationFn: (variables) => {
      const command: SavedViewCommand = {
        type: 'delete',
        command_id: variables.commandId,
        view_id: variables.viewId,
        expected_version: variables.expectedVersion,
      }
      return sendSavedViewCommand(command).then(toSavedViewOutcome)
    },
    affectedKeys: () => [queryKeys.savedViews()],
    onAccepted: (_outcome, variables, queryClient) => {
      removeSavedView(queryClient, variables.viewId, variables.expectedVersion)
    },
  })
}
