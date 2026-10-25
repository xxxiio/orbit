import { useEffect, useMemo, useRef } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import {
  defaultNowMs,
  useProject,
  useProjectTasks,
  useProjects,
  useTaskReadiness,
  useWorkspaceSnapshot,
  userMessage,
} from '@/hooks'
import { humanizeIdentifier } from '@/lib/humanize'
import { useDocumentTitle } from '@/hooks'
import type { WorkItem, WorkflowView, WorkflowViewComponent } from '@/api/adapters'
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
  ClockIcon,
  FolderIcon,
  LayersIcon,
  PlusIcon,
  SettingsIcon,
} from '@/components/ui/icons'
import { Panel } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/components/ui/utils'
import { useWorkspaceActions } from '@/lib/workspace-actions'
import { projectStatusLabel, workflowText } from '@/domain'
import { isTerminalStatus } from '@/features/tasks'
import { ProjectPlanning } from '@/features/planning'
import { ProjectSettings } from './project-settings'
import { recordProjectOpen } from '@/lib/recent-projects'
import { useProjectWorkflow } from './use-project-workflow'
import {
  NewMilestoneButton,
  ViewSurface,
  declaredWorkspaceViews,
  reservedDeclaredViews,
  viewBadge,
  viewBadgeCount,
  viewHeaderAction,
  workspaceViewTabs,
} from '@/features/workflows'

/** The two workspace tools that are not workflow views. Reserved ids. */
const PLANNING_ID = 'planning'
const SETTINGS_ID = 'settings'

/**
 * Work items have no workspace-snapshot projection yet (I3 adds the read).
 * The renderer contract carries the source now so a work-item renderer plugs in
 * instead of adding a fourth parallel copy of the view pipeline.
 */
const NO_WORK_ITEMS: WorkItem[] = []

function fallbackComponent(
  component: Pick<WorkflowViewComponent, 'id' | 'type'> & Partial<WorkflowViewComponent>,
): WorkflowViewComponent {
  return {
    title: null,
    source: null,
    aggregate: null,
    field: null,
    groupBy: null,
    filter: {},
    sort: null,
    descending: false,
    limit: null,
    columns: [],
    text: null,
    actions: [],
    options: {},
    ...component,
  }
}

/**
 * The views a project has before a workflow is pinned (or when the pinned
 * workflow cannot be read). They are ordinary declared views, so the same
 * registry renders them.
 */
const FALLBACK_VIEWS: WorkflowView[] = [
  {
    id: 'overview',
    title: 'Overview',
    description: 'Project summary and the work that is ready to move.',
    layout: { type: 'grid', columns: 2 },
    components: [
      fallbackComponent({ id: 'project', type: 'detail', title: 'Project', source: 'project' }),
      fallbackComponent({
        id: 'tasks',
        type: 'task_list',
        title: 'Tasks',
        source: 'tasks',
        limit: 8,
      }),
    ],
  },
  {
    id: 'tasks',
    title: 'Tasks',
    description: 'Every task in this project.',
    layout: { type: 'vertical', columns: 1 },
    components: [fallbackComponent({ id: 'task_table', type: 'table', title: 'Tasks', source: 'tasks' })],
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

function ViewContentSkeleton() {
  return (
    <div className="flex flex-col gap-5" aria-busy="true" aria-label="Loading this view">
      <div className="grid gap-5 md:grid-cols-2">
        <Skeleton className="h-40 rounded-lg" />
        <Skeleton className="h-40 rounded-lg" />
      </div>
      <Skeleton className="h-56 rounded-lg" />
    </div>
  )
}

export function ProjectWorkspacePage() {
  const { projectId, viewId } = useParams<{ projectId: string; viewId?: string }>()
  const snapshot = useWorkspaceSnapshot()
  const project = useProject(projectId)
  const tasks = useProjectTasks(projectId)
  const workflow = useProjectWorkflow(projectId)
  const { openNewTask } = useWorkspaceActions()

  useDocumentTitle(project ? project.name : 'Project')

  useEffect(() => {
    if (projectId) recordProjectOpen(projectId)
  }, [projectId])

  // `undefined` means "no workflow resolved yet" (fall back to Overview/Tasks);
  // `[]` means the pinned workflow declares no views, which must say so instead
  // of pretending it declared the fallback pair.
  const views = declaredWorkspaceViews(workflow.workflow, FALLBACK_VIEWS)
  // A workflow must not shadow the workspace tools with a declared view id.
  const navViews = workspaceViewTabs(views)
  const reservedViews = reservedDeclaredViews(views)
  const currentId = viewId ?? navViews[0]?.id ?? 'overview'
  const declaredView = navViews.find((view) => view.id === currentId)
  // A reserved id always belongs to the tool, even if a workflow declared it.
  const tool = currentId === PLANNING_ID || currentId === SETTINGS_ID ? currentId : null
  const openCount = tasks.filter((task) => !isTerminalStatus(task.status)).length

  const projectMilestones = useMemo(
    () => snapshot.data?.milestones.filter((milestone) => milestone.projectId === projectId) ?? [],
    [snapshot.data?.milestones, projectId],
  )

  const badgeNeedsReadiness = navViews.some((view) => {
    const badge = viewBadge(view)
    return badge?.kind === 'tasks' && view.components[0]?.filter?.actionable !== undefined
  })
  const badgeReadiness = useTaskReadiness(defaultNowMs(), badgeNeedsReadiness)
  const badgeReadinessByTask = useMemo(
    () => new Map((badgeReadiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry])),
    [badgeReadiness.data],
  )

  const primaryAction = declaredView ? viewHeaderAction(declaredView) : null
  const navRef = useRef<HTMLElement | null>(null)

  // The tab strip scrolls sideways on a phone. Keep the current tab in view so
  // the page never looks like it is on a tab the person cannot see.
  useEffect(() => {
    const active = navRef.current?.querySelector<HTMLElement>('[aria-current="page"]')
    active?.scrollIntoView({ inline: 'center', block: 'nearest' })
  }, [currentId])

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

  const badgeFor = (view: WorkflowView) => {
    const badge = viewBadge(view)
    if (!badge) return null
    // A readiness-backed badge has no honest number until the report arrives.
    if (
      badge.kind === 'tasks' &&
      view.components[0]?.filter?.actionable !== undefined &&
      (badgeReadiness.error || (badgeReadiness.isPending && !badgeReadiness.data))
    ) {
      return null
    }
    const count = viewBadgeCount(view, {
      tasks,
      milestones: projectMilestones,
      readinessByTask: badgeReadinessByTask,
    })
    if (count === null) return null
    return (
      <span className="rounded-full bg-surface-sunken px-1.5 py-0.5 text-2xs tabular-nums text-ink-muted">
        <span className="sr-only">, {count} {badge.noun}</span>
        <span aria-hidden="true">{count}</span>
      </span>
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
              <span className="tabular-nums">{projectMilestones.length}</span> milestone
              {projectMilestones.length === 1 ? '' : 's'}
              <span aria-hidden="true">·</span>
              <span className="font-mono text-2xs">{project.id}</span>
            </p>
          </div>

          <div className="flex shrink-0 flex-wrap items-center gap-2">
            <ProjectSwitcher currentId={project.id} />
            {primaryAction?.kind === 'new_task' ? (
              <Button
                size="sm"
                leadingIcon={<PlusIcon size={15} />}
                onClick={() => openNewTask(project.id)}
              >
                New task
              </Button>
            ) : primaryAction?.kind === 'new_milestone' ? (
              <NewMilestoneButton projectId={project.id} projectName={project.name} />
            ) : null}
            {tool !== SETTINGS_ID ? (
              <Button asChild variant="quiet" size="sm" aria-label="Project settings">
                <Link to={`/projects/${project.id}/${SETTINGS_ID}`}>
                  <SettingsIcon size={16} />
                  <span className="hidden sm:inline">Settings</span>
                </Link>
              </Button>
            ) : null}
          </div>
        </div>

        <nav
          ref={navRef}
          aria-label="Project sections"
          className="-mb-px flex gap-5 overflow-x-auto"
        >
          {navViews.map((view) => {
            const active = view.id === currentId
            return (
              <Link
                key={view.id}
                to={`/projects/${project.id}/${view.id}`}
                aria-current={active ? 'page' : undefined}
                title={workflowText(view.description)}
                className={cn(
                  'relative -mb-px inline-flex shrink-0 items-center gap-1.5 border-b-2 px-0.5 pb-2.5 pt-1 text-sm font-medium transition-colors',
                  active
                    ? 'border-accent text-ink'
                    : 'border-transparent text-ink-muted hover:text-ink',
                  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
                )}
              >
                {workflowText(view.title) || humanizeIdentifier(view.id)}
                {badgeFor(view)}
              </Link>
            )
          })}
          {views.length === 0 ? (
            <span className="pb-2.5 text-sm text-ink-muted">This workflow declares no views</span>
          ) : null}

          <span aria-hidden="true" className="mb-2 w-px shrink-0 self-end bg-line" />

          <Link
            to={`/projects/${project.id}/${PLANNING_ID}`}
            data-tool="planning"
            aria-current={tool === PLANNING_ID ? 'page' : undefined}
            className={cn(
              'relative -mb-px inline-flex shrink-0 items-center gap-1.5 border-b-2 px-0.5 pb-2.5 pt-1 text-sm font-medium transition-colors',
              tool === PLANNING_ID
                ? 'border-accent text-ink'
                : 'border-transparent text-ink-muted hover:text-ink',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
            )}
          >
            <ClockIcon size={14} />
            Planning
          </Link>
          <Link
            to={`/projects/${project.id}/${SETTINGS_ID}`}
            data-tool="settings"
            aria-current={tool === SETTINGS_ID ? 'page' : undefined}
            className={cn(
              'relative -mb-px inline-flex shrink-0 items-center gap-1.5 border-b-2 px-0.5 pb-2.5 pt-1 text-sm font-medium transition-colors',
              tool === SETTINGS_ID
                ? 'border-accent text-ink'
                : 'border-transparent text-ink-muted hover:text-ink',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
            )}
          >
            <SettingsIcon size={14} />
            Settings
          </Link>
        </nav>

        {reservedViews.length > 0 ? (
          <div
            role="status"
            data-reserved-view={reservedViews.map((view) => view.id).join(',')}
            className="flex flex-wrap items-center gap-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-warning-ink"
          >
            <LayersIcon size={14} className="shrink-0" />
            <span className="min-w-0 flex-1">
              The pinned workflow declares a view with the reserved id{' '}
              {reservedViews.map((view) => `“${view.id}”`).join(', ')}. It is not rendered so the{' '}
              {reservedViews.map((view) => (view.id === PLANNING_ID ? 'Planning' : 'Settings')).join(' and ')}{' '}
              tool keeps that route. Rename the view id in the workflow to see it.
            </span>
          </div>
        ) : null}
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

      {tool === PLANNING_ID ? (
        <ProjectPlanning project={project} tasks={tasks} milestones={projectMilestones} />
      ) : tool === SETTINGS_ID ? (
        <ProjectSettings
          project={project}
          workflow={workflow.workflow}
          binding={workflow.binding}
          loading={workflow.loading}
          error={workflow.error}
          onRetry={workflow.refetch}
        />
      ) : !declaredView && !tool && workflow.loading ? (
        <ViewContentSkeleton />
      ) : !declaredView && !tool && workflow.error ? (
        <Panel>
          <EmptyState
            icon={<LayersIcon size={21} />}
            title="This view could not be resolved"
            description="The project's pinned workflow could not be read, so the views it declares are unknown. The workspace falls back to Overview and Tasks once the workflow is readable."
            action={
              <Button variant="secondary" size="sm" onClick={workflow.refetch}>
                Retry the workflow
              </Button>
            }
            className="min-h-[20rem]"
          />
        </Panel>
      ) : !declaredView && !tool && views.length === 0 ? (
        <Panel>
          <EmptyState
            icon={<LayersIcon size={21} />}
            title="This workflow declares no views"
            description="The project follows a workflow whose manifest publishes no views. Nothing is missing from the data, and this workspace does not substitute Overview or Tasks for a declaration that does not exist."
            className="min-h-[20rem]"
          />
        </Panel>
      ) : declaredView ? (
        <ViewSurface
          view={declaredView}
          project={project}
          tasks={tasks}
          milestones={projectMilestones}
          workItems={NO_WORK_ITEMS}
          workflow={workflow.workflow}
        />
      ) : (
        <Panel>
          <EmptyState
            icon={<LayersIcon size={21} />}
            title="This view does not exist"
            description={
              <>
                {viewId ? (
                  <span className="font-medium text-ink-soft">{humanizeIdentifier(viewId)}</span>
                ) : (
                  'The requested view'
                )}{' '}
                is not declared by this project&apos;s workflow. Nothing is hidden: the workflow simply
                has no such view.
              </>
            }
            action={
              <Button asChild variant="secondary" size="sm">
                <Link to={`/projects/${project.id}/${navViews[0]?.id ?? ''}`}>
                  Open {workflowText(navViews[0]?.title) || 'the first view'}
                </Link>
              </Button>
            }
            className="min-h-[22rem]"
          />
        </Panel>
      )}
    </div>
  )
}
