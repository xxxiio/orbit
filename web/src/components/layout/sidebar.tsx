import { Link, NavLink } from 'react-router-dom'
import * as ScrollAreaPrimitive from '@radix-ui/react-scroll-area'
import { OrbitMark, Wordmark } from '@/components/ui/brand'
import { IconButton } from '@/components/ui/icon-button'
import { Skeleton } from '@/components/ui/skeleton'
import { Tooltip } from '@/components/ui/tooltip'
import { cn } from '@/components/ui/utils'
import {
  FolderIcon,
  LoaderIcon,
  LogOutIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
} from '@/components/ui/icons'
import { globalNav, type ProjectNavItem } from '@/components/navigation/nav'
import type { WebSessionMode } from '@/api/session'
import { ThemeToggle } from '@/components/theme/theme-toggle'

export const commandShortcutLabel =
  typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘K' : 'Ctrl K'

function ProjectGroup({
  label,
  projects,
  onNavigate,
}: {
  label: string
  projects: ProjectNavItem[]
  onNavigate?: () => void
}) {
  if (projects.length === 0) return null

  return (
    <div className="pb-1.5">
      <p className="px-2 pb-1 text-2xs font-semibold tracking-label text-ink-muted">{label}</p>
      <ul aria-label={label} className="flex flex-col gap-0.5">
        {projects.map((project) => (
          <li key={project.id}>
            <NavLink
              to={`/projects/${project.id}`}
              onClick={onNavigate}
              className="nav-item py-1.5 text-[0.8125rem]"
              title={project.name}
            >
              <span className="grid h-5 w-5 shrink-0 place-items-center rounded-[5px] bg-surface-sunken text-ink-muted">
                <FolderIcon size={13} />
              </span>
              <span className="truncate">{project.name}</span>
            </NavLink>
          </li>
        ))}
      </ul>
    </div>
  )
}

/**
 * Session facts the sidebar renders. Supplied by the app shell so this
 * presentational module never reaches into a feature module for data.
 */
export interface SidebarSession {
  /** Server-selected auth mode; drives whether the footer is a session or a policy note. */
  mode?: WebSessionMode
  deviceId?: string
  pending: boolean
  error: boolean
  signingOut: boolean
  /** Sign-out only exists for cookie sessions; local-trusted has no session to end. */
  canSignOut: boolean
  onSignOut: () => void
}

export interface SidebarProps {
  /** Empty until a later lane wires project data. */
  projects?: ProjectNavItem[]
  onOpenCommandPalette: () => void
  /** Providing this enables New project; without it the action stays disabled. */
  onNewProject?: () => void
  onNavigate?: () => void
  session?: SidebarSession
  className?: string
}

export function Sidebar({
  projects = [],
  onOpenCommandPalette,
  onNewProject,
  onNavigate,
  session,
  className,
}: SidebarProps) {
  const sessionPending = session?.pending ?? false
  const sessionError = session?.error ?? false
  const signingOut = session?.signingOut ?? false
  const localTrusted = session?.mode === 'local-trusted'
  const canSignOut = !localTrusted && (session?.canSignOut ?? true)

  const pinned = projects.filter((project) => project.pinned)
  const recent = projects.filter((project) => !project.pinned && project.recent).slice(0, 3)
  const others = projects.filter((project) => !project.pinned && !recent.includes(project))

  return (
    <aside
      aria-label="Workspace"
      className={cn('flex h-full min-h-0 flex-col bg-canvas', className)}
    >
      <div className="flex items-center justify-between gap-2 px-4 pb-4 pt-5">
        <Link
          to="/"
          onClick={onNavigate}
          className="group flex items-center gap-2.5 rounded-md pr-8 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        >
          <OrbitMark size={30} className="transition-transform duration-500 ease-out group-hover:rotate-12" />
          <span className="flex flex-col">
            <Wordmark />
            <span className="mt-0.5 text-[0.6875rem] font-medium text-ink-muted">
              Self-hosted
            </span>
          </span>
        </Link>
      </div>

      <div className="px-3 pb-4">
        <button
          type="button"
          onClick={onOpenCommandPalette}
          className={cn(
            'flex h-8 w-full items-center gap-2 rounded-md border border-line bg-surface px-2.5 text-left text-sm text-ink-muted',
            'transition-[border-color,color] duration-150 hover:border-line-strong hover:text-ink',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
          )}
        >
          <SearchIcon size={15} className="shrink-0" />
          <span className="min-w-0 flex-1 truncate">Search or jump to…</span>
          <kbd className="command-key">{commandShortcutLabel}</kbd>
        </button>
      </div>

      <ScrollAreaPrimitive.Root className="min-h-0 flex-1">
        <ScrollAreaPrimitive.Viewport className="h-full">
          <nav aria-label="Primary" className="px-3">
            <ul className="flex flex-col gap-0.5">
              {globalNav.map((item, index) => (
                <li
                  key={item.to}
                  className="animate-fade-up"
                  style={{ animationDelay: `${80 + index * 30}ms` }}
                >
                  <NavLink
                    to={item.to}
                    end={item.end}
                    onClick={onNavigate}
                    className="nav-item"
                  >
                    <item.icon size={17} className="shrink-0 opacity-80" />
                    <span>{item.label}</span>
                  </NavLink>
                </li>
              ))}
            </ul>
          </nav>

          <section aria-labelledby="sidebar-projects-heading" className="mt-7 px-3">
            <div className="flex items-center justify-between px-2 pb-1.5">
              <h2 id="sidebar-projects-heading" className="section-label">
                Projects
              </h2>
            </div>

            {projects.length === 0 ? (
              <div className="px-2 py-1.5">
                <p className="text-xs font-medium text-ink-soft">No projects yet</p>
                <p className="mt-0.5 text-xs leading-5 text-ink-muted">
                  Create one to group its tasks and pin a workflow.
                </p>
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                {pinned.length > 0 ? (
                  <ProjectGroup label="Pinned" projects={pinned} onNavigate={onNavigate} />
                ) : null}
                {recent.length > 0 ? (
                  <ProjectGroup label="Recent" projects={recent} onNavigate={onNavigate} />
                ) : null}
                <ProjectGroup
                  label={recent.length > 0 ? 'Other projects' : 'All projects'}
                  projects={others}
                  onNavigate={onNavigate}
                />
              </div>
            )}

            {onNewProject ? (
              <button
                type="button"
                onClick={onNewProject}
                className="nav-item mt-1 w-full border border-line text-ink-soft"
              >
                <PlusIcon size={16} className="shrink-0" />
                <span>New project</span>
              </button>
            ) : (
              <>
                <Tooltip
                  content="New project is not connected to the server yet."
                  side="right"
                  delayDuration={200}
                >
                  <span
                    tabIndex={0}
                    aria-label="New project — not available yet"
                    className="mt-1 block rounded-md"
                  >
                    <button
                      type="button"
                      disabled
                      aria-describedby="new-project-pending"
                      className="nav-item pointer-events-none w-full border border-dashed border-line text-ink-muted opacity-60"
                    >
                      <PlusIcon size={16} className="shrink-0" />
                      <span>New project</span>
                    </button>
                  </span>
                </Tooltip>
                <p id="new-project-pending" className="sr-only">
                  Project creation is not connected to the server yet.
                </p>
              </>
            )}
          </section>
          <div className="h-6" />
        </ScrollAreaPrimitive.Viewport>
        <ScrollAreaPrimitive.Scrollbar
          orientation="vertical"
          className="flex w-1.5 touch-none select-none p-0.5"
        >
          <ScrollAreaPrimitive.Thumb className="flex-1 rounded-full bg-line-strong/70" />
        </ScrollAreaPrimitive.Scrollbar>
        <ScrollAreaPrimitive.Corner />
      </ScrollAreaPrimitive.Root>

      <div className="border-t border-line px-3 py-3">
        <NavLink
          to="/settings/backup"
          onClick={onNavigate}
          className="nav-item mb-1 py-1.5 text-[0.8125rem]"
        >
          <SettingsIcon size={16} className="shrink-0 opacity-80" />
          <span>Backup &amp; Restore</span>
        </NavLink>
        <ThemeToggle className="px-2 pb-2" />
        {localTrusted ? (
          // A policy note, not a device identity: there is no cookie, no device
          // to name, and nothing to sign out of. It reserves the height of the
          // device block (two leading-4 lines plus py-1.5) so the footer and the
          // scroll area do not shift when the server changes auth mode.
          <p className="mt-1 flex min-h-11 items-center px-1.5 text-xs leading-4 text-ink-muted">
            Local trusted mode
          </p>
        ) : (
          <div className="mt-1 flex items-center gap-2 rounded-md px-1.5 py-1.5">
            <div
              aria-hidden="true"
              className="grid h-7 w-7 shrink-0 place-items-center rounded-full border border-line bg-surface-sunken text-[0.625rem] font-semibold text-ink-soft"
            >
              {session?.deviceId ? session.deviceId.slice(0, 2) : '··'}
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-xs font-medium leading-4 text-ink">This device</p>
              {sessionPending ? (
                <Skeleton className="mt-1 h-2.5 w-24" />
              ) : sessionError ? (
                <p className="truncate text-xs text-ink-muted">Session unavailable</p>
              ) : (
                <p
                  className="truncate font-mono text-[0.625rem] leading-4 text-ink-muted"
                  title={session?.deviceId}
                >
                  {session?.deviceId}
                </p>
              )}
            </div>
            {canSignOut ? (
              <Tooltip content="Sign out" side="top">
                <IconButton
                  label="Sign out"
                  icon={signingOut ? <LoaderIcon size={16} className="animate-spin" /> : <LogOutIcon size={16} />}
                  onClick={() => session?.onSignOut()}
                  disabled={!session || signingOut}
                  size="sm"
                />
              </Tooltip>
            ) : null}
          </div>
        )}
      </div>
    </aside>
  )
}
