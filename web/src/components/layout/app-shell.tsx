import { useEffect, useRef, useState } from 'react'
import { Link, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { OrbitMark, Wordmark } from '@/components/ui/brand'
import { IconButton } from '@/components/ui/icon-button'
import { CommandPalette, type Command } from '@/components/ui/command-palette'
import { Drawer } from '@/components/ui/drawer'
import { MenuIcon, SearchIcon } from '@/components/ui/icons'
import { Sidebar, commandShortcutLabel } from '@/components/layout/sidebar'
import type { ProjectNavItem } from '@/components/navigation/nav'

export interface AppShellProps {
  /** Live project navigation, grouped by the shell. */
  projects?: ProjectNavItem[]
  /** Commands that are actually implemented; empty is a valid, honest state. */
  commands?: Command[]
  onNewProject?: () => void
  /** Opens the same New task dialog the pages use. */
  onNewTask?: () => void
}

export function AppShell({ projects = [], commands = [], onNewProject, onNewTask }: AppShellProps) {
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [commandOpen, setCommandOpen] = useState(false)
  const location = useLocation()
  const navigate = useNavigate()
  const previousPath = useRef(location.pathname)
  const pendingPrefix = useRef<{ key: string; at: number } | null>(null)

  useEffect(() => {
    const isEditable = (target: EventTarget | null) =>
      target instanceof HTMLElement &&
      Boolean(target.closest('input, textarea, select, [contenteditable="true"]'))

    const overlayOpen = () => document.querySelector('[role="dialog"]') !== null

    const onKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase()

      if ((event.metaKey || event.ctrlKey) && key === 'k') {
        event.preventDefault()
        setCommandOpen((open) => !open)
        return
      }
      if (event.metaKey || event.ctrlKey || event.altKey) return
      if (isEditable(event.target) || overlayOpen()) return

      if (pendingPrefix.current?.key === 'g') {
        const fresh = Date.now() - pendingPrefix.current.at < 1200
        pendingPrefix.current = null
        if (fresh && key === 'h') {
          event.preventDefault()
          navigate('/')
          return
        }
        if (fresh && key === 'i') {
          event.preventDefault()
          navigate('/inbox')
          return
        }
        if (fresh && key === 'p') {
          event.preventDefault()
          navigate('/projects')
          return
        }
      }

      if (key === 'g') {
        pendingPrefix.current = { key: 'g', at: Date.now() }
        return
      }

      if (key === 'c' && onNewTask) {
        const capture = document.querySelector<HTMLInputElement>('[data-quick-capture]')
        if (capture && capture.offsetParent !== null) {
          event.preventDefault()
          capture.focus()
          return
        }
        event.preventDefault()
        onNewTask()
        return
      }

      if (key === '/') {
        const search = document.querySelector<HTMLInputElement>('[data-search-input]')
        if (search && search.offsetParent !== null) {
          event.preventDefault()
          search.focus()
          search.select()
          return
        }
        event.preventDefault()
        setCommandOpen(true)
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [navigate, onNewTask])

  useEffect(() => {
    if (previousPath.current === location.pathname) return
    previousPath.current = location.pathname
    setDrawerOpen(false)
    // SPA navigation should announce the new page, not leave focus on a stale link.
    const main = document.getElementById('main')
    main?.focus({ preventScroll: true })
  }, [location.pathname])

  return (
    <div className="min-h-dvh bg-canvas">
      <a href="#main" className="skip-link">
        Skip to content
      </a>

      <div className="lg:flex">
        <div className="hidden lg:sticky lg:top-0 lg:block lg:h-dvh lg:w-[var(--sidebar-width)] lg:shrink-0 lg:border-r lg:border-line">
          <Sidebar
            projects={projects}
            onNewProject={onNewProject}
            onOpenCommandPalette={() => setCommandOpen(true)}
          />
        </div>

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="sticky top-0 z-30 flex h-14 items-center gap-2 border-b border-line bg-canvas/90 px-3 backdrop-blur lg:hidden">
            <IconButton
              label="Open navigation"
              icon={<MenuIcon size={19} />}
              onClick={() => setDrawerOpen(true)}
            />
            <Link
              to="/"
              className="flex items-center gap-2 rounded-md px-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
              aria-label="Orbit — go to Home"
            >
              <OrbitMark size={24} />
              <Wordmark className="text-lg" />
            </Link>
            <div className="ml-auto flex items-center gap-1">
              <IconButton
                label="Open command palette"
                tooltip={`Search or jump to (${commandShortcutLabel})`}
                icon={<SearchIcon size={18} />}
                onClick={() => setCommandOpen(true)}
              />
            </div>
          </header>

          <main id="main" tabIndex={-1} className="flex-1 focus:outline-none">
            <div
              key={location.pathname}
              className="mx-auto w-full animate-fade-in px-[var(--page-pad-x)] py-[var(--page-pad-y)]"
            >
              <Outlet />
            </div>
          </main>
        </div>
      </div>

      <Drawer
        open={drawerOpen}
        onOpenChange={setDrawerOpen}
        title="Navigation"
        description="Workspace sections and projects"
      >
        <Sidebar
          projects={projects}
          onNewProject={onNewProject}
          onNavigate={() => setDrawerOpen(false)}
          onOpenCommandPalette={() => {
            setDrawerOpen(false)
            setCommandOpen(true)
          }}
        />
      </Drawer>

      <CommandPalette open={commandOpen} onOpenChange={setCommandOpen} commands={commands} />
    </div>
  )
}
