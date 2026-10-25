/**
 * The chrome every table shares: the scroll region, the header row and cells,
 * the sortable and plain column headers, and the copy for a truncated or
 * unsupported column set.
 *
 * A surface supplies rows and cells; it never restates the header classes or
 * invents its own wording for a column the workflow asked for and this build
 * cannot show.
 */
import type { ReactNode } from 'react'

export { PlainHeader, SortHeader } from '@/components/ui/table'

/** The horizontal scroller a table lives in. */
export function TableScrollRegion({ children }: { children: ReactNode }) {
  return <div className="scrollbar-slim overflow-x-auto">{children}</div>
}

/** A declared table's header row. */
export function TableHead({ children }: { children: ReactNode }) {
  return (
    <thead>
      <tr className="border-b border-line bg-surface-sunken/40 text-left">{children}</tr>
    </thead>
  )
}

/** One declared column's header cell. */
export function TableHeadCell({ children }: { children: ReactNode }) {
  return (
    <th
      scope="col"
      className="whitespace-nowrap px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint"
    >
      {children}
    </th>
  )
}

/** A named caveat under a table or list: wrong values are worse than none. */
export function ColumnNotice({ children }: { children: ReactNode }) {
  return <p className="text-2xs leading-4 text-warning-ink">{children}</p>
}

/**
 * The one sentence for a declared limit that hid rows. Returns an empty string
 * when nothing was hidden, so callers can interpolate it unconditionally.
 */
export function limitNote(shown: number, total: number): string {
  if (shown >= total) return ''
  return ` Showing the first ${shown} of ${total}; the workflow sets this limit.`
}
