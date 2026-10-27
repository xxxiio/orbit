import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Link, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { OrbitMark, Wordmark } from '@/components/ui/brand'
import { IconButton } from '@/components/ui/icon-button'
import {
  CommandPalette,
  type Command,
  type PaletteSearchGroup,
  type PaletteSearchState,
} from '@/components/ui/command-palette'
import { Drawer } from '@/components/ui/drawer'
import { MenuIcon, SearchIcon } from '@/components/ui/icons'
import { Sidebar, commandShortcutLabel, type SidebarSession } from '@/components/layout/sidebar'
import type { ProjectNavItem } from '@/components/navigation/nav'
import { noteHref } from '@/features/knowledge/knowledge-model'
import {
  classifyError,
  useDebouncedValue,
  useOrbitSearch,
  userMessage,
} from '@/hooks'
import type { SearchResult } from '@/api/adapters'
import { searchGroupLabel } from '@/api/adapters'

export interface AppShellProps {
  /** Live project navigation, grouped by the shell. */
  projects?: ProjectNavItem[]
  /** Commands that are actually implemented; empty is a valid, honest state. */
  commands?: Command[]
  onNewProject?: () => void
  /** Opens the same New task dialog the pages use. */
  onNewTask?: () => void
  /** Session facts rendered in the sidebar footer. */
  session?: SidebarSession
}

export function AppShell({
  projects = [],
  commands = [],
  onNewProject,
  onNewTask,
  session,
}: AppShellProps) {
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [commandOpen, setCommandOpen] = useState(false)
  const [paletteQuery, setPaletteQuery] = useState('')
  const paletteReturnFocus = useRef<HTMLElement | null>(null)
  const location = useLocation()
  const navigate = useNavigate()
  const previousPath = useRef(location.pathname)
  const pendingPrefix = useRef<{ key: string; at: number } | null>(null)

  // A keyboard shortcut is not a Radix Trigger, so the palette has to remember
  // the element to return to itself. Restoring in a timer lets Radix finish its
  // own close pass first.
  const openPalette = () => {
    const active = document.activeElement
    paletteReturnFocus.current = active instanceof HTMLElement ? active : null
    setPaletteQuery('')
    setCommandOpen(true)
  }
  const closePalette = () => {
    setCommandOpen(false)
    setPaletteQuery('')
    window.setTimeout(() => {
      const target = paletteReturnFocus.current
      if (target?.isConnected) target.focus({ preventScroll: true })
      else document.getElementById('main')?.focus({ preventScroll: true })
    }, 0)
  }

  const trimmedQuery = paletteQuery.trim()
  const debouncedQuery = useDebouncedValue(trimmedQuery, 200)
  const searchRead = useOrbitSearch(debouncedQuery, 10)

  const openSearchResult = useCallback(
    (result: SearchResult) => {
      if (result.kind === 'project') {
        navigate(`/projects/${encodeURIComponent(result.id)}`)
        return
      }
      if (result.kind === 'task') {
        const projectId = result.projectId
        const currentProject = /^\/projects\/([^/]+)/.exec(location.pathname)?.[1]
        const sameProject =
          projectId !== null && currentProject !== undefined && decodeURIComponent(currentProject) === projectId
        const viewId = location.pathname.split('/')[3]
        const keepsView = viewId !== 'settings' && viewId !== 'planning'
        if (sameProject && keepsView) {
          const params = new URLSearchParams(location.search)
          params.set('task', result.id)
          navigate(`${location.pathname}?${params.toString()}`)
          return
        }
        const base = projectId ? `/projects/${encodeURIComponent(projectId)}` : '/inbox'
        navigate(`${base}?task=${encodeURIComponent(result.id)}`)
        return
      }
      navigate(result.path ? noteHref(result.path) : '/knowledge')
    },
    [location.pathname, location.search, navigate],
  )

  const searchState = useMemo<PaletteSearchState | undefined>(() => {
    if (trimmedQuery.length === 0) return undefined
    const data = searchRead.data
    const debounceSettled = debouncedQuery === trimmedQuery
    const settledData = data && debounceSettled && data.query === trimmedQuery ? data : null
    const groups: PaletteSearchGroup[] = settledData
      ? settledData.groups.map((group) => ({
          kind: group.kind,
          label: searchGroupLabel(group.kind),
          truncated: group.truncated,
          results: group.results.map((result) => ({
            id: result.id,
            kind: result.kind,
            title: result.title,
            subtitle: result.subtitle,
            projectId: result.projectId,
            path: result.path,
          })),
        }))
      : []
    const pending = !settledData && (searchRead.isFetching || !debounceSettled)
    const error = debounceSettled && searchRead.error
      ? classifyError(searchRead.error) === 'notFound'
        ? 'Search is not available on this server build yet.'
        : userMessage(searchRead.error)
      : null
    return {
      groups,
      total: settledData?.total ?? 0,
      pending,
      error,
      onRetry: () => void searchRead.refetch(),
      onSelect: (result) => openSearchResult(result),
    }
  }, [
    trimmedQuery,
    debouncedQuery,
    searchRead.data,
    searchRead.isFetching,
    searchRead.error,
    searchRead.refetch,
    openSearchResult,
  ])

  useEffect(() => {
    const isEditable = (target: EventTarget | null) =>
      target instanceof HTMLElement &&
      Boolean(target.closest('input, textarea, select, [contenteditable="true"]'))

    const overlayOpen = () => document.querySelector('[role="dialog"]') !== null

    const onKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase()

      if ((event.metaKey || event.ctrlKey) && key === 'k') {
        event.preventDefault()
        if (commandOpen) closePalette()
        else openPalette()
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
        openPalette()
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [commandOpen, navigate, onNewTask])

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
            onOpenCommandPalette={openPalette}
            session={session}
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
                onClick={openPalette}
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
          session={session}
          onOpenCommandPalette={() => {
            setDrawerOpen(false)
            openPalette()
          }}
        />
      </Drawer>

      <CommandPalette
        open={commandOpen}
        onOpenChange={(open) => {
          if (open) openPalette()
          else closePalette()
        }}
        commands={commands}
        query={paletteQuery}
        onQueryChange={setPaletteQuery}
        search={searchState}
        placeholder="Search projects, tasks and knowledge…"
      />
    </div>
  )
}
