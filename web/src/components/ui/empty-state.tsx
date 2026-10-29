import type { ReactNode } from 'react'
import { cn } from './utils'

export interface EmptyStateProps {
  title: string
  description?: ReactNode
  icon?: ReactNode
  eyebrow?: string
  action?: ReactNode
  secondaryAction?: ReactNode
  className?: string
  compact?: boolean
  headingLevel?: 2 | 3 | 'p'
}

export function EmptyState({
  title,
  description,
  icon,
  eyebrow,
  action,
  secondaryAction,
  className,
  compact = false,
  headingLevel = 2,
}: EmptyStateProps) {
  const Heading = headingLevel === 'p' ? 'p' : headingLevel === 3 ? 'h3' : 'h2'
  const actions = [action, secondaryAction].filter(
    (node) => node !== null && node !== undefined && node !== false,
  )

  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center text-center',
        compact ? 'px-4 py-8' : 'px-6 py-14',
        className,
      )}
    >
      {icon ? (
        <div
          aria-hidden="true"
          className={cn(
            'mb-4 flex items-center justify-center rounded-xl border border-line bg-surface text-accent',
            compact ? 'h-9 w-9' : 'h-12 w-12',
          )}
        >
          {icon}
        </div>
      ) : null}
      {eyebrow ? <p className="section-label mb-2">{eyebrow}</p> : null}
      <Heading
        className={cn(
          'font-sans font-medium tracking-tight text-ink',
          compact ? 'text-lg' : 'text-xl',
        )}
      >
        {title}
      </Heading>
      {description ? (
        <p className={cn('mt-2 max-w-sm text-pretty text-sm leading-6 text-ink-muted', compact && 'mt-1.5')}>
          {description}
        </p>
      ) : null}
      {actions.length > 0 ? (
        <div className="mt-5 flex flex-wrap items-center justify-center gap-2">
          {/*
           * Each action node is promoted into the flex row as one item, even when
           * the caller hands over a fragment with several children. Composition
           * stays the caller's choice: a multi-child action must not require a
           * wrapper, and it must never reach a slot primitive as several
           * children (that throws "Expected a single React element child").
           */}
          {actions.map((node, index) => (
            <span key={index} className="contents">
              {node}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  )
}
