import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ReactNode } from 'react'
import { act, renderHook } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import type { Task } from '@/api/adapters'
import type { AuthoritySnapshotResponse, TaskCommand } from '@/api/contracts'
import { queryKeys } from '@/api/queries'
import { useTaskCommand } from '@/api/mutations'
import { useTaskBulkCommand } from '@/features/tasks/use-task-bulk-command'
import { ToastProvider } from '@/components/ui/toast'
import { jsonResponse, snapshotAt, taskRecord, testQueryClient } from './test-utils'

const SELECTED: Task[] = ['t-1', 't-2', 't-3'].map((id) => ({
  id,
  title: `Task ${id}`,
  status: 'inbox',
  projectId: null,
  milestoneId: null,
  estimatedMinutes: null,
  earliestStartAtMs: null,
  deadlineAtMs: null,
  description: '',
  priority: null,
  labels: [],
}))

function useBulk() {
  const command = useTaskCommand()
  const bulk = useTaskBulkCommand({
    command,
    projects: [],
    selectedTasks: SELECTED,
    initialAction: 'status',
    onSucceeded: () => undefined,
  })
  return { command, bulk }
}

/**
 * The bulk loop is where a stale fence is most visible: command #1 advances the
 * authority, so #2 and #3 must fence on the revision the previous acceptance
 * returned. Before the fix every request carried the same pre-loop revision and
 * the server rejected all but the first.
 */
describe('task bulk command revision fence', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('sends strictly non-decreasing revisions and stops showing the old revision after the first acceptance', async () => {
    const client = testQueryClient()
    client.setQueryData(queryKeys.snapshot(), snapshotAt(10, { tasks: SELECTED.map((task) => taskRecord({ id: task.id })) }))

    const seenInCache: number[] = []
    let accepted = 10
    vi.mocked(fetch).mockImplementation(async (_url, init) => {
      const command = JSON.parse((init as RequestInit).body as string) as TaskCommand
      // What the browser still believes is the tip *when it sends this command*.
      seenInCache.push(command.expected_revision)
      accepted += 1
      return jsonResponse({
        revision: accepted,
        duplicate: false,
        task: taskRecord({
          id: command.action.type === 'change_status' ? command.action.task_id : 't-0',
          status: 'planned',
        }),
      })
    })

    function Wrapper({ children }: { children: ReactNode }) {
      return (
        <QueryClientProvider client={client}>
          <ToastProvider>{children}</ToastProvider>
        </QueryClientProvider>
      )
    }

    const { result } = renderHook(() => useBulk(), { wrapper: Wrapper })

    act(() => {
      result.current.bulk.setBulkValue('planned')
    })

    await act(async () => {
      await result.current.bulk.applyBulk()
    })

    const bodies = vi
      .mocked(fetch)
      .mock.calls.map(([, init]) => JSON.parse((init as RequestInit).body as string) as TaskCommand)

    expect(bodies).toHaveLength(3)
    expect(bodies.map((body) => body.expected_revision)).toEqual([10, 11, 12])
    // The cache no longer advertises the pre-bulk revision after the first
    // acceptance, which is exactly why the following two are not stale.
    expect(seenInCache).toEqual([10, 11, 12])
    expect(
      client.getQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot())?.revision,
    ).toBe(13)
    expect(result.current.bulk.bulkError).toBeNull()
  })
})
