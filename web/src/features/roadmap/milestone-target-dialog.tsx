import { useEffect, useState, type FormEvent } from 'react'
import type { Milestone } from '@/api/adapters'
import { newCommandId, useMilestoneCommand, userMessage } from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { AlertCircleIcon } from '@/components/ui/icons'
import { useToast } from '@/components/ui/toast'
import { dateInputValue, parseDateInput } from '@/features/epics'

/**
 * A milestone is a checkpoint, so it has one target date and no duration. The
 * date can be cleared; the milestone then appears in the unscheduled list.
 */
export function MilestoneTargetDialog({
  milestone,
  open,
  onOpenChange,
}: {
  milestone: Milestone | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const command = useMilestoneCommand()
  const { toast } = useToast()
  const [value, setValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (milestone) {
      setValue(dateInputValue(milestone.targetAtMs ?? null))
      setError(null)
    }
  }, [milestone?.id, milestone?.targetAtMs])

  if (!milestone) return null

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (value.length > 0 && parseDateInput(value) === null) {
      setError('Enter a valid date, or clear it.')
      return
    }
    setSaving(true)
    setError(null)
    try {
      await command.mutateAsync({
        commandId: newCommandId(),
        input: {
          type: 'set_target',
          milestoneId: milestone.id,
          targetAtMs: value.length > 0 ? parseDateInput(value) : null,
        },
      })
      toast({ tone: 'success', title: 'Milestone target updated' })
      onOpenChange(false)
    } catch (failure) {
      setError(userMessage(failure))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={milestone.name}
        description="A milestone marks a checkpoint. It has a target date, not a duration."
      >
        <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
          <Field label="Target date" hint="Leave empty to return it to the unscheduled list.">
            <Input
              type="date"
              value={value}
              onChange={(event) => {
                setValue(event.target.value)
                if (error) setError(null)
              }}
              autoFocus
            />
          </Field>
          {error ? (
            <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              {error}
            </p>
          ) : null}
          <div className="flex items-center justify-end gap-2">
            <Button variant="secondary" type="button" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" loading={saving}>
              Save target
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
