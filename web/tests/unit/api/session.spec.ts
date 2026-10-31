import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  fetchSession,
  isLocalTrustedSession,
  type WebSession,
} from '@/api/session'
import { jsonResponse } from './test-utils'

const localTrusted: WebSession = {
  mode: 'local-trusted',
  device_id: 'local-trusted',
  display_name: 'Local browser',
  expires_at_ms: null,
}

const authenticated: WebSession = {
  mode: 'authenticated',
  device_id: 'dev-1',
  display_name: null,
  expires_at_ms: 1_700_000_000_000,
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('web session contract', () => {
  it('parses a local-trusted payload and recognises the mode', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(localTrusted)))

    const session = await fetchSession()

    expect(session).toEqual(localTrusted)
    expect(session.mode).toBe('local-trusted')
    expect(session.expires_at_ms).toBeNull()
    expect(isLocalTrustedSession(session)).toBe(true)
  })

  it('does not treat an authenticated payload as local-trusted', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(authenticated)))

    const session = await fetchSession()

    expect(session).toEqual(authenticated)
    expect(isLocalTrustedSession(session)).toBe(false)
    expect(isLocalTrustedSession(null)).toBe(false)
    expect(isLocalTrustedSession(undefined)).toBe(false)
  })
})
