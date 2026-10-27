/**
 * Markdown note presentation and write-shape helpers.
 *
 * The authority treats a note as the exact bytes on disk: identity lives in the
 * frontmatter, and `Update` fences on the whole body the person was shown. These
 * helpers only split that body into "frontmatter" and "what the editor shows";
 * they never rewrite identity or infer it from a path.
 */

export interface MarkdownParts {
  /** Everything up to and including the closing delimiter, verbatim. */
  frontmatter: string
  /** The editable Markdown after the frontmatter. */
  body: string
  /** The stable `orbit_id` value, or `null` when the file has no frontmatter. */
  noteId: string | null
}

const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---(\r?\n)?/

/**
 * The server accepts an `orbit_id` only as the exact line `orbit_id: <id>` with
 * one space, an id of 1–128 ASCII letters/digits/`-`/`_`, and exactly one such
 * line in the frontmatter. Anything else is not a note identity the authority
 * would accept, so the browser reports no id instead of inventing one.
 */
const ORBIT_ID_LINE = /^orbit_id: ([A-Za-z0-9_-]{1,128})$/

function noteIdFromFrontmatter(frontmatter: string): string | null {
  let id: string | null = null
  for (const rawLine of frontmatter.split('\n')) {
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine
    // `str::lines()` on the server accepts CRLF; only the exact key with a
    // following space is a candidate. Other spellings are ignored metadata.
    if (!line.startsWith('orbit_id: ')) continue
    if (id !== null) return null
    const match = ORBIT_ID_LINE.exec(line)
    if (!match) return null
    id = match[1]
  }
  return id
}

export function splitMarkdown(raw: string): MarkdownParts {
  const match = FRONTMATTER.exec(raw)
  if (!match) return { frontmatter: '', body: raw, noteId: null }
  const frontmatter = raw.slice(0, match[0].length)
  const body = raw.slice(match[0].length)
  return { frontmatter, body, noteId: noteIdFromFrontmatter(match[1]) }
}

/**
 * Rebuild the exact file body. Frontmatter that ends at its closing delimiter
 * without a newline still needs a separator, or the first body line would be
 * glued to `---` and change what the server reads back as identity.
 */
export function joinMarkdown(frontmatter: string, body: string): string {
  if (frontmatter.length === 0) return body
  const separator = frontmatter.endsWith('\n') ? '' : '\n'
  return `${frontmatter}${separator}${body}`
}

/**
 * Mirror the server's path rule so the browser can explain a bad path instead
 * of sending it: relative, `/` segments, no dot segments, no backslashes, `.md`.
 */
export function isNotePath(value: string): boolean {
  if (value.length === 0 || value.length > 2048) return false
  if (value.includes('\\') || value.includes('\0')) return false
  if (!value.toLowerCase().endsWith('.md')) return false
  return value
    .split('/')
    .every((segment) => segment.length > 0 && !segment.startsWith('.') && segment !== '..')
}

/**
 * Strip the leading slash from a router splat. React Router has already
 * decoded each path segment once; decoding again would turn a literal `%20`
 * in a filename into a space and address a different file.
 */
export function notePathFromSplat(splat: string | undefined): string {
  if (!splat) return ''
  return splat.replace(/^\/+/, '')
}

/** A link that survives folders and spaces: each segment is encoded, slashes stay. */
export function noteHref(path: string): string {
  return `/knowledge/${path
    .split('/')
    .map((segment) => encodeURIComponent(segment))
    .join('/')}`
}

/**
 * Where a task opens with its drawer: inside its project when it has one, else
 * in the Inbox. A note's reference chip uses this, so a link out of the note
 * never lands back on the note's own surface.
 */
export function taskSurfaceHref(projectId: string | null | undefined, taskId: string): string {
  const base = projectId ? `/projects/${projectId}` : '/inbox'
  return `${base}?task=${encodeURIComponent(taskId)}`
}

/**
 * The knowledge page's folder destination. The root is `/knowledge` with no
 * query string; any other folder is a real, shareable address.
 */
export function folderHref(folder: string): string {
  return folder.length === 0 ? '/knowledge' : `/knowledge?folder=${encodeURIComponent(folder)}`
}

/** One file name as the vault expects it: a name plus `.md`, never a path. */
export function isValidNoteFileName(name: string): boolean {
  if (name.length === 0 || name.length > 255 - 3) return false
  if (name.includes('/') || name.includes('\\') || name.includes('\0')) return false
  if (!name.toLowerCase().endsWith('.md')) return false
  const base = name.slice(0, -3)
  return base.length > 0 && base !== '.' && base !== '..' && !base.startsWith('.')
}

/** The note path a folder and a file name compose. */
export function notePathInFolder(folder: string, fileName: string): string {
  const name = fileName.trim()
  return folder.length === 0 ? name : `${folder}/${name}`
}

export function noteTitleFromPath(path: string): string {
  const name = path.split('/').pop() ?? path
  return name.replace(/\.md$/i, '')
}

/** The body a new note starts with, including the frontmatter identity. */
export function createNoteBody(noteId: string, title: string, markdown: string): string {
  const heading = title.trim().length > 0 ? `# ${title.trim()}\n` : ''
  const content = markdown.trim().length > 0 ? `\n${markdown.trim()}\n` : ''
  return `---\norbit_id: ${noteId}\n---\n${heading}${content}`
}

/** Human-visible lists of `[[id]]` targets, keeping unresolved ones separate. */
export interface NoteRelations {
  outgoing: string[]
  backlinks: string[]
  unresolved: string[]
}

export function splitRelations(relations: NoteRelations): {
  resolvedOutgoing: string[]
  unresolved: string[]
} {
  const unresolved = new Set(relations.unresolved)
  return {
    resolvedOutgoing: relations.outgoing.filter((id) => !unresolved.has(id)),
    unresolved: relations.unresolved,
  }
}

/** “0 notes”, “1 note”, “2 notes”: the vault header never says “1 notes”. */
export function noteCountLabel(count: number): string {
  return `${count} note${count === 1 ? '' : 's'}`
}
