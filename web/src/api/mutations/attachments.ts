/**
 * Attachment commands: raw-body import and remove.
 *
 * Both are `uncertain` on transport/5xx failures because the server can store
 * the file (or drop the row) and still answer with an error; the UI re-reads the
 * list instead of assuming. An import pins the expected revision per command id
 * so an identical retry replays the same receipt.
 */
import { useRef } from 'react'
import { queryKeys } from '../queries'
import { isAmbiguousError } from '../errors'
import type {
  AttachmentImportResult,
  AttachmentOwnerKind,
  AttachmentRemoveResult,
} from '../contracts'
import { importAttachment, removeAttachment } from '../endpoints/attachments'
import type { ByteUploadProgress } from '../http'
import { toAttachment, type Attachment } from '../adapters'
import { acceptLedgerRevision } from '../accepted-write'
import { snapshotRevision, useApiMutation, type CommandMutation } from './core'

export interface ImportAttachmentVariables {
  /**
   * All three ids are minted by the caller *before* the first request and reused
   * on retry. The server accepts an import by `command_id`, so a lost response
   * can be replayed without storing a second file under a new id.
   */
  commandId: string
  attachmentId: string
  activityId: string
  ownerKind: AttachmentOwnerKind | string
  ownerId: string
  file: Blob | ArrayBuffer | Uint8Array
  filename: string
  contentType?: string
  description?: string
  /** Bytes handed to the network; never acceptance. */
  onProgress?: (progress: ByteUploadProgress) => void
  /** Cancels the in-flight upload; the ids stay reusable for a later attempt. */
  signal?: AbortSignal
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
  // An upload that was not confirmed must be replayed with the *same* expected
  // revision as its first attempt: the server deduplicates on command id and
  // payload, and the first attempt may already have advanced the ledger. Reading
  // a fresh revision on retry would turn a safe replay into an immutable
  // conflict.
  const attemptRevisions = useRef(new Map<string, number>())
  return useApiMutation<ImportAttachmentOutcome, ImportAttachmentVariables>({
    uncertainOnError: true,
    mutationFn: (variables, queryClient) => {
      let expectedRevision = attemptRevisions.current.get(variables.commandId)
      if (expectedRevision === undefined) {
        expectedRevision = snapshotRevision(queryClient)
        attemptRevisions.current.set(variables.commandId, expectedRevision)
      }
      const metadata = {
        command_id: variables.commandId,
        expected_revision: expectedRevision,
        attachment_id: variables.attachmentId,
        activity_id: variables.activityId,
        owner_kind: variables.ownerKind,
        owner_id: variables.ownerId,
        filename: variables.filename,
        content_type: variables.contentType,
        description: variables.description,
      }
      return importAttachment(
        metadata,
        variables.file,
        variables.onProgress,
        variables.signal,
      )
        .then(
          (result: AttachmentImportResult): ImportAttachmentOutcome => ({
            revision: result.revision,
            duplicate: result.duplicate,
            removed: result.removed,
            attachment: result.attachment ? toAttachment(result.attachment) : null,
          }),
        )
        .catch((error: unknown) => {
          // A definitive refusal stored nothing, so its pinned revision can go;
          // an uncertain outcome must keep it for the identical retry.
          if (!isAmbiguousError(error)) {
            attemptRevisions.current.delete(variables.commandId)
          }
          throw error
        })
    },
    affectedKeys: (variables) => [
      queryKeys.snapshot(),
      queryKeys.attachments(variables.ownerKind, variables.ownerId),
      ...(variables.ownerKind === 'task' ? [queryKeys.task(variables.ownerId)] : []),
      ...(variables.ownerKind === 'task'
        ? [queryKeys.taskActivity(variables.ownerId)]
        : []),
    ],
    onAccepted: (outcome, variables, queryClient) => {
      attemptRevisions.current.delete(variables.commandId)
      acceptLedgerRevision(queryClient, outcome.revision)
    },
  })
}

export interface RemoveAttachmentVariables {
  commandId: string
  attachmentId: string
  activityId: string
  ownerKind: AttachmentOwnerKind | string
  ownerId: string
}

export interface RemoveAttachmentOutcome {
  revision: number
  removed: boolean
  duplicate: boolean
  contentRemoved: boolean
  alreadyRemoved: boolean
}

/**
 * Removing writes a ledger activity and drops the stored bytes. The response
 * may be lost after the row is gone, so the disposition is `uncertain` on
 * transport/5xx failures and the caller refetches instead of deleting locally.
 * Removing an already-removed attachment is a 200 no-op (`already_removed`).
 */
export function useRemoveAttachment(): CommandMutation<
  RemoveAttachmentOutcome,
  RemoveAttachmentVariables
> {
  return useApiMutation<RemoveAttachmentOutcome, RemoveAttachmentVariables>({
    uncertainOnError: true,
    mutationFn: (variables, queryClient) =>
      removeAttachment({
        command_id: variables.commandId,
        expected_revision: snapshotRevision(queryClient),
        attachment_id: variables.attachmentId,
        activity_id: variables.activityId,
      }).then(
        (result: AttachmentRemoveResult): RemoveAttachmentOutcome => ({
          revision: result.revision,
          removed: result.removed,
          duplicate: result.duplicate,
          contentRemoved: result.content_removed,
          alreadyRemoved: result.already_removed,
        }),
      ),
    affectedKeys: (variables) => [
      queryKeys.snapshot(),
      queryKeys.attachments(variables.ownerKind, variables.ownerId),
      ...(variables.ownerKind === 'task' ? [queryKeys.task(variables.ownerId)] : []),
      ...(variables.ownerKind === 'task'
        ? [queryKeys.taskActivity(variables.ownerId)]
        : []),
    ],
    onAccepted: (outcome, _variables, queryClient) => {
      acceptLedgerRevision(queryClient, outcome.revision)
    },
  })
}
