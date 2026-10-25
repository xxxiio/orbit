import { forwardRef, type TextareaHTMLAttributes } from 'react'
import { useFieldControl } from './field'
import { cn } from './utils'

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  invalid?: boolean
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(function Textarea(
  { className, invalid, id, ...props },
  ref,
) {
  const field = useFieldControl()
  const isInvalid = invalid ?? field?.invalid ?? false
  const describedBy = props['aria-describedby'] ?? field?.describedBy

  return (
    <textarea
      ref={ref}
      id={id ?? field?.controlId}
      aria-describedby={describedBy}
      aria-invalid={isInvalid || undefined}
      className={cn(
        'min-h-[6.5rem] w-full resize-y rounded-md border border-line bg-surface px-3 py-2 text-base text-ink',
        'placeholder:text-ink-faint',
        'transition-[border-color,box-shadow] duration-150 ease-out',
        'hover:border-line-strong focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25',
        'disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-ink-muted',
        isInvalid && 'border-danger focus:border-danger focus:ring-danger/20',
        className,
      )}
      {...props}
    />
  )
})
