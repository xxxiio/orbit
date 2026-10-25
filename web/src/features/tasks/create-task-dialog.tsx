import { useEffect, useRef, useState, type FormEvent } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import {
  newCommandId,
  queryKeys,
  uncertainMessage,
  useCreateTask,
  useIdempotentAttempt,
  useProjects,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { useToast } from '@/components/ui/toast'
import { FormErrorBanner, LabeledSelect } from '@/components/forms'

const NONE = '__none__'

/**
 * One create attempt: the command id and the task id are minted when the person
 * submits (never inside `mutationFn`), and reused while the submitted payload
 * stays the same. Pressing Create again after an unconfirmed failure therefore
 * replays the server's receipt instead of creating a second task. Editing the
 * fields mints a new attempt, because the server refuses to replay a command id
 * whose payload changed.
 */
interface CreateTaskAttempt {
  key: string
  commandId: string
  taskId: string
}

export interface CreateTaskDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Preselected when the dialog is opened from a project. */
  defaultProjectId?: string
}

export function CreateTaskDialog({ open, onOpenChange, defaultProjectId }: CreateTaskDialogProps) {
  const projects = useProjects()
  const createTask = useCreateTask()
  const queryClient = useQueryClient()
  const { toast } = useToast()

  const [title, setTitle] = useState('')
  const [projectId, setProjectId] = useState<string>(defaultProjectId ?? NONE)
  const [fieldError, setFieldError] = useState<string | null>(null)
  const titleRef = useRef<HTMLInputElement>(null)
  const attempt = useIdempotentAttempt<CreateTaskAttempt>((key) => ({
    key,
    commandId: newCommandId(),
    taskId: newCommandId(),
  }))

  useEffect(() => {
    if (!open) return
    setTitle('')
    setProjectId(defaultProjectId ?? NONE)
    setFieldError(null)
    attempt.clear()
    createTask.reset()
    // Only reset when the dialog opens; `createTask` is a mutation object whose
    // identity changes every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, defaultProjectId])

  const error: ApiError | null = createTask.disposition === 'rejected' ? createTask.error : null
  const uncertain = createTask.disposition === 'uncertain'
  const submitting = createTask.isPending

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (submitting) return
    const trimmed = title.trim()
    if (trimmed.length === 0) {
      setFieldError('Enter a title for this task.')
      titleRef.current?.focus()
      return
    }
    setFieldError(null)

    const selectedProject = projectId === NONE ? undefined : projectId
    const pinned = attempt.ensure(`${trimmed}\u0000${selectedProject ?? ''}`)

    const outcome = await createTask
      .mutateAsync({
        commandId: pinned.commandId,
        taskId: pinned.taskId,
        title: trimmed,
        ...(selectedProject ? { projectId: selectedProject } : {}),
      })
      .catch(() => null)

    if (!outcome) return
    attempt.clear()
    await queryClient.refetchQueries({ queryKey: queryKeys.snapshot() })
    toast({ tone: 'success', title: 'Task created', description: outcome.task.title })
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={(next) => (submitting ? undefined : onOpenChange(next))}>
      <DialogContent
        title="New task"
        description="Capture it now. You can add details, a deadline or a checklist from the task itself."
        size="md"
      >
        <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-4">
          <Field
            label="Title"
            required
            error={fieldError ?? undefined}
            hint="Short and specific: what will be true when this is done?"
          >
            <Input
              ref={titleRef}
              autoFocus
              value={title}
              onChange={(event) => {
                setTitle(event.target.value)
                if (fieldError) setFieldError(null)
              }}
              placeholder="Draft the release notes"
              disabled={submitting}
              maxLength={280}
            />
          </Field>

          <LabeledSelect
            label="Project"
            hint="Leave it unassigned to send the task to the Inbox."
            ariaLabel="Project for the new task"
            value={projectId}
            onValueChange={setProjectId}
            disabled={submitting}
            options={[
              { value: NONE, label: 'No project (Inbox)' },
              ...projects.map((project) => ({ value: project.id, label: project.name })),
            ]}
          />

          {error ? <FormErrorBanner title="The task was not created" description={userMessage(error)} /> : null}
          {uncertain ? (
            <FormErrorBanner
              className="border-warning/40 bg-warning-soft text-warning-ink"
              title="The task may already exist"
              description={uncertainMessage(createTask.error ?? undefined)}
            />
          ) : null}

          <div className="mt-1 flex items-center justify-between gap-3">
            <p className="text-2xs text-ink-muted">
              Press <kbd className="command-key">Enter</kbd> to create
            </p>
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="ghost"
                onClick={() => onOpenChange(false)}
                disabled={submitting}
              >
                Cancel
              </Button>
              <Button type="submit" loading={submitting}>
                {submitting ? 'Creating…' : 'Create task'}
              </Button>
            </div>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
