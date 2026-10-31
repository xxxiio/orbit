import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiRequest } from '@/api/client'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('apiRequest', () => {
  it('follows the cookie-session protocol for JSON requests and errors', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        jsonResponse(
          {
            mode: 'authenticated',
            device_id: 'dev-1',
            display_name: null,
            expires_at_ms: 1_700_000_000_000,
          },
          201,
        ),
      )
      .mockResolvedValueOnce(jsonResponse({ error: 'invalid device credentials' }, 401))
      .mockResolvedValueOnce(new Response(null, { status: 204 }))

    vi.stubGlobal('fetch', fetchMock)

    const session = await apiRequest<{ device_id: string }>('/web/session', {
      method: 'POST',
      body: { device_id: 'dev-1', token: 'secret-token' },
    })
    expect(session.device_id).toBe('dev-1')

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe('/v1/web/session')
    expect(init.credentials).toBe('include')
    expect(init.method).toBe('POST')
    expect(init.body).toBe(JSON.stringify({ device_id: 'dev-1', token: 'secret-token' }))

    const headers = new Headers(init.headers)
    expect(headers.get('x-orbit-csrf')).toBe('1')
    expect(headers.get('content-type')).toBe('application/json')
    expect(headers.get('authorization')).toBeNull()

    const getFailure = await apiRequest('/web/session').catch((error: unknown) => error)
    expect(getFailure).toBeInstanceOf(ApiError)
    expect(getFailure).toMatchObject({ status: 401, message: 'invalid device credentials' })

    const getInit = fetchMock.mock.calls[1]?.[1] as RequestInit
    expect(new Headers(getInit.headers).get('x-orbit-csrf')).toBeNull()

    await expect(apiRequest('/web/session', { method: 'DELETE' })).resolves.toBeUndefined()
    const deleteHeaders = new Headers((fetchMock.mock.calls[2]?.[1] as RequestInit).headers)
    expect(deleteHeaders.get('x-orbit-csrf')).toBe('1')
  })
})
