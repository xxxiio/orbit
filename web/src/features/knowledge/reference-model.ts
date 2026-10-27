/**
 * The two reference grammars a note body can carry, as the vault reads them:
 *
 * - `[[stable-id]]` (or `[[stable-id|label]]`) links another note by its id;
 * - `[project:orbit](orbit://project:orbit)` names a project or task. The text
 *   and the target are both `kind:id`; anything else is not a typed reference.
 *
 * These helpers are read-only projections of the bytes on disk. Link authoring
 * is a fenced `Update` and typed references go through the reference command, so
 * nothing here writes the body.
 */
import type { TypedReference, TypedReferenceKind } from '@/api/contracts'

const REFERENCE_ID = /^[A-Za-z0-9_-]{1,128}$/
const REFERENCE_KINDS: readonly TypedReferenceKind[] = ['project', 'task']

export function isReferenceId(value: string): boolean {
  return REFERENCE_ID.test(value)
}

export function isTypedReferenceKind(value: string): value is TypedReferenceKind {
  return (REFERENCE_KINDS as readonly string[]).includes(value)
}

/** `project:orbit`. This is the wire form the command and reads take. */
export function formatTypedReference(reference: TypedReference): string {
  return `${reference.kind}:${reference.id}`
}

/** Parse a `kind:id` target. A malformed or unsupported kind is `null`. */
export function parseTypedReference(value: string): TypedReference | null {
  const separator = value.indexOf(':')
  if (separator <= 0) return null
  const kind = value.slice(0, separator)
  const id = value.slice(separator + 1)
  if (!isTypedReferenceKind(kind) || !isReferenceId(id)) return null
  return { kind, id }
}

/** How the reference is stored in the Markdown body. */
export function typedReferenceToken(reference: TypedReference): string {
  const target = formatTypedReference(reference)
  return `[${target}](orbit://${target})`
}

const TYPED_LINK = /\[([^\]\n]{1,168})\]\(orbit:\/\/([^)\s]{1,168})\)/g

/**
 * Every typed reference the body declares, deduplicated and ordered by kind then
 * id. A link whose text and target disagree is not a reference and is skipped.
 */
export function findTypedReferences(body: string): TypedReference[] {
  const found = new Map<string, TypedReference>()
  TYPED_LINK.lastIndex = 0
  let match: RegExpExecArray | null
  while ((match = TYPED_LINK.exec(body)) !== null) {
    const [, text, target] = match
    if (text !== target) continue
    const reference = parseTypedReference(target)
    if (!reference) continue
    found.set(formatTypedReference(reference), reference)
  }
  return [...found.values()].sort(
    (left, right) => left.kind.localeCompare(right.kind) || left.id.localeCompare(right.id),
  )
}

export function hasTypedReference(body: string, reference: TypedReference): boolean {
  const target = formatTypedReference(reference)
  return findTypedReferences(body).some((candidate) => formatTypedReference(candidate) === target)
}

/* ------------------------------------------------------------------ */
/* [[note-id]] links                                                   */
/* ------------------------------------------------------------------ */

export function noteLinkToken(id: string): string {
  return `[[${id}]]`
}

function scanNoteLinks(body: string, visit: (id: string, start: number, end: number) => void): void {
  let cursor = 0
  while (cursor < body.length) {
    const start = body.indexOf('[[', cursor)
    if (start === -1) return
    const close = body.indexOf(']]', start + 2)
    if (close === -1) return
    // `[[id|label]]` keeps the label; only the id part is the reference.
    const value = body.slice(start + 2, close).split('|')[0]
    if (isReferenceId(value)) visit(value, start, close + 2)
    cursor = close + 2
  }
}

/** True when the body already links this note id (any label form). */
export function hasNoteLink(body: string, id: string): boolean {
  let found = false
  scanNoteLinks(body, (candidate) => {
    if (candidate === id) found = true
  })
  return found
}

/**
 * Append `[[id]]` as its own final paragraph. `changed: false` means the note
 * already links it, so no write is sent.
 */
export function appendNoteLink(body: string, id: string): { body: string; changed: boolean } {
  if (!isReferenceId(id) || hasNoteLink(body, id)) return { body, changed: false }
  if (body.length === 0) return { body: `${noteLinkToken(id)}\n`, changed: true }
  const separator = body.endsWith('\n\n') ? '' : body.endsWith('\n') ? '\n' : '\n\n'
  return { body: `${body}${separator}${noteLinkToken(id)}\n`, changed: true }
}

/**
 * Remove every `[[id]]` / `[[id|label]]` token that names this note. Other
 * bytes — including the whitespace the token sat in — are left untouched, so the
 * update stays a minimal, explainable diff.
 */
export function removeNoteLink(body: string, id: string): { body: string; changed: boolean } {
  const ranges: Array<[number, number]> = []
  scanNoteLinks(body, (candidate, start, end) => {
    if (candidate === id) ranges.push([start, end])
  })
  if (ranges.length === 0) return { body, changed: false }
  let next = ''
  let cursor = 0
  for (const [start, end] of ranges) {
    next += body.slice(cursor, start)
    cursor = end
  }
  next += body.slice(cursor)
  return { body: next, changed: true }
}

/* ------------------------------------------------------------------ */
/* Display                                                            */
/* ------------------------------------------------------------------ */

/** "Project" / "Task", for a label beside a target. */
export function referenceKindLabel(kind: TypedReferenceKind): string {
  return kind === 'project' ? 'Project' : 'Task'
}

/** The entities this workspace knows about, for telling a live reference from a dead one. */
export interface ReferenceIndex {
  projectIds: ReadonlySet<string>
  taskIds: ReadonlySet<string>
}

/**
 * A reference is broken only when the index is authoritative for that kind and
 * has no such entity. While the workspace is still loading the caller must not
 * call this with an empty index; it would call every reference broken.
 */
export function isReferenceBroken(reference: TypedReference, index: ReferenceIndex): boolean {
  return reference.kind === 'project' ? !index.projectIds.has(reference.id) : !index.taskIds.has(reference.id)
}
