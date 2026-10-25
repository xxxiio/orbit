import { createContext, useContext, useId, type ReactNode } from 'react'
import { AlertCircleIcon } from './icons'
import { cn } from './utils'

export interface FieldControlState {
  controlId: string
  describedBy?: string
  invalid: boolean
}

const FieldContext = createContext<FieldControlState | null>(null)

export function useFieldControl(): FieldControlState | null {
  return useContext(FieldContext)
}

export interface LabelProps extends React.LabelHTMLAttributes<HTMLLabelElement> {
  required?: boolean
}

export function Label({ className, children, required, ...props }: LabelProps) {
  return (
    <label
      className={cn('text-sm font-medium leading-5 text-ink', className)}
      {...props}
    >
      {children}
      {required ? (
        <>
          <span aria-hidden="true" className="ml-0.5 text-danger">
            *
          </span>
          <span className="sr-only"> (required)</span>
        </>
      ) : null}
    </label>
  )
}

export interface FieldProps {
  id?: string
  label: ReactNode
  hint?: ReactNode
  error?: ReactNode
  required?: boolean
  labelExtra?: ReactNode
  className?: string
  children: ReactNode
}

/**
 * Wires a label, hint, and error message to whichever control is rendered inside.
 * Inputs, textareas and select triggers read this context for `id` and
 * `aria-describedby`, so no caller has to hand-maintain those IDs.
 */
export function Field({
  id,
  label,
  hint,
  error,
  required,
  labelExtra,
  className,
  children,
}: FieldProps) {
  const generatedId = useId()
  const controlId = id ?? generatedId
  const hintId = hint ? `${controlId}-hint` : undefined
  const errorId = error ? `${controlId}-error` : undefined
  const describedBy = [hintId, errorId].filter(Boolean).join(' ') || undefined

  const state: FieldControlState = {
    controlId,
    describedBy,
    invalid: Boolean(error),
  }

  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <div className="flex items-baseline justify-between gap-3">
        <Label htmlFor={controlId} required={required}>
          {label}
        </Label>
        {labelExtra}
      </div>
      <FieldContext.Provider value={state}>{children}</FieldContext.Provider>
      {hint ? (
        <p id={hintId} className="text-xs leading-4 text-ink-muted">
          {hint}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} role="alert" className="flex items-start gap-1.5 text-xs leading-4 text-danger">
          <AlertCircleIcon size={13} className="mt-px shrink-0" />
          <span>{error}</span>
        </p>
      ) : null}
    </div>
  )
}
