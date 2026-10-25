/**
 * A drawer whose open resource is URL state.
 *
 * `?task=<id>` / `?item=<id>` survive a reload, can be shared, and Back closes
 * the drawer. Everything else stays on the page. The row that opened the drawer
 * is remembered by a data attribute so focus returns there after the drawer
 * unmounts; the map is module-scoped and keyed by parameter because the page
 * that opens a drawer and the drawer itself each call this hook, and two
 * resources can be open in the same workspace.
 */
import { useCallback } from 'react'
import { useSearchParams } from 'react-router-dom'

export interface ResourceDrawerOptions {
  /** URL parameter that names the open resource (`task`, `item`). */
  param: string
  /** Attribute on the row element, used as the focus-return fallback. */
  rowAttribute: string
  /** Attribute on the row's primary control, preferred for focus return. */
  actionAttribute: string
}

export interface ResourceDrawer {
  /** The id named by the URL, or `null`. */
  id: string | null
  open: (id: string) => void
  close: () => void
}

/** Where each resource's opening row was, so focus can go back to it. */
const returnFocusKeys = new Map<string, string | null>()

export function useResourceDrawer({
  param,
  rowAttribute,
  actionAttribute,
}: ResourceDrawerOptions): ResourceDrawer {
  const [params, setParams] = useSearchParams()
  const id = params.get(param)

  const open = useCallback(
    (nextId: string) => {
      const active = document.activeElement instanceof HTMLElement ? document.activeElement : null
      returnFocusKeys.set(
        param,
        active?.closest<HTMLElement>(`[${rowAttribute}]`)?.getAttribute(rowAttribute) ?? nextId,
      )
      setParams((current) => {
        const next = new URLSearchParams(current)
        next.set(param, nextId)
        return next
      })
    },
    [param, rowAttribute, setParams],
  )

  const close = useCallback(() => {
    const key = returnFocusKeys.get(param) ?? null
    returnFocusKeys.set(param, null)
    setParams(
      (current) => {
        if (!current.has(param)) return current
        const next = new URLSearchParams(current)
        next.delete(param)
        return next
      },
      { replace: true },
    )
    // Radix restores focus when its content unmounts; run after that cleanup so
    // the row's title control, not the document body, ends up focused.
    window.setTimeout(() => {
      if (!key) return
      const row = document.querySelector<HTMLElement>(
        `[${rowAttribute}="${CSS.escape(key)}"]`,
      )
      const target = row?.querySelector<HTMLElement>(`[${actionAttribute}]`) ?? row
      target?.focus({ preventScroll: true })
    }, 80)
  }, [actionAttribute, param, rowAttribute, setParams])

  return { id, open, close }
}
