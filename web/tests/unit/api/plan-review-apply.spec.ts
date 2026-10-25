import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { ClientStateError } from '@/api/errors'
import { useApplyApprovedPlan, usePlanReview } from '@/api/mutations'
import {
  applyApprovedPlan,
  buildApprovedScheduleRequest,
  type ReviewedSchedulePlan,
} from '@/api/endpoints/planning'
import type { ApprovedScheduleRequest, PlanReview, ScheduleApprovalRequest } from '@/api/contracts'
import { jsonResponse, queryWrapper, testQueryClient } from './test-utils'

/**
 * The approval gate: review produces a fingerprinted command; apply echoes that
 * exact review. The client may not invent a fingerprint, cannot apply a plan it
 * never reviewed, and must never regenerate the reviewed command id.
 */
describe('plan review and explicit approval', () => {
  const plan: ScheduleApprovalRequest = {
    command_id: 'cmd-reviewed-7',
    expected_revision: 4,
    window_start_ms: Date.UTC(2026, 9, 26, 9, 0),
    window_end_ms: Date.UTC(2026, 9, 26, 12, 0),
    items: [
      {
        task_id: 't-1',
        starts_at_ms: Date.UTC(2026, 9, 26, 9, 0),
        ends_at_ms: Date.UTC(2026, 9, 26, 10, 0),
        schedule_block_id: 'b-1',
        activity_id: 'a-1',
      },
    ],
  }

  const review: PlanReview = {
    expected_revision: 4,
    actor: 'device-a',
    command_id: 'cmd-reviewed-7',
    fingerprint: 'fp-abc123',
    scheduled_block_ids: ['b-1'],
    scheduled_tasks: ['t-1'],
    scheduled_slots: [
      {
        task_id: 't-1',
        schedule_block_id: 'b-1',
        starts_at_ms: plan.items[0].starts_at_ms,
        ends_at_ms: plan.items[0].ends_at_ms,
        duration_minutes: 60,
      },
    ],
    total_reserved_minutes: 60,
    requires_explicit_approval: true,
    creates_calendar_blocks: true,
  }

  function reviewed(overrides: Partial<PlanReview> = {}): ReviewedSchedulePlan {
    return { plan, review: { ...review, ...overrides } }
  }

  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('returns the server review with fingerprint, command id and explicit-approval flag', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(review))

    const client = testQueryClient()
    const { result } = renderHook(() => usePlanReview(), { wrapper: queryWrapper(client) })

    let returned: Awaited<ReturnType<typeof reviewPlan>> | undefined
    await act(async () => {
      returned = await result.current.reviewPlanAsync(plan)
    })

    expect(returned?.review).toEqual(review)
    await waitFor(() => expect(result.current.review).not.toBeNull())
    expect(result.current.review?.fingerprint).toBe('fp-abc123')
    expect(result.current.review?.command_id).toBe('cmd-reviewed-7')
    expect(result.current.review?.requires_explicit_approval).toBe(true)
    expect(result.current.fingerprint).toBe('fp-abc123')
    expect(result.current.requiresExplicitApproval).toBe(true)

    const [url, init] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/v1/authoritative/plan-review')
    expect(init.method).toBe('POST')
    // The reviewed command is sent as-is: same command id, same window, same items.
    expect(JSON.parse(init.body as string)).toEqual(plan)
  })

  it('refuses to apply without a reviewed plan and does not mint a command id', async () => {
    expect(() => buildApprovedScheduleRequest(null)).toThrow(ClientStateError)
    expect(() => buildApprovedScheduleRequest(undefined)).toThrow(ClientStateError)

    await expect(applyApprovedPlan(null)).rejects.toBeInstanceOf(ClientStateError)
    await expect(applyApprovedPlan(reviewed({ fingerprint: '' }))).rejects.toBeInstanceOf(
      ClientStateError,
    )
    await expect(applyApprovedPlan(reviewed({ fingerprint: '   ' }))).rejects.toBeInstanceOf(
      ClientStateError,
    )

    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })

  it('refuses a review that is not marked as requiring explicit approval', async () => {
    await expect(
      applyApprovedPlan(reviewed({ requires_explicit_approval: false })),
    ).rejects.toBeInstanceOf(ClientStateError)

    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })

  it('refuses an approval that disagrees with the reviewed command id', async () => {
    await expect(
      applyApprovedPlan(reviewed({ command_id: 'cmd-something-else' })),
    ).rejects.toBeInstanceOf(ClientStateError)

    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })

  it('applies exactly the reviewed fingerprint, command id, window and items, and surfaces replayed', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(
      jsonResponse({ accepted_revision: 5, replayed: true, schedule_block_ids: ['b-1'] }),
    )

    const client = testQueryClient()
    const { result } = renderHook(() => useApplyApprovedPlan(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.mutate(reviewed())
    })

    await waitFor(() => expect(result.current.disposition).toBe('accepted'))
    expect(result.current.data?.replayed).toBe(true)
    expect(result.current.data?.acceptedRevision).toBe(5)
    expect(result.current.data?.scheduleBlockIds).toEqual(['b-1'])

    const [url, init] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/v1/authoritative/plan-apply-approved')
    expect(init.method).toBe('POST')

    const body = JSON.parse(init.body as string) as ApprovedScheduleRequest
    expect(body.plan.command_id).toBe(review.command_id)
    expect(body.plan.window_start_ms).toBe(plan.window_start_ms)
    expect(body.plan.window_end_ms).toBe(plan.window_end_ms)
    expect(body.plan.items).toEqual(plan.items)
    expect(body.approval.approved).toBe(true)
    expect(body.approval.review.fingerprint).toBe('fp-abc123')
    expect(body.approval.review).toEqual(review)
  })

  it('rejects a null reviewed plan through the mutation disposition, with no request', async () => {
    const client = testQueryClient()
    const { result } = renderHook(() => useApplyApprovedPlan(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.mutate(null)
    })

    await waitFor(() => expect(result.current.disposition).toBe('rejected'))
    expect(result.current.error).toBeInstanceOf(ClientStateError)
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })
})
