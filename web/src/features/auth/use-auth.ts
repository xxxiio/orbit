/**
 * The single session path for the UI.
 *
 * Session reads, sign-in and sign-out are the API lane's hooks; this module only
 * adds router/toast behaviour on top so `ProtectedRoute`, the sidebar and the
 * login form can never disagree about whether this browser is signed in.
 */
import { useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { isApiError } from '@/api/client'
import { useSession, useSignIn, useSignOut, userMessage } from '@/hooks'
import { useToast } from '@/components/ui/toast'

export { useSession, useSignIn }

export interface SignOutAction {
  signOut: () => void
  pending: boolean
}

/**
 * Clears the cookie server-side, drops local session state through the shared
 * hooks, and lands on the sign-in form. A 401 means the cookie was already
 * gone, so that is a successful sign-out from the person's point of view.
 */
export function useSignOutAction(): SignOutAction {
  const mutation = useSignOut()
  const navigate = useNavigate()
  const { toast } = useToast()

  const signOut = useCallback(() => {
    void mutation
      .mutateAsync(undefined)
      .then(() => {
        navigate('/login', { replace: true })
      })
      .catch((error: unknown) => {
        if (isApiError(error) && error.status === 401) {
          navigate('/login', { replace: true })
          return
        }
        toast({
          tone: 'danger',
          title: 'Could not sign out',
          description: userMessage(error),
        })
      })
  }, [mutation, navigate, toast])

  return { signOut, pending: mutation.isPending }
}
