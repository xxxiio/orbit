import { forwardRef, Children, isValidElement, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { Slot, Slottable } from '@radix-ui/react-slot'
import { LoaderIcon } from './icons'
import { cn } from './utils'

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'quiet' | 'danger'
export type ButtonSize = 'sm' | 'md' | 'lg'

const variantClasses: Record<ButtonVariant, string> = {
  primary:
    'bg-accent text-accent-ink shadow-soft hover:bg-accent-hover active:translate-y-px border border-transparent',
  secondary:
    'border border-line-strong bg-surface text-ink hover:border-ink-faint hover:bg-surface-hover active:translate-y-px',
  ghost: 'border border-transparent bg-transparent text-ink-soft hover:bg-surface-hover hover:text-ink',
  quiet: 'border border-transparent bg-transparent text-ink-muted hover:text-ink',
  danger:
    'border border-transparent bg-danger text-white hover:bg-danger/90 active:translate-y-px dark:text-[hsl(356_60%_9%)]',
}

const sizeClasses: Record<ButtonSize, string> = {
  sm: 'h-8 gap-1.5 rounded-sm px-2.5 text-sm',
  md: 'h-9 gap-2 rounded-md px-3.5 text-sm',
  lg: 'h-11 gap-2 rounded-md px-5 text-base',
}

export interface ButtonStyleOptions {
  variant?: ButtonVariant
  size?: ButtonSize
  className?: string
}

export function buttonClasses({
  variant = 'primary',
  size = 'md',
  className,
}: ButtonStyleOptions = {}): string {
  return cn(
    'inline-flex select-none items-center justify-center whitespace-nowrap font-medium transition-[background-color,border-color,color,box-shadow,transform] duration-150 ease-out',
    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
    'disabled:pointer-events-none disabled:opacity-50',
    variantClasses[variant],
    sizeClasses[size],
    className,
  )
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
  loading?: boolean
  asChild?: boolean
  leadingIcon?: ReactNode
  trailingIcon?: ReactNode
}

/**
 * `asChild` renders a slot: the caller's element *is* the button, and Radix's
 * `Slot` accepts exactly one element (or a `Slottable`). Callers legitimately
 * hand over a fragment, an icon plus a label, or a conditional `null`, so the
 * children are normalised into one element instead of failing at render time.
 */
function slotChild(children: ReactNode): ReactNode {
  if (Children.count(children) === 1 && isValidElement(children)) return children
  return <span className="contents">{children}</span>
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    className,
    variant = 'primary',
    size = 'md',
    loading = false,
    asChild = false,
    leadingIcon,
    trailingIcon,
    children,
    disabled,
    type,
    ...props
  },
  ref,
) {
  const Comp = asChild ? Slot : 'button'

  return (
    <Comp
      ref={ref}
      type={asChild ? undefined : (type ?? 'button')}
      className={buttonClasses({ variant, size, className })}
      disabled={asChild ? undefined : disabled || loading}
      aria-busy={loading || undefined}
      {...props}
    >
      {/* `asChild` slots onto the caller's element, so icons belong to that child. */}
      {asChild ? null : loading ? <LoaderIcon size={16} className="animate-spin" /> : leadingIcon}
      <Slottable>{asChild ? slotChild(children) : children}</Slottable>
      {asChild || loading ? null : trailingIcon}
    </Comp>
  )
})
