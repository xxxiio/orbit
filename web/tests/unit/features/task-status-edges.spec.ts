/**
 * S1 unit coverage: the status controls filter the domain's edge set instead of
 * carrying a transition table of their own. `null` is the honest fallback: the
 * read failed, so every status is offered and the server keeps refusing.
 */
import { describe, expect, it } from 'vitest'
import type { TaskStatus } from '@/api/contracts'
import { statusOptionsFor } from '@/features/tasks'

const inboxEdges: TaskStatus[] = ['planned', 'ready', 'deferred', 'cancelled']

describe('statusOptionsFor', () => {
  it('offers the stored status plus the returned edges, in canonical order', () => {
    expect(statusOptionsFor('inbox', inboxEdges).map((option) => option.value)).toEqual([
      'inbox',
      'planned',
      'ready',
      'deferred',
      'cancelled',
    ])
  })

  it('never offers a target the domain refused, even if it is a known status', () => {
    const offered = statusOptionsFor('inbox', inboxEdges).map((option) => option.value)
    for (const refused of ['in_progress', 'done', 'review']) {
      expect(offered).not.toContain(refused)
    }
  })

  it('offers nothing but the stored status when the domain allows no edge', () => {
    expect(statusOptionsFor('done', []).map((option) => option.value)).toEqual(['done'])
  })

  it('keeps every status when the edge read failed', () => {
    expect(statusOptionsFor('inbox', null)).toHaveLength(10)
  })
})
