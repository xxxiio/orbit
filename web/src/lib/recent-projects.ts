/**
 * Which projects this browser opened, newest first.
 *
 * The server snapshot has no project activity timestamp, so the app does not
 * claim to know what changed recently. This is explicitly local history; the UI
 * labels it as such.
 */
import { useSyncExternalStore } from 'react'

const STORAGE_KEY = 'orbit.recent-projects'
const LIMIT = 8

function read(): string[] {
  if (typeof window === 'undefined') return []
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((value): value is string => typeof value === 'string').slice(0, LIMIT)
  } catch {
    return []
  }
}

let ids: string[] = read()
const listeners = new Set<() => void>()

function emit() {
  for (const listener of listeners) listener()
}

export function recordProjectOpen(projectId: string): void {
  const next = [projectId, ...ids.filter((id) => id !== projectId)].slice(0, LIMIT)
  if (next.length === ids.length && next.every((id, index) => id === ids[index])) return
  ids = next
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(ids))
  } catch {
    // A browser that refuses storage still gets in-memory ordering.
  }
  emit()
}

export function subscribeRecentProjects(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function getRecentProjectIds(): string[] {
  return ids
}

export function useRecentProjectIds(): string[] {
  return useSyncExternalStore(subscribeRecentProjects, getRecentProjectIds, () => [])
}
