/**
 * The chrome every board shares: the scroller, one declared column, an empty
 * column, a card shell and the keyboard hint.
 *
 * Boards differ in what a card says and which move the server offers; they must
 * not differ in how a column looks, how its count is announced, or how the
 * keyboard moves between cards. A new board surface composes these pieces.
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react'
import { cn } from '@/components/ui/utils'

/** The horizontal scroller a board's columns live in. */
export function BoardScrollRegion({
  label,
  children,
}: {
  label: string
  children: ReactNode
}) {
  return (
    <div className="scrollbar-slim -mx-1 overflow-x-auto px-1 pb-2">
      <div role="group" aria-label={label} className="flex min-w-full items-start gap-3">
        {children}
      </div>
    </div>
  )
}

/**
 * One declared column: its label, its count and a fixed width. The pseudo
 * element dot is decoration; the header text and the count pill are the
 * accessible content.
 */
export function BoardColumn({
  label,
  count,
  width,
  noun = 'item',
  dotClassName,
  children,
}: {
  label: string
  count: number
  width: number
  /** Singular noun for the column's accessible name. */
  noun?: string
  /** Tailwind class for the status dot; the default is the neutral marker. */
  dotClassName?: string
  children: ReactNode
}) {
  return (
    <section
      aria-label={`${label} column, ${count} ${noun}${count === 1 ? '' : 's'}`}
      className="flex shrink-0 flex-col rounded-lg border border-line bg-surface-sunken/35"
      style={{ width }}
    >
      <header className="flex items-center gap-2 border-b border-line px-2.5 py-2">
        <span
          aria-hidden="true"
          className={cn('h-1.5 w-1.5 rounded-full', dotClassName ?? 'bg-line-strong')}
        />
        <h3 className="text-xs font-semibold text-ink">{label}</h3>
        <span className="ml-auto rounded-full bg-surface px-1.5 py-0.5 text-2xs tabular-nums text-ink-muted">
          {count}
        </span>
      </header>
      <ul className="flex flex-1 flex-col gap-1.5 p-1.5">{children}</ul>
    </section>
  )
}

/** An empty declared column says so instead of rendering a blank box. */
export function BoardColumnEmpty() {
  return <li className="px-1 py-3 text-2xs text-ink-faint">Nothing here</li>
}

/** The card shell: one border, one shadow, one hover. */
export function BoardCard({
  children,
  className,
  cardId,
}: {
  children: ReactNode
  className?: string
  /** Optional stable id on the card element, for tests and focus scripts. */
  cardId?: string
}) {
  return (
    <article
      data-board-card={cardId}
      className={cn(
        'rounded-md border border-line bg-surface px-2.5 py-2 shadow-[0_1px_0_hsl(var(--ink)/0.03)] transition-colors',
        'hover:border-line-strong',
        className,
      )}
    >
      {children}
    </article>
  )
}

/** The keyboard model a board offers, named where the cards are. */
export function BoardKeyboardHint({ moveLabel = 'move status' }: { moveLabel?: string }) {
  return (
    <p className="hidden items-center gap-1 text-2xs text-ink-muted md:flex">
      <kbd className="command-key">h</kbd>
      <kbd className="command-key">j</kbd>
      <kbd className="command-key">k</kbd>
      <kbd className="command-key">l</kbd>
      <span>move</span>
      <kbd className="command-key">m</kbd>
      <span>{moveLabel}</span>
      <kbd className="command-key">↵</kbd>
      <span>open</span>
    </p>
  )
}

export interface BoardGroup<T> {
  key: string
  rows: readonly T[]
}

/**
 * Roving focus over board cards.
 *
 * The board is one tab stop: arrow keys (`h`/`j`/`k`/`l`) move the logical
 * cursor between cards, and moving it moves real focus so the next keypress
 * (Enter, `m`) reaches the same card. `m` is owned by the caller because a task
 * opens a status menu where a work item opens the sheet that lists its declared
 * moves.
 */
export function useBoardKeyboard<T extends { id: string }>(
  groups: readonly BoardGroup<T>[],
  { onMove }: { onMove?: (row: T) => void } = {},
) {
  const [focus, setFocus] = useState<string | null>(null)
  const cardRefs = useRef(new Map<string, HTMLButtonElement>())

  const setCardRef = useCallback((id: string, element: HTMLButtonElement | null) => {
    if (element) cardRefs.current.set(id, element)
    else cardRefs.current.delete(id)
  }, [])

  // Roving focus: moving the logical cursor moves real focus, so the next
  // keypress (including Enter) reaches the same card.
  useEffect(() => {
    if (!focus) return
    cardRefs.current.get(focus)?.focus({ preventScroll: false })
  }, [focus])

  const focusCard = (id: string | undefined) => {
    if (id) setFocus(id)
  }

  const moveFocus = (row: T, key: 'h' | 'j' | 'k' | 'l') => {
    const columnIndex = groups.findIndex((group) => group.rows.some((entry) => entry.id === row.id))
    if (columnIndex < 0) return
    const column = groups[columnIndex]
    const rowIndex = column.rows.findIndex((entry) => entry.id === row.id)

    if (key === 'j' || key === 'k') {
      const nextIndex = rowIndex + (key === 'j' ? 1 : -1)
      focusCard(column.rows[nextIndex]?.id)
      return
    }

    const direction = key === 'l' ? 1 : -1
    for (let index = columnIndex + direction; index >= 0 && index < groups.length; index += direction) {
      const candidate = groups[index]
      if (candidate.rows.length === 0) continue
      const nextRow = Math.min(rowIndex, candidate.rows.length - 1)
      focusCard(candidate.rows[nextRow]?.id)
      return
    }
  }

  const onCardKeyDown = (event: KeyboardEvent<HTMLButtonElement>, row: T) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return
    const key = event.key.toLowerCase()
    if (key === 'm') {
      if (!onMove) return
      event.preventDefault()
      event.stopPropagation()
      onMove(row)
      return
    }
    if (key !== 'h' && key !== 'j' && key !== 'k' && key !== 'l') return
    event.preventDefault()
    event.stopPropagation()
    moveFocus(row, key)
  }

  const firstCardId = groups.flatMap((group) => group.rows)[0]?.id ?? null

  return {
    focus,
    setFocus,
    cardRefs,
    setCardRef,
    focusCard,
    onCardKeyDown,
    tabStop: focus ?? firstCardId,
  }
}
