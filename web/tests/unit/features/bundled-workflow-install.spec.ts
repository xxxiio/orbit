/**
 * B2 unit coverage: the install control is version-aware. A workflow id that is
 * already published at the bundled revision is hidden; an older stored revision
 * is offered again as an update, because published versions are immutable.
 */
import { describe, expect, it } from 'vitest'
import { installableBundledWorkflows } from '@/features/workflows/bundled-workflow-install'

const bundled = [
  { id: 'development', version: 3, name: 'Development' },
  { id: 'habits', version: 2, name: 'Habits' },
  { id: 'learning', version: 2, name: 'Learning' },
]

describe('installableBundledWorkflows', () => {
  it('offers a bundled workflow whose stored version is older', () => {
    const candidates = installableBundledWorkflows(bundled, [
      { id: 'development', version: 2 },
    ])
    expect(candidates.map((entry) => entry.workflow.id)).toEqual([
      'development',
      'habits',
      'learning',
    ])
    expect(candidates[0]).toMatchObject({
      workflow: { id: 'development', version: 3 },
      storedVersion: 2,
    })
  })

  it('hides a workflow whose stored version is the bundled one or newer', () => {
    const candidates = installableBundledWorkflows(bundled, [
      { id: 'development', version: 3 },
      { id: 'habits', version: 4 },
      { id: 'learning', version: 2 },
    ])
    expect(candidates).toEqual([])
  })

  it('does not treat an unrelated published workflow as bundled', () => {
    const candidates = installableBundledWorkflows(bundled, [
      { id: 'guarded', version: 1 },
    ])
    expect(candidates.map((entry) => entry.workflow.id)).toEqual([
      'development',
      'habits',
      'learning',
    ])
  })
})
