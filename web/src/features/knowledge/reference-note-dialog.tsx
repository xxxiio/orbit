/**
 * Add one typed reference to a note that the person picks.
 *
 * The direction is "note → project/task", so the picker lists notes and the
 * server command appends the token to the chosen note's body. Used by the task
 * drawer's Notes section (`task:<id>`); the note's own panel adds references
 * from the other side.
 */
import { useMemo, useState } from 'react'
import type { KnowledgeDocumentInfo } from '@/api/contracts'
import {
  newCommandId,
  uncertainMessage,
  useIdempotentAttempt,
  useReferenceCommand,
  userMessage,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { SearchIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'

interface LinkAttempt {
  key: string
  commandId: string
  noteId: string
}

export interface ReferenceNoteDialogProps {
  /** `<kind>:<id>`, e.g. `task:task-1`. */
  target: string
  /** How the target reads in copy ("this task", "orbit"). */
  targetLabel: string
  notes: KnowledgeDocumentInfo[]
  onClose: () => void
  onLinked: (note: KnowledgeDocumentInfo) => void
}

export function ReferenceNoteDialog({
  target,
  targetLabel,
  notes,
  onClose,
  onLinked,
}: ReferenceNoteDialogProps) {
  const command = useReferenceCommand()
  const attempt = useIdempotentAttempt<LinkAttempt>((key) => ({
    key,
    commandId: newCommandId(),
    noteId: key.split('\u0000')[0],
  }))
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const candidates = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return notes
      .filter(
        (note) =>
          needle.length === 0 ||
          note.title.toLowerCase().includes(needle) ||
          note.path.toLowerCase().includes(needle) ||
          note.id.toLowerCase().includes(needle),
      )
      .slice(0, 100)
  }, [notes, query])

  const chosen = candidates.find((note) => note.id === selected) ?? null
  const pending = command.isPending
  const uncertain = command.disposition === 'uncertain' && attempt.attempt !== null
  const unavailable = command.error?.status === 404

  const submit = async () => {
    if (!chosen || pending) return
    setNotice(null)
    const key = `${chosen.id}\u0000${target}\u0000add`
    const pinned = attempt.attempt
    if (pinned && pinned.key !== key) {
      if (command.disposition === 'uncertain') {
        setNotice('A previous reference change is still unresolved. Retry it before linking another note.')
        return
      }
      attempt.clear()
    }
    const next = pinned?.key === key ? pinned : attempt.ensure(key)
    try {
      const receipt = await command.mutateAsync({
        commandId: next.commandId,
        noteId: chosen.id,
        target,
        action: 'add',
      })
      attempt.clear()
      if (!receipt.changed) {
        setNotice(`${chosen.title} already references ${targetLabel}. Nothing was written.`)
        return
      }
      onLinked(chosen)
    } catch {
      // The server's answer stays below.
    }
  }

  return (
    <Dialog open onOpenChange={(next) => (!next && !pending ? onClose() : undefined)}>
      <DialogContent
        title="Link a note"
        description={`Adds a reference to ${targetLabel} in the chosen note. The note is re-read and published by the server.`}
        size="md"
      >
        <div className="flex flex-col gap-3" data-dialog="reference-note">
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search note titles, ids or paths…"
            aria-label="Search notes"
            leadingIcon={<SearchIcon size={14} />}
            autoFocus
            disabled={pending}
            data-field="note-search"
          />

          {candidates.length === 0 ? (
            <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-3 text-xs leading-5 text-ink-muted">
              {notes.length === 0
                ? 'The vault has no notes yet. Create one in Knowledge first.'
                : `No note matches “${query.trim()}”.`}
            </p>
          ) : (
            <ul
              aria-label="Notes"
              className="scrollbar-slim max-h-64 overflow-y-auto rounded-md border border-line"
            >
              {candidates.map((note) => {
                const active = note.id === selected
                return (
                  <li key={note.id}>
                    <button
                      type="button"
                      aria-pressed={active}
                      data-note-option={note.id}
                      onClick={() => {
                        setSelected(note.id)
                        setNotice(null)
                      }}
                      disabled={pending}
                      className={cn(
                        'flex w-full flex-col items-start gap-0.5 border-b border-line px-3 py-2 text-left transition-colors last:border-b-0',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/60',
                        active ? 'bg-accent-soft text-accent-soft-ink' : 'hover:bg-surface-hover/60',
                      )}
                    >
                      <span className="w-full truncate text-sm font-medium">{note.title}</span>
                      <span className="w-full truncate font-mono text-2xs text-ink-muted">
                        {note.path} · {note.id}
                      </span>
                    </button>
                  </li>
                )
              })}
            </ul>
          )}

          {notice ? (
            <p role="alert" className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink">
              {notice}
            </p>
          ) : null}

          {command.error ? (
            <div
              role="alert"
              className={cn(
                'rounded-md border px-3 py-2 text-xs leading-5',
                uncertain
                  ? 'border-warning/40 bg-warning-soft text-warning-ink'
                  : 'border-danger/30 bg-danger-soft text-danger-ink',
              )}
            >
              <p className="font-medium">
                {unavailable
                  ? 'This server does not accept reference commands yet.'
                  : uncertain
                    ? 'The reference was not confirmed; the note may already contain it.'
                    : 'The server refused this reference.'}
              </p>
              <p className="mt-0.5">
                {unavailable
                  ? 'The note was not changed. Update the server to link notes to projects and tasks.'
                  : uncertain
                    ? uncertainMessage(command.error)
                    : userMessage(command.error)}
              </p>
            </div>
          ) : null}

          <p role="status" aria-live="polite" className="min-h-4 text-2xs text-ink-muted">
            {pending ? 'Sending the reference…' : null}
          </p>

          <div className="flex flex-wrap items-center justify-end gap-2">
            {uncertain && attempt.attempt ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                disabled={pending}
                onClick={() => {
                  const pinned = attempt.attempt
                  if (!pinned) return
                  void command
                    .mutateAsync({
                      commandId: pinned.commandId,
                      noteId: pinned.noteId,
                      target,
                      action: 'add',
                    })
                    .then((receipt) => {
                      attempt.clear()
                      if (receipt.changed) {
                        const note = notes.find((candidate) => candidate.id === pinned.noteId)
                        if (note) onLinked(note)
                      }
                    })
                    .catch(() => undefined)
                }}
              >
                Retry the same link
              </Button>
            ) : null}
            <Button type="button" variant="secondary" onClick={onClose} disabled={pending}>
              Cancel
            </Button>
            <Button
              type="button"
              onClick={() => void submit()}
              loading={pending}
              disabled={!chosen || pending}
              data-action="link-note"
            >
              Link this note
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
