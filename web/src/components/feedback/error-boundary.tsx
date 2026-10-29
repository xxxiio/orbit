import { Component, type ErrorInfo, type ReactNode } from 'react'
import { OrbitMark } from '@/components/ui/brand'
import { Button } from '@/components/ui/button'

interface Props {
  children: ReactNode
}

interface State {
  error: Error | null
}

/** Last-resort boundary: a render crash should never leave a blank page. */
export class AppErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('Orbit web app crashed', error, info.componentStack)
  }

  render(): ReactNode {
    if (!this.state.error) {
      return this.props.children
    }

    return (
      <div className="grid min-h-dvh place-items-center bg-canvas px-6 py-16 text-ink">
        <main role="alert" className="w-full max-w-md">
          <div className="mb-6 flex items-center gap-3">
            <OrbitMark size={32} />
            <span className="font-sans text-xl font-semibold tracking-tight">Orbit</span>
          </div>
          <p className="section-label mb-2">Unexpected error</p>
          <h1 className="font-sans text-2xl font-medium tracking-tight">This screen stopped working</h1>
          <p className="mt-2 text-sm leading-6 text-ink-soft">
            Something in the interface failed to render. Reloading usually restores the workspace; no
            changes were lost on this screen.
          </p>
          <div className="mt-6 flex items-center gap-2">
            <Button onClick={() => window.location.reload()}>Reload Orbit</Button>
          </div>
          {import.meta.env.DEV ? (
            <pre className="mt-8 overflow-auto rounded-md border border-line bg-surface-sunken p-3 font-mono text-xs text-danger-ink">
              {this.state.error.message}
            </pre>
          ) : null}
        </main>
      </div>
    )
  }
}
