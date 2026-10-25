import { forwardRef, type InputHTMLAttributes } from 'react'
import { useFieldControl } from './field'
import { cn } from './utils'

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  invalid?: boolean
  inputSize?: 'md' | 'lg'
  leadingIcon?: React.ReactNode
  trailing?: React.ReactNode
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className, invalid, inputSize = 'md', leadingIcon, trailing, id, ...props },
  ref,
) {
  const field = useFieldControl()
  const isInvalid = invalid ?? field?.invalid ?? false
  const describedBy = props['aria-describedby'] ?? field?.describedBy

  return (
    <div className="relative">
      {leadingIcon ? (
        <span className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-ink-muted">
          {leadingIcon}
        </span>
      ) : null}
      <input
        ref={ref}
        id={id ?? field?.controlId}
        aria-describedby={describedBy}
        aria-invalid={isInvalid || undefined}
        className={cn(
          'w-full rounded-md border border-line bg-surface text-ink shadow-[inset_0_1px_1px_hsl(var(--ink)/0.03)]',
          'placeholder:text-ink-faint',
          'transition-[border-color,box-shadow] duration-150 ease-out',
          'hover:border-line-strong focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25',
          'disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-ink-muted',
          inputSize === 'lg' ? 'h-11 px-3 text-base' : 'h-9 px-3 text-base',
          leadingIcon && 'pl-9',
          trailing && 'pr-10',
          isInvalid && 'border-danger focus:border-danger focus:ring-danger/20',
          className,
        )}
        {...props}
      />
      {trailing ? (
        <span className="absolute right-1 top-1/2 flex -translate-y-1/2 items-center">{trailing}</span>
      ) : null}
    </div>
  )
})
