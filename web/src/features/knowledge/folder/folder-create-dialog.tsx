/**
 * Create one vault folder, at the root or inside an existing folder.
 *
 * The command id is minted per composed path and kept until the server answers
 * definitively: a folder create can reach the disk before its receipt, so an
 * identical retry must replay the same id. A refusal leaves the tree untouched;
 * the page re-reads it instead of guessing what happened.
 */
import { useState, type FormEvent } from 'react'
import {
  newCommandId,
  uncertainMessage,
  useFolderCreateCommand,
  useIdempotentAttempt,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import type { FolderOption } from '../folder-model'
import { isValidFolderName, joinFolderPath, withFolderOption } from '../folder-model'
import { FolderPicker } from './folder-picker'

interface CreateAttempt {
  key: string
  commandId: string
}

export interface NewFolderDialogProps {
  options: FolderOption[]
  /** The folder the person is looking at; the new folder starts inside it. */
  initialParent: string
  onClose: () => void
  onCreated: (path: string) => void
  /** Re-reads the tree; the recovery step after an uncertain create. */
  onRefreshTree: () => void
}

export function NewFolderDialog({
  options,
  initialParent,
  onClose,
  onCreated,
  onRefreshTree,
}: NewFolderDialogProps) {
  const command = useFolderCreateCommand()
  const [parent, setParent] = useState(initialParent)
  const [name, setName] = useState('')
  const [nameError, setNameError] = useState<string | null>(null)
  const attempt = useIdempotentAttempt<CreateAttempt>((key) => ({
    key,
    commandId: newCommandId(),
  }))

  const trimmed = name.trim()
  const path = joinFolderPath(parent, trimmed)
  const key = path
  const uncertain = command.disposition === 'uncertain'
  const pending = command.isPending
  const blindRetry = uncertain && attempt.isStale(key)
  const error: ApiError | null =
    command.disposition === 'rejected' || uncertain ? command.error : null

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (pending || blindRetry) return
    if (!isValidFolderName(trimmed)) {
      setNameError(
        'Use one folder name without slashes, and do not start it with a dot. Choose the parent above.',
      )
      return
    }
    setNameError(null)

    const pinned = attempt.ensure(key)
    try {
      await command.mutateAsync({ commandId: pinned.commandId, path })
      attempt.clear()
      onCreated(path)
      onClose()
    } catch {
      // The server's answer stays on screen; the attempt stays pinned so an
      // unchanged retry replays the same command id.
    }
  }

  return (
    <Dialog open onOpenChange={(next) => (!next && !pending ? onClose() : undefined)}>
      <DialogContent
        title="New folder"
        description="A folder is a real directory in the server's vault. Its parent must already exist."
        size="md"
      >
        <form onSubmit={submit} noValidate className="flex flex-col gap-4" data-dialog="folder-create">
          <FolderPicker
            label="Inside"
            hint="The new folder is created inside this one."
            value={parent}
            options={withFolderOption(options, parent)}
            onChange={(next) => {
              setParent(next)
              setNameError(null)
            }}
            disabled={pending}
          />

          <Field
            label="Folder name"
            required
            error={nameError}
            hint="One name, no slashes. The folder may stay empty until a note is created inside it."
          >
            <Input
              value={name}
              onChange={(event) => {
                setName(event.target.value)
                setNameError(null)
              }}
              placeholder="Research"
              autoFocus
              disabled={pending}
              spellCheck={false}
              data-field="folder-name"
            />
          </Field>

          <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-xs leading-5 text-ink-muted">
            Creates{' '}
            <span className="font-mono text-ink-soft" data-preview-path>
              {path.length > 0 ? path : '…'}
            </span>
          </p>

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
                  ? 'The folder may or may not have been created.'
                  : 'The server refused this folder.'}
              </p>
              <p className="mt-0.5">{uncertain ? uncertainMessage(error) : userMessage(error)}</p>
              <p className="mt-1.5 font-medium">
                {uncertain
                  ? 'Retry the same folder, or refresh the tree and look for it first.'
                  : 'The tree was re-read rather than guessed. Look for the folder before choosing a different name, and check that the parent still exists.'}
              </p>
            </div>
          ) : null}

          {blindRetry ? (
            <p
              role="alert"
              className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
            >
              The name or location no longer matches the create whose outcome is unknown. Retry it
              unchanged, or refresh the tree and look for the folder first.
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-end gap-2">
            {uncertain ? (
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
              data-action="create-folder"
              title={blindRetry ? 'Retry the unknown create unchanged, or refresh the tree first.' : undefined}
            >
              {uncertain && !blindRetry ? 'Retry the same folder' : 'Create folder'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
