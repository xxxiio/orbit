/**
 * The request form for one offered action.
 *
 * The fields come from the action's own `input_schema`, never from a guess about
 * what the action needs; the values are validated here only so an obviously
 * refused request is not sent, and the server validates them again.
 *
 * An action that changes stored values, acts outside Orbit, or is declared with
 * `external_effect` gets an explicit confirmation step in a focus-trapped
 * dialog. That step states what will happen and that an **executor** runs it:
 * this browser records an intent and nothing else.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import type { CapabilityOffer, WorkItem } from '@/api/adapters'
import {
  newCommandId,
  useIdempotentAttempt,
  useRequestCapabilityAction,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { AlertCircleIcon, LoaderIcon } from '@/components/ui/icons'
import { LabeledSelect } from '@/components/forms/labeled-select'
import { cn } from '@/components/ui/utils'
import { humanizeIdentifier } from '@/lib/humanize'
import {
  capabilityAttemptKey,
  capabilityConfirmation,
  capabilityInputsFromSchema,
  capabilityInputsPayload,
  capabilityRefusal,
  emptyCapabilityInputs,
  hasCapabilityErrors,
  requiresConfirmation,
  validateCapabilityInputs,
  type CapabilityFormErrors,
  type CapabilityFormValues,
  type CapabilityInputField,
} from './capability-model'

export interface CapabilityRequestFormProps {
  item: WorkItem
  offer: CapabilityOffer
  permission: string
  /** The revision whose offers the person is looking at. */
  itemRevision: number
  onCancel: () => void
  /** Called after the server records the intent. */
  onRecorded: (message: string) => void
}

export function CapabilityRequestForm({
  item,
  offer,
  permission,
  itemRevision,
  onCancel,
  onRecorded,
}: CapabilityRequestFormProps) {
  const fields = useMemo(() => capabilityInputsFromSchema(offer.inputSchema), [offer.inputSchema])
  const [values, setValues] = useState<CapabilityFormValues>(() => emptyCapabilityInputs(fields))
  const [errors, setErrors] = useState<CapabilityFormErrors>({})
  const [step, setStep] = useState<'form' | 'confirm'>('form')
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const request = useRequestCapabilityAction()
  const attempt = useIdempotentAttempt<{ key: string; commandId: string }>((key) => ({
    key,
    commandId: newCommandId(),
  }))

  const actionLabel = humanizeIdentifier(offer.actionId)
  const needsConfirmation = requiresConfirmation(offer.externalEffect, permission)
  const confirmation = capabilityConfirmation({
    actionLabel,
    externalEffect: offer.externalEffect,
    permission,
  })
  const pending = request.isPending
  const failure: ApiError | null = request.error
  const refusal = failure ? capabilityRefusal(failure) : null

  const formRef = useRef<HTMLFormElement>(null)
  const submitRef = useRef<HTMLButtonElement>(null)
  const confirmPrimaryRef = useRef<HTMLButtonElement>(null)
  const firstRender = useRef(true)

  // Opening the form replaces the trigger with the fields; put focus on the
  // first control rather than dropping it on the sheet body.
  useEffect(() => {
    const first =
      formRef.current?.querySelector<HTMLElement>(
        'input:not([type="checkbox"]), textarea, [role="combobox"]',
      ) ?? submitRef.current
    first?.focus({ preventScroll: true })
  }, [])

  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false
      return
    }
    if (step === 'confirm') confirmPrimaryRef.current?.focus()
    else submitRef.current?.focus()
  }, [step])

  const patch = (name: string, value: string | boolean) => {
    setValues((current) => ({ ...current, [name]: value }))
    setErrors((current) => {
      if (!(name in current)) return current
      const next = { ...current }
      delete next[name]
      return next
    })
    // A refusal belongs to the payload that was sent; editing starts a new one,
    // so the old answer must not sit beside the new form.
    if (request.error) request.reset()
    setAnnouncement(null)
  }

  const focusField = (name: string) => {
    const control = formRef.current?.querySelector<HTMLElement>(
      `[data-capability-input="${name}"] input, [data-capability-input="${name}"] textarea, [data-capability-input="${name}"] [role="combobox"]`,
    )
    control?.focus({ preventScroll: true })
  }

  /** Focus the first field that failed, so a keyboard user is taken to it. */
  const showErrors = (nextErrors: CapabilityFormErrors) => {
    setErrors(nextErrors)
    setAnnouncement('Some inputs need attention before this can be sent.')
    const first = fields.find((field) => nextErrors[field.name] !== undefined)
    if (first) {
      // After the error is painted, so the field can announce it.
      window.setTimeout(() => focusField(first.name), 0)
    }
  }

  const review = () => {
    const nextErrors = validateCapabilityInputs(fields, values)
    if (hasCapabilityErrors(nextErrors)) {
      showErrors(nextErrors)
      return
    }
    setErrors({})
    setAnnouncement(null)
    setStep('confirm')
  }

  const submit = async () => {
    if (pending) return
    // The executor can lose its announcement while the confirmation is open, so
    // re-check the browser-side rules before sending.
    const nextErrors = validateCapabilityInputs(fields, values)
    if (hasCapabilityErrors(nextErrors)) {
      setStep('form')
      showErrors(nextErrors)
      return
    }
    const inputs = capabilityInputsPayload(fields, values)
    // Minted once for this payload and kept: pressing the button again after a
    // lost answer replays the same intent instead of recording a second one.
    const pinned = attempt.ensure(capabilityAttemptKey(offer.actionId, itemRevision, inputs))
    setAnnouncement('Recording the request…')
    try {
      await request.mutateAsync({
        commandId: pinned.commandId,
        itemId: item.id,
        itemRevision,
        actionId: offer.actionId,
        inputs,
        confirm: needsConfirmation,
      })
      attempt.clear()
      setValues(emptyCapabilityInputs(fields))
      setErrors({})
      onRecorded(
        `${actionLabel} is recorded as a request. It waits for an executor to claim it.`,
      )
    } catch {
      // `request.error` carries the server's answer; the form stays open.
      setAnnouncement(null)
      setStep('form')
    }
  }

  const formId = `capability-form-${offer.actionId}`

  return (
    <>
      <form
        ref={formRef}
        id={formId}
        data-capability-request-form={offer.actionId}
        className="mt-3 flex flex-col gap-3 border-t border-line pt-3"
        onSubmit={(event) => {
          event.preventDefault()
          if (needsConfirmation) review()
          else void submit()
        }}
      >
        {fields.length > 0 ? (
          <div className="grid gap-3 sm:grid-cols-2">
            {fields.map((field) => (
              <CapabilityInputControl
                key={field.name}
                field={field}
                actionId={offer.actionId}
                value={values[field.name] ?? (field.kind === 'boolean' ? false : '')}
                error={errors[field.name]}
                disabled={pending}
                onChange={(next) => patch(field.name, next)}
              />
            ))}
          </div>
        ) : (
          <p className="text-xs leading-5 text-ink-muted">
            This action declares no inputs. Its request carries none.
          </p>
        )}

        {refusal ? (
          <div
            role="alert"
            data-capability-refusal=""
            className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
          >
            <p className="flex items-start gap-1.5 font-medium">
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              <span>{refusal.title}</span>
            </p>
            <p className="mt-1 pl-[1.375rem] break-words">{refusal.detail}</p>
          </div>
        ) : null}

        <div className="flex flex-wrap items-center gap-2">
          <Button ref={submitRef} type="submit" size="sm" loading={pending} disabled={pending}>
            {needsConfirmation ? 'Review the request' : `Request ${actionLabel}`}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="quiet"
            onClick={onCancel}
            disabled={pending}
          >
            Cancel
          </Button>
          <p
            role="status"
            aria-live="polite"
            className="min-w-0 flex-1 text-2xs leading-4 text-ink-muted"
          >
            {pending ? (
              <span className="inline-flex items-center gap-1.5">
                <LoaderIcon size={11} className="animate-spin text-accent" />
                Recording the request…
              </span>
            ) : (
              (announcement ??
              (needsConfirmation
                ? 'You review this before it is sent.'
                : 'The server records the request; an executor runs the action.'))
            )}
          </p>
        </div>
      </form>

      {needsConfirmation ? (
        <Dialog
          open={step === 'confirm'}
          onOpenChange={(open) => {
            if (!open && !pending) setStep('form')
          }}
        >
          <DialogContent
            size="md"
            data-capability-confirm={offer.actionId}
            title={confirmation.title}
            description="Read what will be recorded, then confirm. Nothing runs here."
            className="max-h-[min(44rem,92dvh)] overflow-y-auto"
            onCloseAutoFocus={(event) => {
              // Radix returns focus to whatever opened the dialog. When the form
              // is still mounted that is its primary button; after a recorded
              // request both are gone and the panel takes over.
              if (!submitRef.current?.isConnected) return
              event.preventDefault()
              submitRef.current.focus({ preventScroll: true })
            }}
          >
            <div className="flex flex-col gap-4">
              <dl className="grid gap-x-4 gap-y-2 text-xs sm:grid-cols-2">
                <SummaryRow label="Action" value={actionLabel} />
                <SummaryRow label="Work item" value={item.id} mono />
                <SummaryRow
                  label="Move"
                  value={`${humanizeIdentifier(offer.from)} → ${humanizeIdentifier(offer.to)}`}
                />
                <SummaryRow
                  label="Effect"
                  value={offer.externalEffect ? 'Changes something outside Orbit' : 'Changes stored values'}
                />
              </dl>

              <div
                data-capability-confirm-effect=""
                className="rounded-md border border-warning/35 bg-warning-soft/60 px-3 py-2.5 text-xs leading-5 text-warning-ink"
              >
                <p className="font-medium">{confirmation.effect}</p>
                <p className="mt-1">{confirmation.executor}</p>
              </div>

              <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4">
                <Button
                  ref={confirmPrimaryRef}
                  data-capability-confirm-submit=""
                  onClick={() => void submit()}
                  loading={pending}
                  disabled={pending}
                >
                  {pending ? 'Recording…' : `Record ${actionLabel}`}
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  onClick={() => setStep('form')}
                  disabled={pending}
                >
                  Back to the inputs
                </Button>
              </div>
              <p role="status" aria-live="polite" className="min-h-4 text-2xs leading-4 text-ink-muted">
                {pending ? 'Recording the request…' : announcement}
              </p>
            </div>
          </DialogContent>
        </Dialog>
      ) : null}
    </>
  )
}

function SummaryRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0">
      <dt className="text-2xs font-semibold tracking-label text-ink-faint">{label}</dt>
      <dd className={cn('break-words text-sm leading-5 text-ink-soft', mono && 'font-mono text-xs')}>
        {value}
      </dd>
    </div>
  )
}

function CapabilityInputControl({
  field,
  actionId,
  value,
  error,
  disabled,
  onChange,
}: {
  field: CapabilityInputField
  actionId: string
  value: string | boolean
  error: string | undefined
  disabled: boolean
  onChange: (value: string | boolean) => void
}) {
  const hint = field.description ?? undefined
  const controlId = `cap-${actionId}-${field.name}`
  const isLongText = field.kind === 'text' && (field.maxLength ?? 0) >= 200
  return (
    <div data-capability-input={field.name} className="min-w-0">
      {field.kind === 'boolean' ? (
        <Field id={controlId} label={field.label} hint={hint} error={error}>
          <Checkbox
            checked={value === true}
            onCheckedChange={(checked) => onChange(checked === true)}
            disabled={disabled}
          />
        </Field>
      ) : field.kind === 'choice' ? (
        <LabeledSelect
          label={field.label}
          required={field.required}
          value={typeof value === 'string' ? value : ''}
          onValueChange={onChange}
          options={field.options.map((option) => ({ value: option.value, label: option.label }))}
          placeholder={field.options.length > 0 ? 'Choose a value' : 'No values declared'}
          hint={hint}
          error={error}
          disabled={disabled}
          ariaLabel={field.label}
        />
      ) : (
        <Field
          id={controlId}
          label={field.label}
          required={field.required}
          hint={hint}
          error={error}
        >
          {isLongText ? (
            <Textarea
              value={typeof value === 'string' ? value : ''}
              onChange={(event) => onChange(event.target.value)}
              rows={3}
              disabled={disabled}
              maxLength={field.maxLength ?? undefined}
            />
          ) : (
            <Input
              value={typeof value === 'string' ? value : ''}
              onChange={(event) => onChange(event.target.value)}
              inputMode={
                field.kind === 'integer'
                  ? 'numeric'
                  : field.kind === 'number'
                    ? 'decimal'
                    : undefined
              }
              autoComplete="off"
              spellCheck={false}
              disabled={disabled}
              maxLength={field.maxLength ?? undefined}
            />
          )}
        </Field>
      )}
    </div>
  )
}
