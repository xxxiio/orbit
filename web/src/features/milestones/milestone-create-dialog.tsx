import { useEffect, useRef, useState, type FormEvent } from 'react'
import { newCommandId, useMilestoneCommand, userMessage } from '@/hooks'
import type { Milestone } from '@/api/adapters'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { AlertCircleIcon } from '@/components/ui/icons'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { useToast } from '@/components/ui/toast'
import { MilestoneFormFields } from './milestone-form-fields'
import {
  emptyMilestoneDraft,
  validateMilestoneDraft,
  type MilestoneFormDraft,
} from './milestone-model'

/**
 * Create one milestone with its full description.
 *
 * Both identities are minted for the attempt and reused while the payload is
 * unchanged, so a retry after an unconfirmed failure replays the same receipt
 * instead of creating a duplicate. Editing any field starts a new attempt. The
 * server starts the milestone as `planned` and owns every field rule.
 */
export function MilestoneCreateDialog({
  open,
  onOpenChange,
  projectId,
  projectName,
  onCreated,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectId: string
  projectName?: string
  onCreated?: (milestone: Milestone | null) => void
}) {
  const command = useMilestoneCommand()
  const { toast } = useToast()
  const [draft, setDraft] = useState<MilestoneFormDraft>(emptyMilestoneDraft)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const attempt = useRef<{ commandId: string; milestoneId: string } | null>(null)

  useEffect(() => {
    if (!open) {
      setDraft(emptyMilestoneDraft())
      setError(null)
      setSubmitting(false)
      attempt.current = null
      command.reset()
    }
    // Reset only when the dialog opens/closes, not on every command render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const changeDraft = (next: MilestoneFormDraft) => {
    setDraft(next)
    // The retry key belongs to one payload. A changed draft is a new attempt.
    attempt.current = null
    if (error) setError(null)
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const invalid = validateMilestoneDraft(draft)
    if (invalid) {
      setError(invalid)
      return
    }
    attempt.current ??= { commandId: newCommandId(), milestoneId: newCommandId() }
    setSubmitting(true)
    setError(null)
    try {
      const outcome = await command.mutateAsync({
        commandId: attempt.current.commandId,
        input: {
          type: 'create',
          projectId,
          milestoneId: attempt.current.milestoneId,
          name: draft.name.trim(),
          description: draft.description,
          priority: draft.priority,
          owner: draft.owner?.trim() || null,
          startAtMs: draft.startAtMs,
          targetAtMs: draft.targetAtMs,
        },
      })
      toast({ tone: 'success', title: 'Milestone created' })
      onCreated?.(outcome.milestone)
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
          <MilestoneFormFields draft={draft} onChange={changeDraft} disabled={submitting} autoFocus />

          <Field
            label="Status"
            hint="A new milestone starts as planned. Move it after creation."
          >
            <Select value="planned" disabled>
              <SelectTrigger aria-label="Milestone status">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="planned">Planned</SelectItem>
              </SelectContent>
            </Select>
          </Field>

          <p className="text-2xs leading-4 text-ink-muted">
            Progress is counted from the tasks and Work Items you assign. This checkpoint does not
            invent dates: a target date is optional.
          </p>

          {error ? (
            <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              {error}
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
