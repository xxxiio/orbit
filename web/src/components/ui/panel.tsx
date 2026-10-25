import type { HTMLAttributes, ReactNode } from 'react'
import { cn } from './utils'

export interface PanelProps extends HTMLAttributes<HTMLElement> {
  as?: 'section' | 'div' | 'article' | 'aside'
}

export function Panel({ className, as: Comp = 'section', ...props }: PanelProps) {
  return (
    <Comp
      className={cn(
        'rounded-lg border border-line bg-surface',
        'shadow-[0_1px_0_hsl(var(--ink)/0.02)]',
        className,
      )}
      {...props}
    />
  )
}

export function PanelHeader({
  className,
  title,
  description,
  eyebrow,
  actions,
}: {
  className?: string
  title: ReactNode
  description?: ReactNode
  eyebrow?: ReactNode
  actions?: ReactNode
}) {
  return (
    <div
      className={cn(
        'flex flex-col gap-3 border-b border-line px-5 py-4 sm:flex-row sm:items-center sm:justify-between',
        className,
      )}
    >
      <div className="min-w-0">
        {eyebrow ? <p className="section-label mb-1">{eyebrow}</p> : null}
        <h2 className="text-md font-semibold tracking-tight text-ink">{title}</h2>
        {description ? <p className="mt-0.5 text-sm text-ink-muted">{description}</p> : null}
      </div>
      {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
    </div>
  )
}

export function PanelBody({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('px-5 py-4', className)} {...props} />
}

export function PanelFooter({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn('flex items-center justify-between gap-3 border-t border-line px-5 py-3', className)}
      {...props}
    />
  )
}

/** Alias kept for callers that use card vocabulary. */
export const Card = Panel
export const CardHeader = PanelHeader
export const CardBody = PanelBody
export const CardFooter = PanelFooter
