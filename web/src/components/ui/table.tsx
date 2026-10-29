import { ArrowDownIcon, ArrowUpIcon } from './icons'
import { cn } from './utils'
import type { SortDirection } from '@/lib/declarations'

/**
 * Table column headers shared by every table.
 *
 * A sortable header is a real button with an accessible sort label; a plain
 * header is the same cell without the affordance. Both live in the design
 * system because a table in any feature must look and announce the same way.
 */
export function SortHeader<K extends string>({
  label,
  sortKey,
  sort,
  onSortChange,
}: {
  label: string
  sortKey: K
  sort?: { key: string; direction: SortDirection }
  onSortChange?: (sort: { key: K; direction: SortDirection }) => void
}) {
  const active = sort?.key === sortKey
  return (
    <button
      type="button"
      onClick={() => {
        if (!onSortChange) return
        onSortChange({
          key: sortKey,
          direction: active && sort?.direction === 'asc' ? 'desc' : 'asc',
        })
      }}
      className={cn(
        'group inline-flex items-center gap-1 rounded-sm text-2xs font-semibold tracking-label',
        active ? 'text-ink' : 'text-ink-faint hover:text-ink-soft',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50',
      )}
      aria-label={`Sort by ${label}${active ? (sort?.direction === 'asc' ? ', ascending' : ', descending') : ''}`}
    >
      {label}
      {active ? (
        sort?.direction === 'asc' ? (
          <ArrowUpIcon size={11} />
        ) : (
          <ArrowDownIcon size={11} />
        )
      ) : (
        <ArrowDownIcon size={11} className="opacity-0 transition-opacity group-hover:opacity-50" />
      )}
    </button>
  )
}

/** A column header with no sort affordance. */
export function PlainHeader({ label, width }: { label: string; width?: string }) {
  return (
    <th
      scope="col"
      style={width ? { width } : undefined}
      className="border-b border-line bg-surface-sunken/95 px-3.5 py-2 text-2xs font-semibold tracking-label text-ink-faint backdrop-blur"
    >
      {label}
    </th>
  )
}
