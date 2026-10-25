import { useRef, useState, type FormEvent } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useDocumentTitle } from '@/app/use-document-title'
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
import { ThemeToggle } from '@/components/layout/theme-toggle'
import { cn } from '@/components/ui/utils'
import { describeLoginError, type LoginErrorView } from './login-error'
import { useSignIn } from './use-auth'

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

function OrbitDiagram() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 600 600"
      className="pointer-events-none absolute -right-40 top-1/2 h-[38rem] w-[38rem] -translate-y-1/2 text-accent opacity-70 xl:-right-24"
    >
      <g className="animate-spin-slow" style={{ transformOrigin: '300px 300px' }}>
        <circle cx="300" cy="300" r="228" stroke="currentColor" strokeOpacity="0.12" fill="none" />
        <circle cx="300" cy="300" r="160" stroke="currentColor" strokeOpacity="0.18" fill="none" />
        <ellipse
          cx="300"
          cy="300"
          rx="252"
          ry="106"
          transform="rotate(-34 300 300)"
          stroke="currentColor"
          strokeOpacity="0.32"
          fill="none"
        />
        <ellipse
          cx="300"
          cy="300"
          rx="252"
          ry="106"
          transform="rotate(56 300 300)"
          stroke="currentColor"
          strokeOpacity="0.2"
          fill="none"
        />
      </g>
      <circle cx="300" cy="300" r="52" fill="hsl(var(--accent) / 0.08)" />
      <circle cx="300" cy="300" r="13" fill="hsl(var(--accent))" />
      <circle cx="300" cy="300" r="26" stroke="hsl(var(--accent) / 0.35)" fill="none" />
    </svg>
  )
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

  const redirectTo = (() => {
    const state = location.state as { from?: unknown } | null
    return typeof state?.from === 'string' && state.from.startsWith('/') ? state.from : '/'
  })()

  /**
   * `ProtectedRoute` marks a 401 redirect in router state. Only that path shows
   * the explanation: a plain visit or an explicit sign-out must stay quiet. The
   * notice is persistent until dismissed, and holds no token or device id (and
   * never touches the URL).
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

  return (
    <div className="flex min-h-dvh bg-canvas text-ink">
      <aside className="auth-grid grain relative hidden w-[46%] shrink-0 flex-col justify-between overflow-hidden border-r border-line p-10 lg:flex xl:w-[44%] xl:p-12">
        <div className="relative z-10 flex items-center gap-3">
          <OrbitMark size={36} animated />
          <div className="flex flex-col">
            <Wordmark className="text-2xl" />
            <span className="mt-1 text-[0.625rem] font-medium uppercase tracking-[0.16em] text-ink-faint">
              Self-hosted workspace
            </span>
          </div>
        </div>

        <div className="relative z-10 max-w-[26rem] animate-fade-up pb-6">
          <p className="section-label mb-4">Orbit</p>
          <p className="font-display text-3xl font-medium leading-[1.16] tracking-[-0.02em] text-ink xl:text-[2.75rem]">
            Projects, agents, and the work between them.
          </p>
          <p className="mt-5 max-w-sm text-sm leading-6 text-ink-soft">
            One calm surface for your team and its automations. Everything stays on the server you
            run.
          </p>
        </div>

        <div className="relative z-10 flex items-center gap-2 text-[0.625rem] font-medium uppercase tracking-[0.14em] text-ink-muted">
          <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-accent" />
          Same origin · server-held session
        </div>

        <OrbitDiagram />
      </aside>

      <main className="relative flex min-h-dvh min-w-0 flex-1 flex-col">
        <header className="flex items-center justify-between gap-4 px-5 py-5 sm:px-8 lg:justify-end lg:px-10">
          <div className="flex items-center gap-2.5 lg:hidden">
            <OrbitMark size={28} />
            <Wordmark />
          </div>
          <ThemeToggle compact />
        </header>

        <div className="flex flex-1 items-center justify-center px-5 pb-10 sm:px-8 lg:px-10">
          <div className="w-full max-w-[24rem] animate-fade-up" style={{ animationDelay: '90ms' }}>
            <header>
              <p className="section-label mb-2">Welcome back</p>
              <h1 className="font-display text-3xl font-medium tracking-tight text-ink">
                Sign in to Orbit
              </h1>
              <p className="mt-3 text-sm leading-6 text-ink-soft">
                Use the device credentials issued by your Orbit server.
              </p>
            </header>

            {showExpiryNotice ? (
              <div
                role="status"
                className="mt-6 flex items-start gap-2.5 rounded-md border border-info/35 bg-info-soft px-3 py-2.5 text-info-ink"
              >
                <AlertCircleIcon size={16} className="mt-0.5 shrink-0" />
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold leading-5">Your session expired</p>
                  <p className="mt-0.5 text-xs leading-5 opacity-90">
                    Sign in again to continue.
                  </p>
                </div>
                <button
                  type="button"
                  aria-label="Dismiss session expired notice"
                  onClick={() => setExpiryDismissed(true)}
                  className="-my-1 -mr-1 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-info-ink/80 transition-colors hover:bg-info-ink/10 hover:text-info-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                >
                  <CloseIcon size={14} />
                </button>
              </div>
            ) : null}

            <form onSubmit={handleSubmit} noValidate className="mt-8 flex flex-col gap-5">
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
                  placeholder="e.g. 7f3c9a2e-browser"
                  autoFocus
                  value={deviceId}
                  onChange={(event) => {
                    setDeviceId(event.target.value)
                    if (fieldErrors.deviceId) setFieldErrors((current) => ({ ...current, deviceId: undefined }))
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
                        token. Device enrollment happens with <span className="font-mono">orbit-server</span>,
                        not in this app.
                      </p>
                      <p className="mt-2 text-xs leading-5 text-ink-soft">
                        The token is shown once. Orbit exchanges it for a session cookie and does not
                        keep it in browser storage or the URL.
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
                    if (fieldErrors.token) setFieldErrors((current) => ({ ...current, token: undefined }))
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

            <div className="mt-7 border-t border-line pt-5">
              <div className="flex items-start gap-2.5">
                <CheckCircleIcon size={15} className="mt-0.5 shrink-0 text-success" />
                <p className="text-xs leading-5 text-ink-muted">
                  Sessions are held in an HttpOnly cookie and expire automatically. Closing the tab
                  does not sign you out.
                </p>
              </div>
            </div>
          </div>
        </div>

        <footer className="px-5 pb-6 text-center text-2xs text-ink-muted sm:px-8 lg:px-10 lg:text-left">
          Orbit is operated by whoever runs this server. Your session stays on it.
        </footer>
      </main>
    </div>
  )
}
