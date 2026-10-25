/**
 * The workflow view registry.
 *
 * A `ViewComponent.type` maps to exactly one renderer. Basic, Development,
 * Research, Learning and Habits all resolve through this table, so adding a
 * workflow never means adding a branch to the project page. Keys are component
 * types, never workflow ids.
 *
 * A type this build does not render still has a renderer: an explicit, named
 * panel that says what is missing and why. There is no silent blank surface and
 * no internal identifier in the copy.
 */
import type { ComponentType } from 'react'
import { EmptyState } from '@/components/ui/empty-state'
import { GridIcon } from '@/components/ui/icons'
import { Panel } from '@/components/ui/panel'
import type { ViewRendererProps } from './view-types'
import { KnowledgeRefsPanel } from '@/features/knowledge'
import { AgentJobsPanel } from '@/features/agents'
import {
  componentSpansFullWidth,
  componentTypeName,
  declaredWindowOption,
  hasOwnKey,
  viewLayoutClassName,
} from './view-metadata'
import { BoardView } from './views/board-view'
import { DetailView } from './views/detail-view'
import { ListView } from './views/list-view'
import { MetricView } from './views/metric-view'
import { MilestoneProgressView } from './views/milestone-view'
import { TableView, TaskListView } from './views/task-list-view'
import { TextView } from './views/text-view'
import { TimelineView } from './views/timeline-view'
import { UnavailablePanel } from './views/shared'

function HabitTrackerView(props: ViewRendererProps) {
  const days = declaredWindowOption(props.component, 'days')
  return (
    <UnavailablePanel
      title={componentTypeName(props.component)}
      reason={`Habit tracking is not available in the browser yet.${
        days ? ` This panel declares a ${days}-day window, which this build cannot draw.` : ''
      }`}
      hint="The server stores habits and occurrences; recording them is still a console and API feature."
    />
  )
}

function MetricHistoryView(props: ViewRendererProps) {
  const window = declaredWindowOption(props.component, 'window')
  return (
    <UnavailablePanel
      title={componentTypeName(props.component)}
      reason={`Metric history is not charted in the browser yet.${
        window ? ` This panel declares a ${window}-observation window, which this build cannot chart.` : ''
      }`}
      hint="Metric targets and observations are readable over the API; this screen does not draw them."
    />
  )
}

function KnowledgeRefsView(props: ViewRendererProps) {
  return <KnowledgeRefsPanel component={props.component} />
}

function AgentJobsView(props: ViewRendererProps) {
  return <AgentJobsPanel component={props.component} />
}

export const viewComponentRenderers: Record<string, ComponentType<ViewRendererProps>> = {
  text: TextView,
  list: ListView,
  table: TableView,
  metric: MetricView,
  detail: DetailView,
  kanban: BoardView,
  task_list: TaskListView,
  timeline: TimelineView,
  milestone_progress: MilestoneProgressView,
  // Declared by the bundled workflows, not rendered by this build.
  habit_tracker: HabitTrackerView,
  metric_history: MetricHistoryView,
  knowledge_refs: KnowledgeRefsView,
  agent_jobs: AgentJobsView,
}

export function isViewComponentImplemented(type: string): boolean {
  // A manifest type is data: `constructor` must not resolve to a built-in.
  const renderer = hasOwnKey(viewComponentRenderers, type)
    ? viewComponentRenderers[type]
    : undefined
  return Boolean(renderer) && !UNIMPLEMENTED_TYPES.has(type)
}

const UNIMPLEMENTED_TYPES = new Set(['habit_tracker', 'metric_history'])

/** A type the registry has never heard of: named, refused, never blank. */
function UnknownComponentView(props: ViewRendererProps) {
  return (
    <Panel>
      <EmptyState
        compact
        headingLevel="p"
        icon={<GridIcon size={18} />}
        title={`${componentTypeName(props.component)} is not supported`}
        description="This workflow declares a panel that this build of the web app does not know how to render. Nothing is hidden from the workflow; the panel simply has no renderer here."
        className="min-h-[12rem]"
      />
    </Panel>
  )
}

export function renderViewComponent(props: ViewRendererProps) {
  const { component } = props
  // `group_by` is only meaningful for a board. Any other declaration would be
  // silently dropped otherwise, so it gets a named refusal instead.
  if (component.groupBy && component.type !== 'kanban') {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This panel groups its data by “${component.groupBy}”, which this build cannot render for a ${component.type} panel.`}
        hint="Only a board groups by a declared field (status) in this build."
      />
    )
  }

  const Renderer = hasOwnKey(viewComponentRenderers, component.type)
    ? viewComponentRenderers[component.type]
    : UnknownComponentView
  return <Renderer {...props} />
}

/**
 * One declared view: its components, in the layout the workflow asked for.
 * Wide work (a table, a board) takes the whole row even in a grid.
 */
export function ViewSurface(props: Omit<ViewRendererProps, 'component'>) {
  const { view } = props
  if (view.components.length === 0) {
    return (
      <Panel>
        <EmptyState
          compact
          icon={<GridIcon size={18} />}
          title="This view declares no panels"
          description="The workflow published the view without components. Nothing is missing from the data; there is simply nothing to lay out."
          className="min-h-[12rem]"
        />
      </Panel>
    )
  }

  return (
    <div className={viewLayoutClassName(view)}>
      {view.components.map((component) => (
        <div
          key={component.id}
          data-view-component={component.id}
          data-view-component-type={component.type}
          className="min-w-0"
          style={
            componentSpansFullWidth(component, view.layout)
              ? { gridColumn: '1 / -1' }
              : undefined
          }
        >
          {renderViewComponent({ ...props, component })}
        </div>
      ))}
    </div>
  )
}
