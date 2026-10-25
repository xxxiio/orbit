import type { ViewRendererProps } from '../view-types'
import {
  componentTypeName,
  dataSourceName,
  describeFilter,
  unsupportedTaskFilterKeys,
} from '../view-metadata'
import { columnsFromComponent } from '@/features/tasks'
import { ProjectTaskList } from '@/features/projects/project-task-list'
import { ComponentPanel, ProvenanceNote, UnavailablePanel } from './shared'
import { TaskSummaryList } from './list-view'

/**
 * A task list declared by a workflow. A component with a `limit` is a summary
 * (an overview strip); one without a limit is the project's full task list and
 * gets the URL-backed filters, columns, selection and bulk commands.
 */
export function TaskListView(props: ViewRendererProps) {
  const source = props.component.source ?? 'tasks'
  if (source !== 'tasks') {
    return (
      <UnavailablePanel
        title={componentTypeName(props.component)}
        reason={`This list is declared over “${dataSourceName(source)}”, which the browser does not list yet.`}
      />
    )
  }

  const filter = props.component.filter ?? {}
  const unsupported = unsupportedTaskFilterKeys(filter)
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
    <ComponentPanel
      title={componentTypeName(props.component)}
      description={describeFilter(filter) ?? 'Every task in this project.'}
      bodyClassName="px-3 py-4 sm:px-4"
      footer={
        <ProvenanceNote>
          The view state — search, filters, sort, page, columns and selection — lives in the URL, so
          this list can be linked and reloaded.
        </ProvenanceNote>
      }
    >
      <ProjectTaskList
        project={props.project}
        tasks={props.tasks}
        milestones={props.milestones}
        defaultColumns={columnsFromComponent(props.component)}
      />
    </ComponentPanel>
  )
}

/**
 * The declared table. Task tables are the full project task list; a table over
 * another source is refused by name instead of showing an empty grid.
 */
export function TableView(props: ViewRendererProps) {
  const source = props.component.source ?? 'tasks'
  if (source !== 'tasks') {
    return (
      <UnavailablePanel
        title={componentTypeName(props.component)}
        reason={`This table is declared over “${dataSourceName(source)}”, which the browser does not tabulate yet.`}
        hint="The Task list and Board cover the project's tasks; work items are still a console feature."
      />
    )
  }

  const unsupported = unsupportedTaskFilterKeys(props.component.filter ?? {})
  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={componentTypeName(props.component)}
        reason={`This table filters tasks by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  return (
    <ComponentPanel
      title={componentTypeName(props.component)}
      description="Sort, filter, pick columns and select tasks. The server owns status, readiness and dependencies."
      bodyClassName="px-3 py-4 sm:px-4"
      footer={
        <ProvenanceNote>
          View state lives in the URL. Bulk changes are one server command per task; a refusal is
          shown with the server&apos;s message and the values stay as stored.
        </ProvenanceNote>
      }
    >
      <ProjectTaskList
        project={props.project}
        tasks={props.tasks}
        milestones={props.milestones}
        defaultColumns={columnsFromComponent(props.component)}
      />
    </ComponentPanel>
  )
}
