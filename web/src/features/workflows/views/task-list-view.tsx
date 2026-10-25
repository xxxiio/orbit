import { useMemo } from 'react'
import { defaultNowMs, useTaskReadiness } from '@/hooks'
import { Skeleton } from '@/components/ui/skeleton'
import type { ViewRendererProps } from '../view-types'
import {
  componentTypeName,
  dataSourceName,
  describeFilter,
  taskMatchesDeclaredFilter,
  taskSortDeclaration,
  unsupportedTaskFilterKeys,
} from '../view-metadata'
import {
  DEFAULT_TASK_SORT,
  columnsFromComponent,
  ProjectTaskList,
  type TaskSort,
} from '@/features/tasks'
import {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  UnavailablePanel,
} from './shared'
import { TaskSummaryList } from './list-view'

/**
 * The snapshot tasks narrowed by the component's declared filter. Readiness is
 * only requested when the filter asks for it, and a pending report never widens
 * the set: an `actionable` task without an entry cannot match.
 */
function useDeclaredTaskScope(props: ViewRendererProps, filters: Record<string, string>) {
  const actionable = filters.actionable !== undefined
  const readiness = useTaskReadiness(defaultNowMs(), actionable)
  const readinessByTask = useMemo(
    () => new Map((readiness.data?.tasks ?? []).map((entry) => [entry.taskId, entry])),
    [readiness.data],
  )
  const tasks = useMemo(
    () =>
      props.tasks.filter((task) =>
        taskMatchesDeclaredFilter(
          task,
          filters,
          actionable ? readinessByTask.get(task.id) : undefined,
        ),
      ),
    [props.tasks, filters, actionable, readinessByTask],
  )
  return { tasks, readiness, actionable }
}

function sortDescription(sort: TaskSort | null): string {
  if (!sort) return 'Every task in this project.'
  return `Every task in this project, sorted by ${sort.key} ${
    sort.direction === 'asc' ? 'ascending' : 'descending'
  } as the workflow declares.`
}

/**
 * A full project task list: the URL-backed filters, columns, selection and bulk
 * commands, over the set the workflow declared. `defaultSort` applies the
 * manifest's `sort`/`descending` until the person chooses another sort in the
 * URL.
 */
function FullTaskList({
  props,
  description,
  footerNote,
}: {
  props: ViewRendererProps
  description: string
  footerNote: string
}) {
  const component = props.component
  const filter = component.filter ?? {}
  const unsupported = unsupportedTaskFilterKeys(filter)
  const sortDeclaration = taskSortDeclaration(component)
  const scope = useDeclaredTaskScope(props, filter)

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This list filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  if (sortDeclaration.refusal) {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={sortDeclaration.refusal}
        hint="The list is refused rather than showing the server's order under a declared sort."
      />
    )
  }

  if (scope.actionable && scope.readiness.error) {
    return (
      <ComponentPanel title={componentTypeName(component)}>
        <InlineReadError
          label="Task readiness"
          error={scope.readiness.error}
          onRetry={() => void scope.readiness.refetch()}
          retrying={scope.readiness.isFetching}
        />
      </ComponentPanel>
    )
  }

  if (scope.actionable && scope.readiness.isPending && !scope.readiness.data) {
    return (
      <ComponentPanel title={componentTypeName(component)}>
        <div className="flex flex-col gap-2" aria-busy="true">
          <Skeleton className="h-3 w-1/2" />
          <Skeleton className="h-3 w-1/3" />
        </div>
      </ComponentPanel>
    )
  }

  const filterNote = describeFilter(filter)
  return (
    <ComponentPanel
      title={componentTypeName(component)}
      description={[filterNote, description].filter(Boolean).join(' · ')}
      bodyClassName="px-3 py-4 sm:px-4"
      footer={
        <div className="flex flex-col gap-1">
          <ProvenanceNote>{footerNote}</ProvenanceNote>
          {component.actions.length > 0 ? (
            <ProvenanceNote>
              <DeclaredActionsNote component={component} />
            </ProvenanceNote>
          ) : null}
        </div>
      }
    >
      <ProjectTaskList
        project={props.project}
        tasks={scope.tasks}
        milestones={props.milestones}
        defaultColumns={columnsFromComponent(component)}
        defaultSort={sortDeclaration.sort ?? DEFAULT_TASK_SORT}
      />
    </ComponentPanel>
  )
}

/**
 * A task list declared by a workflow. A component with a `limit` is a summary
 * (an overview strip); one without a limit is the project's full task list and
 * gets the URL-backed filters, columns, selection and bulk commands.
 */
export function TaskListView(props: ViewRendererProps) {
  const source = props.component.source ?? 'tasks'
  const filter = props.component.filter ?? {}
  const unsupported = unsupportedTaskFilterKeys(filter)

  if (source !== 'tasks') {
    return (
      <UnavailablePanel
        title={componentTypeName(props.component)}
        reason={`This list is declared over “${dataSourceName(source)}”, which the browser does not list yet.`}
      />
    )
  }

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(props.component)}
        reason={`This list filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  if (props.component.limit !== null && props.component.limit !== undefined) {
    return <TaskSummaryList props={props} withReadiness={filter.actionable !== undefined} />
  }

  return (
    <FullTaskList
      props={props}
      description={sortDescription(taskSortDeclaration(props.component).sort)}
      footerNote="The view state — search, filters, sort, page, columns and selection — lives in the URL, so this list can be linked and reloaded."
    />
  )
}

/**
 * The declared table. Task tables are the full project task list; a table over
 * another source is refused by name instead of showing an empty grid.
 */
export function TableView(props: ViewRendererProps) {
  const component = props.component
  const source = component.source ?? 'tasks'

  if (source !== 'tasks') {
    return (
      <UnavailablePanel
        title={componentTypeName(component)}
        reason={`This table is declared over “${dataSourceName(source)}”, which the browser does not tabulate yet.`}
        hint="The Task list and Board cover the project's tasks; work items are still a console feature."
      />
    )
  }

  return (
    <FullTaskList
      props={props}
      description={`Sort, filter, pick columns and select tasks. The server owns status, readiness and dependencies. ${sortDescription(
        taskSortDeclaration(component).sort,
      )}`}
      footerNote="View state lives in the URL. Bulk changes are one server command per task; a refusal is shown with the server's message and the values stay as stored."
    />
  )
}
