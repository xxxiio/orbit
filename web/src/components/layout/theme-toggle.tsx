import type { ComponentType } from 'react'
import { useTheme, type ThemePreference } from '@/app/theme'
import { cn } from '@/components/ui/utils'
import { MonitorIcon, MoonIcon, SunIcon, type IconProps } from '@/components/ui/icons'
import { Tooltip } from '@/components/ui/tooltip'

const OPTIONS: Array<{
  value: ThemePreference
  label: string
  Icon: ComponentType<IconProps>
}> = [
  { value: 'light', label: 'Light', Icon: SunIcon },
  { value: 'dark', label: 'Dark', Icon: MoonIcon },
  { value: 'system', label: 'System', Icon: MonitorIcon },
]

export function ThemeToggle({ className, compact = false }: { className?: string; compact?: boolean }) {
  const { preference, setPreference } = useTheme()

  return (
    <div className={cn('flex items-center justify-between gap-2', className)}>
      {compact ? null : <span className="text-xs font-medium text-ink-muted">Theme</span>}
      <div
        role="group"
        aria-label="Color theme"
        className={cn(
          'flex items-center gap-0.5 rounded-md border border-line bg-surface-sunken p-0.5',
          compact && 'border-transparent bg-transparent p-0',
        )}
      >
        {OPTIONS.map(({ value, label, Icon }) => {
          const active = preference === value
          return (
            <Tooltip key={value} content={`${label} theme`} delayDuration={500}>
              <button
                type="button"
                aria-label={`${label} theme`}
                aria-pressed={active}
                onClick={() => setPreference(value)}
                className={cn(
                  'flex items-center justify-center rounded-[5px] transition-colors duration-150',
                  compact ? 'h-6 w-6' : 'h-7 w-7',
                  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                  active
                    ? 'bg-accent-soft text-accent-soft-ink shadow-[inset_0_0_0_1px_hsl(var(--accent)/0.22)]'
                    : 'text-ink-muted hover:bg-surface hover:text-ink',
                )}
              >
                <Icon size={compact ? 13 : 14} />
              </button>
            </Tooltip>
          )
        })}
      </div>
    </div>
  )
}
