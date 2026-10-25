import type { ReactNode } from 'react'
import { Field } from '@/components/ui/field'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'

export interface LabeledSelectOption {
  value: string
  label: string
}

export interface LabeledSelectProps {
  label: ReactNode
  value: string
  onValueChange: (value: string) => void
  options: LabeledSelectOption[]
  hint?: ReactNode
  error?: ReactNode
  required?: boolean
  disabled?: boolean
  placeholder?: string
  /** Accessible name when the visible label alone is ambiguous in context. */
  ariaLabel?: string
  className?: string
}

/**
 * A labeled select for forms: `Field` owns the label/hint/error wiring, so the
 * trigger gets the right id, `aria-describedby` and `aria-invalid` for free.
 */
export function LabeledSelect({
  label,
  value,
  onValueChange,
  options,
  hint,
  error,
  required,
  disabled,
  placeholder,
  ariaLabel,
  className,
}: LabeledSelectProps) {
  return (
    <Field label={label} hint={hint} error={error} required={required} className={className}>
      <Select value={value} onValueChange={onValueChange} disabled={disabled}>
        <SelectTrigger aria-label={ariaLabel}>
          <SelectValue placeholder={placeholder} />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
  )
}
