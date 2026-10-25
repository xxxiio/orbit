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

export function splitMarkdown(raw: string): MarkdownParts {
  const match = FRONTMATTER.exec(raw)
  if (!match) return { frontmatter: '', body: raw, noteId: null }
  const frontmatter = raw.slice(0, match[0].length)
  const body = raw.slice(match[0].length)
  const idLine = /^orbit_id:\s*(\S+)\s*$/m.exec(match[1])
  return { frontmatter, body, noteId: idLine ? idLine[1] : null }
}

export function joinMarkdown(frontmatter: string, body: string): string {
  return `${frontmatter}${body}`
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

/** Strip the leading slash and decode a router splat into a vault-relative path. */
export function notePathFromSplat(splat: string | undefined): string {
  if (!splat) return ''
  const withoutSlash = splat.replace(/^\/+/, '')
  return withoutSlash
    .split('/')
    .map((segment) => {
      try {
        return decodeURIComponent(segment)
      } catch {
        return segment
      }
    })
    .join('/')
}

/** A link that survives folders and spaces: each segment is encoded, slashes stay. */
export function noteHref(path: string): string {
  return `/knowledge/${path
    .split('/')
    .map((segment) => encodeURIComponent(segment))
    .join('/')}`
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
