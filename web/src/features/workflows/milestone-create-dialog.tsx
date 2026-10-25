import { useEffect, useRef, useState, type FormEvent } from 'react'
import { newCommandId, useMilestoneCommand, userMessage } from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { AlertCircleIcon, PlusIcon } from '@/components/ui/icons'
import { useToast } from '@/components/ui/toast'

/**
 * Create one milestone through the milestone command endpoint.
 *
 * Both identities are minted for the attempt (never inside the mutation), so a
 * second click after an unconfirmed failure replays the same receipt instead of
 * creating a duplicate milestone. The server owns name validation and the
 * initial status.
 */
export function MilestoneCreateDialog({
  open,
  onOpenChange,
  projectId,
  projectName,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectId: string
  projectName?: string
}) {
  const command = useMilestoneCommand()
  const { toast } = useToast()
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const attempt = useRef<{ commandId: string; milestoneId: string } | null>(null)

  useEffect(() => {
    if (!open) {
      setName('')
      setError(null)
      setSubmitting(false)
      attempt.current = null
      command.reset()
    }
    // Reset only when the dialog opens/closes, not on every command render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const trimmed = name.trim()
    if (trimmed.length === 0) {
      setError('Give the milestone a name.')
      return
    }
    attempt.current ??= { commandId: newCommandId(), milestoneId: newCommandId() }
    setSubmitting(true)
    setError(null)
    try {
      await command.mutateAsync({
        commandId: attempt.current.commandId,
        input: {
          type: 'create',
          projectId,
          name: trimmed,
          milestoneId: attempt.current.milestoneId,
        },
      })
      toast({ tone: 'success', title: 'Milestone created' })
      onOpenChange(false)
    } catch (failure) {
      setError(userMessage(failure))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="New milestone"
        description={
          projectName
            ? `Add a checkpoint to ${projectName}. The server starts it as planned.`
            : 'Add a checkpoint to this project. The server starts it as planned.'
        }
      >
        <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
          <Field label="Milestone name" error={error ?? undefined} required>
            <Input
              value={name}
              onChange={(event) => {
                setName(event.target.value)
                if (error) setError(null)
              }}
              placeholder="e.g. First review"
              autoFocus
              aria-invalid={error ? true : undefined}
            />
          </Field>

          <p className="text-2xs leading-4 text-ink-muted">
            Milestones have a lifecycle and attached tasks, but no dates in this API yet. Progress is
            counted from the tasks you assign.
          </p>

          {command.disposition === 'uncertain' && command.error ? (
            <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-warning-ink">
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              {userMessage(command.error)}
            </p>
          ) : null}

          <div className="flex items-center justify-end gap-2">
            <Button variant="secondary" size="md" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" size="md" loading={submitting}>
              Create milestone
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** The header action a milestone-bearing view offers. */
export function NewMilestoneButton({
  projectId,
  projectName,
  size = 'sm',
}: {
  projectId: string
  projectName?: string
  size?: 'sm' | 'md'
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button size={size} leadingIcon={<PlusIcon size={15} />} onClick={() => setOpen(true)}>
        New milestone
      </Button>
      <MilestoneCreateDialog
        open={open}
        onOpenChange={setOpen}
        projectId={projectId}
        projectName={projectName}
      />
    </>
  )
}
