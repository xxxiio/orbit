/**
 * Attachments for one owner (task, work item, …).
 *
 * One component, keyed by `ownerKind`, so the task drawer and the work-item
 * sheet render the same list, upload path and removal fence. Uploads are raw
 * bytes with query metadata; the attachment id and command ids are minted before
 * the request and reused on an unconfirmed retry. A file never appears as
 * accepted before the server answers, and removal refetches instead of deleting
 * locally.
 */
import { useEffect, useRef, useState } from 'react'
import type { Attachment, ByteUploadProgress } from '@/hooks'
import {
  attachmentContentPath,
  errorInfo,
  isAmbiguousError,
  isApiError,
  newCommandId,
  uncertainMessage,
  useAttachmentContent,
  useAttachments,
  useImportAttachment,
  useRemoveAttachment,
  userMessage,
  type ApiError,
} from '@/hooks'
import type { AttachmentOwnerKind } from '@/api/contracts'

import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { Field } from '@/components/ui/field'
import {
  AlertCircleIcon,
  DownloadIcon,
  EyeIcon,
  FileIcon,
  ImageIcon,
  LoaderIcon,
  PlusIcon,
  RefreshIcon,
  TrashIcon,
} from '@/components/ui/icons'
import { Input } from '@/components/ui/input'
import { SkeletonText } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { formatMoment } from '@/domain'

type UploadStatus = 'uploading' | 'rejected' | 'uncertain'

interface UploadAttempt {
  key: string
  commandId: string
  attachmentId: string
  activityId: string
  file: File
  filename: string
  contentType: string
  description: string
  status: UploadStatus
  progress: number | null
  error: ApiError | null
}

interface RemoveAttempt {
  attachmentId: string
  status: 'confirm' | 'pending' | 'rejected' | 'uncertain'
  error: ApiError | null
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** Server copy where it is richer than the generic status sentence. */
function uploadErrorCopy(error: ApiError): string {
  if (error.status === 413) {
    return 'This file is larger than the server\'s upload limit. It was not stored; choose a smaller file.'
  }
  const info = errorInfo(error)
  if (error.status === 404 && info.serverMessage) return info.serverMessage
  return userMessage(error)
}

function UploadRow({
  attempt,
  onRetry,
  onCancel,
  onDismiss,
  onRefresh,
}: {
  attempt: UploadAttempt
  onRetry: () => void
  onCancel: () => void
  onDismiss: () => void
  onRefresh: () => void
}) {
  const uploading = attempt.status === 'uploading'
  return (
    <li
      data-upload-state={attempt.status}
      className={cn(
        'rounded-md border px-3 py-2.5 text-xs leading-5',
        attempt.status === 'uncertain'
          ? 'border-warning/40 bg-warning-soft text-warning-ink'
          : attempt.status === 'rejected'
            ? 'border-danger/30 bg-danger-soft text-danger-ink'
            : 'border-line bg-surface-sunken/40 text-ink-soft',
      )}
    >
      <div className="flex items-start gap-2">
        <span className="mt-0.5 shrink-0">
          {uploading ? <LoaderIcon size={14} className="animate-spin text-accent" /> : <AlertCircleIcon size={14} />}
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate font-medium">{attempt.filename}</p>
          {uploading ? (
            <>
              <p className="mt-0.5">
                Sending {formatBytes(attempt.file.size)}
                {attempt.progress !== null ? ` · ${Math.round(attempt.progress * 100)}%` : '…'}
              </p>
              <div
                className="mt-1.5 h-1 overflow-hidden rounded-full bg-line"
                role="progressbar"
                aria-label={`Uploading ${attempt.filename}`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={attempt.progress !== null ? Math.round(attempt.progress * 100) : undefined}
              >
                <span
                  aria-hidden="true"
                  className="block h-full rounded-full bg-accent transition-[width] duration-150"
                  style={{ width: `${attempt.progress !== null ? attempt.progress * 100 : 8}%` }}
                />
              </div>
              <p className="mt-1 text-2xs text-ink-muted">
                Not stored yet — the server confirms when the bytes land.
              </p>
            </>
          ) : (
            <>
              <p className="mt-0.5">
                {attempt.status === 'uncertain'
                  ? uncertainMessage(attempt.error ?? undefined)
                  : attempt.error
                    ? uploadErrorCopy(attempt.error)
                    : 'The upload could not be sent.'}
              </p>
              {attempt.status === 'uncertain' ? (
                <p className="mt-1 font-medium">
                  The file may already be listed. Retry the same upload or refresh the list before
                  choosing it again.
                </p>
              ) : (
                <p className="mt-1 font-medium">The file was not stored.</p>
              )}
            </>
          )}
        </div>
        {uploading ? (
          <Button variant="quiet" size="sm" onClick={onCancel}>
            Cancel
          </Button>
        ) : (
          <div className="flex shrink-0 flex-wrap items-center justify-end gap-1">
            {attempt.status === 'uncertain' ? (
              <>
                <Button variant="secondary" size="sm" onClick={onRetry}>
                  Retry the same upload
                </Button>
                <Button variant="quiet" size="sm" onClick={onRefresh}>
                  Refresh list
                </Button>
              </>
            ) : null}
            <Button
              variant="quiet"
              size="sm"
              onClick={onDismiss}
              aria-label={`Dismiss upload of ${attempt.filename}`}
            >
              Dismiss
            </Button>
          </div>
        )}
      </div>
    </li>
  )
}

function PreviewPanel({ attachment }: { attachment: Attachment }) {
  const [open, setOpen] = useState(false)
  const content = useAttachmentContent(attachment.id, open)
  const [url, setUrl] = useState<string | null>(null)

  useEffect(() => {
    if (!content.data) return
    const objectUrl = URL.createObjectURL(content.data)
    setUrl(objectUrl)
    return () => {
      URL.revokeObjectURL(objectUrl)
      setUrl(null)
    }
  }, [content.data])

  return (
    <div className="mt-2">
      <Button
        variant="quiet"
        size="sm"
        leadingIcon={<EyeIcon size={14} />}
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
      >
        {open ? 'Hide preview' : 'Preview'}
      </Button>
      {open ? (
        <div className="mt-2 rounded-md border border-line bg-surface-sunken/40 p-2">
          {content.isPending ? (
            <div aria-busy="true" className="flex items-center gap-2 text-xs text-ink-muted">
              <LoaderIcon size={14} className="animate-spin text-accent" />
              Loading image…
            </div>
          ) : content.error ? (
            <div className="flex flex-wrap items-center gap-2 text-xs text-danger-ink">
              <span>{userMessage(content.error)}</span>
              <Button variant="quiet" size="sm" onClick={() => void content.refetch()}>
                Retry
              </Button>
            </div>
          ) : url ? (
            <img
              src={url}
              alt={`Preview of ${attachment.filename}`}
              className="max-h-64 max-w-full rounded-sm object-contain"
            />
          ) : (
            <p className="text-xs text-ink-muted">The image could not be shown.</p>
          )}
        </div>
      ) : null}
    </div>
  )
}

function AttachmentRow({
  attachment,
  removal,
  onAskRemove,
  onCancelRemove,
  onConfirmRemove,
  onRefresh,
}: {
  attachment: Attachment
  removal: RemoveAttempt | null
  onAskRemove: () => void
  onCancelRemove: () => void
  onConfirmRemove: () => void
  onRefresh: () => void
}) {
  const pending = removal?.status === 'pending'
  return (
    <li className="px-3 py-3" data-attachment-id={attachment.id}>
      <div className="flex items-start gap-2.5">
        <span className="mt-0.5 grid h-8 w-8 shrink-0 place-items-center rounded-md border border-line bg-surface-sunken text-ink-muted">
          {attachment.isImage ? <ImageIcon size={16} /> : <FileIcon size={16} />}
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-ink" title={attachment.filename}>
            {attachment.filename}
          </p>
          <p className="mt-0.5 flex flex-wrap items-center gap-x-2 text-2xs text-ink-muted">
            <span>{formatBytes(attachment.sizeBytes)}</span>
            <span aria-hidden="true">·</span>
            <span>{attachment.contentType}</span>
            <span aria-hidden="true">·</span>
            <span>{formatMoment(attachment.createdAtMs)}</span>
          </p>
          {attachment.description ? (
            <p className="mt-1 text-xs leading-5 text-ink-soft">{attachment.description}</p>
          ) : null}

          {removal && removal.status !== 'confirm' && removal.status !== 'pending' ? (
            <div
              role="alert"
              className={cn(
                'mt-2 rounded-sm border px-2 py-1.5 text-xs leading-5',
                removal.status === 'uncertain'
                  ? 'border-warning/40 bg-warning-soft text-warning-ink'
                  : 'border-danger/30 bg-danger-soft text-danger-ink',
              )}
            >
              {removal.status === 'uncertain'
                ? `${uncertainMessage(removal.error ?? undefined)} The file may still be listed.`
                : `The server refused the removal: ${userMessage(removal.error)}`}
              <button
                type="button"
                onClick={onRefresh}
                className="ml-1 font-medium underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
              >
                Refresh list
              </button>
            </div>
          ) : null}

          {removal?.status === 'confirm' ? (
            <div className="mt-2 flex flex-wrap items-center gap-2 rounded-sm border border-danger/30 bg-danger-soft px-2 py-1.5">
              <span className="text-xs font-medium text-danger-ink">
                Remove this file from the server?
              </span>
              <Button variant="danger" size="sm" onClick={onConfirmRemove}>
                Remove
              </Button>
              <Button variant="quiet" size="sm" onClick={onCancelRemove}>
                Cancel
              </Button>
            </div>
          ) : null}

          <div className="mt-2 flex flex-wrap items-center gap-1">
            <Button asChild variant="quiet" size="sm">
              <a
                href={attachmentContentPath(attachment.id)}
                download={attachment.filename}
                aria-label={`Download ${attachment.filename}`}
              >
                <DownloadIcon size={14} />
                Download
              </a>
            </Button>
            {attachment.isImage ? <PreviewPanel attachment={attachment} /> : null}
            <Button
              variant="quiet"
              size="sm"
              onClick={onAskRemove}
              disabled={pending}
              loading={pending}
              className="text-danger-ink hover:text-danger-ink"
              aria-label={`Remove ${attachment.filename}`}
            >
              <TrashIcon size={14} />
              Remove
            </Button>
          </div>
        </div>
      </div>
    </li>
  )
}

export interface AttachmentListProps {
  ownerKind: AttachmentOwnerKind | string
  ownerId: string
  /** Rendered as the empty-state hint for the owner's next action. */
  emptyHint?: string
}

export function AttachmentList({ ownerKind, ownerId, emptyHint }: AttachmentListProps) {
  const list = useAttachments(ownerKind, ownerId)
  const upload = useImportAttachment()
  const remove = useRemoveAttachment()
  const inputRef = useRef<HTMLInputElement>(null)
  const abortRef = useRef<AbortController | null>(null)
  const [description, setDescription] = useState('')
  const [attempt, setAttempt] = useState<UploadAttempt | null>(null)
  const [removal, setRemoval] = useState<RemoveAttempt | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)

  useEffect(() => {
    if (!announcement) return
    const timer = window.setTimeout(() => setAnnouncement(null), 6000)
    return () => window.clearTimeout(timer)
  }, [announcement])

  const uploading = attempt?.status === 'uploading'
  // A lost response may already have stored the file. Choosing another file
  // would mint a new attachment id and create a second row, so the picker stays
  // closed until Retry, Refresh or Dismiss resolves the attempt.
  const uncertainUpload = attempt?.status === 'uncertain'
  const uploadBlocked = uploading || uncertainUpload
  const attachments = list.data ?? []

  const runUpload = async (next: UploadAttempt) => {
    setAnnouncement(null)
    setAttempt({ ...next, status: 'uploading', progress: null, error: null })
    const controller = new AbortController()
    abortRef.current = controller
    try {
      await upload.mutateAsync({
        commandId: next.commandId,
        attachmentId: next.attachmentId,
        activityId: next.activityId,
        ownerKind,
        ownerId,
        file: next.file,
        filename: next.filename,
        contentType: next.contentType,
        description: next.description || undefined,
        signal: controller.signal,
        onProgress: (progress: ByteUploadProgress) => {
          setAttempt((current) =>
            current && current.key === next.key
              ? {
                  ...current,
                  progress: progress.total > 0 ? progress.loaded / progress.total : null,
                }
              : current,
          )
        },
      })
      setAttempt(null)
      setDescription('')
      await list.refetch()
      setAnnouncement(`${next.filename} uploaded.`)
    } catch (error) {
      if (controller.signal.aborted) {
        setAttempt(null)
        setAnnouncement('Upload cancelled.')
        return
      }
      const apiError = isApiError(error) ? error : null
      const status: UploadStatus = apiError && isAmbiguousError(apiError) ? 'uncertain' : 'rejected'
      setAttempt({ ...next, status, progress: null, error: apiError })
    } finally {
      if (abortRef.current === controller) abortRef.current = null
    }
  }

  const cancelUpload = () => abortRef.current?.abort()

  const chooseFile = (file: File | undefined) => {
    if (!file) return
    const next: UploadAttempt = {
      key: `${file.name}:${file.size}:${file.lastModified}:${newCommandId()}`,
      commandId: newCommandId(),
      attachmentId: newCommandId(),
      activityId: newCommandId(),
      file,
      filename: file.name,
      contentType: file.type || 'application/octet-stream',
      description: description.trim(),
      status: 'uploading',
      progress: null,
      error: null,
    }
    void runUpload(next)
  }

  const confirmRemove = async (attachment: Attachment) => {
    setRemoval({ attachmentId: attachment.id, status: 'pending', error: null })
    try {
      await remove.mutateAsync({
        commandId: newCommandId(),
        attachmentId: attachment.id,
        activityId: newCommandId(),
        ownerKind,
        ownerId,
      })
      setRemoval(null)
      await list.refetch()
      setAnnouncement(`${attachment.filename} removed.`)
    } catch (error) {
      setRemoval({
        attachmentId: attachment.id,
        status: isApiError(error) && isAmbiguousError(error) ? 'uncertain' : 'rejected',
        error: isApiError(error) ? error : null,
      })
    }
  }

  if (list.isPending && !list.data) {
    return (
      <div aria-busy="true" aria-label="Loading attachments">
        <SkeletonText lines={2} />
      </div>
    )
  }

  if (list.error && !list.data) {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <p className="min-w-0 flex-1 text-xs leading-5 text-danger-ink">
          Attachments could not be read. {userMessage(list.error)}
        </p>
        <Button variant="quiet" size="sm" loading={list.isFetching} onClick={() => void list.refetch()}>
          Retry
        </Button>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-2 rounded-md border border-line bg-surface-sunken/30 px-3 py-2.5 sm:flex-row sm:items-end">
        <Field label="Description" hint="Optional; applies to the next file." className="min-w-0 flex-1">
          <Input
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="What this file shows"
            disabled={uploadBlocked}
          />
        </Field>
        <div className="flex shrink-0 items-center gap-2">
          <input
            ref={inputRef}
            type="file"
            className="sr-only"
            data-attachment-file-input
            aria-label="Choose a file to upload"
            disabled={uploadBlocked}
            onChange={(event) => {
              chooseFile(event.target.files?.[0])
              event.target.value = ''
            }}
          />
          <Button
            leadingIcon={<PlusIcon size={15} />}
            loading={uploading}
            onClick={() => inputRef.current?.click()}
            disabled={uploadBlocked}
          >
            Upload file
          </Button>
          <Button
            variant="quiet"
            size="sm"
            onClick={() => void list.refetch()}
            loading={list.isFetching}
            aria-label="Refresh attachments"
          >
            <RefreshIcon size={14} />
            Refresh
          </Button>
        </div>
      </div>

      {attempt ? (
        <ul aria-label={uploading ? 'Upload in progress' : 'Upload attempts'}>
          <UploadRow
            attempt={attempt}
            onRetry={() => void runUpload(attempt)}
            onCancel={cancelUpload}
            onDismiss={() => setAttempt(null)}
            onRefresh={() => {
              setAttempt(null)
              void list.refetch()
            }}
          />
        </ul>
      ) : null}

      {attachments.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<FileIcon size={18} />}
          title="No attachments yet"
          description={emptyHint ?? 'Upload a file to keep it beside this record. The server stores the bytes.'}
        />
      ) : (
        <ul
          aria-label="Attachments"
          className="divide-y divide-line rounded-md border border-line bg-surface"
        >
          {attachments.map((attachment) => (
            <AttachmentRow
              key={attachment.id}
              attachment={attachment}
              removal={removal?.attachmentId === attachment.id ? removal : null}
              onAskRemove={() =>
                setRemoval({ attachmentId: attachment.id, status: 'confirm', error: null })
              }
              onCancelRemove={() => setRemoval(null)}
              onConfirmRemove={() => void confirmRemove(attachment)}
              onRefresh={() => {
                setRemoval(null)
                void list.refetch()
              }}
            />
          ))}
        </ul>
      )}

      <p role="status" aria-live="polite" className="min-h-4 text-2xs text-ink-muted">
        {announcement}
      </p>
    </div>
  )
}
