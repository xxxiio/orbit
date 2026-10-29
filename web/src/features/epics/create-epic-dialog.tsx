import { useEffect, useRef, useState, type FormEvent } from 'react'
import { newCommandId, useEpicCommand, userMessage } from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { AlertCircleIcon, PlusIcon } from '@/components/ui/icons'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { useToast } from '@/components/ui/toast'
import { dateInputValue, parseDateInput } from './epic-model'
import type { EpicDraft } from '@/api/endpoints/epics'

export const EPIC_PRIORITY_OPTIONS = [
  { value: 'none', label: 'No priority' },
  { value: 'low', label: 'Low' },
  { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' },
  { value: 'urgent', label: 'Urgent' },
] as const

export function EpicDraftFields({
  draft,
  onChange,
  disabled,
}: {
  draft: EpicDraft
  onChange: (next: EpicDraft) => void
  disabled?: boolean
}) {
  const patch = (next: Partial<EpicDraft>) => onChange({ ...draft, ...next })
  return (
    <>
      <Field label="Title" required>
        <Input
          value={draft.title}
          onChange={(event) => patch({ title: event.target.value })}
          placeholder="e.g. Professional web experience"
          autoFocus
          disabled={disabled}
        />
      </Field>

      <Field label="Description" hint="What this body of work delivers.">
        <Textarea
          value={draft.description}
          onChange={(event) => patch({ description: event.target.value })}
          rows={3}
          placeholder="Optional summary for the team."
          disabled={disabled}
        />
      </Field>

      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Priority">
          <Select
            value={draft.priority ?? 'none'}
            onValueChange={(value) => patch({ priority: value === 'none' ? null : value })}
            disabled={disabled}
          >
            <SelectTrigger aria-label="Epic priority">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {EPIC_PRIORITY_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field label="Owner" hint="Optional; a name, not an account.">
          <Input
            value={draft.owner ?? ''}
            onChange={(event) => patch({ owner: event.target.value || null })}
            placeholder="e.g. Alex"
            disabled={disabled}
          />
        </Field>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Start date">
          <Input
            type="date"
            value={dateInputValue(draft.startAtMs)}
            onChange={(event) => patch({ startAtMs: parseDateInput(event.target.value) })}
            disabled={disabled}
          />
        </Field>
        <Field label="Target date" hint="A target, not a promise.">
          <Input
            type="date"
            value={dateInputValue(draft.targetAtMs)}
            onChange={(event) => patch({ targetAtMs: parseDateInput(event.target.value) })}
            disabled={disabled}
          />
        </Field>
      </div>
    </>
  )
}

export function emptyEpicDraft(): EpicDraft {
  return {
    title: '',
    description: '',
    priority: null,
    owner: null,
    startAtMs: null,
    targetAtMs: null,
  }
}

/**
 * Create one epic. Both identities are minted for the attempt and reused on a
 * retry, so an unconfirmed failure cannot create a second epic.
 */
export function CreateEpicDialog({
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
  const command = useEpicCommand()
  const { toast } = useToast()
  const [draft, setDraft] = useState<EpicDraft>(emptyEpicDraft)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const attempt = useRef<{ commandId: string; epicId: string } | null>(null)

  useEffect(() => {
    if (!open) {
      setDraft(emptyEpicDraft())
      setError(null)
      setSubmitting(false)
      attempt.current = null
      command.reset()
    }
    // Reset on open/close only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (draft.title.trim().length === 0) {
      setError('Give the epic a title.')
      return
    }
    if (
      draft.startAtMs !== null &&
      draft.targetAtMs !== null &&
      draft.startAtMs > draft.targetAtMs
    ) {
      setError('The start date must be on or before the target date.')
      return
    }
    attempt.current ??= { commandId: newCommandId(), epicId: newCommandId() }
    setSubmitting(true)
    setError(null)
    try {
      await command.mutateAsync({
        commandId: attempt.current.commandId,
        input: {
          type: 'create',
          projectId,
          epicId: attempt.current.epicId,
          draft,
        },
      })
      toast({ tone: 'success', title: 'Epic created' })
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
        title="New epic"
        description={
          projectName
            ? `Group related work in ${projectName}. A milestone is a checkpoint; an epic is a body of work.`
            : 'Group related work. A milestone is a checkpoint; an epic is a body of work.'
        }
      >
        <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
          <EpicDraftFields draft={draft} onChange={setDraft} disabled={submitting} />
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
            <Button type="submit" loading={submitting}>
              Create epic
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function NewEpicButton({
  projectId,
  projectName,
  size = 'sm',
  variant = 'secondary',
}: {
  projectId: string
  projectName?: string
  size?: 'sm' | 'md'
  variant?: 'primary' | 'secondary'
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        variant={variant}
        size={size}
        leadingIcon={<PlusIcon size={15} />}
        onClick={() => setOpen(true)}
      >
        New epic
      </Button>
      <CreateEpicDialog
        open={open}
        onOpenChange={setOpen}
        projectId={projectId}
        projectName={projectName}
      />
    </>
  )
}
