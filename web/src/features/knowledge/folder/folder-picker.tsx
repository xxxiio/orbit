/**
 * The one folder control: a labelled select over the vault's real folders.
 *
 * Every dialog that needs a location uses this instead of a free-text path, so a
 * person never types a path by hand. Radix `Select` supplies roving focus and
 * type-ahead; the root is offered as an explicit option because the empty string
 * is how the server spells the vault root, not a missing value.
 */
import { Field } from '@/components/ui/field'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { fromPickerValue, toPickerValue, type FolderOption } from '../folder-model'

export interface FolderPickerProps {
  label: string
  hint?: string
  value: string
  options: FolderOption[]
  onChange: (folder: string) => void
  disabled?: boolean
  /** Distinguishes two pickers in the same dialog for tests and analytics. */
  fieldName?: string
}

export function FolderPicker({
  label,
  hint,
  value,
  options,
  onChange,
  disabled = false,
  fieldName = 'folder',
}: FolderPickerProps) {
  return (
    <Field label={label} hint={hint}>
      <Select
        value={toPickerValue(value)}
        onValueChange={(next) => onChange(fromPickerValue(next))}
        disabled={disabled}
      >
        <SelectTrigger data-field={fieldName} data-folder-value={value}>
          <SelectValue placeholder="Choose a folder" />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={toPickerValue(option.value)} data-folder-option={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </Field>
  )
}
