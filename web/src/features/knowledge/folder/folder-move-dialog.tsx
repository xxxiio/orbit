/**
 * Rename or move one folder. The server treats both as the same command: a move
 * whose `to` path is the new location, and a rename is a move that keeps the
 * parent and changes the last segment.
 *
 * The picker never offers the folder itself or anything inside it, because the
 * server refuses a nested destination; it does *not* hide an existing
 * destination, because only the server knows what is on disk right now — a
 * refusal is quoted, and the tree is left as the server describes it.
 *
 * A folder move changes every note path beneath it, so the page re-reads the
 * whole knowledge family and follows an open note.
 */
import { useState, type FormEvent } from 'react'
import {
  newCommandId,
  uncertainMessage,
  useFolderMoveCommand,
  useIdempotentAttempt,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import type { FolderOption } from '../folder-model'
import {
  folderName,
  folderParent,
  isWithinFolder,
  isValidFolderName,
  joinFolderPath,
  withFolderOption,
} from '../folder-model'
import { FolderPicker } from './folder-picker'

interface MoveAttempt {
  key: string
  commandId: string
}

export interface FolderMoveDialogProps {
  path: string
  /** Already excludes the folder and its descendants. */
  options: FolderOption[]
  onClose: () => void
  onMoved: (from: string, to: string) => void
  onRefreshTree: () => void
}

export function FolderMoveDialog({
  path,
  options,
  onClose,
  onMoved,
  onRefreshTree,
}: FolderMoveDialogProps) {
  const command = useFolderMoveCommand()
  const [parent, setParent] = useState(() => folderParent(path))
  const [name, setName] = useState(() => folderName(path))
  const [nameError, setNameError] = useState<string | null>(null)
  const [localError, setLocalError] = useState<string | null>(null)
  const attempt = useIdempotentAttempt<MoveAttempt>((key) => ({
    key,
    commandId: newCommandId(),
  }))

  const trimmed = name.trim()
  const to = joinFolderPath(parent, trimmed)
  const key = `${path}\u0000${to}`
  const pending = command.isPending
  const uncertain = command.disposition === 'uncertain'
  const blindRetry = uncertain && attempt.isStale(key)
  const error: ApiError | null =
    command.disposition === 'rejected' || uncertain ? command.error : null
  const unchanged = to === path

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (pending || blindRetry) return
    if (!isValidFolderName(trimmed)) {
      setNameError(
        'Use one folder name without slashes, and do not start it with a dot. Choose the location above.',
      )
      return
    }
    if (unchanged) {
      setLocalError('The folder is already there. Change the name or the location.')
      return
    }
    if (isWithinFolder(to, path)) {
      setLocalError('A folder cannot move inside itself. Choose a location outside it.')
      return
    }
    setNameError(null)
    setLocalError(null)

    const pinned = attempt.ensure(key)
    try {
      await command.mutateAsync({ commandId: pinned.commandId, from: path, to })
      attempt.clear()
      onMoved(path, to)
      onClose()
    } catch {
      // The server's answer stays on screen; the attempt stays pinned.
    }
  }

  return (
    <Dialog open onOpenChange={(next) => (!next && !pending ? onClose() : undefined)}>
      <DialogContent
        title="Rename or move folder"
        description="Every note inside keeps its stable id, so its links keep working even though its path changes."
        size="md"
      >
        <form onSubmit={submit} noValidate className="flex flex-col gap-4" data-dialog="folder-move">
          <Field label="Name" required error={nameError} hint="The folder's last part, without slashes.">
            <Input
              value={name}
              onChange={(event) => {
                setName(event.target.value)
                setNameError(null)
                setLocalError(null)
              }}
              autoFocus
              disabled={pending}
              spellCheck={false}
              data-field="folder-name"
            />
          </Field>

          <FolderPicker
            label="Location"
            hint="The folder itself and the folders inside it are not offered: the server refuses that move."
            value={parent}
            options={withFolderOption(options, parent)}
            onChange={(next) => {
              setParent(next)
              setLocalError(null)
            }}
            disabled={pending}
          />

          <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-muted">
            <span className="font-mono text-ink-faint" data-current-path>
              {path}
            </span>
            <span aria-hidden="true"> → </span>
            <span className="font-mono text-ink-soft" data-preview-path>
              {to.length > 0 ? to : '…'}
            </span>
          </p>

          {localError ? (
            <p
              role="alert"
              className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
            >
              {localError}
            </p>
          ) : null}

          {error ? (
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
                  ? 'The folder move was not confirmed; the folder may already be at the new path.'
                  : 'The server refused this move.'}
              </p>
              <p className="mt-0.5">{uncertain ? uncertainMessage(error) : userMessage(error)}</p>
              <p className="mt-1.5 font-medium">
                {uncertain
                  ? 'Retry the same move, or refresh the tree and look for the folder first.'
                  : 'The tree was re-read rather than guessed. Check both locations: whether the destination already exists, and whether the current location still does.'}
              </p>
            </div>
          ) : null}

          {blindRetry ? (
            <p
              role="alert"
              className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
            >
              A previous move is still unresolved. Retry it unchanged, or refresh the tree and look
              for the folder at its current path.
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-end gap-2">
            {attempt.attempt ? (
              <Button
                type="button"
                variant="secondary"
                onClick={() => {
                  attempt.clear()
                  onRefreshTree()
                }}
                disabled={pending}
              >
                Refresh the tree
              </Button>
            ) : null}
            <Button type="button" variant="secondary" onClick={onClose} disabled={pending}>
              Cancel
            </Button>
            <Button
              type="submit"
              loading={pending}
              disabled={blindRetry}
              data-action="move-folder"
              title={blindRetry ? 'Retry the unresolved move unchanged, or refresh the tree first.' : undefined}
            >
              {uncertain && !blindRetry ? 'Retry the same move' : 'Rename or move'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
