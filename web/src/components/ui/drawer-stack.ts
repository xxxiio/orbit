import { useSyncExternalStore } from 'react'

/**
 * Which side currently holds an open drawer.
 *
 * Toasts are fixed to a viewport corner. A drawer covers a full-height edge, so
 * a toast on that same edge sits on top of the drawer's own controls. This tiny
 * external store lets the toast viewport move to the opposite edge without
 * threading drawer state through the app shell.
 */
export type DrawerSide = 'left' | 'right'

let openSides: DrawerSide[] = []
const listeners = new Set<() => void>()

function emit(): void {
  for (const listener of listeners) listener()
}

/** Register an open drawer; the returned cleanup closes it. */
export function openDrawer(side: DrawerSide): () => void {
  openSides = [...openSides, side]
  emit()
  let closed = false
  return () => {
    if (closed) return
    closed = true
    const index = openSides.lastIndexOf(side)
    if (index >= 0) openSides = [...openSides.slice(0, index), ...openSides.slice(index + 1)]
    emit()
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function snapshot(): DrawerSide | null {
  return openSides[openSides.length - 1] ?? null
}

export function useOpenDrawerSide(): DrawerSide | null {
  return useSyncExternalStore(subscribe, snapshot, () => null)
}
