import { Link } from 'react-router-dom'
import { useDocumentTitle } from '@/hooks'
import { OrbitMark, Wordmark } from '@/components/ui/brand'
import { buttonClasses } from '@/components/ui/button'
import { ThemeToggle } from '@/components/theme/theme-toggle'
import { ArrowLeftIcon } from '@/components/ui/icons'

export function NotFoundPage() {
  useDocumentTitle('Page not found')

  return (
    <div className="flex min-h-dvh flex-col bg-canvas text-ink">
      <header className="flex items-center justify-between gap-4 px-5 py-5 sm:px-8">
        <Link
          to="/"
          className="flex items-center gap-2.5 rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          aria-label="Orbit — go to Home"
        >
          <OrbitMark size={30} />
          <Wordmark />
        </Link>
        <ThemeToggle compact />
      </header>

      <main className="flex flex-1 items-center px-5 pb-16 sm:px-8 lg:px-12">
        <div className="mx-auto w-full max-w-lg">
          <p className="font-mono text-xs uppercase tracking-[0.18em] text-accent">404</p>
          <h1 className="mt-3 font-display text-3xl font-medium tracking-tight">
            This page isn’t here
          </h1>
          <p className="mt-3 max-w-md text-sm leading-6 text-ink-soft">
            The address may be mistyped, or the page may have moved. Home is always a safe place to
            start again.
          </p>
          <div className="mt-7">
            <Link to="/" className={buttonClasses({ variant: 'secondary', size: 'md' })}>
              <ArrowLeftIcon size={16} />
              Back to Home
            </Link>
          </div>
        </div>
      </main>

      <footer className="px-5 pb-6 text-2xs text-ink-muted sm:px-8">Orbit · self-hosted</footer>
    </div>
  )
}
