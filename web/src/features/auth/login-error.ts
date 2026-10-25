import { ApiError } from '@/api/client'

export type LoginErrorTone = 'danger' | 'warning' | 'info'

export interface LoginErrorView {
  title: string
  message: string
  tone: LoginErrorTone
}

/**
 * Turns transport failures into copy a person can act on.
 * 401 and 429 must stay visibly different: one is a typo, the other is a wait.
 */
export function describeLoginError(error: unknown): LoginErrorView {
  if (error instanceof ApiError) {
    if (error.status === 401) {
      return {
        tone: 'danger',
        title: 'Those credentials did not match',
        message:
          'Check the device ID and token issued by whoever operates this server, then try again.',
      }
    }

    if (error.status === 429) {
      return {
        tone: 'warning',
        title: 'Too many attempts',
        message: 'Wait a few minutes, then try signing in again.',
      }
    }

    if (error.status === 400) {
      return {
        tone: 'danger',
        title: 'The server could not accept that',
        message: error.message || 'Check the device ID and token, then try again.',
      }
    }

    if (error.status === 0) {
      return {
        tone: 'info',
        title: 'The Orbit server is not responding',
        message: 'Check that orbit-server is running, then try again.',
      }
    }

    return {
      tone: 'danger',
      title: 'Sign-in failed',
      message: error.message || 'The server returned an unexpected response.',
    }
  }

  return {
    tone: 'danger',
    title: 'Sign-in failed',
    message: 'Something went wrong before the request completed. Try again.',
  }
}
