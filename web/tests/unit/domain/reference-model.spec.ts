import { describe, expect, it } from 'vitest'
import {
  appendNoteLink,
  findTypedReferences,
  formatTypedReference,
  hasNoteLink,
  hasTypedReference,
  isReferenceBroken,
  isReferenceId,
  noteLinkToken,
  parseTypedReference,
  removeNoteLink,
  typedReferenceToken,
} from '@/features/knowledge/reference-model'

describe('typed reference grammar', () => {
  it('parses a supported target and formats it back', () => {
    expect(parseTypedReference('project:orbit')).toEqual({ kind: 'project', id: 'orbit' })
    expect(parseTypedReference('task:task-1')).toEqual({ kind: 'task', id: 'task-1' })
    expect(formatTypedReference({ kind: 'task', id: 'task-1' })).toBe('task:task-1')
  })

  it('rejects a malformed or unsupported target instead of guessing', () => {
    expect(parseTypedReference('')).toBeNull()
    expect(parseTypedReference('project')).toBeNull()
    expect(parseTypedReference(':orbit')).toBeNull()
    expect(parseTypedReference('project:')).toBeNull()
    expect(parseTypedReference('habit:reading')).toBeNull()
    expect(parseTypedReference('work_item:42')).toBeNull()
    expect(parseTypedReference('PROJECT:orbit')).toBeNull()
    expect(parseTypedReference('project:orb it')).toBeNull()
    expect(parseTypedReference(`project:${'a'.repeat(129)}`)).toBeNull()
    expect(parseTypedReference('project:orbit:extra')).toBeNull()
  })

  it('validates ids with the same rules the server uses', () => {
    expect(isReferenceId('note_42')).toBe(true)
    expect(isReferenceId('a-b_C9')).toBe(true)
    expect(isReferenceId('')).toBe(false)
    expect(isReferenceId('has space')).toBe(false)
    expect(isReferenceId('note.1')).toBe(false)
    expect(isReferenceId('a'.repeat(129))).toBe(false)
  })

  it('describes the token the note body stores', () => {
    expect(typedReferenceToken({ kind: 'project', id: 'orbit' })).toBe(
      '[project:orbit](orbit://project:orbit)',
    )
  })
})

describe('reading typed references from a note body', () => {
  it('finds both kinds, deduplicated and ordered by kind then id', () => {
    const body = [
      '# Orbit',
      '',
      '[task:task-2](orbit://task:task-2)',
      '[project:orbit](orbit://project:orbit)',
      '[task:task-1](orbit://task:task-1)',
      '[project:orbit](orbit://project:orbit)',
    ].join('\n')

    expect(findTypedReferences(body)).toEqual([
      { kind: 'project', id: 'orbit' },
      { kind: 'task', id: 'task-1' },
      { kind: 'task', id: 'task-2' },
    ])
  })

  it('ignores links whose text and target disagree, and unsupported kinds', () => {
    const body = [
      '[orbit](orbit://project:orbit)',
      '[project:orbit](https://example.test)',
      '[habit:reading](orbit://habit:reading)',
      '[project:orbit](orbit://project:orbit)',
    ].join('\n')

    expect(findTypedReferences(body)).toEqual([{ kind: 'project', id: 'orbit' }])
    expect(hasTypedReference(body, { kind: 'project', id: 'orbit' })).toBe(true)
    expect(hasTypedReference(body, { kind: 'task', id: 'task-1' })).toBe(false)
  })

  it('reads an empty body as no references', () => {
    expect(findTypedReferences('')).toEqual([])
    expect(findTypedReferences('# Just prose\n')).toEqual([])
  })

  it('tells a live reference from a dead one once the index is known', () => {
    const index = { projectIds: new Set(['orbit']), taskIds: new Set(['task-1']) }
    expect(isReferenceBroken({ kind: 'project', id: 'orbit' }, index)).toBe(false)
    expect(isReferenceBroken({ kind: 'task', id: 'task-2' }, index)).toBe(true)
  })
})

describe('note links', () => {
  it('appends the token as its own final paragraph', () => {
    expect(noteLinkToken('note-a')).toBe('[[note-a]]')
    expect(appendNoteLink('', 'note-a')).toEqual({ body: '[[note-a]]\n', changed: true })
    expect(appendNoteLink('# A\n', 'note-a')).toEqual({
      body: '# A\n\n[[note-a]]\n',
      changed: true,
    })
    expect(appendNoteLink('# A\n\n', 'note-a')).toEqual({
      body: '# A\n\n[[note-a]]\n',
      changed: true,
    })
  })

  it('does not duplicate a link that is already there, in any label form', () => {
    const body = '# A\n\n[[note-a|the other one]]\n'
    expect(hasNoteLink(body, 'note-a')).toBe(true)
    expect(appendNoteLink(body, 'note-a')).toEqual({ body, changed: false })
    expect(hasNoteLink('[[note-a-b]]', 'note-a')).toBe(false)
  })

  it('removes every token that names the note and leaves other bytes alone', () => {
    expect(removeNoteLink('# A [[note-a]] and [[note-b]]\n', 'note-a')).toEqual({
      body: '# A  and [[note-b]]\n',
      changed: true,
    })
    expect(removeNoteLink('[[note-a|x]] [[note-a]]', 'note-a')).toEqual({
      body: ' ',
      changed: true,
    })
    expect(removeNoteLink('[[note-b]]', 'note-a')).toEqual({ body: '[[note-b]]', changed: false })
  })

  it('ignores brackets that are not a valid reference', () => {
    const body = '[[has space]] [[note.1]] [not a link]'
    expect(removeNoteLink(body, 'note-a')).toEqual({ body, changed: false })
    expect(hasNoteLink(body, 'has')).toBe(false)
  })
})
