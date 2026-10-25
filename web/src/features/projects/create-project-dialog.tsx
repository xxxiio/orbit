import { useEffect, useRef, useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQueryClient } from '@tanstack/react-query'
import {
  newCommandId,
  queryKeys,
  uncertainMessage,
  useAvailableWorkflows,
  useCreateProject,
  useWorkflowCatalog,
  useWorkflowDefinition,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { AlertCircleIcon, CheckIcon } from '@/components/ui/icons'
import { FormErrorBanner } from '@/components/forms'
import { humanizeIdentifier } from '@/lib/humanize'
import { cn } from '@/components/ui/utils'

const DEFAULT_WORKFLOW = ''

function WorkflowOption({
  id,
  version,
  selected,
  onSelect,
}: {
  id: string
  version: number
  selected: boolean
  onSelect: () => void
}) {
  const definition = useWorkflowDefinition(id, version)

  if (definition.isPending) {
    return (
      <div className="rounded-md border border-line bg-surface px-3 py-2.5">
        <Skeleton className="h-2.5 w-24" />
        <Skeleton className="mt-2 h-2.5 w-2/3" />
      </div>
    )
  }

  const name = definition.data?.name ?? humanizeIdentifier(id)
  const description = definition.data?.description
    ? definition.data.description
    : definition.error
      ? 'This workflow is listed by the server, but its description could not be read.'
      : 'No description was published for this workflow.'

  return (
    <label
      className={cn(
        'flex cursor-pointer items-start gap-2.5 rounded-md border px-3 py-2.5 transition-colors',
        selected
          ? 'border-accent bg-accent-soft/50'
          : 'border-line bg-surface hover:border-line-strong hover:bg-surface-hover/40',
      )}
    >
      <input
        type="radio"
        name="project-workflow"
        value={id}
        checked={selected}
        onChange={onSelect}
        className="mt-0.5 h-3.5 w-3.5 accent-[hsl(var(--accent))]"
      />
      <span className="min-w-0 flex-1">
        <span className="flex flex-wrap items-center gap-2">
          <span className="text-sm font-medium text-ink">{name}</span>
          <span className="font-mono text-2xs text-ink-muted">v{definition.data?.version ?? version}</span>
          {selected ? <CheckIcon size={13} className="text-accent" /> : null}
        </span>
        <span className="mt-0.5 block text-xs leading-5 text-ink-muted">{description}</span>
      </span>
    </label>
  )
}

export interface CreateProjectDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/**
 * One create attempt, minted at submit and reused while the payload is the same.
 * A retry after an unconfirmed failure therefore replays the receipt instead of
 * creating a second project; editing the fields mints a new attempt because the
 * server refuses to replay a command id with a different payload.
 */
interface CreateProjectAttempt {
  key: string
  commandId: string
  projectId: string
}

export function CreateProjectDialog({ open, onOpenChange }: CreateProjectDialogProps) {
  const workflows = useAvailableWorkflows()
  const createProject = useCreateProject()
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const { toast } = useToast()

  const [name, setName] = useState('')
  const [workflowId, setWorkflowId] = useState(DEFAULT_WORKFLOW)
  const [fieldError, setFieldError] = useState<string | null>(null)
  const nameRef = useRef<HTMLInputElement>(null)
  const attemptRef = useRef<CreateProjectAttempt | null>(null)

  useEffect(() => {
    if (!open) return
    setName('')
    setWorkflowId(DEFAULT_WORKFLOW)
    setFieldError(null)
    attemptRef.current = null
    createProject.reset()
    // Reset only on open; the mutation object is a fresh handle each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const error: ApiError | null = createProject.disposition === 'rejected' ? createProject.error : null
  const uncertain = createProject.disposition === 'uncertain'
  const submitting = createProject.isPending
  // The default is the task-only workflow the server offers, never a hardcoded
  // id: its name and description come from the manifest the list points at.
  const catalog = useWorkflowCatalog(workflows.data)
  const defaultWorkflow = catalog.defaultWorkflow
  const defaultName = defaultWorkflow
    ? defaultWorkflow.name ?? humanizeIdentifier(defaultWorkflow.id)
    : 'Server default workflow'
  const catalogPending = workflows.isPending || catalog.entries.some((entry) => entry.isPending)
  const additionalWorkflows = catalog.entries.filter((workflow) => workflow.id !== defaultWorkflow?.id)

  function attemptFor(key: string): CreateProjectAttempt {
    if (attemptRef.current?.key !== key) {
      attemptRef.current = { key, commandId: newCommandId(), projectId: newCommandId() }
    }
    return attemptRef.current
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (submitting) return
    const trimmed = name.trim()
    if (trimmed.length === 0) {
      setFieldError('Enter a name for this project.')
      nameRef.current?.focus()
      return
    }
    setFieldError(null)

    const selectedWorkflow = workflowId === DEFAULT_WORKFLOW ? undefined : workflowId
    const attempt = attemptFor(`${trimmed}\u0000${selectedWorkflow ?? ''}`)

    const outcome = await createProject
      .mutateAsync({
        commandId: attempt.commandId,
        projectId: attempt.projectId,
        name: trimmed,
        ...(selectedWorkflow ? { workflowId: selectedWorkflow } : {}),
      })
      .catch(() => null)

    if (!outcome) return
    attemptRef.current = null
    await queryClient.refetchQueries({ queryKey: queryKeys.snapshot() })
    toast({ tone: 'success', title: 'Project created', description: outcome.project.name })
    onOpenChange(false)
    navigate(`/projects/${outcome.project.id}`)
  }

  return (
    <Dialog open={open} onOpenChange={(next) => (submitting ? undefined : onOpenChange(next))}>
      <DialogContent
        title="New project"
        description="Name it, choose how its work is shaped, and start adding tasks."
        size="lg"
        className="max-h-[calc(100dvh-2rem)] overflow-y-auto"
      >
        <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-5">
          <Field
            label="Project name"
            required
            error={fieldError ?? undefined}
            hint="Project names don't need to be unique: two projects can share a name and stay separate."
          >
            <Input
              ref={nameRef}
              autoFocus
              value={name}
              onChange={(event) => {
                setName(event.target.value)
                if (fieldError) setFieldError(null)
              }}
              placeholder="Website rebuild"
              disabled={submitting}
              maxLength={200}
            />
          </Field>

          <fieldset className="flex flex-col gap-2">
            <legend className="text-sm font-medium text-ink">Workflow</legend>
            <p className="text-xs leading-5 text-ink-muted">
              A workflow decides which views and work-item types this project gets.
            </p>

            {catalogPending && !defaultWorkflow ? (
              <div className="mt-1 rounded-md border border-line bg-surface px-3 py-2.5">
                <Skeleton className="h-2.5 w-24" />
                <Skeleton className="mt-2 h-2.5 w-2/3" />
              </div>
            ) : (
              <label
                className={cn(
                  'mt-1 flex cursor-pointer items-start gap-2.5 rounded-md border px-3 py-2.5 transition-colors',
                  workflowId === DEFAULT_WORKFLOW
                    ? 'border-accent bg-accent-soft/50'
                    : 'border-line bg-surface hover:border-line-strong hover:bg-surface-hover/40',
                )}
              >
                <input
                  type="radio"
                  name="project-workflow"
                  value={DEFAULT_WORKFLOW}
                  checked={workflowId === DEFAULT_WORKFLOW}
                  onChange={() => setWorkflowId(DEFAULT_WORKFLOW)}
                  className="mt-0.5 h-3.5 w-3.5 accent-[hsl(var(--accent))]"
                />
                <span className="min-w-0 flex-1">
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium text-ink">{defaultName}</span>
                    <span className="text-2xs font-medium uppercase tracking-label text-accent-soft-ink">Default</span>
                    {workflowId === DEFAULT_WORKFLOW ? <CheckIcon size={13} className="text-accent" /> : null}
                  </span>
                  <span className="mt-0.5 block text-xs leading-5 text-ink-muted">
                    Statuses, priorities, deadlines and milestones for ordinary task work.
                  </span>
                </span>
              </label>
            )}

            {workflows.isPending ? (
              <div className="flex flex-col gap-2">
                <Skeleton className="h-12 w-full" />
                <Skeleton className="h-12 w-full" />
              </div>
            ) : workflows.error ? (
              <div className="flex items-center gap-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-warning-ink">
                <AlertCircleIcon size={14} className="shrink-0" />
                <span className="flex-1">Other workflows could not be listed. The server&apos;s default workflow is still available.</span>
                <Button variant="quiet" size="sm" onClick={() => void workflows.refetch()}>
                  Retry
                </Button>
              </div>
            ) : additionalWorkflows.length === 0 ? (
              <p className="text-xs text-ink-muted">
                No other workflows are installed on this server yet.
              </p>
            ) : (
              additionalWorkflows.map((workflow) => (
                <WorkflowOption
                  key={`${workflow.id}-${workflow.version}`}
                  id={workflow.id}
                  version={workflow.version}
                  selected={workflowId === workflow.id}
                  onSelect={() => setWorkflowId(workflow.id)}
                />
              ))
            )}
          </fieldset>

          {error ? <FormErrorBanner title="The project was not created" description={userMessage(error)} /> : null}
          {uncertain ? (
            <FormErrorBanner
              className="border-warning/40 bg-warning-soft text-warning-ink"
              title="The project may already exist"
              description={uncertainMessage(createProject.error ?? undefined)}
            />
          ) : null}

          <div className="flex items-center justify-end gap-2">
            <Button type="button" variant="ghost" onClick={() => onOpenChange(false)} disabled={submitting}>
              Cancel
            </Button>
            <Button type="submit" loading={submitting}>
              {submitting ? 'Creating…' : 'Create project'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
