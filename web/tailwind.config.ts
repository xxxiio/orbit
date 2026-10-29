import type { Config } from 'tailwindcss'

/**
 * Orbit design tokens live as CSS custom properties in `src/styles/tokens.css`.
 * Tailwind only maps them into utility names so the whole app can be re-themed
 * without touching component classes.
 */
const config: Config = {
  darkMode: ['class', '[data-theme="dark"]'],
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      screens: {
        sm: '560px',
        md: '720px',
        lg: '900px',
        xl: '1200px',
        '2xl': '1440px',
      },
      colors: {
        canvas: 'hsl(var(--canvas) / <alpha-value>)',
        surface: {
          DEFAULT: 'hsl(var(--surface) / <alpha-value>)',
          sunken: 'hsl(var(--surface-sunken) / <alpha-value>)',
          raised: 'hsl(var(--surface-raised) / <alpha-value>)',
          hover: 'hsl(var(--surface-hover) / <alpha-value>)',
        },
        ink: {
          DEFAULT: 'hsl(var(--ink) / <alpha-value>)',
          soft: 'hsl(var(--ink-soft) / <alpha-value>)',
          muted: 'hsl(var(--ink-muted) / <alpha-value>)',
          faint: 'hsl(var(--ink-faint) / <alpha-value>)',
        },
        line: {
          DEFAULT: 'hsl(var(--line) / <alpha-value>)',
          strong: 'hsl(var(--line-strong) / <alpha-value>)',
        },
        accent: {
          DEFAULT: 'hsl(var(--accent) / <alpha-value>)',
          hover: 'hsl(var(--accent-hover) / <alpha-value>)',
          ink: 'hsl(var(--accent-ink) / <alpha-value>)',
          soft: 'hsl(var(--accent-soft) / <alpha-value>)',
          'soft-ink': 'hsl(var(--accent-soft-ink) / <alpha-value>)',
        },
        success: {
          DEFAULT: 'hsl(var(--success) / <alpha-value>)',
          soft: 'hsl(var(--success-soft) / <alpha-value>)',
          ink: 'hsl(var(--success-ink) / <alpha-value>)',
        },
        warning: {
          DEFAULT: 'hsl(var(--warning) / <alpha-value>)',
          soft: 'hsl(var(--warning-soft) / <alpha-value>)',
          ink: 'hsl(var(--warning-ink) / <alpha-value>)',
        },
        danger: {
          DEFAULT: 'hsl(var(--danger) / <alpha-value>)',
          soft: 'hsl(var(--danger-soft) / <alpha-value>)',
          ink: 'hsl(var(--danger-ink) / <alpha-value>)',
        },
        info: {
          DEFAULT: 'hsl(var(--info) / <alpha-value>)',
          soft: 'hsl(var(--info-soft) / <alpha-value>)',
          ink: 'hsl(var(--info-ink) / <alpha-value>)',
        },
      },
      fontFamily: {
        sans: ['"Inter Variable"', 'Inter', 'Arial', 'Helvetica', 'sans-serif'],
        display: ['"Inter Variable"', 'Inter', 'Arial', 'Helvetica', 'sans-serif'],
        mono: ['"JetBrains Mono Variable"', 'ui-monospace', 'SFMono-Regular', 'monospace'],
      },
      fontSize: {
        '2xs': ['0.6875rem', { lineHeight: '1rem' }],
        xs: ['0.75rem', { lineHeight: '1.125rem' }],
        sm: ['0.8125rem', { lineHeight: '1.25rem' }],
        base: ['0.875rem', { lineHeight: '1.375rem' }],
        md: ['0.9375rem', { lineHeight: '1.5rem' }],
        lg: ['1rem', { lineHeight: '1.5rem' }],
        xl: ['1.125rem', { lineHeight: '1.625rem' }],
        '2xl': ['1.5rem', { lineHeight: '1.875rem', letterSpacing: '-0.01em' }],
        '3xl': ['1.5rem', { lineHeight: '1.875rem', letterSpacing: '-0.01em' }],
        display: ['2rem', { lineHeight: '2.375rem', letterSpacing: '-0.02em' }],
      },
      letterSpacing: {
        label: '0',
        wide: '0',
      },
      borderRadius: {
        xs: '0.25rem',
        sm: '0.375rem',
        md: '0.5rem',
        lg: '0.75rem',
        xl: '1rem',
        '2xl': '1.25rem',
      },
      boxShadow: {
        soft: 'var(--shadow-soft)',
        lift: 'var(--shadow-lift)',
        focus: '0 0 0 3px hsl(var(--accent) / 0.28)',
      },
      spacing: {
        sidebar: '16.5rem',
        topbar: '3.5rem',
      },
      maxWidth: {
        prose: '42rem',
        page: '72rem',
      },
      transitionTimingFunction: {
        out: 'cubic-bezier(0.2, 0.8, 0.2, 1)',
        'in-out': 'cubic-bezier(0.65, 0, 0.35, 1)',
      },
      keyframes: {
        'fade-in': {
          from: { opacity: '0' },
          to: { opacity: '1' },
        },
        'fade-out': {
          from: { opacity: '1' },
          to: { opacity: '0' },
        },
        'fade-up': {
          from: { opacity: '0', transform: 'translate3d(0, 10px, 0)' },
          to: { opacity: '1', transform: 'translate3d(0, 0, 0)' },
        },
        'rise-in': {
          from: { opacity: '0', transform: 'translate3d(0, 4px, 0) scale(0.99)' },
          to: { opacity: '1', transform: 'translate3d(0, 0, 0) scale(1)' },
        },
        'rise-out': {
          from: { opacity: '1', transform: 'translate3d(0, 0, 0) scale(1)' },
          to: { opacity: '0', transform: 'translate3d(0, 4px, 0) scale(0.99)' },
        },
        'slide-in-left': {
          from: { transform: 'translate3d(-100%, 0, 0)' },
          to: { transform: 'translate3d(0, 0, 0)' },
        },
        'slide-out-left': {
          from: { transform: 'translate3d(0, 0, 0)' },
          to: { transform: 'translate3d(-100%, 0, 0)' },
        },
        'slide-in-right': {
          from: { transform: 'translate3d(100%, 0, 0)' },
          to: { transform: 'translate3d(0, 0, 0)' },
        },
        'slide-out-right': {
          from: { transform: 'translate3d(0, 0, 0)' },
          to: { transform: 'translate3d(100%, 0, 0)' },
        },
        'dialog-in': {
          from: { opacity: '0', transform: 'translate3d(-50%, -48%, 0) scale(0.985)' },
          to: { opacity: '1', transform: 'translate3d(-50%, -50%, 0) scale(1)' },
        },
        'dialog-out': {
          from: { opacity: '1', transform: 'translate3d(-50%, -50%, 0) scale(1)' },
          to: { opacity: '0', transform: 'translate3d(-50%, -48%, 0) scale(0.985)' },
        },
        shimmer: {
          '100%': { transform: 'translateX(100%)' },
        },
        'spin-slow': {
          to: { transform: 'rotate(360deg)' },
        },
        'dot-pulse': {
          '0%, 100%': { opacity: '0.45' },
          '50%': { opacity: '1' },
        },
      },
      animation: {
        'fade-in': 'fade-in 180ms cubic-bezier(0.2, 0.8, 0.2, 1) both',
        'fade-out': 'fade-out 140ms cubic-bezier(0.65, 0, 0.35, 1) both',
        'fade-up': 'fade-up 460ms cubic-bezier(0.2, 0.8, 0.2, 1) both',
        'rise-in': 'rise-in 140ms cubic-bezier(0.2, 0.8, 0.2, 1) both',
        'rise-out': 'rise-out 120ms cubic-bezier(0.65, 0, 0.35, 1) both',
        'slide-in-left': 'slide-in-left 260ms cubic-bezier(0.2, 0.8, 0.2, 1) both',
        'slide-out-left': 'slide-out-left 180ms cubic-bezier(0.65, 0, 0.35, 1) both',
        'slide-in-right': 'slide-in-right 260ms cubic-bezier(0.2, 0.8, 0.2, 1) both',
        'slide-out-right': 'slide-out-right 180ms cubic-bezier(0.65, 0, 0.35, 1) both',
        'dialog-in': 'dialog-in 180ms cubic-bezier(0.2, 0.8, 0.2, 1) both',
        'dialog-out': 'dialog-out 140ms cubic-bezier(0.65, 0, 0.35, 1) both',
        shimmer: 'shimmer 1.8s infinite',
        'spin-slow': 'spin-slow 80s linear infinite',
        'dot-pulse': 'dot-pulse 6s ease-in-out infinite',
      },
    },
  },
  plugins: [],
}

export default config
