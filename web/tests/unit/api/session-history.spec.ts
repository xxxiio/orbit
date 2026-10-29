import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  hasAuthenticatedBefore,
  rememberAuthenticatedSession,
  resetAuthenticatedHistory,
} from '@/api/session-history'
import {
  handleUnauthorized,
  markSessionActive,
  markSessionSignedOut,
  resetSessionStore,
} from '@/api/session-store'
import { testQueryClient } from './test-utils'

/**
 * The 401 response cannot distinguish "never signed in" from "signed in and
 * lost the session". This marker is that distinction, and it must never hold
 * credential material.
 */
describe('authenticated-session history', () => {
  beforeEach(() => {
    resetSessionStore()
    resetAuthenticatedHistory()
  })

  afterEach(() => {
    resetSessionStore()
    resetAuthenticatedHistory()
  })

  it('starts empty for a first-time visitor', () => {
    expect(hasAuthenticatedBefore()).toBe(false)
  })

  it('remembers only a boolean and no credential material', () => {
    rememberAuthenticatedSession()
    expect(hasAuthenticatedBefore()).toBe(true)
    const raw = window.localStorage.getItem('orbit.auth.seen')
    expect(raw).toBe('1')
    expect(raw).not.toMatch(/token|device|orbit_session/i)
  })

  it('marks a successful session read or sign-in as authenticated', () => {
    markSessionActive()
    expect(hasAuthenticatedBefore()).toBe(true)
  })

  it('forgets the marker on an explicit sign-out', () => {
    markSessionActive()
    markSessionSignedOut('signed-out')
    expect(hasAuthenticatedBefore()).toBe(false)
  })

  it('keeps the marker when a 401 ends the session, so the guard can explain it', () => {
    markSessionActive()
    handleUnauthorized(testQueryClient())
    expect(hasAuthenticatedBefore()).toBe(true)
  })

  it('does not invent a previous session from a 401 on a fresh browser', () => {
    handleUnauthorized(testQueryClient())
    expect(hasAuthenticatedBefore()).toBe(false)
  })
})
