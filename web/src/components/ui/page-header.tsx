import type { ReactNode } from 'react'
import { cn } from './utils'

export interface PageHeaderProps {
  title: ReactNode
  description?: ReactNode
  eyebrow?: ReactNode
  actions?: ReactNode
  className?: string
  /** Hide the bottom rule when the section already provides its own divider. */
  divided?: boolean
}

export function PageHeader({
  title,
  description,
  eyebrow,
  actions,
  className,
  divided = true,
}: PageHeaderProps) {
  return (
    <header
      className={cn(
        'flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between',
        divided && 'border-b border-line pb-6',
        className,
      )}
    >
      <div className="min-w-0">
        {eyebrow ? <p className="section-label mb-2">{eyebrow}</p> : null}
        <h1 className="font-sans text-2xl font-semibold tracking-tight text-ink">
          {title}
        </h1>
        {description ? (
          <p className="mt-2 max-w-prose text-pretty text-sm leading-6 text-ink-soft">{description}</p>
        ) : null}
      </div>
      {actions ? <div className="flex shrink-0 flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  )
}
