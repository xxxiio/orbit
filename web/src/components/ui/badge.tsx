import type { HTMLAttributes, ReactNode } from 'react'
import { cn } from './utils'

export type BadgeTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info'

const toneClasses: Record<BadgeTone, string> = {
  neutral: 'border-line bg-surface-sunken text-ink-soft',
  accent: 'border-accent/30 bg-accent-soft text-accent-soft-ink',
  success: 'border-success/25 bg-success-soft text-success-ink',
  warning: 'border-warning/30 bg-warning-soft text-warning-ink',
  danger: 'border-danger/25 bg-danger-soft text-danger-ink',
  info: 'border-info/25 bg-info-soft text-info-ink',
}

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone
  dot?: boolean
  children: ReactNode
}

export function Badge({ className, tone = 'neutral', dot = false, children, ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-2xs font-semibold leading-4',
        toneClasses[tone],
        className,
      )}
      {...props}
    >
      {dot ? <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-current opacity-80" /> : null}
      {children}
    </span>
  )
}
