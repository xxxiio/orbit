import { useCallback, useEffect } from 'react'
import { useToast } from '@/components/ui/toast'
import { isAmbiguousError, uncertainMessage, userMessage } from '@/api/errors'

/**
 * Command failures are always surfaced, never swallowed. The caller keeps the
 * server's value on screen because no command is applied optimistically.
 *
 * A refusal says what failed. An unconfirmed write must not claim that: the
 * outcome is unknown, so it gets its own message and a refresh hint.
 */
export function useCommandFeedback() {
  const { toast } = useToast()

  const notifyFailure = useCallback(
    (title: string, error: unknown) => {
      if (isAmbiguousError(error)) {
        // Neutral, not danger: the write may have succeeded.
        toast({
          tone: 'default',
          title: 'Outcome not confirmed',
          description: uncertainMessage(error),
        })
        return
      }
      toast({ tone: 'danger', title, description: userMessage(error) })
    },
    [toast],
  )

  const run = useCallback(
    async <T,>(title: string, action: () => Promise<T>): Promise<T | null> => {
      try {
        return await action()
      } catch (error) {
        notifyFailure(title, error)
        return null
      }
    },
    [notifyFailure],
  )

  return { notifyFailure, run }
}

/**
 * The table and the stacked list both exist in the DOM; only one is laid out.
 * Prefer the list the browser is actually showing, and fall back to the first
 * one when there is no layout information (jsdom unit tests).
 */
function resolveVisibleRows(root: ParentNode): ParentNode {
  const lists = Array.from(root.querySelectorAll<HTMLElement>('[data-row-list]'))
  if (lists.length === 0) return root
  return lists.find((list) => list.getClientRects().length > 0) ?? lists[0]
}

function focusRow(row: HTMLElement | undefined): void {
  if (!row) return
  const action = row.querySelector<HTMLElement>('[data-row-action]')
  ;(action ?? row).focus()
}

/**
 * `j` / `k` move between rows, `x` toggles selection, `Enter` activates.
 *
 * The listener is page-wide so a shortcut works before anything on the table has
 * been clicked — but it never steals keys from a field, a menu, a dialog or any
 * control that is not a row title. Focus always lands on the row's title button,
 * so `Enter`/`Space` fire the same action as a click through native button
 * semantics instead of a second, row-only keydown path.
 */
export function useRowKeys({
  onToggleSelect,
}: {
  onToggleSelect?: (rowKey: string) => void
}) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return
      if (event.metaKey || event.ctrlKey || event.altKey) return
      if (!(event.target instanceof HTMLElement)) return
      const target = event.target
      // Fields and open overlays own their keys.
      if (target.closest('input, textarea, select, [contenteditable="true"]')) return
      if (target.closest('[role="dialog"], [role="menu"], [role="listbox"], [role="combobox"]')) return
      // Row controls (the title button) do not block the shortcuts; every other
      // button/link does.
      const interactive = target.closest<HTMLElement>('button, a')
      if (interactive && !interactive.closest('[data-row-action]')) return

      const key = event.key.toLowerCase()
      if (key !== 'j' && key !== 'k' && key !== 'x') return

      const rows = Array.from(
        resolveVisibleRows(document).querySelectorAll<HTMLElement>('[data-row][data-row-key]'),
      )
      if (rows.length === 0) return

      const current = target.closest<HTMLElement>('[data-row][data-row-key]')
      const index = current ? rows.indexOf(current) : -1

      if (key === 'x') {
        if (!current || !onToggleSelect) return
        const rowKey = current.dataset.rowKey
        if (!rowKey) return
        event.preventDefault()
        onToggleSelect(rowKey)
        return
      }

      event.preventDefault()
      const next =
        index === -1
          ? key === 'j'
            ? 0
            : rows.length - 1
          : Math.min(rows.length - 1, Math.max(0, index + (key === 'j' ? 1 : -1)))
      focusRow(rows[next])
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onToggleSelect])
}
