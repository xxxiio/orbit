import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { useProjects } from '@/hooks'
import { Button } from '@/components/ui/button'
import { projectStatusLabel } from '@/features/projects/project-status'
import { isTaskCompleted, isTaskOpen } from '@/features/projects/project-progress'
import type { ViewRendererProps } from '../view-types'
import { componentTypeName } from '../view-metadata'
import { ComponentPanel, ProvenanceNote, UnavailablePanel } from './shared'

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[7.5rem_minmax(0,1fr)] items-start gap-3 border-b border-line py-2 last:border-b-0">
      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</dt>
      <dd className="min-w-0 text-sm leading-5 text-ink-soft">{children}</dd>
    </div>
  )
}

/**
 * Project detail without decoration: identity is immutable in this API, so the
 * panel lists what the server stores, states that rename is not supported, and
 * links to the screens that can actually change something.
 */
export function DetailView({ component, project, tasks, milestones, workflow }: ViewRendererProps) {
  const title = componentTypeName(component)
  const source = component.source ?? 'project'
  const projects = useProjects()

  if (source !== 'project') {
    return (
      <UnavailablePanel
        title={title}
        reason={`This panel is declared against “${source}”, which the browser does not render as a detail panel yet.`}
        hint="Open the Task list or Board for the project's work."
      />
    )
  }

  const open = tasks.filter((task) => isTaskOpen(task.status)).length
  const completed = tasks.filter((task) => isTaskCompleted(task.status)).length

  return (
    <ComponentPanel title={title} eyebrow="Project detail">
      <dl className="flex flex-col">
        <Row label="Name">{project.name}</Row>
        <Row label="Status">{projectStatusLabel(project.status)}</Row>
        <Row label="Workflow">
          {workflow ? `${workflow.name} v${workflow.version}` : 'No workflow is pinned to this project'}
        </Row>
        <Row label="Views">{workflow ? `${workflow.views.length} declared by the workflow` : 'Using the default views'}</Row>
        <Row label="Tasks">
          <span className="tabular-nums">{tasks.length}</span> total ·{' '}
          <span className="tabular-nums">{open}</span> open ·{' '}
          <span className="tabular-nums">{completed}</span> done
        </Row>
        <Row label="Milestones">
          <span className="tabular-nums">{milestones.length}</span> in this project
        </Row>
        <Row label="Project ID">
          <span className="break-all font-mono text-2xs text-ink-muted">{project.id}</span>
        </Row>
      </dl>
      <div className="mt-3 flex flex-col gap-2 border-t border-line pt-3">
        <ProvenanceNote>
          Project names cannot be changed yet: the API has no rename command. The id and name are
          therefore fixed for the life of the project.
        </ProvenanceNote>
        <div className="flex flex-wrap gap-2">
          <Button asChild variant="secondary" size="sm">
            <Link to={`/projects/${project.id}/settings`}>Project settings</Link>
          </Button>
          {projects.length > 1 ? (
            <Button asChild variant="quiet" size="sm">
              <Link to="/projects">All projects</Link>
            </Button>
          ) : null}
        </div>
      </div>
    </ComponentPanel>
  )
}
