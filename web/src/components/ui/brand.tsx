import { cn } from './utils'

export function OrbitMark({
  size = 28,
  animated = false,
  className,
}: {
  size?: number
  animated?: boolean
  className?: string
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      role="img"
      aria-label="Orbit"
      className={cn('shrink-0', className)}
    >
      <circle cx="16" cy="16" r="14.25" fill="hsl(var(--surface))" stroke="hsl(var(--line-strong))" strokeWidth="1" />
      <g
        className={animated ? 'animate-spin-slow' : undefined}
        style={{ transformOrigin: '16px 16px' }}
      >
        <ellipse
          cx="16"
          cy="16"
          rx="10.6"
          ry="4.6"
          transform="rotate(-31 16 16)"
          stroke="hsl(var(--accent))"
          strokeOpacity="0.55"
          strokeWidth="1.35"
        />
        <ellipse
          cx="16"
          cy="16"
          rx="10.6"
          ry="4.6"
          transform="rotate(59 16 16)"
          stroke="hsl(var(--accent))"
          strokeOpacity="0.9"
          strokeWidth="1.35"
        />
      </g>
      <circle cx="16" cy="16" r="3.7" fill="hsl(var(--ink))" />
      <circle cx="16" cy="16" r="1.35" fill="hsl(var(--accent))" />
    </svg>
  )
}

export function Wordmark({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        'font-sans text-[1.375rem] font-semibold leading-none tracking-[-0.025em] text-ink',
        className,
      )}
    >
      Orbit
    </span>
  )
}
