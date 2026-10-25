import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { Tooltip } from './tooltip'
import { cn } from './utils'

export type IconButtonVariant = 'ghost' | 'secondary' | 'quiet'
export type IconButtonSize = 'sm' | 'md'

const variantClasses: Record<IconButtonVariant, string> = {
  ghost: 'text-ink-soft hover:bg-surface-hover hover:text-ink',
  secondary:
    'border border-line-strong bg-surface text-ink-soft hover:border-ink-faint hover:bg-surface-hover hover:text-ink',
  quiet: 'text-ink-muted hover:text-ink',
}

const sizeClasses: Record<IconButtonSize, string> = {
  sm: 'h-7 w-7 rounded-sm',
  md: 'h-8 w-8 rounded-md',
}

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  label: string
  icon: ReactNode
  variant?: IconButtonVariant
  size?: IconButtonSize
  tooltip?: string
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { className, label, icon, variant = 'ghost', size = 'md', tooltip, disabled, ...props },
  ref,
) {
  const button = (
    <button
      ref={ref}
      type="button"
      aria-label={label}
      disabled={disabled}
      className={cn(
        'inline-flex shrink-0 items-center justify-center transition-colors duration-150 ease-out',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
        'disabled:pointer-events-none disabled:opacity-45',
        variantClasses[variant],
        sizeClasses[size],
        className,
      )}
      {...props}
    >
      {icon}
    </button>
  )

  if (!tooltip) {
    return button
  }

  if (disabled) {
    return (
      <Tooltip content={tooltip}>
        <span
          tabIndex={0}
          className="inline-flex rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas"
        >
          {button}
        </span>
      </Tooltip>
    )
  }

  return <Tooltip content={tooltip}>{button}</Tooltip>
})
