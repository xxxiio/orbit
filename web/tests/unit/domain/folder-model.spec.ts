import { describe, expect, it } from 'vitest'
import type { KnowledgeNode } from '@/api/contracts'
import {
  VAULT_ROOT,
  allFolderPaths,
  childFolders,
  countNotesIn,
  findFolder,
  folderBreadcrumb,
  folderName,
  folderOptions,
  folderParent,
  folderSegments,
  fromPickerValue,
  isWithinFolder,
  isValidFolderName,
  isValidFolderPath,
  joinFolderPath,
  noteFileName,
  noteFolder,
  notesInFolder,
  rebasePath,
  toPickerValue,
  withFolderOption,
} from '@/features/knowledge/folder-model'

const tree: KnowledgeNode = {
  kind: 'folder',
  name: 'vault',
  path: '',
  children: [
    {
      kind: 'folder',
      name: 'Work',
      path: 'Work',
      children: [
        { kind: 'note', name: 'a.md', path: 'Work/a.md' },
        {
          kind: 'folder',
          name: 'Research',
          path: 'Work/Research',
          children: [{ kind: 'note', name: 'b.md', path: 'Work/Research/b.md' }],
        },
      ],
    },
    { kind: 'note', name: 'root.md', path: 'root.md' },
  ],
}

describe('folder path helpers', () => {
  it('splits, parents, names and joins a vault path', () => {
    expect(folderSegments('')).toEqual([])
    expect(folderSegments('Work/Research')).toEqual(['Work', 'Research'])
    expect(folderParent('Work/Research')).toBe('Work')
    expect(folderParent('Work')).toBe(VAULT_ROOT)
    expect(folderName('Work/Research')).toBe('Research')
    expect(folderName(VAULT_ROOT)).toBe('')
    expect(joinFolderPath('', 'Work')).toBe('Work')
    expect(joinFolderPath('Work', 'Research')).toBe('Work/Research')
  })

  it('reads a note path as folder plus file name', () => {
    expect(noteFolder('Work/Research/b.md')).toBe('Work/Research')
    expect(noteFolder('a.md')).toBe(VAULT_ROOT)
    expect(noteFileName('Work/Research/b.md')).toBe('b.md')
  })

  it('mirrors the server folder grammar and nothing more', () => {
    expect(isValidFolderName('Research')).toBe(true)
    expect(isValidFolderName('Q1 2026')).toBe(true)
    expect(isValidFolderName('')).toBe(false)
    expect(isValidFolderName('.hidden')).toBe(false)
    expect(isValidFolderName('a/b')).toBe(false)
    expect(isValidFolderName('..')).toBe(false)
    expect(isValidFolderName('a\\b')).toBe(false)

    expect(isValidFolderPath('')).toBe(true)
    expect(isValidFolderPath('Work/Research')).toBe(true)
    expect(isValidFolderPath('Work//Research')).toBe(false)
    expect(isValidFolderPath('Work/.orbit-operations')).toBe(false)
  })

  it('knows whether a path lives inside a folder', () => {
    expect(isWithinFolder('Work/a.md', 'Work')).toBe(true)
    expect(isWithinFolder('Work/Research', 'Work')).toBe(true)
    expect(isWithinFolder('Workshop/a.md', 'Work')).toBe(false)
    expect(isWithinFolder('anything', '')).toBe(true)
  })

  it('rebases a note or folder path after a move, and refuses a foreign path', () => {
    expect(rebasePath('Work/a.md', 'Work', 'Archive')).toBe('Archive/a.md')
    expect(rebasePath('Work/Research/b.md', 'Work', 'Archive')).toBe('Archive/Research/b.md')
    expect(rebasePath('Work', 'Work', 'Archive')).toBe('Archive')
    expect(rebasePath('Personal/a.md', 'Work', 'Archive')).toBeNull()
    expect(rebasePath('Workshop/a.md', 'Work', 'Archive')).toBeNull()
    // The root cannot move, so it never rebases anything.
    expect(rebasePath('Work/a.md', '', 'Archive')).toBeNull()
  })

  it('builds a navigable trail for the current folder', () => {
    expect(folderBreadcrumb('Work/Research')).toEqual([
      { path: '', name: 'Vault' },
      { path: 'Work', name: 'Work' },
      { path: 'Work/Research', name: 'Research' },
    ])
    expect(folderBreadcrumb('')).toEqual([{ path: '', name: 'Vault' }])
  })
})

describe('folder picker model', () => {
  it('lists the root and every folder, indented by depth', () => {
    const options = folderOptions(tree)
    expect(options.map((option) => option.value)).toEqual(['', 'Work', 'Work/Research'])
    expect(options[0].label).toBe('Vault root')
    expect(options[1]).toMatchObject({ label: 'Work', depth: 0 })
    expect(options[2]).toMatchObject({ label: '\u00A0\u00A0Research', depth: 1 })
  })

  it('never offers a folder as its own destination', () => {
    const options = folderOptions(tree, { exclude: 'Work' })
    expect(options.map((option) => option.value)).toEqual([''])
  })

  it('excludes only the moved subtree, keeping its siblings', () => {
    const options = folderOptions(tree, { exclude: 'Work/Research' })
    expect(options.map((option) => option.value)).toEqual(['', 'Work'])
  })

  it('offers the root when the tree has not arrived', () => {
    expect(folderOptions(undefined)).toEqual([
      { value: '', label: 'Vault root', depth: 0 },
    ])
  })

  it('keeps a folder selectable when the tree has not caught up', () => {
    const options = folderOptions(undefined)
    expect(withFolderOption(options, '')).toBe(options)
    expect(withFolderOption(options, 'Work/Research')).toEqual([
      { value: '', label: 'Vault root', depth: 0 },
      { value: 'Work/Research', label: 'Research', depth: 1 },
    ])
    expect(withFolderOption([...options, { value: 'Work', label: 'Work', depth: 0 }], 'Work')).toHaveLength(2)
  })

  it('maps the empty root path to a Radix-safe sentinel', () => {
    expect(toPickerValue('')).not.toBe('')
    expect(fromPickerValue(toPickerValue(''))).toBe('')
    expect(fromPickerValue('Work/Research')).toBe('Work/Research')
    expect(toPickerValue('Work')).toBe('Work')
  })
})

describe('folder tree queries', () => {
  it('finds a folder and lists its direct children', () => {
    expect(findFolder(tree, 'Work/Research')?.name).toBe('Research')
    expect(findFolder(tree, 'Work/Nope')).toBeNull()
    expect(childFolders(tree, '').map((node) => node.path)).toEqual(['Work'])
    expect(childFolders(tree, 'Work').map((node) => node.path)).toEqual(['Work/Research'])
    expect(notesInFolder(tree, 'Work').map((node) => node.path)).toEqual(['Work/a.md'])
    expect(notesInFolder(tree, '').map((node) => node.path)).toEqual(['root.md'])
  })

  it('counts notes under a folder and lists folder paths in tree order', () => {
    expect(countNotesIn(tree)).toBe(3)
    expect(countNotesIn(tree.children[0])).toBe(2)
    expect(allFolderPaths(tree)).toEqual(['Work', 'Work/Research'])
    expect(allFolderPaths(undefined)).toEqual([])
  })
})
