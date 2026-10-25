import { afterEach, describe, expect, it, vi } from 'vitest'
import { fetchWorkItem, fetchWorkItems } from '@/api/endpoints/workflows'
import type { BlueprintWorkItemRecord } from '@/api/contracts'
import { jsonResponse } from './test-utils'

/**
 * Serde trap 3: the items route is polymorphic. `?id=` yields one object and
 * `?project_id=` yields an array. If a single helper branched on `Array.isArray`,
 * a 404 for one item would look like an empty project, so the two wire shapes
 * get two functions with two return types.
 */
describe('work item endpoints', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  const item: BlueprintWorkItemRecord = {
    id: 'w-1',
    project_id: 'p-1',
    kind: 'feature',
    state: 'backlog',
    properties: {},
    revision: 9,
  }

  it('parses ?project_id= as an array from one request', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(jsonResponse([item]))
    vi.stubGlobal('fetch', fetchMock)

    const items = await fetchWorkItems('p-1')

    expect(Array.isArray(items)).toBe(true)
    expect(items).toHaveLength(1)
    expect(items[0]?.id).toBe('w-1')

    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/v1/authoritative/blueprints/items?project_id=p-1')
    expect(url).not.toContain('id=w-1')
  })

  it('parses ?id= as a single object with no array branching', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(jsonResponse(item))
    vi.stubGlobal('fetch', fetchMock)

    const single = await fetchWorkItem('w-1')

    expect(Array.isArray(single)).toBe(false)
    expect(single.revision).toBe(9)
    expect(single.project_id).toBe('p-1')

    const [url] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/v1/authoritative/blueprints/items?id=w-1')
    expect(url).not.toContain('project_id=')
  })

  it('surfaces a missing single item as 404 instead of an empty list', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse({ error: 'work item not found' }, 404))
    vi.stubGlobal('fetch', fetchMock)

    await expect(fetchWorkItem('missing')).rejects.toMatchObject({ status: 404 })
  })
})
