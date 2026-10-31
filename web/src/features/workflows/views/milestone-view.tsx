import { useState } from 'react'
import type { Milestone } from '@/api/adapters'
import { useMilestones, useProjectTasks, useWorkItems } from '@/hooks'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { LayersIcon, PlusIcon } from '@/components/ui/icons'
import { Skeleton } from '@/components/ui/skeleton'
import { useTaskDrawer } from '@/features/tasks'
import {
  MilestoneCreateDialog,
  MilestoneDetailDialog,
  MilestoneEditDialog,
  MilestonesList,
} from '@/features/milestones'
import type { ViewRendererProps } from '../view-types'
import {
  componentTypeName,
  emptyTextOption,
  milestoneMatchesDeclaredFilter,
  unsupportedMilestoneFilterKeys,
  viewHeaderAction,
} from '../view-metadata'
import {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  ProvenanceNote,
  UnavailablePanel,
} from '@/components/surface'

/**
 * The milestone list: cards with the checkpoint's status, dates, progress,
 * blocked count and owner. Opening a card shows its details; create and edit
 * go through the milestone command family.
 *
 * The view reads the milestones query itself because every milestone command
 * fences on that snapshot's revision. Tasks come from the project snapshot and
 * Work Items from the same project read the project page already uses.
 */
export function MilestoneProgressView({
  component,
  view,
  project,
  milestones,
}: ViewRendererProps) {
  const title = componentTypeName(component)
  const emptyText = emptyTextOption(component)
  // The workspace header already offers New milestone when this view's primary
  // surface is milestones; a second button in the panel would be a duplicate.
  const headerOffersCreate = viewHeaderAction(view)?.kind === 'new_milestone'
  const query = useMilestones(project.id)
  // Assignable tasks must be the whole project, not the view's filtered set.
  const projectTasks = useProjectTasks(project.id)
  const workItems = useWorkItems(project.id)
  const { openTask } = useTaskDrawer()
  const [createOpen, setCreateOpen] = useState(false)
  const [detailId, setDetailId] = useState<string | null>(null)
  const [editId, setEditId] = useState<string | null>(null)

  const filter = component.filter ?? {}
  const unsupported = unsupportedMilestoneFilterKeys(filter)
  const all = (query.data ?? milestones).filter((milestone) =>
    milestoneMatchesDeclaredFilter(milestone, filter),
  )
  // A milestone panel inside an overview may declare a limit; keep the list
  // on that panel scoped to what the workflow asked to show.
  const list = component.limit && component.limit > 0 ? all.slice(0, component.limit) : all
  const detail = detailId ? all.find((milestone) => milestone.id === detailId) ?? null : null
  const editing = editId ? all.find((milestone) => milestone.id === editId) ?? null : null

  if (unsupported.length > 0) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This panel filters milestones by ${unsupported.join(', ')}, which this build cannot evaluate.`}
      />
    )
  }

  if (component.sort) {
    return (
      <UnavailablePanel
        title={title}
        reason={`This panel declares a sort by “${component.sort}”. Milestones keep the server's order in this build.`}
      />
    )
  }

  const openDetails = (milestone: Milestone) => setDetailId(milestone.id)
  const openEdit = (milestone: Milestone) => {
    setDetailId(null)
    setEditId(milestone.id)
  }

  return (
    <ComponentPanel
      title={title}
      eyebrow="Milestones"
      description={
        Object.keys(filter).length > 0
          ? `Checkpoints with their progress, dates and owner. Showing milestones where ${Object.entries(
              filter,
            )
              .map(([key, value]) => `${key.replace(/_/g, ' ')} is ${value}`)
              .join(', ')}.`
          : 'Checkpoints with their progress, dates and owner.'
      }
      actions={
        headerOffersCreate ? null : (
          <Button
            variant="secondary"
            size="sm"
            leadingIcon={<PlusIcon size={14} />}
            onClick={() => setCreateOpen(true)}
          >
            New milestone
          </Button>
        )
      }
      bodyClassName="p-0"
      footer={
        <ProvenanceNote>
          The server owns the milestone lifecycle, progress counts and membership. Every change is
          sent and the accepted record is rendered; a refusal is quoted and leaves the stored value
          untouched.
          {all.length > list.length
            ? ` Showing the first ${list.length} of ${all.length} milestones; the workflow sets this limit.`
            : ''}
          {component.actions.length > 0 ? ' ' : ''}
          <DeclaredActionsNote component={component} />
        </ProvenanceNote>
      }
    >
      {query.error ? (
        <div className="p-4">
          <InlineReadError
            label="Milestones"
            error={query.error}
            onRetry={() => void query.refetch()}
            retrying={query.isFetching}
          />
        </div>
      ) : query.isPending && !query.data ? (
        <div className="flex flex-col gap-2 p-4" aria-busy="true">
          <Skeleton className="h-4 w-1/2" />
          <Skeleton className="h-3 w-2/3" />
          <Skeleton className="h-4 w-1/3" />
        </div>
      ) : list.length === 0 ? (
        <EmptyState
          compact
          headingLevel="p"
          icon={<LayersIcon size={18} />}
          title={emptyText ?? 'No milestones yet'}
          description="A milestone is a checkpoint inside this project. Create one, then assign tasks and Work Items to it."
          action={
            headerOffersCreate ? undefined : (
              <Button variant="secondary" size="sm" onClick={() => setCreateOpen(true)}>
                New milestone
              </Button>
            )
          }
        />
      ) : (
        <MilestonesList milestones={list} onOpen={openDetails} onEdit={openEdit} />
      )}

      <MilestoneCreateDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        projectId={project.id}
        projectName={project.name}
      />
      <MilestoneDetailDialog
        milestone={detail}
        tasks={projectTasks}
        workItems={workItems.data ?? []}
        workItemsState={{
          isPending: workItems.isPending,
          error: workItems.error ?? null,
          retrying: workItems.isFetching,
          refetch: () => void workItems.refetch(),
        }}
        open={detail !== null}
        onOpenChange={(next) => {
          if (!next) setDetailId(null)
        }}
        onOpenTask={(taskId) => {
          setDetailId(null)
          openTask(taskId)
        }}
        onEdit={openEdit}
      />
      <MilestoneEditDialog
        milestone={editing}
        open={editing !== null}
        onOpenChange={(next) => {
          if (!next) setEditId(null)
        }}
      />
    </ComponentPanel>
  )
}
