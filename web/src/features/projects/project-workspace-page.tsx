import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, Navigate, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import {
  defaultNowMs,
  useMilestones,
  useProject,
  useProjectTasks,
  useProjects,
  useTaskReadiness,
  useWorkItems,
  useWorkspaceSnapshot,
  userMessage,
} from '@/hooks'
import { humanizeIdentifier } from '@/lib/humanize'
import { useDocumentTitle } from '@/hooks'
import type { Milestone, WorkflowView, WorkflowViewComponent } from '@/api/adapters'
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
import { isTerminalStatus, legacyTasksView, useTaskDrawer, writeTasksView } from '@/features/tasks'
import { TasksWorkspace } from '@/features/tasks/tasks-workspace'
import { RoadmapView } from '@/features/roadmap'
import { MilestoneDetailDialog, MilestoneEditDialog, MilestonesList } from '@/features/milestones'
import { ProjectPlanning } from '@/features/planning'
import {
  SavedViewRestoreProvider,
  searchFromQuery,
  type SavedView,
} from '@/features/saved-views'
import { ProjectSettings } from './project-settings'
import { recordProjectOpen } from '@/lib/recent-projects'
import { useProjectWorkflow } from './use-project-workflow'
import {
  NewMilestoneButton,
  NewWorkItemButton,
  ViewSurface,
  WorkItemSheet,
  declaredWorkspaceViews,
  pinnedKindForView,
  reservedDeclaredViews,
  useWorkItemDrawer,
  viewBadge,
  viewBadgeCount,
  viewHeaderAction,
  workspaceViewTabs,
} from '@/features/workflows'

/** The two workspace tools that are not workflow views. Reserved ids. */
const PLANNING_ID = 'planning'
const SETTINGS_ID = 'settings'

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

type ProjectSectionKind = 'overview' | 'tasks' | 'roadmap' | 'milestones' | 'declared'

export interface ProjectSection {
  id: string
  title: string
  kind: ProjectSectionKind
  view?: WorkflowView
  /** A workflow-declared `tasks` view over tasks, used as the list configuration. */
  listView?: WorkflowView
  /** A workflow-declared `board` view over tasks, used as the board configuration. */
  boardView?: WorkflowView
}

function syntheticView(
  id: string,
  title: string,
  type: string,
  source: string = 'tasks',
): WorkflowView {
  return {
    id,
    title,
    description: '',
    layout: { type: 'vertical', columns: 1 },
    components: [fallbackComponent({ id: `${id}-surface`, type, title, source })],
  }
}

/**
 * A declared view whose every surface reads tasks (or has no explicit source).
 * A mixed view that also declares a Work Item or other surface is NOT consumed
 * into the Tasks tab: that would strand the other surface with no route.
 */
function isTaskSourceView(view: WorkflowView): boolean {
  if (view.components.length === 0) return false
  return view.components.every((component) => {
    const source = component.source ?? null
    return source === null || source === 'tasks'
  })
}

/**
 * Canonical project sections. Tasks is one section with three URL views; a
 * workflow's own `tasks`/`board` declarations become that section's list/board
 * configuration only when they read tasks. A board over work items (or any
 * other source) keeps its own declared section and route.
 */
export function projectSections(navViews: WorkflowView[]): ProjectSection[] {
  if (navViews.length === 0) return []
  const overviewView = navViews.find((view) => view.id === 'overview')
  const listView = navViews.find((view) => view.id === 'tasks' && isTaskSourceView(view))
  const boardView = navViews.find((view) => view.id === 'board' && isTaskSourceView(view))
  const milestoneView =
    navViews.find((view) => view.id === 'milestones') ??
    navViews.find((view) => view.id === 'timeline')
  const consumed = new Set(
    [overviewView?.id, listView?.id, boardView?.id, milestoneView?.id].filter(
      (id): id is string => Boolean(id),
    ),
  )
  const sections: ProjectSection[] = [
    {
      id: 'overview',
      title: 'Overview',
      kind: 'overview',
      view: overviewView ?? syntheticView('overview', 'Overview', 'detail'),
    },
    {
      id: 'tasks',
      title: 'Tasks',
      kind: 'tasks',
      view: listView ?? syntheticView('tasks', 'Tasks', 'task_list'),
      listView,
      boardView,
    },
    { id: 'roadmap', title: 'Roadmap', kind: 'roadmap' },
    {
      id: 'milestones',
      title: 'Milestones',
      kind: 'milestones',
      view: milestoneView ?? syntheticView('milestones', 'Milestones', 'milestone_progress', 'milestones'),
    },
  ]
  for (const view of navViews) {
    if (consumed.has(view.id)) continue
    if (view.id === 'backlog' || view.id === 'roadmap') continue
    // The canonical Tasks route owns `tasks`; a workflow view with that id over
    // another source still needs its own address.
    const id = view.id === 'tasks' ? 'tasks-view' : view.id
    sections.push({
      id,
      title: workflowText(view.title) || humanizeIdentifier(view.id),
      kind: 'declared',
      view,
    })
  }
  return sections
}

/** A short, stable display key; the immutable id stays internal. */
function projectDisplayKey(projectId: string): string {
  return projectId.length > 18 ? projectId.slice(0, 8) : projectId
}

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
  const workItems = useWorkItems(projectId)
  const workflow = useProjectWorkflow(projectId)
  const itemDrawer = useWorkItemDrawer()
  const taskDrawer = useTaskDrawer()
  const { openNewTask } = useWorkspaceActions()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const [openMilestoneId, setOpenMilestoneId] = useState<string | null>(null)
  const [editMilestone, setEditMilestone] = useState<Milestone | null>(null)

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
  const sections = projectSections(navViews)
  // The binding can resolve a moment before the manifest query reports pending;
  // treat "a binding exists but its definition is not readable yet" as loading
  // so a canonical/legacy route never flashes "This view does not exist".
  const workflowResolving =
    workflow.loading || (Boolean(workflow.binding) && !workflow.workflow && !workflow.error)
  // Canonical routes win, but a workflow-declared id (`tasks`, `timeline`)
  // keeps resolving to the section that now owns it. Existing links and saved
  // views survive the rename. Without a view id there is no resolution to do:
  // the bare project address opens the first section (Overview), never a
  // section that happens to carry no declared view of its own.
  const currentSection = viewId
    ? sections.find((section) => section.id === viewId) ??
      sections.find((section) => section.view?.id === viewId)
    : undefined
  const currentId = currentSection?.id ?? viewId ?? sections[0]?.id ?? 'overview'
  const declaredView = currentSection?.view ?? navViews.find((view) => view.id === currentId)
  // Saved views store the shared Tasks filters plus a list/board display. Both
  // presentations live on the one Tasks route, so restore never has to hunt for
  // a workflow view that happens to render that way.
  const restoreSavedView = useMemo<{ restore: (view: SavedView) => void }>(
    () => ({
      restore: (view) => {
        if (!project) return
        const saved = new URLSearchParams(view.query)
        // The stored query keeps the exact Tasks view (including Backlog). A
        // saved view without one falls back to its list/board display.
        const params = saved.has('view')
          ? saved
          : writeTasksView(saved, view.display === 'board' ? 'board' : 'list')
        navigate(`/projects/${project.id}/tasks${searchFromQuery(params.toString())}`)
      },
    }),
    [navigate, project],
  )
  // A reserved id always belongs to the tool, even if a workflow declared it.
  const tool = currentId === PLANNING_ID || currentId === SETTINGS_ID ? currentId : null
  const openCount = tasks.filter((task) => !isTerminalStatus(task.status)).length

  // The milestone read also carries the ledger revision milestone commands
  // fence on. Reading it here keeps milestone dialogs and the roadmap usable
  // without every surface having to own a milestone list of its own.
  const milestoneRead = useMilestones(projectId)
  const projectMilestones = useMemo(
    () =>
      milestoneRead.data ??
      snapshot.data?.milestones.filter((milestone) => milestone.projectId === projectId) ??
      [],
    [milestoneRead.data, snapshot.data?.milestones, projectId],
  )
  const openMilestone = openMilestoneId
    ? projectMilestones.find((milestone) => milestone.id === openMilestoneId) ?? null
    : null
  // The canonical Milestones section owns the milestone cards, but a workflow
  // may declare other panels on the same view (for example a schedule). Those
  // panels are not milestone lists and stay reachable below the cards.
  const milestoneExtraComponents =
    currentSection?.view?.components.filter((component) => component.source !== 'milestones') ?? []

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
  const pinnedKind = pinnedKindForView(declaredView)
  const workItemsState = {
    isPending: workItems.isPending,
    error: workItems.error ?? null,
    retrying: workItems.isFetching,
    refetch: () => void workItems.refetch(),
  }
  const openWorkItem = itemDrawer.itemId
    ? (workItems.data ?? []).find((item) => item.id === itemDrawer.itemId) ?? null
    : null
  const navRef = useRef<HTMLElement | null>(null)

  // The tab strip scrolls sideways on a phone. Keep the current tab in view so
  // the page never looks like it is on a tab the person cannot see.
  useEffect(() => {
    const active = navRef.current?.querySelector<HTMLElement>('[aria-current="page"]')
    active?.scrollIntoView({ inline: 'center', block: 'nearest' })
  }, [currentId])

  // Old List/Board/Backlog links (and a consumed workflow `board`) point at the
  // one Tasks route. A workflow board over another source keeps its own section
  // and route, so it is not redirected — and while the pinned workflow is still
  // loading, its views are unknown, so nothing is redirected into the Tasks tab
  // that a declared section may own once the manifest arrives.
  const legacyView = legacyTasksView(viewId)
  if (
    projectId &&
    legacyView &&
    !workflowResolving &&
    !sections.some((section) => section.id === viewId)
  ) {
    const params = writeTasksView(searchParams, legacyView)
    return (
      <Navigate
        replace
        to={`/projects/${projectId}/tasks${searchFromQuery(params.toString())}`}
      />
    )
  }

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
    // Same rule for work items: the badge counts a read the page owns.
    if (badge.kind === 'work_items' && (workItems.error || !workItems.data)) {
      return null
    }
    const count = viewBadgeCount(view, {
      tasks,
      milestones: projectMilestones,
      workItems: workItems.data,
      workflow: workflow.workflow,
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
    <SavedViewRestoreProvider value={restoreSavedView}>
      <div className="flex flex-col gap-5">
        <header className="flex flex-col gap-4 border-b border-line pb-1">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <p className="section-label">Project</p>
              <div className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1.5">
                <h1 className="font-sans text-2xl font-semibold tracking-tight text-ink">{project.name}</h1>
                <Badge tone={project.status === 'active' ? 'success' : 'neutral'} dot>
                  {projectStatusLabel(project.status)}
                </Badge>
                <span className="text-2xs text-ink-muted">Workflow</span>
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
                <span className="font-mono text-2xs">{projectDisplayKey(project.id)}</span>
              </p>
            </div>

            <div className="flex w-full shrink-0 flex-wrap items-center gap-2 sm:w-auto">
              <ProjectSwitcher currentId={project.id} />
              {currentSection?.kind === 'tasks' ? null : primaryAction?.kind === 'new_task' ? (
                <Button
                  size="sm"
                  leadingIcon={<PlusIcon size={15} />}
                  onClick={() => openNewTask(project.id)}
                >
                  New task
                </Button>
              ) : primaryAction?.kind === 'new_milestone' ? (
                <NewMilestoneButton projectId={project.id} projectName={project.name} />
              ) : primaryAction?.kind === 'new_work_item' ? (
                <NewWorkItemButton
                  projectId={project.id}
                  workflow={workflow.workflow}
                  pinnedKind={pinnedKind}
                  variant="primary"
                  size="sm"
                  label="New work item"
                />
              ) : (
                <Button
                  size="sm"
                  leadingIcon={<PlusIcon size={15} />}
                  onClick={() => openNewTask(project.id)}
                >
                  New task
                </Button>
              )}
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
            {sections.map((section) => {
              const active = section.id === currentId
              return (
                <Link
                  key={section.id}
                  to={`/projects/${project.id}/${section.id}`}
                  aria-current={active ? 'page' : undefined}
                  title={section.view ? workflowText(section.view.description) : section.title}
                  className={cn(
                    'relative -mb-px inline-flex shrink-0 items-center gap-1.5 border-b-2 px-0.5 pb-2.5 pt-1 text-sm font-medium transition-colors',
                    active
                      ? 'border-accent text-ink'
                      : 'border-transparent text-ink-muted hover:text-ink',
                    'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-2 focus-visible:ring-offset-canvas',
                  )}
                >
                  {section.title}
                  {section.kind === 'tasks' ? (
                    <span className="rounded-full bg-surface-sunken px-1.5 py-0.5 text-2xs tabular-nums text-ink-muted">
                      {tasks.length}
                    </span>
                  ) : section.kind === 'milestones' ? (
                    <span className="rounded-full bg-surface-sunken px-1.5 py-0.5 text-2xs tabular-nums text-ink-muted">
                      {projectMilestones.length}
                    </span>
                  ) : section.view ? (
                    badgeFor(section.view)
                  ) : null}
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
        ) : !tool && !currentSection && workflowResolving ? (
          <ViewContentSkeleton />
        ) : !tool && sections.length === 0 && workflow.error ? (
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
        ) : !tool && sections.length === 0 ? (
          <Panel>
            <EmptyState
              icon={<LayersIcon size={21} />}
              title="This workflow declares no views"
              description="The project follows a workflow whose manifest publishes no views. Nothing is missing from the data, and this workspace does not substitute Overview or Tasks for a declaration that does not exist."
              className="min-h-[20rem]"
            />
          </Panel>
        ) : !tool && currentSection?.kind === 'tasks' ? (
          <TasksWorkspace
            project={project}
            tasks={tasks}
            milestones={projectMilestones}
            listView={currentSection.listView}
            boardView={currentSection.boardView}
          />
        ) : !tool && currentSection?.kind === 'roadmap' ? (
          <RoadmapView
            projectName={project.name}
            tasks={tasks}
            milestones={projectMilestones}
            onOpenTask={(taskId) => taskDrawer.openTask(taskId)}
            onOpenMilestone={(milestone) => setOpenMilestoneId(milestone.id)}
          />
        ) : !tool && currentSection?.kind === 'milestones' ? (
          <>
            <Panel>
            <div className="flex flex-col gap-3 border-b border-line px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
              <div className="min-w-0">
                <p className="section-label">Milestones</p>
                <h2 className="mt-1 text-md font-semibold tracking-tight text-ink">
                  Checkpoints and their work
                </h2>
                <p className="mt-0.5 text-sm text-ink-muted">
                  Each milestone groups tasks and supported Work Items. Progress comes from the
                  server; opening a card shows its work and activity.
                </p>
              </div>
              <span className="shrink-0 text-xs tabular-nums text-ink-muted">
                {projectMilestones.length} milestone{projectMilestones.length === 1 ? '' : 's'}
              </span>
            </div>
            {projectMilestones.length === 0 ? (
              <EmptyState
                icon={<LayersIcon size={21} />}
                title="No milestones yet"
                description="Create a milestone to group work around a checkpoint, then assign tasks and Work Items to it."
                className="min-h-[16rem]"
              />
            ) : (
              <MilestonesList
                milestones={projectMilestones}
                onOpen={(milestone) => setOpenMilestoneId(milestone.id)}
                onEdit={(milestone) => setEditMilestone(milestone)}
              />
            )}
          </Panel>
            {currentSection.view && milestoneExtraComponents.length > 0 ? (
              <ViewSurface
                view={{ ...currentSection.view, components: milestoneExtraComponents }}
                project={project}
                tasks={tasks}
                milestones={projectMilestones}
                workItems={workItems.data ?? []}
                workItemsState={workItemsState}
                workflow={workflow.workflow}
                workflowLoading={workflow.loading}
                workflowRetry={workflow.refetch}
              />
            ) : null}
          </>
        ) : declaredView ? (
          <ViewSurface
            view={declaredView}
            project={project}
            tasks={tasks}
            milestones={projectMilestones}
            workItems={workItems.data ?? []}
            workItemsState={workItemsState}
            workflow={workflow.workflow}
            workflowLoading={workflow.loading}
            workflowRetry={workflow.refetch}
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
                  <Link to={`/projects/${project.id}/${sections[0]?.id ?? ''}`}>
                    Open {sections[0]?.title || 'the first view'}
                  </Link>
                </Button>
              }
              className="min-h-[22rem]"
            />
          </Panel>
        )}

        <MilestoneDetailDialog
          milestone={openMilestone}
          tasks={tasks}
          workItems={workItems.data ?? []}
          workItemsState={workItemsState}
          open={openMilestone !== null}
          onOpenChange={(open) => {
            if (!open) setOpenMilestoneId(null)
          }}
          onOpenTask={(taskId) => {
            setOpenMilestoneId(null)
            taskDrawer.openTask(taskId)
          }}
          onEdit={(milestone) => setEditMilestone(milestone)}
        />

        <MilestoneEditDialog
          milestone={editMilestone}
          open={editMilestone !== null}
          onOpenChange={(open) => {
            if (!open) setEditMilestone(null)
          }}
        />

        {itemDrawer.itemId ? (
          <WorkItemSheet
            item={openWorkItem}
            workflow={workflow.workflow}
            onClose={itemDrawer.closeItem}
            onRefresh={workItemsState.refetch}
          />
        ) : null}
      </div>
    </SavedViewRestoreProvider>
  )
}
