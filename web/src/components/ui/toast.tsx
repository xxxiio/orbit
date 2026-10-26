import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import * as ToastPrimitive from '@radix-ui/react-toast'
import { AlertCircleIcon, CheckCircleIcon, CloseIcon } from './icons'
import { cn } from './utils'
import { useOpenDrawerSide } from './drawer-stack'

export type ToastTone = 'default' | 'success' | 'danger'

export interface ToastOptions {
  title: string
  description?: string
  tone?: ToastTone
  duration?: number
}

interface ToastRecord extends ToastOptions {
  id: number
}

interface ToastContextValue {
  toast: (options: ToastOptions) => void
  dismissAll: () => void
}

const ToastContext = createContext<ToastContextValue | null>(null)

export function useToast(): ToastContextValue {
  const context = useContext(ToastContext)
  if (!context) {
    throw new Error('useToast must be used inside ToastProvider')
  }
  return context
}

/**
 * Radix Toast handles timing, swipe, and portal layering.
 * The viewport is an explicit `aria-live` region so announcements are predictable.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastRecord[]>([])
  const nextId = useRef(1)
  // A right-side drawer owns the right edge, so notifications move left (and
  // vice versa) instead of covering the drawer that just failed.
  const drawerSide = useOpenDrawerSide()

  const toast = useCallback((options: ToastOptions) => {
    const id = nextId.current++
    setToasts((current) => [...current, { ...options, id }])
  }, [])

  const dismissAll = useCallback(() => setToasts([]), [])

  const remove = useCallback((id: number) => {
    setToasts((current) => current.filter((item) => item.id !== id))
  }, [])

  const value = useMemo(() => ({ toast, dismissAll }), [toast, dismissAll])

  return (
    <ToastContext.Provider value={value}>
      <ToastPrimitive.Provider duration={6000} swipeDirection="right">
        {children}
        {toasts.map((item) => {
          const tone = item.tone ?? 'default'
          return (
            <ToastPrimitive.Root
              key={item.id}
              open
              type={tone === 'danger' ? 'foreground' : 'background'}
              duration={item.duration ?? 6000}
              onOpenChange={(open) => {
                if (!open) remove(item.id)
              }}
              className={cn(
                'pointer-events-auto flex w-full items-start gap-3 rounded-lg border bg-surface p-3.5 shadow-lift',
                'data-[state=open]:animate-rise-in data-[state=closed]:animate-fade-out',
                tone === 'danger' ? 'border-danger/40' : 'border-line',
              )}
            >
              <span
                className={cn(
                  'mt-0.5 shrink-0',
                  tone === 'success' && 'text-success',
                  tone === 'danger' && 'text-danger',
                  tone === 'default' && 'text-ink-muted',
                )}
              >
                {tone === 'success' ? (
                  <CheckCircleIcon size={17} />
                ) : tone === 'danger' ? (
                  <AlertCircleIcon size={17} />
                ) : (
                  <span aria-hidden="true" className="block h-1.5 w-1.5 rounded-full bg-accent" />
                )}
              </span>
              <div className="min-w-0 flex-1">
                <ToastPrimitive.Title className="text-sm font-semibold leading-5 text-ink">
                  {item.title}
                </ToastPrimitive.Title>
                {item.description ? (
                  <ToastPrimitive.Description className="mt-0.5 text-sm leading-5 text-ink-muted">
                    {item.description}
                  </ToastPrimitive.Description>
                ) : null}
              </div>
              <ToastPrimitive.Close asChild>
                <button
                  type="button"
                  aria-label="Dismiss notification"
                  className="-mr-1 -mt-1 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                >
                  <CloseIcon size={14} />
                </button>
              </ToastPrimitive.Close>
            </ToastPrimitive.Root>
          )
        })}
        <ToastPrimitive.Viewport
          aria-live="polite"
          aria-label="Notifications"
          className={cn(
            'fixed bottom-4 z-[80] flex w-[min(24rem,calc(100vw-2rem))] flex-col gap-2 outline-none',
            drawerSide === 'right' ? 'left-4' : 'right-4',
          )}
        />
      </ToastPrimitive.Provider>
    </ToastContext.Provider>
  )
}
