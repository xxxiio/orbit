/**
 * Link another note by appending its stable id as `[[id]]`.
 *
 * Only real note ids are offered, because the vault's link grammar cannot carry
 * spaces or titles: free text would produce a token that silently resolves to
 * nothing. The append itself is a fenced `Update` (see `useNoteBodyEdit`), so the
 * note is re-read first and an external writer's change is reported, never
 * overwritten.
 */
import { useMemo, useState } from 'react'
import type { UseQueryResult } from '@tanstack/react-query'
import type { KnowledgeDocumentInfo } from '@/api/contracts'
import { uncertainMessage, userMessage, type ApiError } from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { SearchIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { appendNoteLink } from '../reference-model'
import { useNoteBodyEdit } from './use-note-body-edit'

export interface NoteLinkDialogProps {
  currentNoteId: string
  currentTitle: string
  notes: KnowledgeDocumentInfo[]
  document: UseQueryResult<{ path: string; body: string }, ApiError>
  onClose: () => void
  onLinked: (note: KnowledgeDocumentInfo) => void
}

export function NoteLinkDialog({
  currentNoteId,
  currentTitle,
  notes,
  document,
  onClose,
  onLinked,
}: NoteLinkDialogProps) {
  const edit = useNoteBodyEdit({ noteId: currentNoteId, document })
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const candidates = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return notes
      .filter((note) => note.id !== currentNoteId)
      .filter(
        (note) =>
          needle.length === 0 ||
          note.title.toLowerCase().includes(needle) ||
          note.path.toLowerCase().includes(needle) ||
          note.id.toLowerCase().includes(needle),
      )
      .slice(0, 100)
  }, [notes, currentNoteId, query])

  const chosen = candidates.find((note) => note.id === selected) ?? null
  const pending = edit.pending

  const submit = async () => {
    if (!chosen || pending) return
    setNotice(null)
    const outcome = await edit.run(`${currentNoteId}\u0000link\u0000${chosen.id}`, (body) =>
      appendNoteLink(body, chosen.id),
    )
    switch (outcome.status) {
      case 'accepted':
        onLinked(chosen)
        return
      case 'unchanged':
        setNotice(`${chosen.title} is already linked from this note. Nothing was sent.`)
        return
      case 'stale':
        setNotice('The note identity changed on disk. Reload the note before linking; nothing was sent.')
        return
      case 'unreadable':
        setNotice('The note could not be re-read, so nothing was sent. Try again.')
        return
      case 'blocked':
        setNotice('A previous write is still unresolved. Retry it before linking another note.')
        return
      default:
        return
    }
  }

  return (
    <Dialog open onOpenChange={(next) => (!next && !pending ? onClose() : undefined)}>
      <DialogContent
        title="Link a note"
        description={`Appends a [[note id]] link to ${currentTitle}. The note is re-read before it is written.`}
        size="md"
      >
        <div className="flex flex-col gap-3" data-dialog="note-link">
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search note titles, ids or paths…"
            aria-label="Search notes to link"
            leadingIcon={<SearchIcon size={14} />}
            autoFocus
            disabled={pending}
            data-field="note-search"
          />

          {candidates.length === 0 ? (
            <p className="rounded-md border border-line bg-surface-sunken/40 px-3 py-3 text-xs leading-5 text-ink-muted">
              {notes.length <= 1
                ? 'There is no other note to link in this vault yet.'
                : `No note matches “${query.trim()}”.`}
            </p>
          ) : (
            <ul
              aria-label="Notes to link"
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
                      className={cn(
                        'flex w-full flex-col items-start gap-0.5 border-b border-line px-3 py-2 text-left transition-colors last:border-b-0',
                        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/60',
                        active ? 'bg-accent-soft text-accent-soft-ink' : 'hover:bg-surface-hover/60',
                      )}
                      disabled={pending}
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
            <p
              role="alert"
              className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
            >
              {notice}
            </p>
          ) : null}

          {edit.error ? (
            <div
              role="alert"
              className={cn(
                'rounded-md border px-3 py-2 text-xs leading-5',
                edit.uncertain
                  ? 'border-warning/40 bg-warning-soft text-warning-ink'
                  : 'border-danger/30 bg-danger-soft text-danger-ink',
              )}
            >
              <p className="font-medium">
                {edit.uncertain
                  ? 'The link write was not confirmed; the note may already contain it.'
                  : 'The server refused this change to the note.'}
              </p>
              <p className="mt-0.5">
                {edit.uncertain ? uncertainMessage(edit.error) : userMessage(edit.error)}
              </p>
            </div>
          ) : null}

          <div className="flex flex-wrap items-center justify-end gap-2">
            {edit.uncertain ? (
              <Button type="button" variant="secondary" size="sm" onClick={() => void edit.retry()} disabled={pending}>
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
