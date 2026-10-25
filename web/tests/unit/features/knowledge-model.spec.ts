import { describe, expect, it } from 'vitest'
import {
  createNoteBody,
  isNotePath,
  joinMarkdown,
  noteHref,
  notePathFromSplat,
  noteTitleFromPath,
  splitMarkdown,
  splitRelations,
} from '@/features/knowledge/knowledge-model'

describe('knowledge markdown model', () => {
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

  it('decodes a router splat per segment and preserves folder separators', () => {
    expect(notePathFromSplat('/notes/a%20b.md')).toBe('notes/a b.md')
    expect(notePathFromSplat('notes/research/readiness.md')).toBe('notes/research/readiness.md')
    expect(notePathFromSplat(undefined)).toBe('')
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
