import { describe, expect, it } from 'vitest'
import type { SchedulePlanView } from '@/api/adapters'
import {
  buildScheduleApprovalRequest,
  defaultWindow,
  formatDuration,
  parseLocalDateTime,
  parseWindow,
  readCapacity,
  toDateInput,
  toTimeInput,
} from '@/features/planning/planning-utils'

function plan(overrides: Partial<SchedulePlanView> = {}): SchedulePlanView {
  return {
    taskRevision: 7,
    scheduleRevision: 7,
    startsAtMs: new Date(2026, 9, 25, 9, 0).getTime(),
    endsAtMs: new Date(2026, 9, 25, 12, 0).getTime(),
    availableMinutes: 180,
    allocatedMinutes: 90,
    occupiedMinutes: 0,
    freeWindows: [],
    slots: [
      {
        taskId: 't-1',
        title: 'Draft the review notes',
        projectId: 'p-1',
        startsAtMs: new Date(2026, 9, 25, 9, 0).getTime(),
        endsAtMs: new Date(2026, 9, 25, 10, 0).getTime(),
        minutes: 60,
        ranking: 1,
        explanation: ['Fits an uninterrupted 60-minute opening'],
      },
    ],
    unplaced: [],
    warnings: [],
    ...overrides,
  }
}

describe('planning window parsing', () => {
  it('reads local date and time inputs as one instant', () => {
    const ms = parseLocalDateTime('2026-10-25', '09:30')
    expect(ms).toBe(new Date(2026, 9, 25, 9, 30).getTime())
    expect(toDateInput(ms as number)).toBe('2026-10-25')
    expect(toTimeInput(ms as number)).toBe('09:30')
  })

  it('rejects dates and times the browser would silently normalise', () => {
    expect(parseLocalDateTime('2026-02-30', '09:00')).toBeNull()
    expect(parseLocalDateTime('2026-13-01', '09:00')).toBeNull()
    expect(parseLocalDateTime('2026-10-25', '25:00')).toBeNull()
    expect(parseLocalDateTime('2026-10-25', '9:00')).toBeNull()
    expect(parseLocalDateTime('', '09:00')).toBeNull()
  })

  it('requires a window that ends after it starts', () => {
    const sameDay = parseWindow({ date: '2026-10-25', from: '12:00', to: '09:00' })
    expect(sameDay?.error).toMatch(/end after it starts/)
    const equal = parseWindow({ date: '2026-10-25', from: '09:00', to: '09:00' })
    expect(equal?.error).toMatch(/end after it starts/)
    const valid = parseWindow({ date: '2026-10-25', from: '09:00', to: '12:00' })
    expect(valid?.error).toBeNull()
    expect(valid?.startsAtMs).toBe(new Date(2026, 9, 25, 9, 0).getTime())
  })

  it('defaults to the next 09:00–12:00 window, never one that is already past', () => {
    // Before 09:00 the default is today's window.
    expect(defaultWindow(new Date(2026, 9, 25, 7, 30).getTime())).toEqual({
      date: '2026-10-25',
      from: '09:00',
      to: '12:00',
    })
    // At or after 09:00 the default moves to tomorrow rather than opening on a
    // window that has already started (or ended).
    expect(defaultWindow(new Date(2026, 9, 25, 9, 0).getTime())).toEqual({
      date: '2026-10-26',
      from: '09:00',
      to: '12:00',
    })
    expect(defaultWindow(new Date(2026, 9, 25, 15, 45).getTime())).toEqual({
      date: '2026-10-26',
      from: '09:00',
      to: '12:00',
    })
  })
})

describe('authored capacity', () => {
  it('clamps a URL value to one day and treats junk as unauthored', () => {
    expect(readCapacity('120')).toBe(120)
    expect(readCapacity('1440')).toBe(1440)
    expect(readCapacity('5000')).toBe(1440)
    expect(readCapacity('0')).toBe(0)
    expect(readCapacity('-15')).toBe(0)
    expect(readCapacity('12.5')).toBe(0)
    expect(readCapacity('lots')).toBe(0)
    expect(readCapacity(null)).toBe(0)
  })
})

describe('schedule approval request', () => {
  it('takes the revision and slot times from the plan and mints identities once', () => {
    const identities = ['block-1', 'activity-1', 'block-2', 'activity-2']
    let index = 0
    const request = buildScheduleApprovalRequest(plan(), 'command-1', () => {
      const next = identities[index]
      index += 1
      return next
    })

    expect(request.command_id).toBe('command-1')
    expect(request.expected_revision).toBe(7)
    expect(request.window_start_ms).toBe(plan().startsAtMs)
    expect(request.window_end_ms).toBe(plan().endsAtMs)
    expect(request.items).toEqual([
      {
        task_id: 't-1',
        starts_at_ms: plan().slots[0].startsAtMs,
        ends_at_ms: plan().slots[0].endsAtMs,
        schedule_block_id: 'block-1',
        activity_id: 'activity-1',
      },
    ])
    expect(index).toBe(2)
  })

  it('does not compute times: an empty plan yields an empty item list', () => {
    const request = buildScheduleApprovalRequest(plan({ slots: [] }), 'command-2', () => 'id')
    expect(request.items).toEqual([])
  })
})

describe('duration formatting', () => {
  it('names minutes the way the schedule does', () => {
    expect(formatDuration(45)).toBe('45 min')
    expect(formatDuration(60)).toBe('1 h')
    expect(formatDuration(90)).toBe('1 h 30 min')
    expect(formatDuration(null)).toBe('—')
  })
})
