import type { ReactNode } from 'react'
import { cn } from './utils'

export interface ToolbarProps {
  children?: ReactNode
  start?: ReactNode
  end?: ReactNode
  label: string
  className?: string
}

/** A light-weight action strip. Not sticky by default; pages opt in. */
export function Toolbar({ children, start, end, label, className }: ToolbarProps) {
  return (
    <div
      role="toolbar"
      aria-label={label}
      className={cn(
        'flex min-h-10 flex-wrap items-center gap-2 rounded-md border border-line bg-surface/60 px-2 py-1.5',
        className,
      )}
    >
      {start ? <div className="flex items-center gap-1.5">{start}</div> : null}
      {children ? <div className="flex flex-1 flex-wrap items-center gap-1.5">{children}</div> : null}
      {end ? <div className="ml-auto flex items-center gap-1.5">{end}</div> : null}
    </div>
  )
}

export function ToolbarDivider() {
  return <span aria-hidden="true" className="mx-1 h-4 w-px bg-line" />
}
