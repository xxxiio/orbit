/**
 * Attachment rows. Size, checksum and creation time are the server's numbers;
 * `isImage` is the server's own preview verdict, not a client guess from the
 * content type.
 */
import type { AttachmentView } from '../contracts'

export interface Attachment {
  id: string
  ownerKind: string
  ownerId: string
  filename: string
  contentType: string
  sizeBytes: number
  checksum: string
  createdAtMs: number
  description: string | null
  isImage: boolean
}

export function toAttachment(view: AttachmentView): Attachment {
  return {
    id: view.id,
    ownerKind: view.owner_kind,
    ownerId: view.owner_id,
    filename: view.filename,
    contentType: view.content_type,
    sizeBytes: view.size_bytes,
    checksum: view.checksum,
    createdAtMs: view.created_at_ms,
    description: view.description,
    isImage: view.is_image,
  }
}
