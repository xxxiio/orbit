/**
 * Knowledge commands. A write publishes to disk before the receipt, so an error
 * does not prove nothing changed: the disposition is `uncertain` and an
 * identical retry replays the same command id.
 *
 * Folder commands and the reference command follow the same rule. A rejection
 * also refreshes the knowledge family (`invalidateOnRejection`): a folder move
 * answers 409 both for "the destination exists" and for a replay that lost its
 * answer, and the tree is the only honest account of what happened.
 */
import { queryKeys } from '../queries'
import type {
  CreateFolderResponse,
  MarkdownCommand,
  MarkdownReceipt,
  MarkdownReferenceReceipt,
  MoveFolderResponse,
  ReferenceAction,
} from '../contracts'
import {
  createFolder,
  moveFolder,
  sendMarkdownCommand,
  sendReferenceCommand,
} from '../endpoints/knowledge'
import { useApiMutation, type CommandMutation } from './core'

export interface MarkdownCommandVariables {
  /**
   * Minted by the caller before the first attempt. A knowledge write publishes
   * to disk before the receipt, so an error can still mean the file changed;
   * the identical retry must replay this id, not mint a new one.
   */
  commandId: string
  command: MarkdownCommand
}

/**
 * Knowledge writes are published to disk before the receipt is accepted, so an
 * error does not prove nothing changed. Treat it as `uncertain`: re-read the
 * document, and if the bytes still match what the command expects, replay the
 * **same** command id (same id + same payload replays; a new id is never needed
 * while the outcome is unknown).
 */
export function useMarkdownCommand(): CommandMutation<MarkdownReceipt, MarkdownCommandVariables> {
  return useApiMutation<MarkdownReceipt, MarkdownCommandVariables>({
    uncertainOnError: true,
    mutationFn: ({ command, commandId }) =>
      sendMarkdownCommand({ command_id: commandId, command }),
    affectedKeys: () => [queryKeys.knowledgeAll()],
  })
}

export interface FolderCreateVariables {
  /** Minted by the caller; an unchanged retry must replay it. */
  commandId: string
  /** Vault-relative folder path; the parent must already exist. */
  path: string
}

export function useFolderCreateCommand(): CommandMutation<
  CreateFolderResponse,
  FolderCreateVariables
> {
  return useApiMutation<CreateFolderResponse, FolderCreateVariables>({
    uncertainOnError: true,
    invalidateOnRejection: true,
    mutationFn: ({ commandId, path }) => createFolder({ command_id: commandId, command: { path } }),
    affectedKeys: () => [queryKeys.knowledgeAll()],
  })
}

export interface FolderMoveVariables {
  commandId: string
  from: string
  to: string
}

/** A rename is a move whose `to` keeps the same parent and changes the name. */
export function useFolderMoveCommand(): CommandMutation<MoveFolderResponse, FolderMoveVariables> {
  return useApiMutation<MoveFolderResponse, FolderMoveVariables>({
    uncertainOnError: true,
    invalidateOnRejection: true,
    mutationFn: ({ commandId, from, to }) =>
      moveFolder({ command_id: commandId, command: { from, to } }),
    affectedKeys: () => [queryKeys.knowledgeAll()],
  })
}

export interface ReferenceCommandVariables {
  commandId: string
  noteId: string
  /** `<kind>:<id>`; the server refuses a kind it does not support. */
  target: string
  action: ReferenceAction
}

/**
 * Add or remove one typed reference. The server mutates and publishes the body
 * through the Markdown command path, so the same uncertainty rule applies: an
 * ambiguous failure keeps the command id for the identical retry.
 */
export function useReferenceCommand(): CommandMutation<
  MarkdownReferenceReceipt,
  ReferenceCommandVariables
> {
  return useApiMutation<MarkdownReferenceReceipt, ReferenceCommandVariables>({
    uncertainOnError: true,
    invalidateOnRejection: true,
    mutationFn: ({ commandId, noteId, target, action }) =>
      sendReferenceCommand({ command_id: commandId, note_id: noteId, target, action }),
    affectedKeys: () => [queryKeys.knowledgeAll()],
  })
}
