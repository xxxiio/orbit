/**
 * Reads and controls shared by the I2 knowledge specs.
 *
 * Every function here goes through the real server: `page.request` shares the
 * browser's cookie jar, so a read is the same authoritative read the app makes,
 * not a fixture shortcut. UI helpers only click real controls — no `dispatch`,
 * no fixed sleeps.
 */
import { expect, type Locator, type Page } from '@playwright/test'
import type { E2EServer } from './server'

export interface VaultNote {
  id: string
  path: string
  title: string
}

export interface KnowledgeTreeNode {
  kind: 'folder' | 'note'
  name: string
  path: string
  children?: KnowledgeTreeNode[]
}

export interface TypedReferenceWire {
  kind: 'project' | 'task'
  id: string
}

export interface ProjectKnowledgeWire {
  project_id: string
  notes: Array<{ id: string; path: string; title: string; references: TypedReferenceWire[] }>
  tasks: string[]
}

async function json<T>(page: Page, server: E2EServer, path: string, status = 200): Promise<T> {
  const response = await page.request.get(`${server.base}${path}`)
  expect(response.status(), `GET ${path}`).toBe(status)
  return (await response.json()) as T
}

/** The folder/note tree as the server reports it. */
export function treeAt(page: Page, server: E2EServer): Promise<KnowledgeTreeNode> {
  return json<KnowledgeTreeNode>(page, server, '/v1/authoritative/knowledge-tree')
}

/** `{ id, path }` for every published note, ordered by id on the wire. */
export function inventoryAt(page: Page, server: E2EServer): Promise<VaultNote[]> {
  return json<VaultNote[]>(page, server, '/v1/authoritative/markdown-inventory')
}

/** One note's exact bytes. A missing note is a failed assertion, not `null`. */
export async function documentAt(
  page: Page,
  server: E2EServer,
  path: string,
): Promise<{ path: string; body: string }> {
  return json<{ path: string; body: string }>(
    page,
    server,
    `/v1/authoritative/markdown-document?path=${encodeURIComponent(path)}`,
  )
}

/** The status of a document read, for asserting a path is gone. */
export async function documentStatus(
  page: Page,
  server: E2EServer,
  path: string,
): Promise<number> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/markdown-document?path=${encodeURIComponent(path)}`,
  )
  return response.status()
}

export function relationsAt(
  page: Page,
  server: E2EServer,
  id: string,
): Promise<{ outgoing: string[]; backlinks: string[]; unresolved: string[] }> {
  return json(page, server, `/v1/authoritative/markdown-relations?id=${encodeURIComponent(id)}`)
}

export function referencesAt(
  page: Page,
  server: E2EServer,
  target: string,
): Promise<{ target: string; notes: Array<{ id: string; path: string; title: string }> }> {
  return json(
    page,
    server,
    `/v1/authoritative/markdown-references?target=${encodeURIComponent(target)}`,
  )
}

export function projectKnowledgeAt(
  page: Page,
  server: E2EServer,
  projectId: string,
): Promise<ProjectKnowledgeWire> {
  return json(
    page,
    server,
    `/v1/authoritative/project-knowledge?project_id=${encodeURIComponent(projectId)}`,
  )
}

/** Every folder path in the tree, root excluded, in tree order. */
export function folderPaths(tree: KnowledgeTreeNode): string[] {
  const paths: string[] = []
  const walk = (node: KnowledgeTreeNode) => {
    if (node.kind !== 'folder') return
    if (node.path.length > 0) paths.push(node.path)
    for (const child of node.children ?? []) walk(child)
  }
  walk(tree)
  return paths
}

/** Every note path in the tree, in tree order. */
export function notePaths(tree: KnowledgeTreeNode): string[] {
  const paths: string[] = []
  const walk = (node: KnowledgeTreeNode) => {
    if (node.kind === 'note') {
      paths.push(node.path)
      return
    }
    for (const child of node.children ?? []) walk(child)
  }
  walk(tree)
  return paths
}

/** The inventory row for a path, or a failed assertion naming the path. */
export function noteAt(notes: VaultNote[], path: string): VaultNote {
  const note = notes.find((candidate) => candidate.path === path)
  expect(note, `the inventory must list ${path}`).toBeTruthy()
  return note as VaultNote
}

/** Pick a value from a Radix `Select` by its visible option label. */
export async function chooseOption(
  page: Page,
  trigger: Locator,
  label: string | RegExp,
): Promise<void> {
  await trigger.click()
  await page.getByRole('option', { name: label }).click()
}

/** The knowledge page's folder address, exactly as `folderHref` builds it. */
export function folderUrl(folder: string): string {
  return folder.length === 0 ? '/knowledge' : `/knowledge?folder=${encodeURIComponent(folder)}`
}

/** A URL assertion pattern for `folderUrl` (the query string is percent-encoded). */
export function folderUrlPattern(folder: string): RegExp {
  const escaped = folderUrl(folder).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return new RegExp(`${escaped}$`)
}

/** The note-create dialog's folder picker, scoped to the dialog. */
export function noteCreateDialog(page: Page): Locator {
  return page.locator('[data-dialog="note-create"]')
}

export function folderCreateDialog(page: Page): Locator {
  return page.locator('[data-dialog="folder-create"]')
}

export function folderMoveDialog(page: Page): Locator {
  return page.locator('[data-dialog="folder-move"]')
}

export function noteMoveDialog(page: Page): Locator {
  return page.locator('[data-dialog="note-move"]')
}

export function noteLinkDialog(page: Page): Locator {
  return page.locator('[data-dialog="note-link"]')
}

/** The folder list inside the folder view (not the tree). */
export function folderList(page: Page): Locator {
  return page.locator('ul[aria-label="Folders"]')
}

/** The notes list inside the folder view (not the tree). */
export function folderNotes(page: Page): Locator {
  return page.locator('ul[aria-label="Notes in this folder"]')
}
