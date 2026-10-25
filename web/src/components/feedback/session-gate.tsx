import { Link } from 'react-router-dom'
import { OrbitMark } from '@/components/ui/brand'
import { Button } from '@/components/ui/button'
import { ErrorState } from '@/components/ui/error-state'
import { Skeleton } from '@/components/ui/skeleton'

/** Shown while GET /v1/web/session is in flight. */
export function SessionGate() {
  return (
    <div className="grid min-h-dvh place-items-center bg-canvas px-6">
      <div aria-live="polite" aria-busy="true" className="flex flex-col items-center gap-5">
        <OrbitMark size={44} animated />
        <div className="w-44 space-y-2">
          <Skeleton className="mx-auto h-2.5 w-28" />
          <Skeleton className="mx-auto h-2.5 w-20" />
        </div>
        <p className="text-xs text-ink-muted">Checking your session…</p>
      </div>
    </div>
  )
}

/** Non-401 failures while checking the session: the server exists but the answer did not. */
export function SessionUnavailable({
  onRetry,
  retrying,
}: {
  onRetry: () => void
  retrying?: boolean
}) {
  return (
    <div className="grid min-h-dvh place-items-center bg-canvas px-6">
      <ErrorState
        title="The session could not be checked"
        description="Orbit could not confirm this browser's session. The server may be restarting."
        onRetry={onRetry}
        retrying={retrying}
        retryLabel="Check again"
        action={
          <Button asChild variant="ghost" size="sm">
            <Link to="/login">Go to sign in</Link>
          </Button>
        }
        className="max-w-md"
      />
    </div>
  )
}
