import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import type { Project, Task } from '@/api/adapters'
import { useProjects, useTaskCounts, useWorkspaceSnapshot, userMessage } from '@/hooks'
import { useDocumentTitle } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { IconButton } from '@/components/ui/icon-button'
import { FolderIcon, GridIcon, ListIcon, PlusIcon } from '@/components/ui/icons'
import { PageHeader } from '@/components/ui/page-header'
import { Panel } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { useRecentProjectIds } from '@/lib/recent-projects'
import { projectStatusLabel } from './project-status'
import { isTaskCompleted, isTaskOpen } from './project-progress'
import { useProjectWorkflow } from './use-project-workflow'

type Layout = 'grid' | 'list'

interface ProjectStats {
  total: number
  done: number
  open: number
  blocked: number
  inProgress: number
}

function statsFor(tasks: Task[]): ProjectStats {
  let done = 0
  let open = 0
  let blocked = 0
  let inProgress = 0
  for (const task of tasks) {
    if (isTaskCompleted(task.status)) {
      done += 1
      continue
    }
    if (!isTaskOpen(task.status)) continue
    open += 1
    if (task.status === 'blocked') blocked += 1
    if (task.status === 'in_progress' || task.status === 'review') inProgress += 1
  }
  return { total: tasks.length, done, open, blocked, inProgress }
}

function ProgressBar({ stats }: { stats: ProjectStats }) {
  if (stats.total === 0) {
    return <div aria-hidden="true" className="h-1.5 w-full rounded-full bg-surface-sunken" />
  }
  const segments = [
    { key: 'done', count: stats.done, className: 'bg-success/80' },
    { key: 'in-progress', count: stats.inProgress, className: 'bg-accent/80' },
    { key: 'blocked', count: stats.blocked, className: 'bg-danger/70' },
    {
      key: 'open',
      count: Math.max(0, stats.open - stats.inProgress - stats.blocked),
      className: 'bg-line-strong',
    },
  ].filter((segment) => segment.count > 0)

  return (
    <div aria-hidden="true" className="flex h-1.5 w-full gap-0.5 overflow-hidden rounded-full">
      {segments.map((segment) => (
        <span
          key={segment.key}
          className={segment.className}
          style={{ width: `${(segment.count / stats.total) * 100}%` }}
        />
      ))}
    </div>
  )
}

function WorkflowBadge({ projectId }: { projectId: string }) {
  const { label, loading, error } = useProjectWorkflow(projectId)
  if (loading) return <Skeleton className="h-4 w-20" />
  if (error && !label) return <span className="text-2xs text-ink-faint">Workflow unavailable</span>
  if (!label) return <span className="text-2xs text-ink-faint">No workflow pinned</span>
  return <Badge tone="accent">{label}</Badge>
}

function ProjectCard({ project, stats }: { project: Project; stats: ProjectStats }) {
  return (
    <Link
      to={`/projects/${project.id}`}
      className={cn(
        'group flex flex-col gap-4 rounded-lg border border-line bg-surface p-4 transition-all duration-150',
        'hover:-translate-y-0.5 hover:border-line-strong hover:shadow-soft',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2.5">
          <span className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-md bg-surface-sunken text-ink-muted transition-colors group-hover:bg-accent-soft group-hover:text-accent-soft-ink">
            <FolderIcon size={15} />
          </span>
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold text-ink">{project.name}</h2>
            <p className="mt-0.5 font-mono text-2xs text-ink-faint">{project.id.slice(0, 12)}</p>
          </div>
        </div>
        <Badge tone={project.status === 'active' ? 'success' : 'neutral'} dot>
          {projectStatusLabel(project.status)}
        </Badge>
      </div>

      <WorkflowBadge projectId={project.id} />

      <div>
        <ProgressBar stats={stats} />
        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-ink-muted">
          <span className="tabular-nums">{stats.open} open</span>
          <span className="tabular-nums">{stats.done} done</span>
          {stats.blocked > 0 ? (
            <span className="tabular-nums text-danger-ink">{stats.blocked} blocked</span>
          ) : null}
          <span className="ml-auto tabular-nums">{stats.total} total</span>
        </div>
      </div>
    </Link>
  )
}

function ProjectRow({ project, stats }: { project: Project; stats: ProjectStats }) {
  return (
    <tr className="border-b border-line last:border-b-0 hover:bg-surface-hover/50">
      <td className="px-4 py-3">
        <Link
          to={`/projects/${project.id}`}
          className="flex items-center gap-2.5 rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        >
          <span className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-surface-sunken text-ink-muted">
            <FolderIcon size={15} />
          </span>
          <span className="min-w-0">
            <span className="block truncate text-sm font-medium text-ink">{project.name}</span>
            <span className="block font-mono text-2xs text-ink-faint">{project.id.slice(0, 12)}</span>
          </span>
        </Link>
      </td>
      <td className="px-4 py-3">
        <WorkflowBadge projectId={project.id} />
      </td>
      <td className="px-4 py-3 text-xs tabular-nums text-ink-soft">{stats.open}</td>
      <td className="px-4 py-3 text-xs tabular-nums text-ink-soft">{stats.done}</td>
      <td className="px-4 py-3 text-xs tabular-nums text-ink-soft">{stats.total}</td>
      <td className="px-4 py-3">
        <Badge tone={project.status === 'active' ? 'success' : 'neutral'}>
          {projectStatusLabel(project.status)}
        </Badge>
      </td>
    </tr>
  )
}

function ProjectsSkeleton({ layout }: { layout: Layout }) {
  if (layout === 'list') {
    return (
      <div className="rounded-lg border border-line bg-surface p-4">
        {Array.from({ length: 5 }, (_, index) => (
          <div key={index} className="flex items-center gap-4 border-b border-line py-3 last:border-b-0">
            <Skeleton className="h-7 w-7 rounded-md" />
            <Skeleton className="h-3 w-40" />
            <Skeleton className="h-4 w-20" />
            <Skeleton className="ml-auto h-3 w-12" />
          </div>
        ))}
      </div>
    )
  }
  return (
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
      {Array.from({ length: 6 }, (_, index) => (
        <div key={index} className="rounded-lg border border-line bg-surface p-4">
          <div className="flex items-center gap-2.5">
            <Skeleton className="h-7 w-7 rounded-md" />
            <Skeleton className="h-3 w-32" />
          </div>
          <Skeleton className="mt-3 h-4 w-20" />
          <Skeleton className="mt-4 h-1.5 w-full" />
          <Skeleton className="mt-2 h-2.5 w-24" />
        </div>
      ))}
    </div>
  )
}

export function ProjectsPage() {
  useDocumentTitle('Projects')
  const { openNewProject } = useWorkspaceActions()
  const snapshot = useWorkspaceSnapshot()
  const projects = useProjects()
  const counts = useTaskCounts()
  const recentIds = useRecentProjectIds()
  const [layout, setLayout] = useState<Layout>('grid')

  const statsByProject = useMemo(() => {
    const byId = new Map<string, ProjectStats>()
    for (const project of projects) byId.set(project.id, statsFor([]))
    const buckets = new Map<string, Task[]>()
    for (const task of snapshot.data?.tasks ?? []) {
      if (!task.projectId) continue
      const list = buckets.get(task.projectId) ?? []
      list.push(task)
      buckets.set(task.projectId, list)
    }
    for (const [projectId, tasks] of buckets) byId.set(projectId, statsFor(tasks))
    return byId
  }, [projects, snapshot.data])

  const orderedProjects = useMemo(() => {
    const rank = new Map(recentIds.map((id, index) => [id, index]))
    return [...projects].sort((left, right) => {
      const leftRank = rank.get(left.id)
      const rightRank = rank.get(right.id)
      if (leftRank !== undefined && rightRank !== undefined) return leftRank - rightRank
      if (leftRank !== undefined) return -1
      if (rightRank !== undefined) return 1
      return left.name.localeCompare(right.name)
    })
  }, [projects, recentIds])

  const empty = projects.length === 0
  const totalOpen = counts.open
  const blockedTotal = counts.byStatus.blocked

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Workspace"
        title="Projects"
        description="Projects on this server, ordered by what you opened most recently in this browser."
        actions={
          <Button leadingIcon={<PlusIcon size={16} />} onClick={() => openNewProject()}>
            New project
          </Button>
        }
      />

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-xs text-ink-muted">
          <span className="tabular-nums">{projects.length}</span> project
          {projects.length === 1 ? '' : 's'} · <span className="tabular-nums">{counts.total}</span> tasks ·{' '}
          <span className="tabular-nums">{totalOpen}</span> open
          {blockedTotal > 0 ? (
            <>
              {' · '}
              <span className="tabular-nums text-danger-ink">{blockedTotal} blocked</span>
            </>
          ) : null}
        </p>
        <div
          role="group"
          aria-label="Project layout"
          className="flex items-center gap-1 rounded-md border border-line bg-surface p-0.5"
        >
          <IconButton
            label="Grid layout"
            icon={<GridIcon size={15} />}
            size="sm"
            aria-pressed={layout === 'grid'}
            onClick={() => setLayout('grid')}
            className={layout === 'grid' ? 'bg-accent-soft text-accent-soft-ink' : undefined}
          />
          <IconButton
            label="List layout"
            icon={<ListIcon size={15} />}
            size="sm"
            aria-pressed={layout === 'list'}
            onClick={() => setLayout('list')}
            className={layout === 'list' ? 'bg-accent-soft text-accent-soft-ink' : undefined}
          />
        </div>
      </div>

      {snapshot.isPending && !snapshot.data ? (
        <ProjectsSkeleton layout={layout} />
      ) : snapshot.error && !snapshot.data ? (
        <Panel>
          <ErrorState
            title="Projects could not be loaded"
            description={userMessage(snapshot.error)}
            onRetry={() => void snapshot.refetch()}
            retrying={snapshot.isFetching}
          />
        </Panel>
      ) : empty ? (
        <section className="rounded-lg border border-dashed border-line-strong/70 bg-surface/45">
          <EmptyState
            icon={<FolderIcon size={21} />}
            title="No projects yet"
            description="A project gives related tasks a home and pins a workflow. Start with one, then add tasks to it."
            action={
              <Button leadingIcon={<PlusIcon size={15} />} onClick={() => openNewProject()}>
                Create the first project
              </Button>
            }
            className="min-h-[20rem]"
          />
        </section>
      ) : layout === 'grid' ? (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
          {orderedProjects.map((project) => (
            <ProjectCard
              key={project.id}
              project={project}
              stats={statsByProject.get(project.id) ?? statsFor([])}
            />
          ))}
          <button
            type="button"
            onClick={() => openNewProject()}
            className={cn(
              'group flex min-h-[10.5rem] flex-col items-start justify-center gap-1.5 rounded-lg border border-dashed border-line-strong/70 bg-surface/40 p-4 text-left transition-colors',
              'hover:border-accent/50 hover:bg-accent-soft/20',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
            )}
          >
            <span className="grid h-8 w-8 place-items-center rounded-md border border-line bg-surface text-ink-muted transition-colors group-hover:border-accent/30 group-hover:text-accent-soft-ink">
              <PlusIcon size={15} />
            </span>
            <span className="text-sm font-medium text-ink">New project</span>
            <span className="text-xs leading-5 text-ink-muted">Name it and choose a workflow.</span>
          </button>
        </div>
      ) : (
        <div className="scrollbar-slim overflow-x-auto rounded-lg border border-line bg-surface">
          <table className="w-full text-left text-sm">
            <caption className="sr-only">Projects with workflow, task counts and status</caption>
            <thead>
              <tr className="bg-surface-sunken/95">
                <th scope="col" className="border-b border-line px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Project
                </th>
                <th scope="col" className="border-b border-line px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Workflow
                </th>
                <th scope="col" className="border-b border-line px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Open
                </th>
                <th scope="col" className="border-b border-line px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Done
                </th>
                <th scope="col" className="border-b border-line px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Total
                </th>
                <th scope="col" className="border-b border-line px-4 py-2 text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Status
                </th>
              </tr>
            </thead>
            <tbody>
              {orderedProjects.map((project) => (
                <ProjectRow
                  key={project.id}
                  project={project}
                  stats={statsByProject.get(project.id) ?? statsFor([])}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {snapshot.error && snapshot.data ? (
        <p role="alert" className="text-xs text-danger-ink">
          The project list may be stale: {userMessage(snapshot.error)}.
        </p>
      ) : null}
    </div>
  )
}
