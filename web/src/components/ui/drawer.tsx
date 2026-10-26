import { type ReactNode, useEffect, useRef } from 'react'
import * as DialogPrimitive from '@radix-ui/react-dialog'
import { CloseIcon } from './icons'
import { DialogOverlay } from './dialog'
import { cn } from './utils'
import { openDrawer } from './drawer-stack'

export interface DrawerProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: ReactNode
  description?: ReactNode
  side?: 'left' | 'right'
  children: ReactNode
  className?: string
  showClose?: boolean
  /** Accessible name for the close control, when the sheet is not navigation. */
  closeLabel?: string
}

/**
 * A modal sheet built on Radix Dialog, so focus trapping, Escape handling,
 * scroll locking, and focus restoration come from the platform primitive.
 */
export function Drawer({
  open,
  onOpenChange,
  title,
  description,
  side = 'left',
  children,
  className,
  showClose = true,
  closeLabel = 'Close panel',
}: DrawerProps) {
  // Toasts watch this so they never park on top of an open drawer's controls.
  useEffect(() => {
    if (!open) return
    return openDrawer(side)
  }, [open, side])

  // Radix restores focus to the trigger by itself in the common case, but a
  // deep-linked task drawer and a drawer opened from a key press have no
  // registered Trigger; in those paths focus fell to <body>. Capture the
  // pre-open element and restore it explicitly on close (falling back to the
  // main landmark).
  const returnFocusRef = useRef<HTMLElement | null>(
    typeof document !== 'undefined' && document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null,
  )

  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogOverlay />
        <DialogPrimitive.Content
          onOpenAutoFocus={() => {
            if (returnFocusRef.current && returnFocusRef.current !== document.body) return
            const active = document.activeElement
            if (active instanceof HTMLElement && active !== document.body) {
              returnFocusRef.current = active
            }
          }}
          onCloseAutoFocus={(event) => {
            // Focus restoration is deliberate here: Radix can leave focus on
            // <body> when the dialog had no registered Trigger.
            event.preventDefault()
            const target = returnFocusRef.current
            if (target?.isConnected) target.focus({ preventScroll: true })
            else document.getElementById('main')?.focus({ preventScroll: true })
          }}
          className={cn(
            'fixed inset-y-0 z-50 flex h-dvh w-[min(19rem,88vw)] flex-col bg-canvas text-ink shadow-lift focus:outline-none',
            side === 'left'
              ? 'left-0 border-r border-line data-[state=open]:animate-slide-in-left data-[state=closed]:animate-slide-out-left'
              : 'right-0 border-l border-line data-[state=open]:animate-slide-in-right data-[state=closed]:animate-slide-out-right',
            className,
          )}
        >
          <DialogPrimitive.Title className="sr-only">{title}</DialogPrimitive.Title>
          {description ? (
            <DialogPrimitive.Description className="sr-only">{description}</DialogPrimitive.Description>
          ) : null}
          {children}
          {showClose ? (
            <DialogPrimitive.Close asChild>
              <button
                type="button"
                aria-label={closeLabel}
                className="absolute right-3 top-4 inline-flex h-8 w-8 items-center justify-center rounded-md text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
              >
                <CloseIcon size={17} />
              </button>
            </DialogPrimitive.Close>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
