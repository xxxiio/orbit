import { useCallback } from 'react'
import { useSearchParams } from 'react-router-dom'

/**
 * The task drawer is a URL state, not a local flag: `?task=<id>` survives a
 * reload, can be shared, and Back closes it. Everything else stays on the page.
 *
 * The row that opened the drawer is remembered by its `data-row-key`. It lives
 * at module scope because the page that opens the drawer and the drawer itself
 * each call this hook: they must agree on where focus goes back to.
 */
let returnFocusKey: string | null = null

export function useTaskDrawer() {
  const [params, setParams] = useSearchParams()
  const taskId = params.get('task')

  const openTask = useCallback(
    (id: string) => {
      const active = document.activeElement instanceof HTMLElement ? document.activeElement : null
      returnFocusKey = active?.closest<HTMLElement>('[data-row-key]')?.dataset.rowKey ?? id
      setParams((current) => {
        const next = new URLSearchParams(current)
        next.set('task', id)
        return next
      })
    },
    [setParams],
  )

  const closeTask = useCallback(() => {
    const key = returnFocusKey
    returnFocusKey = null
    setParams(
      (current) => {
        if (!current.has('task')) return current
        const next = new URLSearchParams(current)
        next.delete('task')
        return next
      },
      { replace: true },
    )
    // Radix restores focus when its content unmounts; run after that cleanup so
    // the row's title control, not the document body, ends up focused.
    window.setTimeout(() => {
      if (!key) return
      const row = document.querySelector<HTMLElement>(`[data-row-key="${CSS.escape(key)}"]`)
      const target = row?.querySelector<HTMLElement>('[data-row-action]') ?? row
      target?.focus({ preventScroll: true })
    }, 80)
  }, [setParams])

  return { taskId, openTask, closeTask }
}
