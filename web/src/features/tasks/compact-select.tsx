import { ChevronDownIcon, LoaderIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'

export interface CompactOption {
  value: string
  label: string
}

export interface CompactSelectProps {
  /** Accessible name; the visible label is the selected value. */
  label: string
  value: string
  options: CompactOption[]
  onChange: (value: string) => void
  pending?: boolean
  disabled?: boolean
  className?: string
  /** Shows the control in the danger tone after a rejected change. */
  invalid?: boolean
}

/**
 * A compact, native select for in-row edits. Native controls keep the table
 * honest: keyboard use, type-ahead and screen-reader labelling come from the
 * platform, and the value is always the server's value because it is controlled
 * by the row's task record.
 */
export function CompactSelect({
  label,
  value,
  options,
  onChange,
  pending = false,
  disabled = false,
  className,
  invalid = false,
}: CompactSelectProps) {
  return (
    <span className={cn('relative inline-flex items-center', className)}>
      <select
        aria-label={label}
        aria-busy={pending || undefined}
        value={value}
        disabled={pending || disabled}
        onChange={(event) => onChange(event.target.value)}
        className={cn(
          'h-7 max-w-full cursor-pointer appearance-none truncate rounded-sm border border-line bg-surface pl-2 pr-6 text-xs font-medium text-ink-soft',
          'transition-[border-color,color] duration-150 hover:border-line-strong hover:text-ink',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50',
          'disabled:cursor-progress disabled:opacity-70',
          invalid && 'border-danger/60 text-danger-ink',
        )}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <span
        aria-hidden="true"
        className="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 text-ink-muted"
      >
        {pending ? <LoaderIcon size={12} /> : <ChevronDownIcon size={12} />}
      </span>
    </span>
  )
}
