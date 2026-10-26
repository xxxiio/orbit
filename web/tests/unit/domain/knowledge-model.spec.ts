import { describe, expect, it } from 'vitest'
import {
  createNoteBody,
  isNotePath,
  joinMarkdown,
  noteCountLabel,
  noteHref,
  notePathFromSplat,
  noteTitleFromPath,
  splitMarkdown,
  splitRelations,
} from '@/features/knowledge/knowledge-model'

describe('knowledge markdown model', () => {
  it('pluralises the vault count', () => {
    expect(noteCountLabel(0)).toBe('0 notes')
    expect(noteCountLabel(1)).toBe('1 note')
    expect(noteCountLabel(2)).toBe('2 notes')
  })

  it('splits frontmatter from the editable body without rewriting either', () => {
    const raw = '---\norbit_id: n-1\ntitle: Hello\n---\n# Hello\n\nBody.\n'
    const parts = splitMarkdown(raw)
    expect(parts.noteId).toBe('n-1')
    expect(parts.frontmatter).toBe('---\norbit_id: n-1\ntitle: Hello\n---\n')
    expect(parts.body).toBe('# Hello\n\nBody.\n')
    expect(joinMarkdown(parts.frontmatter, parts.body)).toBe(raw)
  })

  it('keeps a file without frontmatter readable and says it has no note id', () => {
    const parts = splitMarkdown('# Orphan\n')
    expect(parts.noteId).toBeNull()
    expect(parts.frontmatter).toBe('')
    expect(parts.body).toBe('# Orphan\n')
  })

  it('creates frontmatter with the server-recognised orbit_id key', () => {
    const body = createNoteBody('n-42', 'Decision', 'Why it changed.')
    expect(body.startsWith('---\norbit_id: n-42\n---\n# Decision\n')).toBe(true)
    expect(splitMarkdown(body).noteId).toBe('n-42')
  })

  it('mirrors the server path rules for notes and links', () => {
    expect(isNotePath('notes/a.md')).toBe(true)
    expect(isNotePath('notes/.hidden.md')).toBe(false)
    expect(isNotePath('/notes/a.md')).toBe(false)
    expect(isNotePath('notes/a.txt')).toBe(false)
    expect(isNotePath('notes//a.md')).toBe(false)
    expect(isNotePath('notes\\a.md')).toBe(false)

    expect(noteHref('notes/a b.md')).toBe('/knowledge/notes/a%20b.md')
    expect(noteTitleFromPath('notes/research/readiness.md')).toBe('readiness')
  })

  it('uses the router splat as-is instead of decoding it a second time', () => {
    // React Router decoded `%20` once already; a literal `%20` in a filename
    // must stay literal rather than become a space.
    expect(notePathFromSplat('/notes/a%20b.md')).toBe('notes/a%20b.md')
    expect(notePathFromSplat('/notes/a b.md')).toBe('notes/a b.md')
    expect(notePathFromSplat('notes/research/readiness.md')).toBe('notes/research/readiness.md')
    expect(notePathFromSplat(undefined)).toBe('')
  })

  it('guarantees a newline before an edited body', () => {
    expect(joinMarkdown('---\norbit_id: n-1\n---', 'Body.')).toBe('---\norbit_id: n-1\n---\nBody.')
    expect(joinMarkdown('---\norbit_id: n-1\n---\n', 'Body.')).toBe('---\norbit_id: n-1\n---\nBody.')
    expect(joinMarkdown('', 'Body.')).toBe('Body.')
  })

  it("parses orbit_id with the server's exact identity rules", () => {
    const note = (line: string) => splitMarkdown(`---\n${line}\n---\nBody.\n`).noteId
    expect(note('orbit_id: n-1')).toBe('n-1')
    expect(note('orbit_id: note_42')).toBe('note_42')
    expect(note('orbit_id:  spaced')).toBeNull()
    expect(note('orbit_id:\ttabbed')).toBeNull()
    expect(note('orbit_id: trailing ')).toBeNull()
    expect(note('orbit_id: note.1')).toBeNull()
    expect(note('orbit_id: n-1\norbit_id: n-2')).toBeNull()
    expect(note(`orbit_id: ${'a'.repeat(129)}`)).toBeNull()
    expect(splitMarkdown('---\r\norbit_id: n-1\r\n---\r\nBody.\r\n').noteId).toBe('n-1')
  })

  it('separates unresolved references from resolvable outgoing links', () => {
    expect(
      splitRelations({
        outgoing: ['n-1', 'missing'],
        backlinks: ['n-2'],
        unresolved: ['missing'],
      }),
    ).toEqual({ resolvedOutgoing: ['n-1'], unresolved: ['missing'] })
  })
})
