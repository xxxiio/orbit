/**
 * Vault folder paths, and the folder picker every knowledge dialog shares.
 *
 * The server treats a folder as a plain slash-separated path under the vault
 * root (`KnowledgeOperations`), and refuses dot segments, backslashes, an empty
 * segment and a leading dot. These helpers mirror exactly that grammar so the
 * browser can explain a bad name before sending it — and nothing more: whether a
 * destination exists, is nested, or is already taken stays the server's answer.
 */
import type { KnowledgeNode } from '@/api/contracts'

/** The vault root is the empty folder path. */
export const VAULT_ROOT = ''

export function folderSegments(path: string): string[] {
  return path.length === 0 ? [] : path.split('/')
}

/** `Work/Research/notes` → `Work/Research`; a top-level folder → the root. */
export function folderParent(path: string): string {
  const segments = folderSegments(path)
  return segments.slice(0, -1).join('/')
}

/** The last segment of a folder path; the root has no name. */
export function folderName(path: string): string {
  const segments = folderSegments(path)
  return segments.length === 0 ? '' : segments[segments.length - 1]
}

/** Join a parent folder and one name. The root parent contributes nothing. */
export function joinFolderPath(parent: string, name: string): string {
  return parent.length === 0 ? name : `${parent}/${name}`
}

/** The folder a note path lives in (`Work/a.md` → `Work`). */
export function noteFolder(path: string): string {
  return folderParent(path)
}

/** The file name of a note path, including `.md`. */
export function noteFileName(path: string): string {
  const segments = folderSegments(path)
  return segments.length === 0 ? '' : segments[segments.length - 1]
}

/**
 * One folder name, as the server would validate it. A name never contains a
 * separator: the picker supplies the parent, so a slash is a mistake here.
 */
export function isValidFolderName(name: string): boolean {
  if (name.length === 0 || name.length > 255) return false
  if (name.includes('/') || name.includes('\\') || name.includes('\0')) return false
  if (name === '.' || name === '..') return false
  return !name.startsWith('.')
}

/** A whole folder path: the root, or every segment a valid name. */
export function isValidFolderPath(path: string): boolean {
  if (path.length === 0) return true
  if (path.length > 1024) return false
  return folderSegments(path).every((segment) => isValidFolderName(segment))
}

/** True when `path` is the folder itself or something inside it. */
export function isWithinFolder(path: string, folder: string): boolean {
  if (folder.length === 0) return true
  return path === folder || path.startsWith(`${folder}/`)
}

/**
 * Move a path (folder or note) after its ancestor folder was renamed. Returns
 * `null` when the path was not under `from`, so the caller can tell "unchanged"
 * from "followed".
 */
export function rebasePath(path: string, from: string, to: string): string | null {
  if (from.length === 0) return null
  if (path === from) return to
  if (!path.startsWith(`${from}/`)) return null
  return `${to}${path.slice(from.length)}`
}

export interface BreadcrumbEntry {
  /** `''` for the vault root. */
  path: string
  name: string
}

/** `Work/Research` → root, `Work`, `Work/Research`, for a navigable trail. */
export function folderBreadcrumb(folder: string): BreadcrumbEntry[] {
  const trail: BreadcrumbEntry[] = [{ path: VAULT_ROOT, name: 'Vault' }]
  let current = ''
  for (const segment of folderSegments(folder)) {
    current = joinFolderPath(current, segment)
    trail.push({ path: current, name: segment })
  }
  return trail
}

export interface FolderOption {
  /** The folder path, `''` for the root. */
  value: string
  /** Indented for a flat list; the root reads "Vault root". */
  label: string
  depth: number
}

/**
 * Radix `Select` cannot use `''` as an item value (it means "nothing selected"),
 * and the vault root *is* the empty path. One sentinel maps between the two.
 * NUL is not a legal folder character, so no real folder can collide with it.
 */
const ROOT_PICKER_VALUE = '\u0000vault-root'

export function toPickerValue(folder: string): string {
  return folder.length === 0 ? ROOT_PICKER_VALUE : folder
}

export function fromPickerValue(value: string): string {
  return value === ROOT_PICKER_VALUE ? VAULT_ROOT : value
}

/**
 * A flat, indented list of every folder plus the root, for a `Select`.
 *
 * `exclude` drops a folder *and its descendants*: a folder cannot be moved into
 * itself, and a picker must not offer a destination the server would refuse.
 */
export function folderOptions(
  tree: KnowledgeNode | undefined,
  { exclude = null }: { exclude?: string | null } = {},
): FolderOption[] {
  const options: FolderOption[] = [{ value: VAULT_ROOT, label: 'Vault root', depth: 0 }]
  if (!tree) return options

  const walk = (node: KnowledgeNode, depth: number) => {
    if (node.kind !== 'folder') return
    if (node.path.length > 0) {
      if (exclude !== null && isWithinFolder(node.path, exclude)) return
      options.push({ value: node.path, label: `${'\u00A0\u00A0'.repeat(depth)}${node.name}`, depth })
    }
    for (const child of node.children) {
      walk(child, depth + (node.path.length === 0 ? 0 : 1))
    }
  }
  walk(tree, 0)
  return options
}

/**
 * Keep a folder selectable even when the tree has not caught up (a deep link, a
 * refresh in flight, a folder another device just renamed). Without this the
 * control would render its placeholder while the state held a real path.
 */
export function withFolderOption(options: FolderOption[], folder: string): FolderOption[] {
  if (folder.length === 0 || options.some((option) => option.value === folder)) return options
  const segments = folderSegments(folder)
  return [
    ...options,
    { value: folder, label: segments[segments.length - 1] ?? folder, depth: segments.length - 1 },
  ]
}

/** The folder node at `path`, or `null` when the tree has no such folder. */
export function findFolder(tree: KnowledgeNode | undefined, path: string): KnowledgeNode | null {
  if (!tree || tree.kind !== 'folder') return null
  if (tree.path === path) return tree
  if (!isWithinFolder(path, tree.path)) return null
  for (const child of tree.children) {
    const found = findFolder(child, path)
    if (found) return found
  }
  return null
}

/** The immediate child folders of `path`, in tree order. */
export function childFolders(tree: KnowledgeNode | undefined, path: string): KnowledgeNode[] {
  const node = findFolder(tree, path)
  if (!node || node.kind !== 'folder') return []
  return node.children.filter((child) => child.kind === 'folder')
}

/** Notes directly inside `path` (not in a child folder), in tree order. */
export function notesInFolder(tree: KnowledgeNode | undefined, path: string): KnowledgeNode[] {
  const node = findFolder(tree, path)
  if (!node || node.kind !== 'folder') return []
  return node.children.filter((child) => child.kind === 'note')
}

export function countNotesIn(node: KnowledgeNode): number {
  return node.kind === 'note' ? 1 : node.children.reduce((total, child) => total + countNotesIn(child), 0)
}

/** Every folder path under the tree, root excluded, in tree order. */
export function allFolderPaths(tree: KnowledgeNode | undefined): string[] {
  if (!tree || tree.kind !== 'folder') return []
  const paths: string[] = []
  const walk = (node: KnowledgeNode) => {
    if (node.kind !== 'folder') return
    if (node.path.length > 0) paths.push(node.path)
    for (const child of node.children) walk(child)
  }
  walk(tree)
  return paths
}
