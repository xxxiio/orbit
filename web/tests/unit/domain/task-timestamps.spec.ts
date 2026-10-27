import { describe, expect, it } from 'vitest'
import { formatTaskTimestamp } from '@/domain'

/**
 * The timestamp label is pure, so the tests pin the locale and zone. The same
 * instant must read as "today" or "older" according to the zone the person is
 * in, never according to a fabricated `Date.now()` in the component.
 */
describe('formatTaskTimestamp', () => {
  const options = { locale: 'en-US', timeZone: 'UTC' } as const

  it('returns null for a missing or unusable value instead of inventing a date', () => {
    expect(formatTaskTimestamp(null, options)).toBeNull()
    expect(formatTaskTimestamp(Number.NaN, options)).toBeNull()
    expect(formatTaskTimestamp(Number.POSITIVE_INFINITY, options)).toBeNull()
  })

  it('labels a timestamp from today, with the time and a full value', () => {
    const now = Date.UTC(2026, 2, 10, 9, 0)
    const value = Date.UTC(2026, 2, 10, 8, 30)
    const formatted = formatTaskTimestamp(value, { ...options, now })

    expect(formatted).not.toBeNull()
    expect(formatted?.label).toBe('Today, 8:30 AM')
    expect(formatted?.full).toContain('2026')
    expect(formatted?.full).toMatch(/UTC|Coordinated Universal Time/)
  })

  it('labels an older timestamp with how many days ago it was', () => {
    const now = Date.UTC(2026, 2, 10, 9, 0)
    const value = Date.UTC(2026, 2, 3, 14, 5)
    const formatted = formatTaskTimestamp(value, { ...options, now })

    expect(formatted?.label).toBe('Tue, Mar 3 · 7 days ago')
    expect(formatted?.full).toContain('Mar 3, 2026')
  })

  it('uses the requested zone for the today/older boundary', () => {
    const value = Date.UTC(2026, 2, 3, 23, 30)
    const now = Date.UTC(2026, 2, 4, 0, 30)

    expect(formatTaskTimestamp(value, { ...options, now })?.label).toContain('1 day ago')
    expect(
      formatTaskTimestamp(value, {
        now,
        locale: 'en-US',
        timeZone: 'America/Los_Angeles',
      })?.label,
    ).toContain('Today')
  })
})
