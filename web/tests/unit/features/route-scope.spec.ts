import { describe, expect, it } from 'vitest'
import { isKnowledgePath } from '@/app/route-scope'

/**
 * The drawer mount decision is a route decision, so it is tested as one: every
 * knowledge path is a note context, and no other surface loses `?task=`.
 */
describe('route scope', () => {
  it('recognises the knowledge surface at its root', () => {
    expect(isKnowledgePath('/knowledge')).toBe(true)
    expect(isKnowledgePath('/knowledge/')).toBe(true)
  })

  it('recognises a note or folder under the knowledge surface', () => {
    expect(isKnowledgePath('/knowledge/notes/orbit-overview.md')).toBe(true)
    expect(isKnowledgePath('/knowledge/notes/decisions/cookie-sessions.md')).toBe(true)
    expect(isKnowledgePath('/knowledge/')).toBe(true)
  })

  it('leaves every other surface with the documented task drawer', () => {
    for (const path of [
      '/',
      '/inbox',
      '/projects',
      '/projects/p-research',
      '/projects/p-research/board',
      '/projects/p-research/planning',
      '/agents',
      '/agents/job-1',
    ]) {
      expect(isKnowledgePath(path), path).toBe(false)
    }
  })

  it('does not claim a path that only starts with the same letters', () => {
    expect(isKnowledgePath('/knowledges')).toBe(false)
    expect(isKnowledgePath('/knowledge-base/notes/x.md')).toBe(false)
  })

  it('agrees with the router, which matches route paths case-insensitively', () => {
    expect(isKnowledgePath('/Knowledge/notes/x.md')).toBe(true)
  })
})
