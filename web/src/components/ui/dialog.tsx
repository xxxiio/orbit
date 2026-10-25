import { forwardRef, type ReactNode } from 'react'
import * as DialogPrimitive from '@radix-ui/react-dialog'
import { CloseIcon } from './icons'
import { cn } from './utils'

export const Dialog = DialogPrimitive.Root
export const DialogTrigger = DialogPrimitive.Trigger
export const DialogClose = DialogPrimitive.Close

export function DialogOverlay({
  className,
  ...props
}: React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>) {
  return (
    <DialogPrimitive.Overlay
      className={cn(
        'fixed inset-0 z-50 bg-[hsl(228_24%_6%/0.46)] backdrop-blur-[2px]',
        'data-[state=open]:animate-fade-in data-[state=closed]:animate-fade-out',
        className,
      )}
      {...props}
    />
  )
}

const sizeClasses = {
  sm: 'max-w-sm',
  md: 'max-w-lg',
  lg: 'max-w-2xl',
  xl: 'max-w-4xl',
}

export interface DialogContentProps
  extends Omit<React.ComponentPropsWithoutRef<typeof DialogPrimitive.Content>, 'title'> {
  title: ReactNode
  description?: ReactNode
  size?: keyof typeof sizeClasses
  hideHeader?: boolean
  showClose?: boolean
}

export const DialogContent = forwardRef<
  React.ElementRef<typeof DialogPrimitive.Content>,
  DialogContentProps
>(function DialogContent(
  {
    className,
    children,
    title,
    description,
    size = 'md',
    hideHeader = false,
    showClose = true,
    ...props
  },
  ref,
) {
  return (
    <DialogPrimitive.Portal>
      <DialogOverlay />
      <DialogPrimitive.Content
        ref={ref}
        className={cn(
          'fixed left-1/2 top-1/2 z-50 w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-line bg-surface p-6 text-ink shadow-lift',
          'data-[state=open]:animate-dialog-in data-[state=closed]:animate-dialog-out',
          'focus:outline-none',
          sizeClasses[size],
          className,
        )}
        {...props}
      >
        <DialogPrimitive.Title className={hideHeader ? 'sr-only' : 'pr-8 font-display text-xl font-medium tracking-tight'}>
          {title}
        </DialogPrimitive.Title>
        {description ? (
          <DialogPrimitive.Description
            className={hideHeader ? 'sr-only' : 'mt-1.5 text-sm leading-5 text-ink-muted'}
          >
            {description}
          </DialogPrimitive.Description>
        ) : null}
        {hideHeader ? null : <div className="mt-5">{children}</div>}
        {hideHeader ? children : null}
        {showClose ? (
          <DialogPrimitive.Close asChild>
            <button
              type="button"
              aria-label="Close dialog"
              className="absolute right-3.5 top-3.5 inline-flex h-7 w-7 items-center justify-center rounded-sm text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
            >
              <CloseIcon size={16} />
            </button>
          </DialogPrimitive.Close>
        ) : null}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  )
})
