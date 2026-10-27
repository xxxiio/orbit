/**
 * Which surface a pathname belongs to, for shell decisions that must be made
 * once rather than by every page.
 *
 * `/knowledge` is the one surface where `?task=` is a *note context* rather than
 * a request to open the task drawer, so the layout needs to recognise it
 * without importing the knowledge feature.
 */
const KNOWLEDGE = '/knowledge'

/** True for `/knowledge` and anything beneath it (`/knowledge/notes/x.md`). */
export function isKnowledgePath(pathname: string): boolean {
  // React Router matches route paths case-insensitively, so the check that
  // decides whether to mount the drawer has to agree with it.
  const path = pathname.toLowerCase()
  return path === KNOWLEDGE || path.startsWith(`${KNOWLEDGE}/`)
}
