import { useEffect, useRef, useState, type FormEvent } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { queryKeys, useCreateTask, useProjects, userMessage, type ApiError } from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { AlertCircleIcon } from '@/components/ui/icons'
import { useToast } from '@/components/ui/toast'
import { cn } from '@/components/ui/utils'

const NONE = '__none__'

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

  useEffect(() => {
    if (!open) return
    setTitle('')
    setProjectId(defaultProjectId ?? NONE)
    setFieldError(null)
    createTask.reset()
    // Only reset when the dialog opens; `createTask` is a mutation object whose
    // identity changes every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, defaultProjectId])

  const error: ApiError | null = createTask.disposition === 'rejected' ? createTask.error : null
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

    const outcome = await createTask
      .mutateAsync({
        title: trimmed,
        ...(projectId === NONE ? {} : { projectId }),
      })
      .catch(() => null)

    if (!outcome) return
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

          <Field label="Project" hint="Leave it unassigned to send the task to the Inbox.">
            <Select value={projectId} onValueChange={setProjectId} disabled={submitting}>
              <SelectTrigger aria-label="Project for the new task">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NONE}>No project (Inbox)</SelectItem>
                {projects.map((project) => (
                  <SelectItem key={project.id} value={project.id}>
                    {project.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>

          {error ? (
            <div
              role="alert"
              className={cn(
                'flex items-start gap-2 rounded-md border border-danger/35 bg-danger-soft px-3 py-2.5 text-danger-ink',
              )}
            >
              <AlertCircleIcon size={15} className="mt-0.5 shrink-0" />
              <div className="min-w-0">
                <p className="text-sm font-semibold leading-5">The task was not created</p>
                <p className="mt-0.5 text-xs leading-5">{userMessage(error)}</p>
              </div>
            </div>
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
