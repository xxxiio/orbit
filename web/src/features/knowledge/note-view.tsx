/**
 * One Markdown note: rendered body, `[[id]]` relations, typed references, and
 * precise edit / move / link.
 *
 * Writes never send a blind whole-body overwrite. Edit re-reads the file, checks
 * it still matches the bytes the editor was opened from, then computes
 * `expected`/`replacement` from that fresh read. Move re-reads the file for
 * `expected` immediately before sending. Link and unlink do the same through
 * `useNoteBodyEdit`. The same command id is reused for an identical retry
 * because the file may already have changed on disk.
 *
 * The relations panel (`note/note-relations`), the reference panel
 * (`note/note-references`) and the dialogs own their own chrome; this module owns
 * the page and the editor's byte fence.
 */
import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import type { MarkdownCommand } from '@/api/contracts'
import {
  newCommandId,
  uncertainMessage,
  useIdempotentAttempt,
  useMarkdownCommand,
  useMarkdownDocument,
  useMarkdownInventory,
  useMarkdownRelations,
  useProject,
  useWorkspaceSnapshot,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { BookIcon, LinkIcon, LoaderIcon, RefreshIcon } from '@/components/ui/icons'
import { Markdown, MarkdownEditor } from '@/components/markdown'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Skeleton, SkeletonText } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { cn } from '@/components/ui/utils'
import type { FolderOption } from './folder-model'
import { noteFolder } from './folder-model'
import {
  folderHref,
  isNotePath,
  joinMarkdown,
  noteHref,
  noteTitleFromPath,
  splitMarkdown,
  taskSurfaceHref,
} from './knowledge-model'
import { NoteContextBar } from './note/note-context-bar'
import { NoteMoveDialog } from './note/note-move-dialog'
import { NoteLinkDialog } from './note/note-link-dialog'
import { NoteReferences } from './note/note-references'
import { NoteRelations } from './note/note-relations'
import { useNoteBodyEdit } from './note/use-note-body-edit'
import { removeNoteLink } from './reference-model'

interface SaveAttempt {
  key: string
  commandId: string
  command: MarkdownCommand
}

export interface NoteContext {
  /** The project this note was opened from. */
  projectId?: string
  /** The task this note was opened from. */
  taskId?: string
}

export interface NoteViewProps {
  path: string
  onRefreshTree: () => void
  folderOptions: FolderOption[]
  context?: NoteContext
  /**
   * Drops the task context from the address. On this surface `?task=` is a note
   * context, so removing it leaves the note open and only stops the link.
   */
  onClearTaskContext?: () => void
}

export function NoteView({
  path,
  onRefreshTree,
  folderOptions,
  context,
  onClearTaskContext,
}: NoteViewProps) {
  const document = useMarkdownDocument(path)
  const inventory = useMarkdownInventory()
  const snapshot = useWorkspaceSnapshot()
  const parts = splitMarkdown(document.data?.body ?? '')
  const noteId = parts.noteId
  const relations = useMarkdownRelations(noteId ?? undefined)
  const command = useMarkdownCommand()
  const bodyEdit = useNoteBodyEdit({ noteId, document })
  const project = useProject(context?.projectId)
  const { toast } = useToast()
  const navigate = useNavigate()

  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState('')
  const [baseline, setBaseline] = useState<string | null>(null)
  const [localError, setLocalError] = useState<string | null>(null)
  const [stale, setStale] = useState(false)
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const [moveOpen, setMoveOpen] = useState(false)
  const [linkOpen, setLinkOpen] = useState(false)
  const [unlinkingId, setUnlinkingId] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const attempt = useIdempotentAttempt<SaveAttempt, { command: MarkdownCommand }>((key, payload) => ({
    key,
    commandId: newCommandId(),
    command: payload.command,
  }))

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
  const writing = command.isPending || saving
  const refused = command.disposition === 'rejected'
  const uncertain = command.disposition === 'uncertain'
  const commandError: ApiError | null = refused || uncertain ? command.error : null

  const resetEdit = () => {
    setEditing(false)
    setDraft('')
    setBaseline(null)
    setLocalError(null)
    setStale(false)
    attempt.clear()
  }

  const reload = async () => {
    const dirty = editing && baseline !== null && draft !== splitMarkdown(baseline).body
    if (
      dirty &&
      !window.confirm('Discard the unsaved edits and reload this note from disk?')
    ) {
      return
    }
    resetEdit()
    command.reset()
    bodyEdit.reset()
    setLocalError(null)
    setAnnouncement(null)
    try {
      await Promise.all([
        document.refetch({ throwOnError: true }),
        inventory.refetch({ throwOnError: true }),
      ])
      setAnnouncement('Note reloaded from disk.')
    } catch {
      setLocalError(
        'The note could not be re-read, so the text on screen may be older than the file. Try again.',
      )
    }
  }

  const send = async (pinned: SaveAttempt) => {
    setLocalError(null)
    try {
      await command.mutateAsync({ commandId: pinned.commandId, command: pinned.command })
      resetEdit()
      setAnnouncement('Note saved.')
      toast({ tone: 'success', title: 'Note saved', description: path })
      await Promise.all([document.refetch(), inventory.refetch()])
    } catch {
      // `command.error` and its disposition carry the server's answer.
    }
  }

  const save = async () => {
    // The pre-read below is asynchronous: without this guard a double click
    // would send the same edit twice before the first command is pending.
    if (saving || command.isPending) return
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

    setSaving(true)
    try {
      const key = `${path}\u0000${baseline}\u0000${draft}`
      if (attempt.attempt?.key === key) {
        await send(attempt.attempt)
        return
      }

      // Read the file again immediately before writing. The editor's baseline
      // is a fence, not a cached body to overwrite blindly. `throwOnError`
      // matters: in React Query v5 a plain `refetch()` resolves with stale
      // `data` when the read fails, which would build an Update from bytes the
      // server never confirmed.
      let freshRaw: string | undefined
      try {
        const fresh = await document.refetch({ throwOnError: true })
        freshRaw = fresh.data?.body
      } catch {
        setLocalError('The note could not be re-read, so nothing was sent. Try again.')
        return
      }
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

      const pinned = attempt.ensure(key, {
        command: {
          kind: 'Update',
          note_id: noteId,
          expected: freshRaw,
          replacement: joinMarkdown(freshParts.frontmatter, draft),
        },
      })
      await send(pinned)
    } finally {
      setSaving(false)
    }
  }

  const unlink = async (targetId: string) => {
    if (!noteId || editing || command.isPending) return
    setUnlinkingId(targetId)
    setLocalError(null)
    const outcome = await bodyEdit.run(`${path}\u0000unlink\u0000${targetId}`, (body) =>
      removeNoteLink(body, targetId),
    )
    setUnlinkingId(null)
    switch (outcome.status) {
      case 'accepted':
        setAnnouncement(`Link to ${targetId} removed.`)
        return
      case 'unchanged':
        setLocalError(`This note does not contain a [[${targetId}]] link. Nothing was sent.`)
        return
      case 'stale':
        setLocalError(
          'The note identity changed on disk. Reload the note before unlinking; nothing was sent.',
        )
        return
      case 'unreadable':
        setLocalError('The note could not be re-read, so nothing was sent. Try again.')
        return
      case 'blocked':
        setLocalError(
          'A previous link change is still unresolved. Retry it, or reload the note, before unlinking another link.',
        )
        return
      default:
        return
    }
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
              <Link to={folderHref(noteFolder(path))}>Back to the folder</Link>
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
  const idToPath = new Map((inventory.data ?? []).map((note) => [note.id, note.path]))
  const linkOutcome = bodyEdit.error && !bodyEdit.pending ? bodyEdit.error : null

  /**
   * The task named by `?task=`. On this surface that parameter is a note
   * context, so the note states which task it means and where that task lives.
   * A task the workspace no longer has still keeps its id visible.
   */
  const contextTask = context?.taskId
    ? snapshot.data?.tasks.find((task) => task.id === context.taskId) ?? null
    : null
  const contextTaskHref = context?.taskId
    ? taskSurfaceHref(contextTask?.projectId ?? context?.projectId, context.taskId)
    : null
  const backHref = context?.projectId
    ? `/projects/${context.projectId}${context.taskId ? `?task=${encodeURIComponent(context.taskId)}` : ''}`
    : contextTaskHref
  const backLabel = context?.projectId
    ? `Back to ${project?.name ?? 'the project'}`
    : 'Back to the task'

  /**
   * Dropping the task context removes the control that had focus, so focus moves
   * to the note itself rather than falling to `<body>`. The note is the stable
   * thing the person was reading.
   */
  const clearTaskContext = () => {
    onClearTaskContext?.()
    window.document.querySelector<HTMLElement>('[data-note-panel]')?.focus({ preventScroll: true })
    setAnnouncement('Task context removed. This note is no longer linked to a task.')
  }

  return (
    <>
      {context?.taskId && contextTaskHref ? (
        <div className="mb-4">
          <NoteContextBar
            taskId={context.taskId}
            taskTitle={contextTask?.title}
            taskHref={contextTaskHref}
            onClear={clearTaskContext}
          />
        </div>
      ) : null}

      <Panel
        data-note-panel
        tabIndex={-1}
        className="min-w-0 overflow-hidden focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas"
      >
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
            <span
              className="font-mono text-2xs text-ink-muted"
              title={path}
              data-note-location-path={noteFolder(path)}
            >
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
                leadingIcon={<LinkIcon size={14} />}
                onClick={() => setLinkOpen(true)}
                disabled={writing || editing || !noteId}
                title={noteId ? undefined : 'A note needs an orbit_id before it can be linked.'}
                data-note-action="link"
              >
                Link
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setMoveOpen(true)}
                disabled={writing || editing || !noteId}
                title="Rename or move this note"
                data-note-action="move"
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
                    attempt.clear()
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
                  {uncertain ? 'Retry the same change' : 'Save note'}
                </Button>
              ) : null}
            </div>
          }
        />

        <PanelBody className="min-w-0">
          {backHref ? (
            <p className="mb-4 text-2xs">
              <Link
                to={backHref}
                data-note-back
                className="font-medium text-accent-soft-ink underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
              >
                ← {backLabel}
              </Link>
            </p>
          ) : null}

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
              readable, but the server cannot accept edits, links or moves until it has a stable id.
            </div>
          ) : null}

          {editing ? (
            <MarkdownEditor
              value={draft}
              onChange={(next) => {
                setDraft(next)
                setLocalError(null)
              }}
              label="Note body"
              disabled={writing}
              spellCheck={false}
              fieldClassName="min-h-[24rem] font-mono text-sm leading-6"
              previewClassName="min-h-[24rem]"
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

          {linkOutcome ? (
            <div
              role="alert"
              className={cn(
                'mt-4 rounded-md border px-3 py-2 text-xs leading-5',
                bodyEdit.uncertain
                  ? 'border-warning/40 bg-warning-soft text-warning-ink'
                  : 'border-danger/30 bg-danger-soft text-danger-ink',
              )}
              data-link-error
            >
              <p className="font-medium">
                {bodyEdit.uncertain
                  ? 'The link change was not confirmed; the note may already have changed.'
                  : 'The server refused this change to the note.'}
              </p>
              <p className="mt-0.5">
                {bodyEdit.uncertain
                  ? uncertainMessage(bodyEdit.error)
                  : userMessage(bodyEdit.error)}
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                {bodyEdit.uncertain ? (
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={bodyEdit.pending}
                    onClick={() => void bodyEdit.retry()}
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
                {uncertain && attempt.attempt ? (
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => attempt.attempt && void send(attempt.attempt)}
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
            {writing || bodyEdit.pending ? (
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

      <NoteRelations
        noteId={noteId}
        relations={relations}
        inventoryError={inventory.error && !inventory.data ? inventory.error : null}
        idToPath={idToPath}
        onUnlink={noteId && !editing ? (id) => void unlink(id) : undefined}
        unlinkingId={unlinkingId}
        unlinkDisabled={editing || bodyEdit.pending || command.isPending}
      />

      <NoteReferences
        noteId={noteId}
        body={parts.body}
        projectId={context?.projectId}
        taskId={context?.taskId}
        readOnly={editing || writing || bodyEdit.pending}
        onChanged={() => {
          void document.refetch()
          void inventory.refetch()
        }}
      />

      {moveOpen ? (
        <NoteMoveDialog
          currentPath={path}
          noteId={noteId ?? ''}
          options={folderOptions}
          onClose={() => setMoveOpen(false)}
          onRefreshTree={onRefreshTree}
          onMoved={(target) => {
            setMoveOpen(false)
            navigate(noteHref(target))
            toast({ tone: 'success', title: 'Note moved', description: target })
          }}
        />
      ) : null}

      {linkOpen && noteId ? (
        <NoteLinkDialog
          currentNoteId={noteId}
          currentTitle={title}
          notes={inventory.data ?? []}
          document={document}
          onClose={() => setLinkOpen(false)}
          onLinked={(note) => {
            setLinkOpen(false)
            setAnnouncement(`Linked ${note.title}.`)
            toast({ tone: 'success', title: 'Notes linked', description: note.title })
          }}
        />
      ) : null}
    </>
  )
}
