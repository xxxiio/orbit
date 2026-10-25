import type { ReactNode } from 'react'
import { AlertTriangleIcon } from './icons'
import { Button } from './button'
import { cn } from './utils'

export interface ErrorStateProps {
  title?: string
  description?: ReactNode
  onRetry?: () => void
  retrying?: boolean
  retryLabel?: string
  action?: ReactNode
  className?: string
  compact?: boolean
}

export function ErrorState({
  title = 'Something went wrong',
  description = 'The request could not be completed. Try again.',
  onRetry,
  retrying = false,
  retryLabel = 'Try again',
  action,
  className,
  compact = false,
}: ErrorStateProps) {
  return (
    <div
      role="alert"
      className={cn(
        'flex flex-col items-center justify-center text-center',
        compact ? 'px-4 py-8' : 'px-6 py-14',
        className,
      )}
    >
      <div
        aria-hidden="true"
        className="mb-4 flex h-10 w-10 items-center justify-center rounded-xl border border-danger/25 bg-danger-soft text-danger"
      >
        <AlertTriangleIcon size={18} />
      </div>
      <h2 className="font-display text-lg font-medium tracking-tight text-ink">{title}</h2>
      <p className="mt-1.5 max-w-sm text-sm leading-6 text-ink-muted">{description}</p>
      {onRetry || action ? (
        <div className="mt-5 flex items-center gap-2">
          {onRetry ? (
            <Button variant="secondary" size="sm" onClick={onRetry} loading={retrying}>
              {retryLabel}
            </Button>
          ) : null}
          {action}
        </div>
      ) : null}
    </div>
  )
}
