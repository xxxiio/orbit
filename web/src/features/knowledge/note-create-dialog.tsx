/**
 * Create a file-backed Markdown note.
 *
 * The location comes from the folder picker and the name from one field, so no
 * one types a path by hand. The note id and command id are minted once per
 * submitted payload, before the request. `Create` publishes to disk before the
 * receipt, so an error can still mean the note exists; an unchanged retry
 * replays the same ids, while a changed payload cannot mint a new id until the
 * person looks at the tree.
 */
import { useState, type FormEvent } from 'react'
import {
  newCommandId,
  uncertainMessage,
  useIdempotentAttempt,
  useMarkdownCommand,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { useToast } from '@/components/ui/toast'
import type { FolderOption } from './folder-model'
import { createNoteBody, isValidNoteFileName, notePathInFolder } from './knowledge-model'
import { FolderPicker } from './folder/folder-picker'
import { withFolderOption } from './folder-model'

interface CreateAttempt {
  key: string
  noteId: string
  commandId: string
}

export interface NewNoteDialogProps {
  /** Every existing folder, plus the vault root. */
  options: FolderOption[]
  /** Where the picker starts: the folder the person is looking at. */
  initialFolder: string
  onClose: () => void
  onCreated: (path: string) => void
  /** Re-reads the tree; the recovery step after an uncertain create. */
  onRefreshTree: () => void
}

export function NewNoteDialog({
  options,
  initialFolder,
  onClose,
  onCreated,
  onRefreshTree,
}: NewNoteDialogProps) {
  const command = useMarkdownCommand()
  const { toast } = useToast()
  const [folder, setFolder] = useState(initialFolder)
  const [fileName, setFileName] = useState('')
  const [title, setTitle] = useState('')
  const [markdown, setMarkdown] = useState('')
  const [nameError, setNameError] = useState<string | null>(null)
  const [titleError, setTitleError] = useState<string | null>(null)
  const attempt = useIdempotentAttempt<CreateAttempt>((attemptKey) => ({
    key: attemptKey,
    noteId: newCommandId(),
    commandId: newCommandId(),
  }))

  const trimmedName = fileName.trim()
  const path = notePathInFolder(folder, trimmedName)
  const key = `${path}\u0000${title}\u0000${markdown}`
  const uncertainAttempt = command.disposition === 'uncertain' && attempt.attempt !== null
  const blindRetry = uncertainAttempt && attempt.isStale(key)
  const submitting = command.isPending
  const error: ApiError | null =
    command.disposition === 'rejected' || command.disposition === 'uncertain' ? command.error : null

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (submitting || blindRetry) return
    if (!isValidNoteFileName(trimmedName)) {
      setNameError(
        'Use one file name ending in .md, for example decision.md. The folder comes from the list above.',
      )
      return
    }
    if (title.trim().length === 0) {
      setTitleError('A note needs a title for its first heading.')
      return
    }
    setNameError(null)
    setTitleError(null)

    const pinned = attempt.ensure(key)
    try {
      await command.mutateAsync({
        commandId: pinned.commandId,
        command: {
          kind: 'Create',
          note_id: pinned.noteId,
          path,
          body: createNoteBody(pinned.noteId, title, markdown),
        },
      })
      attempt.clear()
      toast({ tone: 'success', title: 'Note created', description: path })
      onCreated(path)
      onClose()
    } catch {
      // The server's answer stays on screen; the attempt stays pinned for an
      // identical retry because the file may already have been written.
    }
  }

  return (
    <Dialog open onOpenChange={(next) => (!next && !submitting ? onClose() : undefined)}>
      <DialogContent
        title="New note"
        description="The note is a Markdown file in the server's vault. Its stable id lives in the frontmatter."
        size="lg"
      >
        <form onSubmit={submit} noValidate className="flex flex-col gap-4" data-dialog="note-create">
          <FolderPicker
            label="Folder"
            hint="The file is created inside this folder. Folders must already exist."
            value={folder}
            options={withFolderOption(options, folder)}
            onChange={(next) => {
              setFolder(next)
              setNameError(null)
            }}
            disabled={submitting}
          />

          <Field
            label="File name"
            required
            error={nameError}
            hint="One name ending in .md. The note id is separate and never inferred from it."
          >
            <Input
              value={fileName}
              onChange={(event) => {
                setFileName(event.target.value)
                setNameError(null)
              }}
              placeholder="decision.md"
              autoFocus
              disabled={submitting}
              spellCheck={false}
              data-field="note-name"
            />
          </Field>

          <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-muted">
            Creates{' '}
            <span className="break-all font-mono text-ink-soft" data-preview-path>
              {path.length > 0 ? path : '…'}
            </span>
          </p>

          <Field
            label="Title"
            required
            error={titleError}
            hint="Used for the first heading; the note id is separate and never inferred from it."
          >
            <Input
              value={title}
              onChange={(event) => {
                setTitle(event.target.value)
                setTitleError(null)
              }}
              placeholder="Decision: cookie sessions"
              disabled={submitting}
            />
          </Field>

          <Field label="Markdown" hint="Plain Markdown. Use [[note-id]] to link another note by its stable id.">
            <Textarea
              value={markdown}
              onChange={(event) => setMarkdown(event.target.value)}
              className="min-h-[10rem] font-mono text-sm leading-6"
              placeholder="What changed, why, and what to do next."
              disabled={submitting}
            />
          </Field>

          {error ? (
            <div
              role="alert"
              className={
                command.disposition === 'uncertain'
                  ? 'rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink'
                  : 'rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink'
              }
            >
              <p className="font-medium">
                {command.disposition === 'uncertain'
                  ? 'The note may or may not have been created.'
                  : 'The server refused this note.'}
              </p>
              <p className="mt-0.5">
                {command.disposition === 'uncertain'
                  ? `${uncertainMessage(error)} The file may already be on disk.`
                  : userMessage(error)}
              </p>
              <p className="mt-1.5 font-medium">
                {command.disposition === 'uncertain'
                  ? 'Retry the same note, or refresh the tree and look for it before choosing a different file name.'
                  : 'The note was not stored. Change the folder, file name or fields and try again.'}
              </p>
            </div>
          ) : null}

          {blindRetry ? (
            <p
              role="alert"
              className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
            >
              These fields no longer match the create whose outcome is unknown. Retry it unchanged,
              or refresh the tree and look for the note first.
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-end gap-2">
            {uncertainAttempt ? (
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  attempt.clear()
                  onRefreshTree()
                }}
                disabled={submitting}
              >
                Refresh the tree
              </Button>
            ) : null}
            <Button type="button" variant="secondary" onClick={onClose} disabled={submitting}>
              Cancel
            </Button>
            <Button
              type="submit"
              loading={submitting}
              disabled={blindRetry}
              data-action="create-note"
              title={
                blindRetry
                  ? 'Retry the unknown create unchanged, or refresh the tree first.'
                  : undefined
              }
            >
              {uncertainAttempt && !blindRetry ? 'Retry the same note' : 'Create note'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
