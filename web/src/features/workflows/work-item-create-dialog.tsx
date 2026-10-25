/**
 * Create a work item from a declared kind.
 *
 * The kind comes from the pinned view when it names one; otherwise the person
 * chooses. Controls are typed from the kind's `Field` declarations, and every
 * required field is validated before anything is sent. The server owns the
 * initial state and the property rules; the dialog never mints a state.
 */
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import type { WorkItem, WorkItemField, Workflow } from '@/api/adapters'
import {
  uncertainMessage,
  useCreateWorkItem,
  userMessage,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { PlusIcon } from '@/components/ui/icons'
import { Switch } from '@/components/ui/switch'
import { useToast } from '@/components/ui/toast'
import {
  FormErrorBanner,
  InlineFieldError,
  LabeledSelect,
} from '@/components/forms'
import { cn } from '@/components/ui/utils'
import {
  fieldLabel,
  kindFor,
  kindLabel,
  stateLabel,
} from './work-item-model'
import { writeAcceptedWorkItem } from './work-item-cache'

interface FieldDraft {
  text: Record<string, string>
  integer: Record<string, string>
  boolean: Record<string, boolean | undefined>
  choice: Record<string, string>
}

function initialDraft(kindId: string | undefined, workflow: Workflow | undefined): FieldDraft {
  const draft: FieldDraft = { text: {}, integer: {}, boolean: {}, choice: {} }
  const kind = kindFor(workflow, kindId)
  for (const field of kind?.fields ?? []) {
    const value = field.default
    if (field.kind.type === 'text') draft.text[field.id] = typeof value === 'string' ? value : ''
    else if (field.kind.type === 'integer') {
      draft.integer[field.id] = typeof value === 'number' ? String(value) : ''
    } else if (field.kind.type === 'boolean') {
      draft.boolean[field.id] = typeof value === 'boolean' ? value : undefined
    } else {
      draft.choice[field.id] = typeof value === 'string' ? value : ''
    }
  }
  return draft
}

function validate(
  kindId: string | undefined,
  workflow: Workflow | undefined,
  draft: FieldDraft,
): { errors: Record<string, string>; properties: Record<string, unknown> } {
  const kind = kindFor(workflow, kindId)
  const errors: Record<string, string> = {}
  const properties: Record<string, unknown> = {}
  for (const field of kind?.fields ?? []) {
    const label = fieldLabel(field, field.id)
    if (field.kind.type === 'text') {
      const value = draft.text[field.id]?.trim() ?? ''
      if (field.required && value.length === 0) {
        errors[field.id] = `${label} is required.`
        continue
      }
      if (value.length > 0) properties[field.id] = value
      continue
    }
    if (field.kind.type === 'integer') {
      const raw = draft.integer[field.id]?.trim() ?? ''
      if (raw.length === 0) {
        if (field.required) errors[field.id] = `${label} is required.`
        continue
      }
      if (!/^-?\d+$/.test(raw)) {
        errors[field.id] = `${label} must be a whole number.`
        continue
      }
      properties[field.id] = Number(raw)
      continue
    }
    if (field.kind.type === 'boolean') {
      const value = draft.boolean[field.id]
      if (value === undefined) {
        if (field.required) errors[field.id] = `Choose yes or no for ${label.toLowerCase()}.`
        continue
      }
      properties[field.id] = value
      continue
    }
    const value = draft.choice[field.id] ?? ''
    if (value.length === 0) {
      if (field.required) errors[field.id] = `${label} is required.`
      continue
    }
    properties[field.id] = value
  }
  return { errors, properties }
}

function DraftField({
  field,
  draft,
  setDraft,
  error,
  disabled,
}: {
  field: WorkItemField
  draft: FieldDraft
  setDraft: (update: (current: FieldDraft) => FieldDraft) => void
  error: string | undefined
  disabled: boolean
}) {
  const label = fieldLabel(field, field.id)
  if (field.kind.type === 'text') {
    return (
      <Field label={label} required={field.required} error={error} hint={field.required ? undefined : 'Optional.'}>
        <Input
          value={draft.text[field.id] ?? ''}
          disabled={disabled}
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              text: { ...current.text, [field.id]: event.target.value },
            }))
          }
          placeholder={`Add ${label.toLowerCase()}`}
        />
      </Field>
    )
  }
  if (field.kind.type === 'integer') {
    return (
      <Field
        label={label}
        required={field.required}
        error={error}
        hint={field.required ? 'Whole numbers only.' : 'Optional. Whole numbers only.'}
      >
        <Input
          type="number"
          inputMode="numeric"
          step={1}
          value={draft.integer[field.id] ?? ''}
          disabled={disabled}
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              integer: { ...current.integer, [field.id]: event.target.value },
            }))
          }
        />
      </Field>
    )
  }
  if (field.kind.type === 'boolean') {
    return (
      <Field label={label} required={field.required} error={error}>
        <div className="flex items-center gap-2">
          <Switch
            checked={draft.boolean[field.id] === true}
            disabled={disabled}
            onCheckedChange={(checked) =>
              setDraft((current) => ({
                ...current,
                boolean: { ...current.boolean, [field.id]: checked },
              }))
            }
            aria-label={label}
          />
          <span className="text-sm text-ink-soft">
            {draft.boolean[field.id] === undefined
              ? 'Not chosen'
              : draft.boolean[field.id]
                ? 'Yes'
                : 'No'}
          </span>
        </div>
      </Field>
    )
  }
  return (
    <LabeledSelect
      label={label}
      required={field.required}
      error={error}
      value={draft.choice[field.id] ?? ''}
      placeholder="Choose one"
      disabled={disabled}
      options={field.kind.options.map((option) => ({
        value: option,
        label: stateLabel(option),
      }))}
      onValueChange={(value) =>
        setDraft((current) => ({
          ...current,
          choice: { ...current.choice, [field.id]: value },
        }))
      }
    />
  )
}

export interface NewWorkItemButtonProps {
  projectId: string
  workflow: Workflow | undefined
  /** The kind the current view pins, if it pins one. */
  pinnedKind?: string | null
  /** A preselected kind when the board is showing a chosen kind (not pinned). */
  initialKind?: string | null
  /** Never send an empty kind list into the dialog. */
  variant?: 'primary' | 'secondary' | 'quiet'
  size?: 'sm' | 'md'
  label?: string
  className?: string
  onCreated?: (item: WorkItem) => void
}

export function NewWorkItemButton({
  projectId,
  workflow,
  pinnedKind = null,
  initialKind = null,
  variant = 'secondary',
  size = 'sm',
  label = 'New work item',
  className,
  onCreated,
}: NewWorkItemButtonProps) {
  const [open, setOpen] = useState(false)
  const kinds = workflow?.kinds ?? []
  const disabled = kinds.length === 0

  return (
    <>
      <Button
        variant={variant}
        size={size}
        className={className}
        leadingIcon={<PlusIcon size={15} />}
        disabled={disabled}
        title={
          disabled
            ? 'The pinned workflow is still loading or declares no work-item kinds.'
            : undefined
        }
        onClick={() => setOpen(true)}
      >
        {label}
      </Button>
      {open ? (
        <NewWorkItemDialog
          projectId={projectId}
          workflow={workflow}
          pinnedKind={pinnedKind}
          initialKind={initialKind}
          onClose={() => setOpen(false)}
          onCreated={onCreated}
        />
      ) : null}
    </>
  )
}

export function NewWorkItemDialog({
  projectId,
  workflow,
  pinnedKind,
  initialKind = null,
  onClose,
  onCreated,
}: {
  projectId: string
  workflow: Workflow | undefined
  pinnedKind: string | null
  initialKind?: string | null
  onClose: () => void
  onCreated?: (item: WorkItem) => void
}) {
  const create = useCreateWorkItem()
  const queryClient = useQueryClient()
  const { toast } = useToast()

  const kinds = workflow?.kinds ?? []
  const pinned = pinnedKind ? kindFor(workflow, pinnedKind) : undefined
  const [kindId, setKindId] = useState<string>(
    pinned?.id ?? kindFor(workflow, initialKind ?? undefined)?.id ?? kinds[0]?.id ?? '',
  )
  const kind = kindFor(workflow, kindId)
  const [draft, setDraft] = useState<FieldDraft>(() => initialDraft(kindId, workflow))
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [submitted, setSubmitted] = useState(false)
  const firstFieldRef = useRef<HTMLDivElement>(null)

  // Choosing another kind is a different schema; drafts must not leak across.
  useEffect(() => {
    setDraft(initialDraft(kindId, workflow))
    setErrors({})
    setSubmitted(false)
  }, [kindId, workflow])

  const rejection = create.disposition === 'rejected' ? create.error : null
  const uncertain = create.disposition === 'uncertain'
  const submitting = create.isPending

  const nameOptions = useMemo(
    () =>
      kinds.map((candidate) => ({
        value: candidate.id,
        label: kindLabel(candidate, candidate.id),
      })),
    [kinds],
  )

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (submitting || !kind) return
    setSubmitted(true)
    const checked = validate(kind.id, workflow, draft)
    if (Object.keys(checked.errors).length > 0) {
      setErrors(checked.errors)
      firstFieldRef.current?.querySelector<HTMLElement>('input, button, [role="combobox"]')?.focus()
      return
    }
    setErrors({})
    try {
      const item = await create.mutateAsync({
        projectId,
        kind: kind.id,
        properties: checked.properties,
      })
      writeAcceptedWorkItem(queryClient, item)
      toast({
        tone: 'success',
        title: 'Work item created',
        description: `${kindLabel(kind, kind.id)} · ${stateLabel(item.state)}`,
      })
      onCreated?.(item)
      onClose()
    } catch {
      // The form shows `create.error`; nothing was written optimistically.
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next && !submitting) onClose()
      }}
    >
      <DialogContent
        title="New work item"
        description="Fields come from the workflow's declaration. The server assigns the initial state."
        size="md"
      >
        <form onSubmit={submit} noValidate className="flex flex-col gap-4">
          {pinned ? (
            <div className="flex items-center gap-2 rounded-md border border-line bg-surface-sunken/40 px-3 py-2">
              <span className="text-xs text-ink-muted">Kind</span>
              <span className="text-sm font-medium text-ink">{kindLabel(pinned, pinned.id)}</span>
              <span className="ml-auto text-2xs text-ink-faint">Pinned by this view</span>
            </div>
          ) : kinds.length > 1 ? (
            <LabeledSelect
              label="Kind"
              required
              value={kindId}
              options={nameOptions}
              onValueChange={setKindId}
              hint="Each kind has its own fields and states."
            />
          ) : (
            <div className="flex items-center gap-2 rounded-md border border-line bg-surface-sunken/40 px-3 py-2">
              <span className="text-xs text-ink-muted">Kind</span>
              <span className="text-sm font-medium text-ink">
                {kind ? kindLabel(kind, kind.id) : 'No kind declared'}
              </span>
            </div>
          )}

          {kind ? (
            <div ref={firstFieldRef} className="flex flex-col gap-3">
              {kind.fields.length === 0 ? (
                <p className="text-xs leading-5 text-ink-muted">
                  This kind declares no fields. The item starts in {stateLabel(kind.initialState)}.
                </p>
              ) : (
                kind.fields.map((field) => (
                  <DraftField
                    key={field.id}
                    field={field}
                    draft={draft}
                    setDraft={setDraft}
                    error={submitted ? errors[field.id] : undefined}
                    disabled={submitting || uncertain}
                  />
                ))
              )}
            </div>
          ) : (
            <p className="text-xs leading-5 text-danger-ink">
              The pinned workflow declares no work-item kind this view can add.
            </p>
          )}

          {rejection ? (
            <FormErrorBanner
              title="The server refused this work item"
              description={userMessage(rejection)}
            />
          ) : null}
          {uncertain ? (
            <p
              role="alert"
              className={cn(
                'rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink',
              )}
            >
              {uncertainMessage(create.error ?? undefined)} Check the board before creating this
              item again: the first request may have been stored.
            </p>
          ) : null}
          <InlineFieldError
            error={
              submitted && Object.keys(errors).length > 0
                ? 'Fill the required fields before creating this work item.'
                : null
            }
          />

          <div className="flex items-center justify-end gap-2">
            <Button variant="secondary" size="md" onClick={onClose} disabled={submitting}>
              Cancel
            </Button>
            <Button
              type="submit"
              size="md"
              loading={submitting}
              disabled={!kind || uncertain}
              title={uncertain ? 'Check the board before creating this item again.' : undefined}
            >
              Create work item
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
