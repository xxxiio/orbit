import { forwardRef, useId, type ReactNode } from 'react'
import * as SwitchPrimitive from '@radix-ui/react-switch'
import { cn } from './utils'

const trackClasses = cn(
  'peer inline-flex h-[1.125rem] w-8 shrink-0 cursor-pointer items-center rounded-full border border-transparent bg-line-strong',
  'transition-colors duration-150 ease-out',
  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
  'data-[state=checked]:bg-accent disabled:cursor-not-allowed disabled:opacity-50',
)

export interface SwitchProps
  extends Omit<React.ComponentPropsWithoutRef<typeof SwitchPrimitive.Root>, 'children'> {
  /** Optional labelled row. Without it the bare control is rendered. */
  label?: ReactNode
  description?: ReactNode
}

export const Switch = forwardRef<React.ElementRef<typeof SwitchPrimitive.Root>, SwitchProps>(
  function Switch({ className, label, description, id, ...props }, ref) {
    const generatedId = useId()
    const controlId = id ?? generatedId
    const descriptionId = label && description ? `${controlId}-description` : undefined

    const control = (
      <SwitchPrimitive.Root
        ref={ref}
        id={controlId}
        aria-describedby={descriptionId}
        className={cn(trackClasses, !label && className)}
        {...props}
      >
        <SwitchPrimitive.Thumb
          className={cn(
            'pointer-events-none block h-3.5 w-3.5 rounded-full bg-white shadow-sm ring-0',
            'transition-transform duration-150 ease-out',
            'translate-x-[2px] data-[state=checked]:translate-x-[16px]',
            'dark:bg-canvas',
          )}
        />
      </SwitchPrimitive.Root>
    )

    if (!label) {
      return control
    }

    return (
      <label
        className={cn(
          'flex cursor-pointer items-start justify-between gap-4 rounded-md',
          className,
        )}
      >
        <span className="min-w-0">
          <span className="block text-sm leading-5 text-ink">{label}</span>
          {description ? (
            <span id={descriptionId} className="mt-0.5 block text-xs leading-5 text-ink-muted">
              {description}
            </span>
          ) : null}
        </span>
        <span className="mt-0.5 shrink-0">{control}</span>
      </label>
    )
  },
)
