/**
 * The milestone form fields shared by create and edit.
 *
 * Status is deliberately not here: a new milestone is `planned` (the server
 * decides), and an existing one moves through its own status command so a
 * refusal is quoted against the status control instead of the whole form.
 */
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import {
  MILESTONE_PRIORITY_OPTIONS,
  dateInputValue,
  parseDateInput,
  type MilestoneFormDraft,
} from './milestone-model'

export function MilestoneFormFields({
  draft,
  onChange,
  disabled,
  autoFocus = false,
}: {
  draft: MilestoneFormDraft
  onChange: (next: MilestoneFormDraft) => void
  disabled?: boolean
  autoFocus?: boolean
}) {
  const patch = (next: Partial<MilestoneFormDraft>) => onChange({ ...draft, ...next })

  return (
    <>
      <Field label="Name" required>
        <Input
          value={draft.name}
          onChange={(event) => patch({ name: event.target.value })}
          placeholder="e.g. First review"
          autoFocus={autoFocus}
          disabled={disabled}
        />
      </Field>

      <Field label="Description" hint="What reaching this checkpoint means.">
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
            <SelectTrigger aria-label="Milestone priority">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {MILESTONE_PRIORITY_OPTIONS.map((option) => (
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
        <Field label="Start date" hint="Optional; when this checkpoint's work begins.">
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
