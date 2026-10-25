/**
 * Create a work item from a declared kind.
 *
 * The kind comes from the pinned view when it names one; otherwise the person
 * chooses. Controls are typed from the kind's `Field` declarations, and every
 * required field is validated before anything is sent. The server owns the
 * initial state and the property rules; the dialog never mints a state.
 */
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import type { WorkItem, WorkItemField, Workflow } from '@/api/adapters'
import {
  newCommandId,
  uncertainMessage,
  useCreateWorkItem,
  userMessage,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { PlusIcon } from '@/components/ui/icons'
import { useToast } from '@/components/ui/toast'
import {
  FormErrorBanner,
  InlineFieldError,
  LabeledSelect,
} from '@/components/forms'
import { cn } from '@/components/ui/utils'
import { fieldLabel, kindFor, kindLabel, stateLabel } from './work-item-model'
import { PropertyEditor, type PropertyDraftValue } from './work-item-fields'

interface FieldDraft {
  text: Record<string, string>
  integer: Record<string, string>
  boolean: Record<string, boolean | undefined>
  choice: Record<string, string>
}

/**
 * One create attempt: the command id and the item id are minted when the person
 * submits (never inside `mutationFn`), and reused while the submitted payload
 * stays the same. Pressing Create again after an unconfirmed failure therefore
 * repeats the same item id — the server cannot store a second row under it —
 * instead of creating a duplicate. Editing the payload mints a new attempt,
 * because it is a different item.
 */
interface CreateWorkItemAttempt {
  key: string
  commandId: string
  itemId: string
}

/** The submitted payload, in the shape the attempt key is derived from. */
function payloadKey(kindId: string, properties: Record<string, unknown>): string {
  return `${kindId}\u0000${JSON.stringify(properties)}`
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

/** The draft record's value for one field, in the editor's own shape. */
function draftValue(field: WorkItemField, draft: FieldDraft): PropertyDraftValue {
  if (field.kind.type === 'text') return draft.text[field.id] ?? ''
  if (field.kind.type === 'integer') return draft.integer[field.id] ?? ''
  if (field.kind.type === 'boolean') return draft.boolean[field.id]
  return draft.choice[field.id] ?? ''
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
  const write = (next: string | boolean) =>
    setDraft((current) => {
      if (field.kind.type === 'boolean') {
        return {
          ...current,
          boolean: { ...current.boolean, [field.id]: next === true },
        }
      }
      const text = typeof next === 'string' ? next : ''
      if (field.kind.type === 'integer') {
        return { ...current, integer: { ...current.integer, [field.id]: text } }
      }
      if (field.kind.type === 'text') {
        return { ...current, text: { ...current.text, [field.id]: text } }
      }
      return { ...current, choice: { ...current.choice, [field.id]: text } }
    })

  return (
    <PropertyEditor
      mode="create"
      field={field}
      value={draftValue(field, draft)}
      onChange={write}
      required={field.required}
      error={error}
      disabled={disabled}
      showUnchosenState
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
  const attemptRef = useRef<CreateWorkItemAttempt | null>(null)

  // Choosing another kind is a different schema; drafts must not leak across.
  useEffect(() => {
    setDraft(initialDraft(kindId, workflow))
    setErrors({})
    setSubmitted(false)
  }, [kindId, workflow])

  const rejection = create.disposition === 'rejected' ? create.error : null
  const uncertain = create.disposition === 'uncertain'
  const submitting = create.isPending
  const pinnedId = create.pinnedId
  const pinnedAttempt = pinnedId !== null && attemptRef.current?.itemId === pinnedId
  const refusedButPinned = pinnedAttempt && rejection !== null

  // The payload on screen and the payload of the attempt whose outcome is
  // unknown. An unchanged retry is safe — it repeats the same ids, so the
  // server cannot store a second item. A changed payload would be a second,
  // blind create while the first may already exist, so it is not offered.
  //
  // The pin outlives a contradictory answer: a duplicate refusal followed by a
  // 404 read is still not proof that the earlier attempt can be abandoned.
  const currentKey = payloadKey(kindId, validate(kindId, workflow, draft).properties)
  const unconfirmedKey = create.variables
    ? payloadKey(create.variables.kind, create.variables.properties ?? {})
    : null
  const blindRetry = pinnedAttempt && unconfirmedKey !== null && unconfirmedKey !== currentKey

  function attemptFor(key: string): CreateWorkItemAttempt {
    if (attemptRef.current?.key !== key) {
      attemptRef.current = { key, commandId: newCommandId(), itemId: newCommandId() }
    }
    return attemptRef.current
  }

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
    if (submitting || blindRetry || !kind) return
    setSubmitted(true)
    const checked = validate(kind.id, workflow, draft)
    if (Object.keys(checked.errors).length > 0) {
      setErrors(checked.errors)
      firstFieldRef.current?.querySelector<HTMLElement>('input, button, [role="combobox"]')?.focus()
      return
    }
    setErrors({})
    const attempt = attemptFor(payloadKey(kind.id, checked.properties))
    try {
      const item = await create.mutateAsync({
        commandId: attempt.commandId,
        id: attempt.itemId,
        projectId,
        kind: kind.id,
        properties: checked.properties,
      })
      attemptRef.current = null
      toast({
        tone: 'success',
        title: 'Work item created',
        description: `${kindLabel(kind, kind.id)} · ${stateLabel(item.state)}`,
      })
      onCreated?.(item)
      onClose()
    } catch {
      // The form shows `create.error`; nothing was written optimistically. The
      // attempt is kept so the next submit of the same payload reuses its ids.
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
                    disabled={submitting}
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
              {uncertainMessage(create.error ?? undefined)}
              {blindRetry
                ? ''
                : ' Retrying with these fields repeats the same item id, so it cannot create a second item.'}
            </p>
          ) : null}
          {refusedButPinned ? (
            <p
              role="alert"
              data-pinned-item-id={pinnedId ?? undefined}
              className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
            >
              The server refused the last attempt, but this create still holds item id{' '}
              <span className="font-mono">{pinnedId}</span>. The earlier attempt is unresolved, so
              changing fields cannot mint a different item. Retry the same request, or close and
              check the board before creating something else.
            </p>
          ) : null}
          {blindRetry ? (
            <p
              role="alert"
              className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
            >
              These fields no longer match the request whose outcome is unknown. Retry the request
              unchanged, or close this dialog and check the board before creating a different item.
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
              disabled={!kind || blindRetry}
              title={
                blindRetry
                  ? 'Retry the unknown request unchanged, or check the board first.'
                  : undefined
              }
            >
              {pinnedAttempt && !blindRetry ? 'Retry the same request' : 'Create work item'}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
