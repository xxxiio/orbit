/**
 * Move or rename one note: pick a folder and one file name, never type a path.
 *
 * The command needs the bytes the server must still hold, so the file is read
 * again immediately before the send and the move is refused locally when the
 * identity changed. The same command id is reused for an identical retry: the
 * previous move may already have happened, and a new id would only hide that.
 */
import { useState, type FormEvent } from 'react'
import type { MarkdownCommand } from '@/api/contracts'
import {
  newCommandId,
  uncertainMessage,
  useIdempotentAttempt,
  useMarkdownCommand,
  useMarkdownDocument,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import type { FolderOption } from '@/features/knowledge/folder-model'
import { noteFileName, noteFolder, withFolderOption } from '@/features/knowledge/folder-model'
import { FolderPicker } from '@/features/knowledge/folder/folder-picker'
import {
  isValidNoteFileName,
  notePathInFolder,
  splitMarkdown,
} from '@/features/knowledge/knowledge-model'

interface MoveAttempt {
  key: string
  commandId: string
  command: MarkdownCommand
}

export function NoteMoveDialog({
  currentPath,
  noteId,
  options,
  onClose,
  onMoved,
  onRefreshTree,
}: {
  currentPath: string
  noteId: string
  options: FolderOption[]
  onClose: () => void
  onMoved: (path: string) => void
  onRefreshTree: () => void
}) {
  const source = useMarkdownDocument(currentPath)
  const command = useMarkdownCommand()
  const [folder, setFolder] = useState(() => noteFolder(currentPath))
  const [fileName, setFileName] = useState(() => noteFileName(currentPath))
  const [error, setError] = useState<string | null>(null)
  const [blindRetry, setBlindRetry] = useState(false)
  const attempt = useIdempotentAttempt<MoveAttempt, { command: MarkdownCommand }>((key, payload) => ({
    key,
    commandId: newCommandId(),
    command: payload.command,
  }))
  const writing = command.isPending
  const uncertain = command.disposition === 'uncertain'
  const refused = command.disposition === 'rejected'

  const target = notePathInFolder(folder, fileName.trim())

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (writing || blindRetry) return
    if (!isValidNoteFileName(fileName.trim())) {
      setError('Use one file name ending in .md, for example decision.md.')
      return
    }
    const next = target
    if (next === currentPath) {
      setError('Choose a different folder or file name. The note is already there.')
      return
    }

    const key = `${currentPath}\u0000${next}`
    const pinned = attempt.attempt
    if (pinned?.key === key) {
      try {
        await command.mutateAsync({ commandId: pinned.commandId, command: pinned.command })
        onMoved(next)
      } catch {
        // The server's answer is rendered below.
      }
      return
    }
    if (pinned && pinned.key !== key) {
      if (command.disposition === 'uncertain') {
        setBlindRetry(true)
        setError(
          'The previous move may have completed. Retry it unchanged, or refresh the tree before choosing another location.',
        )
        return
      }
      // A definitive refusal did not move anything; a new destination is a
      // genuinely new command, not a blind retry.
      attempt.clear()
      setBlindRetry(false)
    }

    // As in `save`, a failed refetch must reject instead of resolving with the
    // cached body: `expected` is a fence on bytes the server must still hold.
    let raw: string | undefined
    try {
      const fresh = await source.refetch({ throwOnError: true })
      raw = fresh.data?.body
    } catch {
      setError('The note could not be re-read, so nothing was moved. Try again.')
      return
    }
    if (raw === undefined) {
      setError('The note could not be re-read, so nothing was moved. Try again.')
      return
    }
    if (splitMarkdown(raw).noteId !== noteId) {
      setError('The note identity changed on disk. Reload it before moving; nothing was sent.')
      return
    }

    const pinnedNext = attempt.ensure(key, {
      command: {
        kind: 'Move',
        note_id: noteId,
        from: currentPath,
        to: next,
        expected: raw,
      },
    })
    try {
      await command.mutateAsync({ commandId: pinnedNext.commandId, command: pinnedNext.command })
      onMoved(next)
    } catch {
      // The server's answer is rendered below.
    }
  }

  const commandError: ApiError | null = uncertain || refused ? command.error : null

  return (
    <Dialog open onOpenChange={(next) => (!next && !writing ? onClose() : undefined)}>
      <DialogContent
        title="Rename or move note"
        description="The file is moved on disk; its stable id and links stay unchanged."
        size="md"
      >
        <form onSubmit={submit} noValidate className="flex flex-col gap-4" data-dialog="note-move">
          <FolderPicker
            label="Folder"
            hint="The destination folder must already exist."
            value={folder}
            options={withFolderOption(options, folder)}
            onChange={(next) => {
              setFolder(next)
              setError(null)
            }}
            disabled={writing}
          />

          <Field
            label="File name"
            required
            hint="Keep or change the name; the .md extension is required."
          >
            <Input
              value={fileName}
              onChange={(event) => {
                setFileName(event.target.value)
                setError(null)
              }}
              autoFocus
              disabled={writing}
              spellCheck={false}
              data-field="note-name"
            />
          </Field>

          <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-muted">
            <span className="break-all font-mono text-ink-faint" data-current-path>
              {currentPath}
            </span>
            <span aria-hidden="true"> → </span>
            <span className="break-all font-mono text-ink-soft" data-preview-path>
              {target.length > 0 ? target : '…'}
            </span>
          </p>

          <p className="text-2xs leading-4 text-ink-faint">
            The note&apos;s title is its first heading, not its file name. Renaming the file keeps
            the note&apos;s stable id, so every link to it still resolves.
          </p>

          {error ? (
            <p
              role="alert"
              className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
            >
              {error}
            </p>
          ) : null}

          {commandError ? (
            <div
              role="alert"
              className={
                uncertain
                  ? 'rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink'
                  : 'rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink'
              }
            >
              <p className="font-medium">
                {uncertain
                  ? 'The move was not confirmed; the file may already be at the new path.'
                  : 'The server refused this move.'}
              </p>
              <p className="mt-0.5">
                {uncertain ? uncertainMessage(commandError) : userMessage(commandError)}
              </p>
            </div>
          ) : null}

          {blindRetry ? (
            <p
              role="alert"
              className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
            >
              A previous move is still unresolved. Retry it unchanged, or refresh the tree and open
              the note at its current path.
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-end gap-2">
            {attempt.attempt ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => {
                  attempt.clear()
                  setBlindRetry(false)
                  setError(null)
                  onRefreshTree()
                }}
                disabled={writing}
              >
                Refresh the tree
              </Button>
            ) : null}
            <Button type="button" variant="secondary" onClick={onClose} disabled={writing}>
              Cancel
            </Button>
            <Button type="submit" loading={writing} disabled={blindRetry} data-action="move-note">
              {uncertain && !blindRetry ? 'Retry the same move' : 'Rename or move'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
