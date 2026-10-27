import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { QueryClient } from '@tanstack/react-query'
import type {
  CapabilityActionRequestRecord,
  CapabilityListResponse,
  CapabilityOfferRecord,
  CapabilityReceiptRecord,
} from '@/api/contracts'
import type { CapabilityRequest } from '@/api/adapters'
import { ApiError, ClientStateError } from '@/api/errors'
import {
  useCapabilityRequests,
  useCapabilityRequestShow,
  queryKeys,
} from '@/api/queries'
import { useReconcileCapabilityAction, useRequestCapabilityAction } from '@/api/mutations'
import { jsonResponse, queryWrapper, testQueryClient } from './test-utils'

/**
 * The capability commands' house rules: the fence is the revision whose offers
 * the person was shown, an unconfirmed answer is `uncertain`, and nothing the
 * browser holds ever calls the executor-only claim/result routes.
 */

function requestRecord(
  overrides: Partial<CapabilityActionRequestRecord> = {},
): CapabilityActionRequestRecord {
  return {
    command_id: 'cap-1',
    item_id: 'wi-1',
    item_revision: 3,
    action_id: 'append_note',
    capability_id: 'orbit.reference.work-item-note',
    from: 'open',
    to: 'closed',
    state: 'pending',
    inputs: { note: 'Hello' },
    created_at_ms: 1_760_000_000_000,
    executor_device_id: null,
    ...overrides,
  }
}

function receiptRecord(overrides: Partial<CapabilityReceiptRecord> = {}): CapabilityReceiptRecord {
  return {
    outcome: 'failed',
    detail: { reason: 'stopped' },
    reported_at_ms: 1_760_000_600_000,
    reported_by: 'operator-1',
    ...overrides,
  }
}

function offerRecord(overrides: Partial<CapabilityOfferRecord> = {}): CapabilityOfferRecord {
  return {
    action_id: 'append_note',
    capability_id: 'orbit.reference.work-item-note',
    from: 'open',
    to: 'closed',
    description: 'Append',
    input_schema: { type: 'object' },
    external_effect: false,
    available: true,
    code: null,
    reason: null,
    ...overrides,
  }
}

function sentBodies(path: string): Array<Record<string, unknown>> {
  return vi
    .mocked(fetch)
    .mock.calls.filter(([url]) => String(url).includes(path))
    .map(([, init]) => JSON.parse((init as RequestInit).body as string))
}

function requestUrls(): string[] {
  return vi.mocked(fetch).mock.calls.map(([url]) => String(url))
}

describe('capability request command', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  function seedOffers(client: QueryClient, revision = 3): void {
    client.setQueryData(queryKeys.capabilityOffers('wi-1', revision), {
      itemId: 'wi-1',
      itemRevision: revision,
      offers: [],
    })
  }

  it('fences on the revision whose offers the person was shown, not the newest cached item', async () => {
    const client = testQueryClient()
    seedOffers(client, 3)
    // The item has moved on since the offers were read; the request must still
    // consent to the offers the person actually saw.
    client.setQueryData(queryKeys.workItems('p-1'), [
      { id: 'wi-1', project_id: 'p-1', kind: 'ticket', state: 'open', properties: {}, revision: 9 },
    ])
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ request: requestRecord() }))

    const { result } = renderHook(() => useRequestCapabilityAction(), {
      wrapper: queryWrapper(client),
    })

    await act(async () => {
      await result.current.mutateAsync({
        commandId: 'cap-1',
        itemId: 'wi-1',
        itemRevision: 3,
        actionId: 'append_note',
        inputs: { note: 'Hello' },
        confirm: true,
      })
    })

    expect(sentBodies('/capability-actions/request')).toEqual([
      {
        command_id: 'cap-1',
        item_id: 'wi-1',
        expected_revision: 3,
        action_id: 'append_note',
        inputs: { note: 'Hello' },
        confirm: true,
      },
    ])
  })

  it('sends nothing when the offers it would fence against are gone', async () => {
    const client = testQueryClient()
    const { result } = renderHook(() => useRequestCapabilityAction(), {
      wrapper: queryWrapper(client),
    })

    let failure: unknown
    await act(async () => {
      failure = await result.current
        .mutateAsync({
          commandId: 'cap-1',
          itemId: 'wi-1',
          itemRevision: 3,
          actionId: 'append_note',
          inputs: {},
          confirm: false,
        })
        .catch((error: unknown) => error)
    })

    expect(failure).toBeInstanceOf(ClientStateError)
    expect((failure as ApiError).message).toMatch(/Open this item’s actions again/)
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
    await waitFor(() => expect(result.current.disposition).toBe('rejected'))
  })

  it('treats a lost answer as uncertain and refreshes the offers, the history and the item', async () => {
    const client = testQueryClient()
    seedOffers(client)
    const invalidate = vi.spyOn(client, 'invalidateQueries')
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({ error: 'storage unavailable' }, 503))

    const { result } = renderHook(() => useRequestCapabilityAction(), {
      wrapper: queryWrapper(client),
    })

    await act(async () => {
      await result.current
        .mutateAsync({
          commandId: 'cap-1',
          itemId: 'wi-1',
          itemRevision: 3,
          actionId: 'append_note',
          inputs: { note: 'Hello' },
          confirm: true,
        })
        .catch(() => null)
    })

    await waitFor(() => expect(result.current.disposition).toBe('uncertain'))
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.capabilityOffersAll('wi-1') })
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.capabilityRequests('wi-1') })
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.workItemsAll() })
  })

  it('replays the same command id rather than minting a second intent', async () => {
    const client = testQueryClient()
    seedOffers(client)
    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ request: requestRecord() }))
      .mockResolvedValueOnce(jsonResponse({ request: requestRecord() }))

    const { result } = renderHook(() => useRequestCapabilityAction(), {
      wrapper: queryWrapper(client),
    })
    const variables = {
      commandId: 'cap-1',
      itemId: 'wi-1',
      itemRevision: 3,
      actionId: 'append_note',
      inputs: { note: 'Hello' },
      confirm: true,
    }

    await act(async () => {
      await result.current.mutateAsync(variables)
      await result.current.mutateAsync(variables)
    })

    expect(sentBodies('/capability-actions/request').map((body) => body.command_id)).toEqual([
      'cap-1',
      'cap-1',
    ])
  })
})

describe('capability reconcile command', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('records a decision and stores the settled receipt', async () => {
    const client = testQueryClient()
    const settled = requestRecord({ state: 'failed' })
    vi.mocked(fetch).mockResolvedValueOnce(
      jsonResponse({ request: settled, receipt: receiptRecord() }),
    )

    const { result } = renderHook(() => useReconcileCapabilityAction(), {
      wrapper: queryWrapper(client),
    })

    await act(async () => {
      await result.current.mutateAsync({
        commandId: 'cap-1',
        itemId: 'wi-1',
        decision: 'confirm_failed',
        detail: 'the process stopped',
      })
    })

    expect(sentBodies('/capability-actions/reconcile')).toEqual([
      { command_id: 'cap-1', decision: 'confirm_failed', detail: 'the process stopped' },
    ])
    const stored = client.getQueryData<{ request: CapabilityRequest; receipt: unknown }>(
      queryKeys.capabilityRequest('cap-1'),
    )
    expect(stored?.request.state).toBe('failed')
    expect(stored?.request.recordedState).toBe('failed')
    expect(stored?.receipt).toMatchObject({ outcome: 'failed', reportedBy: 'operator-1' })
  })

  it('never calls the executor-only claim or result routes', async () => {
    const client = testQueryClient()
    seedOffersFor(client)
    vi.mocked(fetch).mockResolvedValue(jsonResponse({ request: requestRecord() }))

    const request = renderHook(() => useRequestCapabilityAction(), {
      wrapper: queryWrapper(client),
    })
    await act(async () => {
      await request.result.current
        .mutateAsync({
          commandId: 'cap-1',
          itemId: 'wi-1',
          itemRevision: 3,
          actionId: 'append_note',
          inputs: { note: 'Hello' },
          confirm: true,
        })
        .catch(() => null)
    })

    const urls = requestUrls()
    expect(urls.some((url) => url.includes('/capability-actions/claim'))).toBe(false)
    expect(urls.some((url) => url.includes('/capability-actions/result'))).toBe(false)
  })

  function seedOffersFor(client: QueryClient): void {
    client.setQueryData(queryKeys.capabilityOffers('wi-1', 3), {
      itemId: 'wi-1',
      itemRevision: 3,
      offers: [offerRecord()],
    })
  }
})

describe('capability request polling', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('re-reads while a request is open and stops once it settles', async () => {
    const client = testQueryClient()
    let state = 'pending'
    vi.mocked(fetch).mockImplementation(async () => {
      const body: CapabilityListResponse = {
        requests: [requestRecord({ state })],
      }
      return jsonResponse(body)
    })

    const { result } = renderHook(
      () => useCapabilityRequests('wi-1', { pollIntervalMs: 20 }),
      { wrapper: queryWrapper(client) },
    )

    await waitFor(() => expect(result.current.data?.[0]?.state).toBe('pending'))
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.length).toBeGreaterThanOrEqual(2))

    state = 'succeeded'
    await waitFor(() => expect(result.current.data?.[0]?.state).toBe('succeeded'))
    const settledCalls = vi.mocked(fetch).mock.calls.length
    await new Promise((resolve) => globalThis.setTimeout(resolve, 120))
    expect(vi.mocked(fetch).mock.calls.length).toBe(settledCalls)
  })
})

describe('capability request receipt read', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('is not read while the outcome is outstanding', async () => {
    const client = testQueryClient()
    renderHook(() => useCapabilityRequestShow('cap-1', false), {
      wrapper: queryWrapper(client),
    })
    await new Promise((resolve) => globalThis.setTimeout(resolve, 10))
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })
})
