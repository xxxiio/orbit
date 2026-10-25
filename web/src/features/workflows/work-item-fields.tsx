/**
 * The typed property editors a work-item field needs.
 *
 * A field is text, integer, boolean or a declared choice. The same control
 * renders it in the sheet (edit that commits on blur) and in the create dialog
 * (a draft that is validated on submit); only the surrounding wrapper and the
 * hint copy differ, so the typed control itself exists once.
 */
import type { ReactNode } from 'react'
import type { WorkItemField } from '@/api/adapters'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { LabeledSelect, PropertyField } from '@/components/forms'
import { humanizeIdentifier } from '@/lib/humanize'
import { fieldLabel } from './work-item-model'

export const INTEGER_PATTERN = /^-?\d+$/

/** A draft value as the editor holds it: numbers stay strings until commit. */
export type PropertyDraftValue = string | boolean | undefined

export interface PropertyEditorProps {
  field: WorkItemField
  /** Edit mode commits on blur; create mode is a draft validated on submit. */
  mode: 'edit' | 'create'
  value: PropertyDraftValue
  onChange: (value: string | boolean) => void
  disabled?: boolean
  error?: string | null
  /** A write for this field is in flight. */
  pending?: boolean
  required?: boolean
  /** Overrides the editor's default hint. */
  hint?: ReactNode
  onBlur?: () => void
  /** Enter commits in edit mode; create mode submits through the form. */
  onEnter?: () => void
  /** Show the create dialog's "Not chosen" boolean state. */
  showUnchosenState?: boolean
}

function choiceOptions(field: WorkItemField): Array<{ value: string; label: string }> {
  return field.kind.type === 'choice'
    ? field.kind.options.map((option) => ({ value: option, label: humanizeIdentifier(option) }))
    : []
}

export function PropertyEditor({
  field,
  mode,
  value,
  onChange,
  disabled = false,
  error,
  pending = false,
  required = false,
  hint,
  onBlur,
  onEnter,
  showUnchosenState = false,
}: PropertyEditorProps) {
  const label = fieldLabel(field, field.id)
  const enter = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter') return
    event.preventDefault()
    onEnter?.()
  }

  if (field.kind.type === 'text') {
    const input = (
      <Input
        value={typeof value === 'string' ? value : ''}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        onBlur={onBlur}
        onKeyDown={mode === 'edit' ? enter : undefined}
        placeholder={`Add ${label.toLowerCase()}`}
      />
    )
    if (mode === 'edit') {
      return (
        <PropertyField label={label} pending={pending}>
          {input}
        </PropertyField>
      )
    }
    return (
      <Field label={label} required={required} error={error ?? undefined} hint={required ? undefined : 'Optional.'}>
        {input}
      </Field>
    )
  }

  if (field.kind.type === 'integer') {
    const input = (
      <Input
        type="number"
        inputMode="numeric"
        step={1}
        value={typeof value === 'string' ? value : ''}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        onBlur={onBlur}
        onKeyDown={mode === 'edit' ? enter : undefined}
      />
    )
    if (mode === 'edit') {
      return (
        <Field
          label={label}
          error={error ?? undefined}
          hint={hint ?? 'Whole numbers only. Clearing a stored number is not supported.'}
        >
          {input}
        </Field>
      )
    }
    return (
      <Field
        label={label}
        required={required}
        error={error ?? undefined}
        hint={required ? 'Whole numbers only.' : 'Optional. Whole numbers only.'}
      >
        {input}
      </Field>
    )
  }

  if (field.kind.type === 'boolean') {
    const checked = value === true
    const control = (
      <Switch
        label={mode === 'edit' ? (checked ? 'Yes' : 'No') : undefined}
        checked={checked}
        disabled={disabled}
        onCheckedChange={(next) => onChange(next)}
        aria-label={mode === 'edit' ? `${label}: ${checked ? 'yes' : 'no'}` : label}
      />
    )
    if (mode === 'edit') {
      return (
        <PropertyField label={label} pending={pending}>
          {control}
        </PropertyField>
      )
    }
    return (
      <Field label={label} required={required} error={error ?? undefined}>
        <div className="flex items-center gap-2">
          {control}
          {showUnchosenState ? (
            <span className="text-sm text-ink-soft">
              {value === undefined ? 'Not chosen' : checked ? 'Yes' : 'No'}
            </span>
          ) : null}
        </div>
      </Field>
    )
  }

  return (
    <LabeledSelect
      label={label}
      ariaLabel={mode === 'edit' ? label : undefined}
      required={mode === 'create' ? required : undefined}
      error={error ?? undefined}
      value={typeof value === 'string' ? value : ''}
      placeholder={mode === 'edit' ? 'Not set' : 'Choose one'}
      options={choiceOptions(field)}
      disabled={disabled}
      hint={mode === 'edit' ? (pending ? 'Saving…' : undefined) : undefined}
      onValueChange={(next) => onChange(next)}
    />
  )
}
