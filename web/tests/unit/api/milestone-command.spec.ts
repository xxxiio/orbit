import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { queryKeys } from '@/api/queries'
import { useMilestoneCommand } from '@/api/mutations'
import type { MilestoneCommand, MilestoneRecord, MilestoneSnapshot } from '@/api/contracts'
import { jsonResponse, queryWrapper, testQueryClient } from './test-utils'

/**
 * Milestone commands are fenced by the *milestones* snapshot revision, which is
 * a different revision from the workspace snapshot. Reading it once at mount
 * would send a stale fence after any milestone refresh; reading it inside
 * `mutationFn` always sends the freshest one. The command id must be minted per
 * attempt, so a duplicate click cannot reuse the previous key.
 */
describe('useMilestoneCommand revision fence', () => {
  const milestone: MilestoneRecord = {
    id: 'm-1',
    project_id: 'p-1',
    name: 'Beta',
    status: 'planned',
  }

  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  function milestonesAt(revision: number): MilestoneSnapshot {
    return { revision, milestones: [milestone] }
  }

  it('sends the refreshed milestones revision, not the mount-time one', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.milestones(), milestonesAt(3))

    vi.mocked(fetch).mockResolvedValueOnce(
      jsonResponse({ revision: 12, duplicate: false, milestone: { ...milestone, name: 'Beta 2' } }),
    )

    const { result } = renderHook(() => useMilestoneCommand(), { wrapper: queryWrapper(client) })

    // A milestone refresh lands between render and click.
    act(() => {
      client.setQueryData(queryKeys.milestones(), milestonesAt(11))
    })

    act(() => {
      result.current.renameMilestone({ milestoneId: 'm-1', name: 'Beta 2' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('accepted'))

    expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1)
    const [url, init] = vi.mocked(fetch).mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/v1/authoritative/milestone-commands')
    expect(init.method).toBe('POST')
    expect(new Headers(init.headers).get('x-orbit-csrf')).toBe('1')

    const command = JSON.parse(init.body as string) as MilestoneCommand
    expect(command.expected_revision).toBe(11)
    expect(command.expected_revision).not.toBe(3)
    expect(command.action).toEqual({ type: 'rename', milestone_id: 'm-1', name: 'Beta 2' })
    expect(typeof command.command_id).toBe('string')
    expect(command.command_id.length).toBeGreaterThan(0)
  })

  it('mints a fresh command id per attempt and shapes each action on the wire', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.milestones(), milestonesAt(1))

    vi.mocked(fetch)
      .mockResolvedValueOnce(jsonResponse({ revision: 2, duplicate: false, milestone }))
      .mockResolvedValueOnce(jsonResponse({ revision: 3, duplicate: false, milestone: null }))

    const { result } = renderHook(() => useMilestoneCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.changeMilestoneStatus({ milestoneId: 'm-1', status: 'in_progress' })
    })
    await waitFor(() => expect(result.current.disposition).toBe('accepted'))

    act(() => {
      result.current.deleteMilestone({ milestoneId: 'm-1' })
    })
    await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledTimes(2))

    const calls = vi.mocked(fetch).mock.calls.map((call) => {
      const init = call[1] as RequestInit
      return JSON.parse(init.body as string) as MilestoneCommand
    })

    expect(calls[0].action).toEqual({
      type: 'change_status',
      milestone_id: 'm-1',
      status: 'in_progress',
    })
    expect(calls[1].action).toEqual({ type: 'delete', milestone_id: 'm-1' })
    expect(calls[0].command_id).not.toBe(calls[1].command_id)
  })

  it('refuses locally when the milestones snapshot has never loaded', async () => {
    const client = testQueryClient()
    const { result } = renderHook(() => useMilestoneCommand(), { wrapper: queryWrapper(client) })

    act(() => {
      result.current.createMilestone({ projectId: 'p-1', name: 'Gamma' })
    })

    await waitFor(() => expect(result.current.disposition).toBe('rejected'))
    expect(result.current.error?.message).toMatch(/milestone list is still loading/i)
    expect(vi.mocked(fetch)).not.toHaveBeenCalled()
  })
})
