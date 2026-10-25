import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { ClientStateError } from '@/api/errors'
import { useDayPlan, useSchedulePlan } from '@/api/queries'
import {
  fetchDayPlan,
  fetchProjectDependencies,
  fetchSchedulePlan,
  fetchTaskGraph,
  fetchWorkBrief,
  type DayPlanParams,
  type SchedulePlanParams,
} from '@/api/endpoints/planning'
import type { DayPlan, SchedulePlan } from '@/api/contracts'
import { jsonResponse, queryWrapper, testQueryClient } from './test-utils'

const NOW_MS = 1_700_000_000_000

const dayPlan: DayPlan = {
  revision: 2,
  now_ms: NOW_MS,
  available_minutes: 90,
  allocated_minutes: 30,
  remaining_minutes: 60,
  selected_count: 1,
  actionable_count: 1,
  blocked_count: 0,
  tasks: [
    {
      task_id: 't-1',
      title: 'Draft the brief',
      project_id: 'p-1',
      disposition: 'selected',
      estimated_minutes: 30,
      allocated_minutes: 30,
      start_offset_minutes: 0,
      blocking_task_ids: [],
      explanation: ['Ready and fits the authored capacity.'],
      rank: 1,
    },
  ],
  projects: [
    { project_id: 'p-1', selected: 1, actionable: 1, blocked: 0, allocated_minutes: 30 },
  ],
  warnings: [],
}

const schedulePlan: SchedulePlan = {
  task_revision: 8,
  schedule_revision: 9,
  starts_at_ms: Date.UTC(2026, 9, 26, 9, 0),
  ends_at_ms: Date.UTC(2026, 9, 26, 12, 0),
  available_minutes: 180,
  allocated_minutes: 0,
  occupied_minutes: 0,
  free_windows: [],
  slots: [],
  unplaced: [],
  warnings: [],
}

describe('planning reads: caller-supplied parameters', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('keeps the day plan idle with zero authored capacity, then sends the authored number', async () => {
    const client = testQueryClient()
    const initialProps: { params: DayPlanParams } = {
      params: { nowMs: NOW_MS, availableMinutes: 0 },
    }

    const { result, rerender } = renderHook(
      ({ params }: { params: DayPlanParams }) => useDayPlan(params),
      { wrapper: queryWrapper(client), initialProps },
    )

    // No capacity authored: nothing is requested and no capacity is guessed.
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
    expect(result.current.fetchStatus).toBe('idle')
    expect(result.current.data).toBeUndefined()

    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(dayPlan))
    rerender({ params: { nowMs: NOW_MS, availableMinutes: 90 } })

    await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(result.current.data?.availableMinutes).toBe(90))

    const [url] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit]
    expect(url).toContain('/v1/authoritative/day-plan?')
    expect(url).toContain(`now_ms=${NOW_MS}`)
    expect(url).toContain('available_minutes=90')
    expect(url).not.toContain('undefined')
  })

  it('keeps the schedule plan idle until both window ends are supplied', async () => {
    const client = testQueryClient()
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(schedulePlan))

    const { result, rerender } = renderHook(
      ({ params }: { params: SchedulePlanParams | null }) => useSchedulePlan(params),
      {
        wrapper: queryWrapper(client),
        initialProps: { params: null as SchedulePlanParams | null },
      },
    )

    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
    expect(result.current.fetchStatus).toBe('idle')

    rerender({
      params: { startsAtMs: schedulePlan.starts_at_ms, endsAtMs: schedulePlan.ends_at_ms },
    })

    await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1))
    const [url] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit]
    expect(url).toContain(`starts_at_ms=${schedulePlan.starts_at_ms}`)
    expect(url).toContain(`ends_at_ms=${schedulePlan.ends_at_ms}`)
    expect(url).not.toContain('undefined')
  })

  it('serialises an explicit schedule window and rejects an inverted window before any fetch', async () => {
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(schedulePlan))

    const plan = await fetchSchedulePlan({
      startsAtMs: schedulePlan.starts_at_ms,
      endsAtMs: schedulePlan.ends_at_ms,
    })
    expect(plan.task_revision).toBe(8)

    const [url] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit]
    expect(url).toBe(
      `/v1/authoritative/schedule-plan?starts_at_ms=${schedulePlan.starts_at_ms}&ends_at_ms=${schedulePlan.ends_at_ms}`,
    )

    vi.mocked(fetch).mockClear()
    await expect(
      fetchSchedulePlan({ startsAtMs: schedulePlan.ends_at_ms, endsAtMs: schedulePlan.starts_at_ms }),
    ).rejects.toBeInstanceOf(ClientStateError)
    await expect(
      fetchSchedulePlan({ startsAtMs: schedulePlan.starts_at_ms, endsAtMs: schedulePlan.starts_at_ms }),
    ).rejects.toBeInstanceOf(ClientStateError)
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })

  it('turns missing or unusable required parameters into typed client errors, not requests', async () => {
    const missing = undefined as unknown as number

    await expect(
      fetchDayPlan({ nowMs: NOW_MS, availableMinutes: missing }),
    ).rejects.toBeInstanceOf(ClientStateError)
    await expect(fetchDayPlan({ nowMs: NOW_MS, availableMinutes: 0 })).rejects.toBeInstanceOf(
      ClientStateError,
    )
    await expect(fetchTaskGraph(missing)).rejects.toBeInstanceOf(ClientStateError)
    await expect(
      fetchWorkBrief({ nowMs: NOW_MS, endsAtMs: missing }),
    ).rejects.toBeInstanceOf(ClientStateError)
    await expect(fetchProjectDependencies({ nowMs: missing })).rejects.toBeInstanceOf(
      ClientStateError,
    )
    await expect(
      fetchProjectDependencies({ nowMs: NOW_MS, projectId: '  ' }),
    ).rejects.toBeInstanceOf(ClientStateError)

    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })

  it('sends exactly one request per authored day plan and none for a disabled hook', async () => {
    const client = testQueryClient()
    vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(dayPlan))

    const { result } = renderHook(
      () => useDayPlan({ nowMs: NOW_MS, availableMinutes: 45 }),
      { wrapper: queryWrapper(client) },
    )

    await waitFor(() => expect(result.current.data?.allocatedMinutes).toBe(30))
    expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1)

    const [url] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit]
    expect(url).toContain('available_minutes=45')
  })
})

describe('planning queries never claim acceptance', () => {
  it('exposes query state only, because a plan read writes nothing', () => {
    const client = testQueryClient()
    const { result } = renderHook(() => useSchedulePlan(null), { wrapper: queryWrapper(client) })

    expect(Object.keys(result.current)).not.toContain('disposition')
    expect(result.current.fetchStatus).toBe('idle')
    expect(result.current.data).toBeUndefined()
  })
})
