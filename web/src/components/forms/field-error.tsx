import type { ReactNode } from 'react'
import { AlertCircleIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'

/**
 * Form primitives: the label/control/hint/error wiring and the two error
 * surfaces (field-scoped and form-scoped) that dialogs and drawers share.
 */

export function InlineFieldError({
  error,
  action,
}: {
  error: ReactNode
  /** Recovery for this exact refusal, rendered beside the one copy of its copy. */
  action?: ReactNode
}) {
  if (!error) return null
  return (
    <div className="mt-1.5 flex flex-wrap items-start gap-x-2 gap-y-1 text-xs leading-5 text-danger-ink">
      <p role="alert" className="flex min-w-0 items-start gap-1.5">
        <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
        <span className="min-w-0">{error}</span>
      </p>
      {action ? <span className="shrink-0">{action}</span> : null}
    </div>
  )
}

/** A rejected form submission: what failed, then the server's own message. */
export function FormErrorBanner({
  title,
  description,
  className,
}: {
  title: string
  description: ReactNode
  className?: string
}) {
  return (
    <div
      role="alert"
      className={cn(
        'flex items-start gap-2 rounded-md border border-danger/35 bg-danger-soft px-3 py-2.5 text-danger-ink',
        className,
      )}
    >
      <AlertCircleIcon size={15} className="mt-0.5 shrink-0" />
      <div className="min-w-0">
        <p className="text-sm font-semibold leading-5">{title}</p>
        <p className="mt-0.5 text-xs leading-5">{description}</p>
      </div>
    </div>
  )
}
