/**
 * Declaration resolvers shared by every list surface.
 *
 * A workflow may declare a `sort` or a `columns` list over any source. The
 * vocabulary differs per resource (a task has a due date; a work item has the
 * fields its kind declares) but the *rules* are the same: resolve the declared
 * name against a descriptor list, keep the declaration's order, and name a
 * spelling this build cannot honour instead of silently dropping it.
 *
 * These are pure functions over a descriptor list, so a new resource adds
 * descriptors — not a second resolver.
 */

export type SortDirection = 'asc' | 'desc'

/** One sortable field: its canonical key, declared spellings and comparator. */
export interface SortDescriptor<T> {
  key: string
  /** Other names a declaration may use for this field (`name` for `title`). */
  aliases: readonly string[]
  compare: (left: T, right: T) => number
}

export interface SortDeclaration<T> {
  /** Canonical descriptor key; `null` keeps the source's order. */
  key: string | null
  direction: SortDirection
  compare: ((left: T, right: T) => number) | null
  /** A declared sort this build cannot apply, named for a visible refusal. */
  refusal: string | null
}

function descriptorFor<T>(
  descriptors: readonly SortDescriptor<T>[],
  declared: string,
): SortDescriptor<T> | undefined {
  const wanted = declared.toLowerCase()
  return descriptors.find(
    (descriptor) =>
      descriptor.key.toLowerCase() === wanted ||
      descriptor.aliases.some((alias) => alias.toLowerCase() === wanted),
  )
}

/**
 * Resolve a declared `sort`/`descending` pair against a descriptor list.
 * `refusalFor` owns the sentence, because each resource names itself; the
 * resolution rules stay here so two resources can never disagree about
 * case-insensitivity or alias handling.
 */
export function resolveSort<T>(
  declaration: { sort?: string | null; descending?: boolean },
  descriptors: readonly SortDescriptor<T>[],
  refusalFor: (declared: string) => string,
): SortDeclaration<T> {
  const declared = declaration.sort?.trim()
  if (!declared) return { key: null, direction: 'asc', compare: null, refusal: null }
  const descriptor = descriptorFor(descriptors, declared)
  if (!descriptor) {
    return { key: null, direction: 'asc', compare: null, refusal: refusalFor(declared) }
  }
  return {
    key: descriptor.key,
    direction: declaration.descending ? 'desc' : 'asc',
    compare: descriptor.compare,
    refusal: null,
  }
}

/** A stable, copied sort. A declaration without a comparator keeps the order. */
export function sortByDescriptor<T>(
  rows: readonly T[],
  declaration: SortDeclaration<T>,
): T[] {
  if (!declaration.compare) return [...rows]
  const compare = declaration.compare
  const direction = declaration.direction === 'asc' ? 1 : -1
  return [...rows].sort((left, right) => compare(left, right) * direction)
}

/** One declared column's canonical key, label and accepted spellings. */
export interface ColumnDescriptor {
  key: string
  aliases: readonly string[]
  label: string
}

export interface ColumnResolution<C> {
  columns: C[]
  /** Declared names no descriptor claims, in declaration order. */
  unsupported: string[]
}

/**
 * Map declared column names onto descriptors, keeping the declared order and
 * reporting every name this build cannot render. Duplicates are preserved: a
 * declaration that names a column twice asked for it twice.
 */
export function resolveColumns<C>(
  declared: readonly string[],
  descriptors: readonly ColumnDescriptor[],
  materialise: (descriptor: ColumnDescriptor) => C,
): ColumnResolution<C> {
  const columns: C[] = []
  const unsupported: string[] = []
  for (const raw of declared) {
    const name = raw.trim()
    if (name.length === 0) continue
    const wanted = name.toLowerCase()
    const descriptor = descriptors.find(
      (candidate) =>
        candidate.key.toLowerCase() === wanted ||
        candidate.aliases.some((alias) => alias.toLowerCase() === wanted),
    )
    if (descriptor) columns.push(materialise(descriptor))
    else unsupported.push(name)
  }
  return { columns, unsupported }
}

/** The descriptor a declared name resolves to, or `null`. */
export function columnDescriptorFor(
  descriptors: readonly ColumnDescriptor[],
  name: string,
): ColumnDescriptor | null {
  const wanted = name.trim().toLowerCase()
  if (wanted.length === 0) return null
  return (
    descriptors.find(
      (descriptor) =>
        descriptor.key.toLowerCase() === wanted ||
        descriptor.aliases.some((alias) => alias.toLowerCase() === wanted),
    ) ?? null
  )
}
