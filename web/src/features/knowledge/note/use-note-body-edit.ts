/**
 * Link and unlink a note by rewriting the Markdown body through the existing
 * `Update` command.
 *
 * The discipline is the editor's, because it is the same write:
 *
 * 1. re-read the note immediately before sending (`expected` fences on bytes
 *    the server must still hold);
 * 2. refuse locally when the note's identity changed on disk;
 * 3. mint one command id per payload and replay **that** id while the outcome is
 *    unknown — a link may already have landed;
 * 4. never retry a refusal, and never rewrite a different payload while an
 *    earlier attempt is unresolved.
 *
 * The transform runs on the *fresh* body, so the person's text is never
 * overwritten from a stale copy.
 */
import type { UseQueryResult } from '@tanstack/react-query'
import type { MarkdownCommand } from '@/api/contracts'
import type { MutationDisposition } from '@/api/mutations/core'
import {
  newCommandId,
  useIdempotentAttempt,
  useMarkdownCommand,
  type ApiError,
} from '@/hooks'
import { joinMarkdown, splitMarkdown } from '../knowledge-model'

export interface BodyTransform {
  body: string
  changed: boolean
}

export type NoteBodyOutcome =
  /** The server accepted the write. */
  | { status: 'accepted'; changed: boolean }
  /** Nothing to do: the body already had (or lacked) the token. No request sent. */
  | { status: 'unchanged' }
  /** The server refused; `error` explains why. Nothing was written. */
  | { status: 'failed' }
  /** The file changed identity on disk; nothing was sent. */
  | { status: 'stale' }
  /** The fresh read failed; nothing was sent. */
  | { status: 'unreadable' }
  /** Another payload's outcome is still unknown, or a write is in flight. */
  | { status: 'blocked' }

interface BodyAttempt {
  key: string
  commandId: string
  command: MarkdownCommand
}

export interface NoteBodyEdit {
  pending: boolean
  disposition: MutationDisposition
  error: ApiError | null
  /** An attempt is pinned and its outcome is still unknown. */
  uncertain: boolean
  /** True while a pinned attempt exists; the retry target for the UI. */
  hasAttempt: boolean
  /** Send a transform for `key`. Reusing a key replays the pinned command. */
  run: (key: string, transform: (body: string) => BodyTransform) => Promise<NoteBodyOutcome>
  /** Replay the pinned command unchanged after an uncertain failure. */
  retry: () => Promise<NoteBodyOutcome>
  /** Drop the pinned attempt after the person has refreshed the note. */
  reset: () => void
}

export function useNoteBodyEdit({
  noteId,
  document,
}: {
  noteId: string | null
  document: UseQueryResult<{ path: string; body: string }, ApiError>
}): NoteBodyEdit {
  const command = useMarkdownCommand()
  const attempt = useIdempotentAttempt<BodyAttempt, { command: MarkdownCommand }>((key, payload) => ({
    key,
    commandId: newCommandId(),
    command: payload.command,
  }))
  const pending = command.isPending
  const uncertain = command.disposition === 'uncertain' && attempt.attempt !== null
  const error: ApiError | null =
    command.disposition === 'rejected' || command.disposition === 'uncertain' ? command.error : null

  const send = async (pinned: BodyAttempt): Promise<NoteBodyOutcome> => {
    try {
      await command.mutateAsync({ commandId: pinned.commandId, command: pinned.command })
      await document.refetch()
      attempt.clear()
      return { status: 'accepted', changed: true }
    } catch {
      return { status: 'failed' }
    }
  }

  const run = async (
    key: string,
    transform: (body: string) => BodyTransform,
  ): Promise<NoteBodyOutcome> => {
    if (pending) return { status: 'blocked' }
    if (!noteId) return { status: 'blocked' }

    const pinned = attempt.attempt
    if (pinned && pinned.key === key) return send(pinned)
    if (pinned && command.disposition === 'uncertain') return { status: 'blocked' }
    if (pinned) attempt.clear()

    let raw: string | undefined
    try {
      const fresh = await document.refetch({ throwOnError: true })
      raw = fresh.data?.body
    } catch {
      return { status: 'unreadable' }
    }
    if (raw === undefined) return { status: 'unreadable' }

    const parts = splitMarkdown(raw)
    if (parts.noteId !== noteId) return { status: 'stale' }

    const next = transform(parts.body)
    if (!next.changed) return { status: 'unchanged' }

    const pinnedNext = attempt.ensure(key, {
      command: {
        kind: 'Update',
        note_id: noteId,
        expected: raw,
        replacement: joinMarkdown(parts.frontmatter, next.body),
      },
    })
    return send(pinnedNext)
  }

  return {
    pending,
    disposition: command.disposition,
    error,
    uncertain,
    hasAttempt: attempt.attempt !== null,
    run,
    retry: async () => {
      const pinned = attempt.attempt
      if (!pinned || pending) return { status: 'blocked' }
      return send(pinned)
    },
    reset: () => {
      attempt.clear()
      command.reset()
    },
  }
}
