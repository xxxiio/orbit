import { useRef, useState, type FormEvent } from 'react'
import { Navigate, useLocation, useNavigate } from 'react-router-dom'
import { useDocumentTitle } from '@/hooks'
import { OrbitMark, Wordmark } from '@/components/ui/brand'
import { Button } from '@/components/ui/button'
import { Field } from '@/components/ui/field'
import { IconButton } from '@/components/ui/icon-button'
import { Input } from '@/components/ui/input'
import {
  AlertCircleIcon,
  AlertTriangleIcon,
  CheckCircleIcon,
  CloseIcon,
  EyeIcon,
  EyeOffIcon,
  KeyIcon,
} from '@/components/ui/icons'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ThemeToggle } from '@/components/theme/theme-toggle'
import { SessionGate } from '@/components/feedback/session-gate'
import { cn } from '@/components/ui/utils'
import { describeLoginError, type LoginErrorView } from './login-error'
import { useSession, useSignIn } from './use-auth'

const toneClasses: Record<LoginErrorView['tone'], string> = {
  danger: 'border-danger/35 bg-danger-soft text-danger-ink',
  warning: 'border-warning/40 bg-warning-soft text-warning-ink',
  info: 'border-info/35 bg-info-soft text-info-ink',
}

const toneIcons = {
  danger: AlertCircleIcon,
  warning: AlertTriangleIcon,
  info: AlertCircleIcon,
}

export function LoginPage() {
  useDocumentTitle('Sign in')

  const [deviceId, setDeviceId] = useState('')
  const [token, setToken] = useState('')
  const [showToken, setShowToken] = useState(false)
  const [fieldErrors, setFieldErrors] = useState<{ deviceId?: string; token?: string }>({})
  const [formError, setFormError] = useState<LoginErrorView | null>(null)

  const deviceRef = useRef<HTMLInputElement>(null)
  const tokenRef = useRef<HTMLInputElement>(null)
  const navigate = useNavigate()
  const location = useLocation()
  const signIn = useSignIn()
  const { isLocalTrusted, isLoading: sessionLoading } = useSession()

  const redirectTo = (() => {
    const state = location.state as { from?: unknown } | null
    return typeof state?.from === 'string' && state.from.startsWith('/') ? state.from : '/'
  })()

  /**
   * `ProtectedRoute` marks a real expiry in router state. Only that path shows
   * the explanation: a plain visit or an explicit sign-out stays quiet. The
   * notice holds no token or device id and never touches the URL.
   */
  const sessionExpired = Boolean(
    (location.state as { sessionExpired?: unknown } | null)?.sessionExpired,
  )
  const [expiryDismissed, setExpiryDismissed] = useState(false)
  const showExpiryNotice = sessionExpired && !expiryDismissed

  const submitting = signIn.isPending

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (submitting) return

    const nextErrors: { deviceId?: string; token?: string } = {}
    if (!deviceId.trim()) nextErrors.deviceId = 'Enter the device ID for this browser.'
    if (!token) nextErrors.token = 'Enter the device token.'
    setFieldErrors(nextErrors)
    setFormError(null)

    if (nextErrors.deviceId) {
      deviceRef.current?.focus()
      return
    }
    if (nextErrors.token) {
      tokenRef.current?.focus()
      return
    }

    signIn
      .mutateAsync({ deviceId: deviceId.trim(), token })
      .then(() => {
        navigate(redirectTo, { replace: true })
      })
      .catch((error: unknown) => {
        setFormError(describeLoginError(error))
      })
  }

  const ErrorIcon = formError ? toneIcons[formError.tone] : null

  // A server in local-trusted mode has no device to enroll: entering the
  // intended destination is the whole sign-in flow. The gate stays up while the
  // session read is in flight so the form never flashes first.
  if (sessionLoading) {
    return <SessionGate />
  }
  if (isLocalTrusted) {
    return <Navigate to={redirectTo} replace />
  }

  return (
    <div className="flex min-h-dvh flex-col bg-canvas text-ink">
      <header className="flex items-center justify-between gap-4 px-5 py-4 sm:px-8">
        <div className="flex items-center gap-2.5">
          <OrbitMark size={24} />
          <Wordmark className="text-lg" />
        </div>
        <ThemeToggle compact />
      </header>

      <main className="flex flex-1 items-center justify-center px-4 pb-16 pt-5 sm:px-6">
        <div className="w-full max-w-[25rem]">
          <div className="rounded-lg border border-line bg-surface p-6 shadow-soft sm:p-8">
            <header>
              <p className="section-label mb-1.5">Orbit workspace</p>
              <h1 className="text-2xl font-semibold tracking-tight text-ink">Sign in to Orbit</h1>
              <p className="mt-2 text-sm leading-6 text-ink-soft">
                Use the device credentials issued by your Orbit server.
              </p>
            </header>

            {showExpiryNotice ? (
              <div
                role="status"
                className="mt-5 flex items-start gap-2.5 rounded-md border border-info/35 bg-info-soft px-3 py-2.5 text-info-ink"
              >
                <AlertCircleIcon size={16} className="mt-0.5 shrink-0" />
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold leading-5">Your session ended</p>
                  <p className="mt-0.5 text-xs leading-5 opacity-90">
                    This can happen after the session expires or the device is revoked. Sign in
                    again to continue.
                  </p>
                </div>
                <button
                  type="button"
                  aria-label="Dismiss session ended notice"
                  onClick={() => setExpiryDismissed(true)}
                  className="-my-1 -mr-1 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-info-ink/80 transition-colors hover:bg-info-ink/10 hover:text-info-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                >
                  <CloseIcon size={14} />
                </button>
              </div>
            ) : null}

            <form onSubmit={handleSubmit} noValidate className="mt-6 flex flex-col gap-4">
              <Field
                label="Device ID"
                hint="The identifier for this browser's enrollment."
                error={fieldErrors.deviceId}
              >
                <Input
                  ref={deviceRef}
                  name="device_id"
                  autoComplete="username"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                  inputSize="lg"
                  placeholder="e.g. browser-laptop"
                  autoFocus
                  value={deviceId}
                  onChange={(event) => {
                    setDeviceId(event.target.value)
                    if (fieldErrors.deviceId)
                      setFieldErrors((current) => ({ ...current, deviceId: undefined }))
                    if (formError) setFormError(null)
                  }}
                />
              </Field>

              <Field
                label="Device token"
                hint="Exchanged for an HttpOnly session cookie. Not kept in browser storage."
                error={fieldErrors.token}
                labelExtra={
                  <Popover>
                    <PopoverTrigger asChild>
                      <button
                        type="button"
                        className="-my-0.5 rounded-sm py-0.5 text-xs font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                      >
                        Where do I get these?
                      </button>
                    </PopoverTrigger>
                    <PopoverContent align="end" className="w-80">
                      <div className="flex items-center gap-2 text-ink">
                        <KeyIcon size={16} className="text-accent" />
                        <p className="text-sm font-semibold">Device credentials</p>
                      </div>
                      <p className="mt-2 text-xs leading-5 text-ink-soft">
                        Ask whoever operates this Orbit server to enroll this browser and issue a
                        token. Device enrollment happens with{' '}
                        <span className="font-mono">orbit-server</span>, not in this app.
                      </p>
                      <p className="mt-2 text-xs leading-5 text-ink-soft">
                        The token is shown once. Orbit exchanges it for a session cookie and does
                        not keep it in browser storage or the URL.
                      </p>
                    </PopoverContent>
                  </Popover>
                }
              >
                <Input
                  ref={tokenRef}
                  name="token"
                  type={showToken ? 'text' : 'password'}
                  autoComplete="current-password"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                  inputSize="lg"
                  placeholder="Paste the token issued to this device"
                  value={token}
                  onChange={(event) => {
                    setToken(event.target.value)
                    if (fieldErrors.token)
                      setFieldErrors((current) => ({ ...current, token: undefined }))
                    if (formError) setFormError(null)
                  }}
                  trailing={
                    <IconButton
                      label={showToken ? 'Hide token' : 'Show token'}
                      size="sm"
                      icon={showToken ? <EyeOffIcon size={15} /> : <EyeIcon size={15} />}
                      onClick={() => setShowToken((visible) => !visible)}
                    />
                  }
                />
              </Field>

              {formError && ErrorIcon ? (
                <div
                  role="alert"
                  className={cn(
                    'flex items-start gap-2.5 rounded-md border px-3 py-2.5',
                    toneClasses[formError.tone],
                  )}
                >
                  <ErrorIcon size={16} className="mt-0.5 shrink-0" />
                  <div className="min-w-0">
                    <p className="text-sm font-semibold leading-5">{formError.title}</p>
                    <p className="mt-0.5 text-xs leading-5 opacity-90">{formError.message}</p>
                  </div>
                </div>
              ) : null}

              <Button type="submit" size="lg" loading={submitting} className="mt-1 w-full">
                {submitting ? 'Signing in…' : 'Sign in'}
              </Button>
            </form>

            <div className="mt-6 flex items-start gap-2.5 border-t border-line pt-4">
              <CheckCircleIcon size={15} className="mt-0.5 shrink-0 text-success" />
              <p className="text-xs leading-5 text-ink-muted">
                Sessions are held in an HttpOnly cookie and expire automatically. Closing the tab
                does not sign you out.
              </p>
            </div>
          </div>

          <p className="mt-4 text-center text-xs leading-5 text-ink-muted">
            Orbit is operated by whoever runs this server. Your session stays on it.
          </p>
        </div>
      </main>
    </div>
  )
}
