import { describe, expect, it } from 'vitest'
import {
  ApiError,
  ClientStateError,
  classifyError,
  errorInfo,
  scrubInternalNames,
  userMessage,
  type ApiErrorKind,
} from '@/api/errors'

/**
 * The user-facing product name is "Workflow". The server still uses the old
 * internal name in a few error strings, so every message that reaches a person
 * is scrubbed. This test is the guard for `userMessage()`, which is the only
 * error copy the UI is allowed to render.
 */
const LEGACY_WORD = /blueprint/i

interface Case {
  error: unknown
  kind: ApiErrorKind
}

const CASES: Case[] = [
  { error: new ApiError(401, 'unauthorized'), kind: 'unauthorized' },
  { error: new ApiError(403, 'device identity mismatch'), kind: 'forbidden' },
  { error: new ApiError(403, 'csrf check failed'), kind: 'forbidden' },
  { error: new ApiError(404, 'task not found'), kind: 'notFound' },
  { error: new ApiError(404, 'Markdown document not found'), kind: 'notFound' },
  { error: new ApiError(409, 'blueprint conflict'), kind: 'conflict' },
  { error: new ApiError(409, 'immutable sequence already used'), kind: 'conflict' },
  { error: new ApiError(429, 'too many attempts'), kind: 'rateLimited' },
  { error: new ApiError(400, 'blueprint operation rejected'), kind: 'validation' },
  { error: new ApiError(400, 'invalid device credentials'), kind: 'validation' },
  { error: new ApiError(413, 'payload too large'), kind: 'validation' },
  { error: new ApiError(500, 'blueprint storage error'), kind: 'server' },
  { error: new ApiError(503, 'blueprint storage unavailable'), kind: 'server' },
  { error: new ApiError(0, 'Could not reach the Orbit server.'), kind: 'offline' },
  { error: new ClientStateError('The workspace is still loading. Try again in a moment.'), kind: 'validation' },
  { error: new Error('unexpected explosion'), kind: 'server' },
]

describe('user-facing error copy', () => {
  it('never shows the legacy internal feature name', () => {
    for (const { error, kind } of CASES) {
      expect(classifyError(error)).toBe(kind)
      const message = userMessage(error)
      expect(message.length).toBeGreaterThan(0)
      expect(message).not.toMatch(LEGACY_WORD)
      expect(message).not.toMatch(/\/v1\//)
      expect(message).not.toMatch(/Expected HTTP/i)
    }
  })

  it('scrubs quoted server text, singular and plural, whatever the casing', () => {
    expect(scrubInternalNames('blueprint conflict')).toBe('workflow conflict')
    expect(scrubInternalNames('Blueprint operation rejected')).toBe('workflow operation rejected')
    expect(scrubInternalNames('blueprints storage unavailable')).toBe('workflows storage unavailable')
    expect(scrubInternalNames('no such word here')).toBe('no such word here')
  })

  it('gives every kind a distinct, actionable sentence', () => {
    const messages = new Set<string>()
    for (const { error } of CASES) {
      const message = userMessage(error)
      messages.add(message)
      expect(message).not.toMatch(/\b[45]\d\d\b/)
      expect(message.trim()).toBe(message)
    }
    // Not one generic fallback for everything: at least the main kinds differ.
    expect(messages.size).toBeGreaterThanOrEqual(7)
  })

  it('exposes a retry window for 429 when the server provides one', () => {
    const withWindow = new ApiError(429, 'too many attempts', {
      error: 'too many attempts',
      retry_after_seconds: 30,
    })
    const info = errorInfo(withWindow)
    expect(info.kind).toBe('rateLimited')
    expect(info.retryAfterSeconds).toBe(30)
    expect(info.userMessage).toContain('30 seconds')

    const withoutWindow = errorInfo(new ApiError(429, 'too many attempts'))
    expect(withoutWindow.retryAfterSeconds).toBeNull()
  })
})
