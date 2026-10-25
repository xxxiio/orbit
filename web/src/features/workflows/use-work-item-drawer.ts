/**
 * The work-item sheet is URL state (`?item=<id>`), like the task drawer: a
 * refresh keeps it open, Back closes it, and a link can point at one item.
 * The query parameter is namespaced away from `?task=` so both drawers can
 * coexist in the same workspace.
 */
import { useCallback } from 'react'
import { useSearchParams } from 'react-router-dom'

let returnFocusKey: string | null = null

export function useWorkItemDrawer() {
  const [params, setParams] = useSearchParams()
  const itemId = params.get('item')

  const openItem = useCallback(
    (id: string) => {
      const active = document.activeElement instanceof HTMLElement ? document.activeElement : null
      returnFocusKey =
        active?.closest<HTMLElement>('[data-work-item-key]')?.dataset.workItemKey ?? id
      setParams((current) => {
        const next = new URLSearchParams(current)
        next.set('item', id)
        return next
      })
    },
    [setParams],
  )

  const closeItem = useCallback(() => {
    const key = returnFocusKey
    returnFocusKey = null
    setParams(
      (current) => {
        if (!current.has('item')) return current
        const next = new URLSearchParams(current)
        next.delete('item')
        return next
      },
      { replace: true },
    )
    window.setTimeout(() => {
      if (!key) return
      const row = document.querySelector<HTMLElement>(`[data-work-item-key="${CSS.escape(key)}"]`)
      const target = row?.querySelector<HTMLElement>('[data-work-item-action]') ?? row
      target?.focus({ preventScroll: true })
    }, 80)
  }, [setParams])

  return { itemId, openItem, closeItem }
}
