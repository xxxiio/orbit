import { useEffect } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import {
  useProject,
  useProjectTasks,
  useProjects,
  useWorkspaceSnapshot,
  userMessage,
} from '@/hooks'
import { humanizeIdentifier } from '@/lib/humanize'
import { useDocumentTitle } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import {
  CheckIcon,
  ChevronDownIcon,
  FolderIcon,
  LayersIcon,
  PlusIcon,
} from '@/components/ui/icons'
import { Panel } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { isTerminalStatus } from '@/features/tasks'
import { ProjectOverview } from './project-overview'
import { projectStatusLabel } from './project-status'
import { ProjectTasks } from './project-tasks'
import { recordProjectOpen } from '@/lib/recent-projects'
import { useProjectWorkflow } from './use-project-workflow'

const FALLBACK_VIEWS = [
  {
    id: 'overview',
    title: 'Overview',
    description: 'Project summary and next actions.',
  },
  {
    id: 'tasks',
    title: 'Tasks',
    description: 'Every task in this project.',
  },
]

function ProjectSwitcher({ currentId }: { currentId: string }) {
  const projects = useProjects()
  const navigate = useNavigate()
  const current = projects.find((project) => project.id === currentId)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="secondary"
          size="sm"
          leadingIcon={<FolderIcon size={15} />}
          trailingIcon={<ChevronDownIcon size={14} />}
          className="max-w-[14rem]"
        >
          <span className="truncate">{current?.name ?? 'Switch project'}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-80 w-64 overflow-y-auto">
        <DropdownMenuLabel>Switch project</DropdownMenuLabel>
        {projects.map((project) => (
          <DropdownMenuItem
            key={project.id}
            onSelect={() => navigate(`/projects/${project.id}`)}
            className="gap-2"
          >
            <span className="min-w-0 flex-1 truncate">{project.name}</span>
            {project.id === currentId ? <CheckIcon size={13} className="shrink-0 text-accent" /> : null}
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => navigate('/projects')}>All projects</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function WorkspaceSkeleton() {
  return (
    <div className="flex flex-col gap-6" aria-busy="true" aria-label="Loading project">
      <div className="border-b border-line pb-4">
        <Skeleton className="h-2.5 w-16" />
        <Skeleton className="mt-3 h-8 w-64" />
        <Skeleton className="mt-3 h-3 w-48" />
        <div className="mt-5 flex gap-5">
          <Skeleton className="h-4 w-20" />
          <Skeleton className="h-4 w-16" />
          <Skeleton className="h-4 w-16" />
        </div>
      </div>
      <div className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_22rem]">
        <div className="flex flex-col gap-5">
          <Skeleton className="h-56 rounded-lg" />
          <Skeleton className="h-64 rounded-lg" />
        </div>
        <div className="flex flex-col gap-5">
          <Skeleton className="h-40 rounded-lg" />
          <Skeleton className="h-48 rounded-lg" />
        </div>
      </div>
    </div>
  )
}

function UnavailableView({
  name,
  projectId,
  description,
}: {
  name: string
  projectId: string
  description?: string
}) {
  return (
    <Panel>
      <EmptyState
        icon={<LayersIcon size={21} />}
        title="This view is not available yet"
        description={
          <>
            <span className="font-medium text-ink-soft">{name}</span> is declared by this project&apos;s
            workflow, but the web app does not render it yet.{' '}
            {description ? <span className="text-ink-muted">{description} </span> : null}
            The work itself is unchanged — use Overview or Tasks for now.
          </>
        }
        action={
          <Button asChild variant="secondary" size="sm">
            <Link to={`/projects/${projectId}/tasks`}>Open Tasks</Link>
          </Button>
        }
        className="min-h-[22rem]"
      />
    </Panel>
  )
}

export function ProjectWorkspacePage() {
  const { projectId, viewId } = useParams<{ projectId: string; viewId?: string }>()
  const snapshot = useWorkspaceSnapshot()
  const project = useProject(projectId)
  const tasks = useProjectTasks(projectId)
  const projects = useProjects()
  const workflow = useProjectWorkflow(projectId)
  const { openNewTask } = useWorkspaceActions()

  useDocumentTitle(project ? project.name : 'Project')

  useEffect(() => {
    if (projectId) recordProjectOpen(projectId)
  }, [projectId])

  const views = workflow.workflow?.views.length ? workflow.workflow.views : FALLBACK_VIEWS
  const currentId = viewId ?? views[0]?.id ?? 'overview'
  const currentView = views.find((view) => view.id === currentId)
  const openCount = tasks.filter((task) => !isTerminalStatus(task.status)).length

  if ((snapshot.isPending && !snapshot.data) || (projectId && !project && snapshot.isFetching)) {
    return <WorkspaceSkeleton />
  }

  if (snapshot.error && !snapshot.data) {
    return (
      <Panel>
        <ErrorState
          title="The project could not be loaded"
          description={userMessage(snapshot.error)}
          onRetry={() => void snapshot.refetch()}
          retrying={snapshot.isFetching}
        />
      </Panel>
    )
  }

  if (!project || !projectId) {
    return (
      <Panel>
        <EmptyState
          icon={<FolderIcon size={21} />}
          title="Project not found"
          description="This project is not on the server. It may have been removed from another device."
          action={
            <Button asChild variant="secondary" size="sm">
              <Link to="/projects">Back to projects</Link>
            </Button>
          }
          className="min-h-[20rem]"
        />
      </Panel>
    )
  }

  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-col gap-4 border-b border-line pb-1">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <p className="section-label">Project</p>
            <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1.5">
              <h1 className="font-display text-3xl font-medium tracking-tight text-ink">{project.name}</h1>
              <Badge tone={project.status === 'active' ? 'success' : 'neutral'} dot>
                {projectStatusLabel(project.status)}
              </Badge>
              {workflow.label ? (
                <Badge tone="accent">{workflow.label}</Badge>
              ) : workflow.loading ? (
                <Skeleton className="h-4 w-20" />
              ) : workflow.binding ? (
                <Badge tone="neutral">Workflow unavailable</Badge>
              ) : (
                <Badge tone="neutral">No workflow pinned</Badge>
              )}
            </div>
            <p className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-ink-muted">
              <span className="tabular-nums">{tasks.length}</span> task{tasks.length === 1 ? '' : 's'}
              <span aria-hidden="true">·</span>
              <span className="tabular-nums">{openCount}</span> open
              <span aria-hidden="true">·</span>
              <span className="font-mono text-2xs">{project.id}</span>
            </p>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            <ProjectSwitcher currentId={project.id} />
            <Button
              size="sm"
              leadingIcon={<PlusIcon size={15} />}
              onClick={() => openNewTask(project.id)}
            >
              New task
            </Button>
          </div>
        </div>

        <nav aria-label="Project views" className="-mb-px flex gap-5 overflow-x-auto">
          {views.map((view) => {
            const active = view.id === currentId
            return (
              <Link
                key={view.id}
                to={`/projects/${project.id}/${view.id}`}
                aria-current={active ? 'page' : undefined}
                title={view.description}
                className={cn(
                  'relative -mb-px inline-flex shrink-0 items-center gap-1.5 border-b-2 px-0.5 pb-2.5 pt-1 text-sm font-medium transition-colors',
                  active
                    ? 'border-accent text-ink'
                    : 'border-transparent text-ink-muted hover:text-ink',
                  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
                )}
              >
                {view.title}
                {view.id === 'tasks' ? (
                  <span className="rounded-full bg-surface-sunken px-1.5 py-0.5 text-2xs tabular-nums text-ink-muted">
                    {tasks.length}
                  </span>
                ) : null}
              </Link>
            )
          })}
          {views.length === 0 ? (
            <span className="pb-2.5 text-sm text-ink-muted">No views declared</span>
          ) : null}
        </nav>
      </header>

      {workflow.error ? (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-warning-ink">
          <LayersIcon size={14} className="shrink-0" />
          <span className="min-w-0 flex-1">
            The pinned workflow could not be read, so the default views are shown. {userMessage(workflow.error)}
          </span>
          <Button variant="quiet" size="sm" onClick={workflow.refetch}>
            Retry
          </Button>
        </div>
      ) : null}

      {currentId === 'overview' ? (
        <ProjectOverview project={project} tasks={tasks} workflow={workflow} />
      ) : currentId === 'tasks' ? (
        <ProjectTasks
          project={project}
          tasks={tasks}
          projects={projects}
          milestones={snapshot.data?.milestones.filter((milestone) => milestone.projectId === project.id) ?? []}
        />
      ) : (
        <UnavailableView
          name={currentView?.title ?? humanizeIdentifier(currentId)}
          description={currentView?.description}
          projectId={project.id}
        />
      )}
    </div>
  )
}
