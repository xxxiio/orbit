/**
 * One Markdown note: rendered body, `[[id]]` relations, precise edit and move.
 *
 * Writes never send a blind whole-body overwrite. Edit re-reads the file, checks
 * it still matches the bytes the editor was opened from, then computes
 * `expected`/`replacement` from that fresh read. Move re-reads the file for
 * `expected` immediately before sending. The same command id is reused for an
 * identical retry because the file may already have changed on disk.
 */
import { useEffect, useRef, useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import type { MarkdownCommand } from '@/api/contracts'
import {
  newCommandId,
  uncertainMessage,
  useMarkdownCommand,
  useMarkdownDocument,
  useMarkdownInventory,
  useMarkdownRelations,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { BookIcon, LoaderIcon, RefreshIcon } from '@/components/ui/icons'
import { Markdown } from '@/components/markdown'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Skeleton, SkeletonText } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import { useToast } from '@/components/ui/toast'
import { cn } from '@/components/ui/utils'
import {
  isNotePath,
  joinMarkdown,
  noteHref,
  noteTitleFromPath,
  splitMarkdown,
  splitRelations,
} from './knowledge-model'

interface SaveAttempt {
  key: string
  commandId: string
  command: MarkdownCommand
}

export interface NoteViewProps {
  path: string
  onRefreshTree: () => void
}

export function NoteView({ path, onRefreshTree }: NoteViewProps) {
  const document = useMarkdownDocument(path)
  const inventory = useMarkdownInventory()
  const parts = splitMarkdown(document.data?.body ?? '')
  const noteId = parts.noteId
  const relations = useMarkdownRelations(noteId ?? undefined)
  const command = useMarkdownCommand()
  const { toast } = useToast()
  const navigate = useNavigate()

  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState('')
  const [baseline, setBaseline] = useState<string | null>(null)
  const [localError, setLocalError] = useState<string | null>(null)
  const [stale, setStale] = useState(false)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const [moveOpen, setMoveOpen] = useState(false)
  const attemptRef = useRef<SaveAttempt | null>(null)

  useEffect(() => {
    if (!announcement) return
    const timer = window.setTimeout(() => setAnnouncement(null), 6000)
    return () => window.clearTimeout(timer)
  }, [announcement])

  useEffect(() => {
    if (!editing || baseline === null || !document.data) return
    setStale(document.data.body !== baseline)
  }, [document.data, editing, baseline])

  const inventoryEntry = inventory.data?.find((note) => note.path === path)
  const title = inventoryEntry?.title || noteTitleFromPath(path)
  const writing = command.isPending
  const refused = command.disposition === 'rejected'
  const uncertain = command.disposition === 'uncertain'
  const commandError: ApiError | null = refused || uncertain ? command.error : null

  const resetEdit = () => {
    setEditing(false)
    setDraft('')
    setBaseline(null)
    setLocalError(null)
    setStale(false)
    attemptRef.current = null
  }

  const reload = async () => {
    resetEdit()
    setAnnouncement('Note reloaded from disk.')
    await Promise.all([document.refetch(), inventory.refetch()])
  }

  const send = async (attempt: SaveAttempt) => {
    setLocalError(null)
    try {
      await command.mutateAsync({ commandId: attempt.commandId, command: attempt.command })
      resetEdit()
      setAnnouncement('Note saved.')
      toast({ tone: 'success', title: 'Note saved', description: path })
      await Promise.all([document.refetch(), inventory.refetch()])
    } catch {
      // `command.error` and its disposition carry the server's answer.
    }
  }

  const save = async () => {
    if (!noteId) {
      setLocalError(
        'This file has no orbit_id in its frontmatter, so the server cannot accept an edit. Add the id outside Orbit first.',
      )
      return
    }
    if (baseline === null) {
      setLocalError('Open the note again before saving.')
      return
    }
    if (draft === splitMarkdown(baseline).body) {
      resetEdit()
      return
    }

    const key = `${path}\u0000${baseline}\u0000${draft}`
    if (attemptRef.current?.key === key) {
      await send(attemptRef.current)
      return
    }

    // Read the file again immediately before writing. The editor's baseline is
    // a fence, not a cached body to overwrite blindly.
    const fresh = await document.refetch()
    const freshRaw = fresh.data?.body
    if (freshRaw === undefined) {
      setLocalError('The note could not be re-read, so nothing was sent. Try again.')
      return
    }
    if (freshRaw !== baseline) {
      setStale(true)
      setLocalError(
        'The file changed on disk since you began editing. Reload the note and reapply your change.',
      )
      return
    }
    const freshParts = splitMarkdown(freshRaw)
    if (freshParts.noteId !== noteId) {
      setLocalError(
        'The note identity changed on disk. Reload the note before editing; nothing was sent.',
      )
      return
    }

    const attempt: SaveAttempt = {
      key,
      commandId: newCommandId(),
      command: {
        kind: 'Update',
        note_id: noteId,
        expected: freshRaw,
        replacement: joinMarkdown(freshParts.frontmatter, draft),
      },
    }
    attemptRef.current = attempt
    await send(attempt)
  }

  if (document.isPending && !document.data) {
    return (
      <Panel aria-busy="true" aria-label="Loading note">
        <div className="border-b border-line px-5 py-4">
          <Skeleton className="h-3 w-24" />
          <Skeleton className="mt-3 h-7 w-72" />
        </div>
        <div className="px-5 py-5">
          <SkeletonText lines={6} />
        </div>
      </Panel>
    )
  }

  if (document.error && !document.data) {
    const status = document.error.status
    const missing = status === 404
    return (
      <Panel>
        <EmptyState
          icon={<BookIcon size={21} />}
          title={missing ? 'This note could not be found' : 'This note could not be read'}
          description={
            missing ? (
              <>
                The vault has no file at <span className="font-mono text-ink-soft">{path}</span>.
                It may have been moved or removed from another device.
              </>
            ) : (
              <>
                {userMessage(document.error)}
                {isNotePath(path) ? null : ' This path is not a valid vault path.'}
              </>
            )
          }
          action={
            <Button asChild variant="secondary" size="sm">
              <Link to="/knowledge">Back to Knowledge</Link>
            </Button>
          }
          secondaryAction={
            <Button variant="quiet" size="sm" onClick={() => void document.refetch()}>
              Try again
            </Button>
          }
          className="min-h-[20rem]"
        />
      </Panel>
    )
  }

  const body = editing ? draft : parts.body
  const relationRows = relations.data ? splitRelations(relations.data) : null
  const idToPath = new Map((inventory.data ?? []).map((note) => [note.id, note.path]))

  return (
    <>
      <Panel className="min-w-0 overflow-hidden">
        <PanelHeader
          eyebrow="Note"
          title={
            <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
              {title}
              {noteId ? (
                <span className="font-mono text-2xs font-normal text-ink-faint" title="Stable note id">
                  {noteId}
                </span>
              ) : (
                <Badge tone="warning">No note id</Badge>
              )}
            </span>
          }
          description={
            <span className="font-mono text-2xs text-ink-muted" title={path}>
              {path}
            </span>
          }
          actions={
            <div className="flex flex-wrap items-center gap-1.5">
              <Button
                variant="quiet"
                size="sm"
                leadingIcon={<RefreshIcon size={14} />}
                onClick={() => void reload()}
                disabled={writing}
              >
                Reload
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setMoveOpen(true)}
                disabled={writing || editing || !noteId}
              >
                Move
              </Button>
              {editing ? (
                <Button variant="secondary" size="sm" onClick={resetEdit} disabled={writing}>
                  Cancel
                </Button>
              ) : (
                <Button
                  size="sm"
                  onClick={() => {
                    setDraft(parts.body)
                    setBaseline(document.data?.body ?? parts.body)
                    setLocalError(null)
                    setStale(false)
                    attemptRef.current = null
                    setEditing(true)
                  }}
                  disabled={!noteId}
                  title={noteId ? undefined : 'A note needs an orbit_id before it can be edited.'}
                >
                  Edit
                </Button>
              )}
              {editing ? (
                <Button size="sm" loading={writing} onClick={() => void save()}>
                  {uncertain || refused ? 'Retry the same change' : 'Save note'}
                </Button>
              ) : null}
            </div>
          }
        />

        <PanelBody className="min-w-0">
          {stale ? (
            <div
              role="alert"
              className="mb-4 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
            >
              The file changed on disk while this editor was open. Reload the note before saving;
              nothing has been overwritten.
            </div>
          ) : null}

          {!noteId ? (
            <div className="mb-4 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink">
              This file has no <span className="font-mono">orbit_id</span> in its frontmatter. It is
              readable, but the server cannot accept edits or moves until it has a stable id.
            </div>
          ) : null}

          {editing ? (
            <Textarea
              value={draft}
              onChange={(event) => {
                setDraft(event.target.value)
                setLocalError(null)
              }}
              aria-label="Note body"
              className="min-h-[24rem] font-mono text-sm leading-6"
              disabled={writing}
              spellCheck={false}
            />
          ) : (
            <div data-note-body className="max-w-[72ch]">
              <Markdown source={body} />
            </div>
          )}

          {localError ? (
            <div
              role="alert"
              className="mt-4 rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
            >
              {localError}
            </div>
          ) : null}

          {commandError ? (
            <div
              role="alert"
              className={cn(
                'mt-4 rounded-md border px-3 py-2 text-xs leading-5',
                uncertain
                  ? 'border-warning/40 bg-warning-soft text-warning-ink'
                  : 'border-danger/30 bg-danger-soft text-danger-ink',
              )}
            >
              <p className="font-medium">
                {uncertain
                  ? 'The write was not confirmed; the file may still have changed.'
                  : 'The server refused this write.'}
              </p>
              <p className="mt-0.5">
                {uncertain ? uncertainMessage(commandError) : userMessage(commandError)}
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                {attemptRef.current ? (
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => attemptRef.current && void send(attemptRef.current)}
                    disabled={writing}
                  >
                    Retry the same change
                  </Button>
                ) : null}
                <Button size="sm" variant="quiet" onClick={() => void reload()} disabled={writing}>
                  Reload note
                </Button>
              </div>
            </div>
          ) : null}

          <p role="status" aria-live="polite" className="mt-3 min-h-4 text-2xs text-ink-muted">
            {writing ? (
              <span className="inline-flex items-center gap-1.5">
                <LoaderIcon size={12} className="animate-spin text-accent" />
                Saving to disk…
              </span>
            ) : (
              announcement
            )}
          </p>

          <p className="mt-2 text-2xs leading-4 text-ink-faint">
            Saves read the file again and send the exact bytes the server must replace. If the
            write fails, the file may still have changed; reload before editing again.
          </p>
        </PanelBody>
      </Panel>

      <Panel className="mt-4 overflow-hidden" aria-label="Relations">
        <PanelHeader
          title="Relations"
          description="Links are explicit [[note-id]] references, resolved against the vault's stable ids."
          actions={
            <span className="text-2xs text-ink-muted">
              {relations.data
                ? `${relations.data.outgoing.length} outgoing · ${relations.data.backlinks.length} backlinks`
                : ''}
            </span>
          }
        />
        <PanelBody>
          {!noteId ? (
            <p className="text-xs leading-5 text-ink-muted">
              Relations are unavailable without an <span className="font-mono">orbit_id</span>.
            </p>
          ) : relations.isPending && !relations.data ? (
            <SkeletonText lines={2} />
          ) : relations.error && !relations.data ? (
            <ErrorState
              compact
              title="Relations could not be read"
              description={userMessage(relations.error)}
              onRetry={() => void relations.refetch()}
              retrying={relations.isFetching}
            />
          ) : relationRows ? (
            <div className="flex flex-col gap-4">
              <RelationGroup
                label="Links to"
                ids={relationRows.resolvedOutgoing}
                idToPath={idToPath}
                empty="No resolvable outgoing links."
              />
              <RelationGroup
                label="Linked from"
                ids={relations.data?.backlinks ?? []}
                idToPath={idToPath}
                empty="No other note links here yet."
              />
              {relationRows.unresolved.length > 0 ? (
                <div>
                  <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                    Unresolved references
                  </p>
                  <ul className="mt-1.5 flex flex-wrap gap-1.5" aria-label="Unresolved references">
                    {relationRows.unresolved.map((id) => (
                      <li key={id}>
                        <span
                          className="inline-flex items-center rounded-full border border-dashed border-line-strong bg-surface-sunken px-2 py-0.5 font-mono text-2xs text-ink-muted"
                          title="No note in this vault has this id"
                        >
                          [[{id}]]
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
            </div>
          ) : null}
        </PanelBody>
      </Panel>

      {moveOpen ? (
        <NoteMoveDialog
          currentPath={path}
          noteId={noteId ?? ''}
          onClose={() => setMoveOpen(false)}
          onRefreshTree={onRefreshTree}
          onMoved={(target) => {
            setMoveOpen(false)
            navigate(noteHref(target))
            toast({ tone: 'success', title: 'Note moved', description: target })
          }}
        />
      ) : null}
    </>
  )
}

function RelationGroup({
  label,
  ids,
  idToPath,
  empty,
}: {
  label: string
  ids: string[]
  idToPath: Map<string, string>
  empty: string
}) {
  return (
    <div>
      <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</p>
      {ids.length === 0 ? (
        <p className="mt-1 text-xs leading-5 text-ink-muted">{empty}</p>
      ) : (
        <ul className="mt-1.5 flex flex-wrap gap-1.5" aria-label={label}>
          {ids.map((id) => {
            const target = idToPath.get(id)
            return (
              <li key={id}>
                {target ? (
                  <Link
                    to={noteHref(target)}
                    className="inline-flex items-center gap-1.5 rounded-full border border-accent/30 bg-accent-soft px-2 py-0.5 font-mono text-2xs text-accent-soft-ink transition-colors hover:border-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                    title={target}
                  >
                    {id}
                  </Link>
                ) : (
                  <span className="inline-flex items-center rounded-full border border-line bg-surface-sunken px-2 py-0.5 font-mono text-2xs text-ink-muted">
                    {id}
                  </span>
                )}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}

interface MoveAttempt {
  key: string
  commandId: string
  command: MarkdownCommand
}

function NoteMoveDialog({
  currentPath,
  noteId,
  onClose,
  onMoved,
  onRefreshTree,
}: {
  currentPath: string
  noteId: string
  onClose: () => void
  onMoved: (path: string) => void
  onRefreshTree: () => void
}) {
  const source = useMarkdownDocument(currentPath)
  const command = useMarkdownCommand()
  const [target, setTarget] = useState(currentPath)
  const [error, setError] = useState<string | null>(null)
  const [blindRetry, setBlindRetry] = useState(false)
  const attemptRef = useRef<MoveAttempt | null>(null)
  const writing = command.isPending
  const uncertain = command.disposition === 'uncertain'
  const refused = command.disposition === 'rejected'

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (writing || blindRetry) return
    const next = target.trim().replace(/^\/+/, '')
    if (!isNotePath(next)) {
      setError('Use a relative .md path such as archive/decision.md.')
      return
    }
    if (next === currentPath) {
      setError('Choose a different path. The note is already there.')
      return
    }

    const key = `${currentPath}\u0000${next}`
    if (attemptRef.current?.key === key) {
      try {
        await command.mutateAsync({
          commandId: attemptRef.current.commandId,
          command: attemptRef.current.command,
        })
        onMoved(next)
      } catch {
        // The server's answer is rendered below.
      }
      return
    }
    if (attemptRef.current && attemptRef.current.key !== key) {
      if (command.disposition === 'uncertain') {
        setBlindRetry(true)
        setError(
          'The previous move may have completed. Retry it unchanged, or refresh the tree before choosing another path.',
        )
        return
      }
      // A definitive refusal did not move anything; a new destination is a
      // genuinely new command, not a blind retry.
      attemptRef.current = null
      setBlindRetry(false)
    }

    const fresh = await source.refetch()
    const raw = fresh.data?.body
    if (raw === undefined) {
      setError('The note could not be re-read, so nothing was moved. Try again.')
      return
    }
    if (splitMarkdown(raw).noteId !== noteId) {
      setError('The note identity changed on disk. Reload it before moving; nothing was sent.')
      return
    }

    const attempt: MoveAttempt = {
      key,
      commandId: newCommandId(),
      command: {
        kind: 'Move',
        note_id: noteId,
        from: currentPath,
        to: next,
        expected: raw,
      },
    }
    attemptRef.current = attempt
    try {
      await command.mutateAsync({ commandId: attempt.commandId, command: attempt.command })
      onMoved(next)
    } catch {
      // The server's answer is rendered below.
    }
  }

  const commandError: ApiError | null = uncertain || refused ? command.error : null

  return (
    <Dialog open onOpenChange={(next) => (!next && !writing ? onClose() : undefined)}>
      <DialogContent
        title="Move note"
        description="The file is moved on disk; its stable id and links stay unchanged."
        size="md"
      >
        <form onSubmit={submit} noValidate className="flex flex-col gap-4">
          <Field
            label="New path"
            required
            hint="The destination folder must already exist. The .md extension is required."
          >
            <Input
              value={target}
              onChange={(event) => {
                setTarget(event.target.value)
                setError(null)
              }}
              autoFocus
              disabled={writing}
              spellCheck={false}
            />
          </Field>

          {error ? (
            <p role="alert" className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink">
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
            <p role="alert" className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink">
              A previous move is still unresolved. Retry it unchanged, or refresh the tree and open
              the note at its current path.
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-end gap-2">
            {attemptRef.current ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => {
                  attemptRef.current = null
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
            <Button type="submit" loading={writing} disabled={blindRetry}>
              {uncertain && !blindRetry ? 'Retry the same move' : 'Move note'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
