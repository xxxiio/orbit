import { describe, expect, it } from 'vitest'
import {
  parseDateTimeLocalInput,
  parseEstimateInput,
  toDateTimeLocalInput,
} from '@/features/tasks/task-drawer/planning-fields'

describe('task drawer planning inputs', () => {
  it('treats an empty estimate as “clear” and refuses zero or fractions', () => {
    expect(parseEstimateInput('')).toEqual({ value: null, error: null })
    expect(parseEstimateInput('  ')).toEqual({ value: null, error: null })
    expect(parseEstimateInput('60')).toEqual({ value: 60, error: null })
    expect(parseEstimateInput('0').value).toBeNull()
    expect(parseEstimateInput('0').error).toMatch(/at least one minute/i)
    expect(parseEstimateInput('1.5').error).toMatch(/whole minutes/i)
    expect(parseEstimateInput('-4').error).toMatch(/whole minutes/i)
  })

  it('round-trips a local date and time without shifting it', () => {
    const ms = new Date(2030, 0, 15, 12, 30, 0, 0).getTime()
    expect(toDateTimeLocalInput(ms)).toBe('2030-01-15T12:30')
    expect(parseDateTimeLocalInput('2030-01-15T12:30')).toEqual({ value: ms, error: null })
    expect(parseDateTimeLocalInput('')).toEqual({ value: null, error: null })
  })

  it('does not accept a normalised or incomplete date', () => {
    expect(parseDateTimeLocalInput('2030-02-31T10:00').error).toMatch(/date and time/i)
    expect(parseDateTimeLocalInput('not-a-date').error).toMatch(/date and time/i)
  })
})
