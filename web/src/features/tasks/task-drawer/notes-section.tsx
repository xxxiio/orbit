/**
 * The task drawer's Notes section: the notes that reference this task.
 *
 * The association is the reference grammar (`task:<id>` inside the note body),
 * not a new table. "Link a note" adds that reference to a chosen note through
 * the server's reference command; unlinking removes it. A note that references a
 * task which no longer exists is not this section's problem — it is shown as
 * broken in the note itself.
 *
 * A server without the reference read answers 404, and that is reported as
 * unavailable instead of "no notes reference this task".
 */
import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import type { KnowledgeDocumentInfo } from '@/api/contracts'
import {
  newCommandId,
  uncertainMessage,
  useIdempotentAttempt,
  useMarkdownInventory,
  useMarkdownReferences,
  useReferenceCommand,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { AlertCircleIcon, LinkIcon, LoaderIcon } from '@/components/ui/icons'
import { SkeletonText } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { noteHref } from '@/features/knowledge/knowledge-model'
import { ReferenceNoteDialog } from '@/features/knowledge/reference-note-dialog'
import { Section } from './section'

interface UnlinkAttempt {
  key: string
  commandId: string
  noteId: string
}

export interface NotesSectionProps {
  taskId: string
  projectId?: string
}

export function NotesSection({ taskId, projectId }: NotesSectionProps) {
  const target = `task:${taskId}`
  const references = useMarkdownReferences(target)
  const inventory = useMarkdownInventory()
  const command = useReferenceCommand()
  const attempt = useIdempotentAttempt<UnlinkAttempt>((key) => ({
    key,
    commandId: newCommandId(),
    noteId: key.split('\u0000')[0],
  }))
  const [dialogOpen, setDialogOpen] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)

  useEffect(() => {
    if (!announcement) return
    const timer = window.setTimeout(() => setAnnouncement(null), 6000)
    return () => window.clearTimeout(timer)
  }, [announcement])

  const unavailable = references.error?.status === 404
  const pending = command.isPending
  const uncertain = command.disposition === 'uncertain' && attempt.attempt !== null
  const error: ApiError | null =
    command.disposition === 'rejected' || uncertain ? command.error : null
  const notes = references.data?.notes ?? []
  const linkedIds = new Set(notes.map((note) => note.id))

  const unlink = async (note: KnowledgeDocumentInfo) => {
    if (pending) return
    const key = `${note.id}\u0000${target}\u0000remove`
    setNotice(null)
    setAnnouncement(null)
    const pinned = attempt.attempt
    if (pinned && pinned.key !== key) {
      if (command.disposition === 'uncertain') {
        setNotice('A previous unlink is still unresolved. Retry it before unlinking another note.')
        return
      }
      attempt.clear()
    }
    const next = pinned?.key === key ? pinned : attempt.ensure(key)
    try {
      const receipt = await command.mutateAsync({
        commandId: next.commandId,
        noteId: note.id,
        target,
        action: 'remove',
      })
      attempt.clear()
      await references.refetch()
      setAnnouncement(
        receipt.changed
          ? `${note.title} no longer references this task.`
          : `${note.title} did not reference this task; nothing was written.`,
      )
    } catch {
      // Rendered below.
    }
  }

  const status = references.isPending && !references.data

  return (
    <Section
      title="Notes"
      description="Notes that reference this task. The reference is stored in the note, so it survives a rename here."
      aside={
        <Button
          variant="quiet"
          size="sm"
          leadingIcon={<LinkIcon size={14} />}
          onClick={() => setDialogOpen(true)}
          disabled={unavailable || status || pending}
          title={unavailable ? 'This server does not accept note references yet.' : undefined}
          data-action="link-note-open"
        >
          Link a note
        </Button>
      }
    >
      <div data-task-notes className="flex flex-col gap-2">
        {status ? (
          <div aria-busy="true" aria-label="Loading notes">
            <SkeletonText lines={2} />
          </div>
        ) : unavailable ? (
          <p className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink">
            This server does not provide note references yet, so this task&apos;s notes cannot be
            listed. Nothing is assumed to be empty.
          </p>
        ) : references.error && !references.data ? (
          <div className="flex flex-wrap items-center gap-2">
            <p className="min-w-0 flex-1 text-xs leading-5 text-danger-ink">
              Notes that reference this task could not be read. {userMessage(references.error)}
            </p>
            <Button
              variant="quiet"
              size="sm"
              loading={references.isFetching}
              onClick={() => void references.refetch()}
            >
              Retry
            </Button>
          </div>
        ) : notes.length === 0 ? (
          <p className="text-xs leading-5 text-ink-muted">
            No note references this task yet. Link a note to connect the decision or research behind
            it.
          </p>
        ) : (
          <ul aria-label="Notes referencing this task" className="flex flex-col gap-1">
            {notes.map((note) => (
              <li
                key={note.id}
                data-note-id={note.id}
                className="flex items-center gap-2 rounded-sm border border-line bg-surface px-2 py-1.5"
              >
                <div className="min-w-0 flex-1">
                  <Link
                    to={noteContextHref(note, taskId, projectId)}
                    className="block truncate text-sm font-medium text-ink underline-offset-2 hover:text-accent-soft-ink hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                  >
                    {note.title}
                  </Link>
                  <span className="block truncate font-mono text-2xs text-ink-muted">{note.path}</span>
                </div>
                <Button
                  variant="quiet"
                  size="sm"
                  onClick={() => void unlink(note)}
                  disabled={pending}
                  aria-label={`Unlink ${note.title} from this task`}
                  data-note-unlink={note.id}
                >
                  {attempt.attempt?.noteId === note.id && pending ? (
                    <LoaderIcon size={13} className="animate-spin text-accent" />
                  ) : (
                    'Unlink'
                  )}
                </Button>
              </li>
            ))}
          </ul>
        )}

        {notice ? (
          <p role="alert" className="text-xs leading-5 text-ink-soft">
            {notice}
          </p>
        ) : null}

        {error ? (
          <div
            role="alert"
            className={cn(
              'rounded-md border px-3 py-2 text-xs leading-5',
              uncertain
                ? 'border-warning/40 bg-warning-soft text-warning-ink'
                : 'border-danger/30 bg-danger-soft text-danger-ink',
            )}
          >
            <p className="flex items-start gap-1.5 font-medium">
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              <span className="min-w-0">
                {uncertain
                  ? 'The unlink was not confirmed; the note may already have changed.'
                  : 'The server refused the unlink.'}
              </span>
            </p>
            <p className="mt-0.5">
              {uncertain ? uncertainMessage(error) : userMessage(error)}
            </p>
            {uncertain && attempt.attempt ? (
              <Button
                size="sm"
                variant="secondary"
                className="mt-2"
                disabled={pending}
                onClick={() => {
                  const pinned = attempt.attempt
                  if (!pinned) return
                  void command
                    .mutateAsync({
                      commandId: pinned.commandId,
                      noteId: pinned.noteId,
                      target,
                      action: 'remove',
                    })
                    .then(() => {
                      attempt.clear()
                      void references.refetch()
                    })
                    .catch(() => undefined)
                }}
              >
                Retry the same unlink
              </Button>
            ) : null}
          </div>
        ) : null}

        <p role="status" aria-live="polite" className="min-h-4 text-2xs text-ink-muted">
          {announcement}
        </p>
      </div>

      {dialogOpen ? (
        <ReferenceNoteDialog
          target={target}
          targetLabel="this task"
          notes={(inventory.data ?? []).filter((note) => !linkedIds.has(note.id))}
          onClose={() => setDialogOpen(false)}
          onLinked={(note) => {
            setDialogOpen(false)
            setAnnouncement(`${note.title} now references this task.`)
            void references.refetch()
            void inventory.refetch()
          }}
        />
      ) : null}
    </Section>
  )
}

function noteContextHref(
  note: KnowledgeDocumentInfo,
  taskId: string,
  projectId: string | undefined,
): string {
  const params = new URLSearchParams()
  if (projectId) params.set('project', projectId)
  params.set('task', taskId)
  return `${noteHref(note.path)}?${params.toString()}`
}
