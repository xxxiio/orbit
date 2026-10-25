import { useCallback, useRef, type ReactNode } from 'react'
import { cn } from './utils'

export interface DataTableColumn<T> {
  /** Stable identifier used for React keys and test hooks. */
  key: string
  header: ReactNode
  render: (row: T) => ReactNode
  align?: 'left' | 'right' | 'center'
  className?: string
  headerClassName?: string
  width?: string
}

export interface DataTableProps<T> {
  columns: Array<DataTableColumn<T>>
  rows: T[]
  getRowKey: (row: T) => string
  caption: string
  /** Rendered inside the table body when there are no rows. */
  empty?: ReactNode
  onRowActivate?: (row: T) => void
  className?: string
  /** Accessible description for a row's activation action. */
  rowActionLabel?: (row: T) => string
}

/**
 * Semantic table with a sticky header and roving keyboard focus over rows.
 * Arrow keys move focus, Home/End jump, Enter/Space activates a row.
 * Sorting and pagination belong to the feature that owns the data.
 */
export function DataTable<T>({
  columns,
  rows,
  getRowKey,
  caption,
  empty,
  onRowActivate,
  className,
  rowActionLabel,
}: DataTableProps<T>) {
  const bodyRef = useRef<HTMLTableSectionElement>(null)

  const focusRow = useCallback((index: number) => {
    const body = bodyRef.current
    if (!body) return
    const rowElements = Array.from(body.querySelectorAll<HTMLTableRowElement>('tr[data-row]'))
    const target = rowElements[Math.max(0, Math.min(index, rowElements.length - 1))]
    target?.focus()
  }, [])

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLTableRowElement>, index: number) => {
      if (rows.length === 0) return
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        focusRow(index + 1)
      } else if (event.key === 'ArrowUp') {
        event.preventDefault()
        focusRow(index - 1)
      } else if (event.key === 'Home') {
        event.preventDefault()
        focusRow(0)
      } else if (event.key === 'End') {
        event.preventDefault()
        focusRow(rows.length - 1)
      } else if ((event.key === 'Enter' || event.key === ' ') && onRowActivate) {
        event.preventDefault()
        onRowActivate(rows[index])
      }
    },
    [focusRow, onRowActivate, rows],
  )

  return (
    <div
      className={cn(
        'scrollbar-slim relative max-h-[min(70vh,42rem)] overflow-auto rounded-lg border border-line bg-surface',
        className,
      )}
    >
      <table className="w-full border-separate border-spacing-0 text-left text-sm">
        <caption className="sr-only">{caption}</caption>
        <thead className="sticky top-0 z-10">
          <tr>
            {columns.map((column) => (
              <th
                key={column.key}
                scope="col"
                style={column.width ? { width: column.width } : undefined}
                className={cn(
                  'border-b border-line bg-surface-sunken/95 px-3.5 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint backdrop-blur',
                  column.align === 'right' && 'text-right',
                  column.align === 'center' && 'text-center',
                  column.headerClassName,
                )}
              >
                {column.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody ref={bodyRef}>
          {rows.length === 0 ? (
            <tr>
              <td colSpan={columns.length} className="border-b border-line p-0">
                {empty ?? (
                  <p className="px-4 py-10 text-center text-sm text-ink-muted">No rows to show yet.</p>
                )}
              </td>
            </tr>
          ) : (
            rows.map((row, index) => (
              <tr
                key={getRowKey(row)}
                data-row=""
                tabIndex={0}
                aria-label={rowActionLabel?.(row)}
                onKeyDown={(event) => handleKeyDown(event, index)}
                onClick={(event) => {
                  if (!onRowActivate) return
                  const target = event.target as HTMLElement
                  if (target.closest('button, a, input, select, textarea, [role="button"]')) return
                  onRowActivate(row)
                }}
                className={cn(
                  'border-b border-line bg-surface transition-colors last:border-b-0',
                  'hover:bg-surface-hover/70',
                  'focus-visible:bg-accent-soft/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent/50',
                  onRowActivate && 'cursor-pointer',
                )}
              >
                {columns.map((column) => (
                  <td
                    key={column.key}
                    className={cn(
                      'px-3.5 py-2.5 align-middle text-ink-soft',
                      column.align === 'right' && 'text-right',
                      column.align === 'center' && 'text-center',
                      column.className,
                    )}
                  >
                    {column.render(row)}
                  </td>
                ))}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  )
}
