import { forwardRef, useId, type ReactNode } from 'react'
import * as CheckboxPrimitive from '@radix-ui/react-checkbox'
import { CheckIcon } from './icons'
import { cn } from './utils'

const controlClasses = cn(
  'peer h-4 w-4 shrink-0 rounded-[4px] border border-line-strong bg-surface text-accent-ink shadow-[inset_0_1px_1px_hsl(var(--ink)/0.04)]',
  'transition-colors duration-150 ease-out',
  'hover:border-ink-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
  'data-[state=checked]:border-accent data-[state=checked]:bg-accent data-[state=indeterminate]:border-accent data-[state=indeterminate]:bg-accent',
  'disabled:cursor-not-allowed disabled:opacity-50',
)

export interface CheckboxProps
  extends Omit<React.ComponentPropsWithoutRef<typeof CheckboxPrimitive.Root>, 'children'> {
  /** Optional labelled row. Without it the bare control is rendered. */
  label?: ReactNode
  description?: ReactNode
}

export const Checkbox = forwardRef<React.ElementRef<typeof CheckboxPrimitive.Root>, CheckboxProps>(
  function Checkbox({ className, label, description, id, ...props }, ref) {
    const generatedId = useId()
    const controlId = id ?? generatedId
    const descriptionId = label && description ? `${controlId}-description` : undefined

    const control = (
      <CheckboxPrimitive.Root
        ref={ref}
        id={controlId}
        aria-describedby={descriptionId}
        className={cn(controlClasses, !label && className)}
        {...props}
      >
        <CheckboxPrimitive.Indicator className="flex items-center justify-center">
          <CheckIcon size={11} strokeWidth={2.6} />
        </CheckboxPrimitive.Indicator>
      </CheckboxPrimitive.Root>
    )

    if (!label) {
      return control
    }

    return (
      <label className={cn('flex cursor-pointer items-start gap-2.5', className)}>
        <span className="mt-0.5">{control}</span>
        <span className="min-w-0">
          <span className="block text-sm leading-5 text-ink">{label}</span>
          {description ? (
            <span id={descriptionId} className="mt-0.5 block text-xs leading-5 text-ink-muted">
              {description}
            </span>
          ) : null}
        </span>
      </label>
    )
  },
)
