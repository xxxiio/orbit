import { api, apiUrl } from '../client'
import { getBlob, postBytes, withQuery } from '../http'
import type {
  AttachmentAssociateRequest,
  AttachmentAssociateResult,
  AttachmentImportMetadata,
  AttachmentImportResult,
  AttachmentListResponse,
  AttachmentOwnerKind,
  AttachmentRemoveRequest,
  AttachmentRemoveResult,
} from '../contracts'

const BASE = '/authoritative'

export function fetchAttachments(
  ownerKind: AttachmentOwnerKind | string,
  ownerId: string,
  signal?: AbortSignal,
): Promise<AttachmentListResponse> {
  return api.get<AttachmentListResponse>(
    withQuery(`${BASE}/attachments`, { owner_kind: ownerKind, owner_id: ownerId }),
    { signal },
  )
}

/**
 * `attachment-import` is the one endpoint whose body is not JSON: the metadata
 * travels in the query string and the file bytes are the request body (no
 * multipart, no base64). `Content-Type` of the request is ignored by the
 * server; the stored type comes from `content_type`.
 */
export function importAttachment(
  metadata: AttachmentImportMetadata,
  bytes: Blob | ArrayBuffer | Uint8Array,
): Promise<AttachmentImportResult> {
  const path = withQuery(`${BASE}/attachment-import`, {
    command_id: metadata.command_id,
    expected_revision: metadata.expected_revision,
    attachment_id: metadata.attachment_id,
    activity_id: metadata.activity_id,
    owner_kind: metadata.owner_kind,
    owner_id: metadata.owner_id,
    filename: metadata.filename,
    content_type: metadata.content_type,
    description: metadata.description,
  })
  return postBytes<AttachmentImportResult>(path, bytes)
}

/** Path for an `<img src>` / download link; the cookie authenticates it. */
export function attachmentContentPath(id: string): string {
  return apiUrl(`${BASE}/attachments/${encodeURIComponent(id)}/content`)
}

export function fetchAttachmentContent(id: string, signal?: AbortSignal): Promise<Blob> {
  return getBlob(`${BASE}/attachments/${encodeURIComponent(id)}/content`, { signal })
}

/** Removing an already-removed attachment is a 200 no-op. */
export function removeAttachment(body: AttachmentRemoveRequest): Promise<AttachmentRemoveResult> {
  return api.post<AttachmentRemoveResult>(`${BASE}/attachment-remove`, body)
}

export function associateAttachment(
  body: AttachmentAssociateRequest,
): Promise<AttachmentAssociateResult> {
  return api.post<AttachmentAssociateResult>(`${BASE}/attachment-associate`, body)
}
