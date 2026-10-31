import { useEffect, useRef, useState, type FormEvent } from 'react'
import type { Milestone } from '@/api/adapters'
import type { MilestoneStatus } from '@/api/contracts'
import { newCommandId, useMilestoneCommand, userMessage } from '@/hooks'
import { Badge } from '@/components/ui/badge'
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
import { milestoneStatusLabel } from '@/domain'
import { MilestoneFormFields } from './milestone-form-fields'
import {
  milestoneDraftFrom,
  milestoneNextStatuses,
  milestoneProgressLabel,
  toMilestoneDraft,
  validateMilestoneDraft,
  type MilestoneFormDraft,
} from './milestone-model'

/**
 * Edit a milestone's fields and move it through its legal next states.
 *
 * Fields save in one `update` command (the retry key belongs to that payload).
 * Status has its own command so a refused edge is quoted beside the status
 * control and leaves the stored value untouched.
 */
export function MilestoneEditDialog({
  milestone,
  open,
  onOpenChange,
  onSaved,
}: {
  milestone: Milestone | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onSaved?: (milestone: Milestone | null) => void
}) {
  const command = useMilestoneCommand()
  const { toast } = useToast()
  const [draft, setDraft] = useState<MilestoneFormDraft>(() =>
    milestone ? milestoneDraftFrom(milestone) : { name: '', description: '', priority: null, owner: null, startAtMs: null, targetAtMs: null },
  )
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const attempt = useRef<{ commandId: string } | null>(null)

  useEffect(() => {
    if (open && milestone) setDraft(milestoneDraftFrom(milestone))
    setError(null)
    setSaving(false)
    attempt.current = null
    // Reset when a different milestone is opened, never on a status answer
    // that would discard an unsaved field edit.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [milestone?.id, open])

  useEffect(() => {
    if (!open) {
      setError(null)
      setSaving(false)
      attempt.current = null
      command.reset()
    }
    // Reset only when the dialog opens/closes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  if (!milestone) return null

  // A status answer names the same milestone; show that accepted record rather
  // than waiting for the milestones read to settle.
  const live = command.data?.milestone?.id === milestone.id ? command.data.milestone : milestone
  const current = milestoneDraftFrom(live)
  const dirty =
    draft.name !== current.name ||
    draft.description !== current.description ||
    draft.priority !== current.priority ||
    draft.owner !== current.owner ||
    draft.startAtMs !== current.startAtMs ||
    draft.targetAtMs !== current.targetAtMs

  const nextStatuses = milestoneNextStatuses(live.status)

  const changeDraft = (next: MilestoneFormDraft) => {
    setDraft(next)
    attempt.current = null
    if (error) setError(null)
  }

  const save = async (event?: FormEvent) => {
    event?.preventDefault()
    const invalid = validateMilestoneDraft(draft)
    if (invalid) {
      setError(invalid)
      return
    }
    attempt.current ??= { commandId: newCommandId() }
    setSaving(true)
    setError(null)
    try {
      const outcome = await command.mutateAsync({
        commandId: attempt.current.commandId,
        input: { type: 'update', milestoneId: milestone.id, draft: toMilestoneDraft(draft) },
      })
      toast({ tone: 'success', title: 'Milestone updated' })
      onSaved?.(outcome.milestone)
      onOpenChange(false)
    } catch (failure) {
      setError(userMessage(failure))
    } finally {
      setSaving(false)
    }
  }

  const changeStatus = async (status: MilestoneStatus) => {
    setError(null)
    try {
      await command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'change_status', milestoneId: milestone.id, status },
      })
    } catch (failure) {
      setError(userMessage(failure))
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Edit milestone"
        description={`Milestone · ${milestoneProgressLabel(live)}`}
      >
        <form onSubmit={(event) => void save(event)} className="flex flex-col gap-4" noValidate>
          <MilestoneFormFields
            draft={draft}
            onChange={changeDraft}
            disabled={saving || command.isPending}
            autoFocus
          />

          <Field
            label="Status"
            hint={
              nextStatuses.length === 0
                ? 'This milestone is in a terminal state; it cannot move again.'
                : 'Only the next states the server allows are offered.'
            }
          >
            <div className="flex flex-wrap items-center gap-2">
              <Select
                value={live.status}
                onValueChange={(value) => void changeStatus(value as MilestoneStatus)}
                disabled={command.isPending || nextStatuses.length === 0}
              >
                <SelectTrigger className="w-[11rem]" aria-label="Milestone status">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={live.status}>
                    {milestoneStatusLabel(live.status)}
                  </SelectItem>
                  {nextStatuses.map((status) => (
                    <SelectItem key={status} value={status}>
                      {milestoneStatusLabel(status)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Badge tone="neutral" dot>
                {milestoneStatusLabel(live.status)}
              </Badge>
            </div>
          </Field>

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
            <Button type="submit" size="md" loading={saving} disabled={!dirty || command.isPending}>
              Save changes
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
