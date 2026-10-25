import type { ReactNode } from 'react'
import { LoaderIcon } from '@/components/ui/icons'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { cn } from '@/components/ui/utils'

/** One property row: its label, an optional in-flight note, then the control. */
export function PropertyField({
  label,
  pending,
  hint,
  children,
}: {
  label: string
  pending?: boolean
  hint?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="flex items-center gap-1.5 text-xs font-medium text-ink-muted">
        {label}
        {pending ? <LoaderIcon size={11} className="animate-spin text-accent" /> : null}
      </span>
      {children}
      {hint}
    </div>
  )
}

export interface PropertySelectProps {
  label: string
  value: string
  onValueChange: (value: string) => void
  options: Array<{ value: string; label: string }>
  /** Accessible name for the trigger, e.g. "Task status". */
  ariaLabel: string
  pending?: boolean
  disabled?: boolean
  placeholder?: string
  hint?: ReactNode
  triggerClassName?: string
}

/**
 * The drawer's property control. Same markup a task row uses for status,
 * priority, project or milestone, so any resource drawer can reuse it.
 */
export function PropertySelect({
  label,
  value,
  onValueChange,
  options,
  ariaLabel,
  pending,
  disabled,
  placeholder,
  hint,
  triggerClassName,
}: PropertySelectProps) {
  return (
    <PropertyField label={label} pending={pending} hint={hint}>
      <Select value={value} onValueChange={onValueChange} disabled={disabled}>
        <SelectTrigger className={cn('h-9 text-sm', triggerClassName)} aria-label={ariaLabel}>
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
    </PropertyField>
  )
}
