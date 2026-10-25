import { describe, expect, it } from 'vitest'
import { ApiError } from '@/api/client'
import { describeLoginError } from '@/features/auth/login-error'

describe('describeLoginError', () => {
  it('tells a wrong credential apart from a rate limit and a dead server', () => {
    const invalid = describeLoginError(new ApiError(401, 'invalid device credentials'))
    const limited = describeLoginError(new ApiError(429, 'too many attempts'))
    const offline = describeLoginError(new ApiError(0, 'Could not reach the Orbit server.'))

    expect(invalid.tone).toBe('danger')
    expect(invalid.title).toMatch(/did not match/i)
    expect(invalid.message).toMatch(/device ID and token/i)

    expect(limited.tone).toBe('warning')
    expect(limited.title).toMatch(/too many attempts/i)
    expect(limited.message).toMatch(/wait a few minutes/i)
    expect(limited.title).not.toBe(invalid.title)

    expect(offline.title).toMatch(/not responding/i)
    expect(offline.message).toMatch(/orbit-server/i)

    expect(describeLoginError(new Error('boom')).title).toMatch(/sign-in failed/i)
  })
})
