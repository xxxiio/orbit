/**
 * Declared column and option readers.
 *
 * The manifest validator bounds the *keys* of `options`, not the values, so a
 * panel reads each declared value here and refuses a spelling it cannot honour
 * instead of silently treating it as absent.
 */
import type { WorkflowViewComponent } from '@/api/adapters'

/**
 * Own-property test for manifest-keyed tables. `in` and bracket lookup would
 * walk `Object.prototype`, so a type named `constructor` would find a built-in.
 */
export function hasOwnKey(table: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(table, key)
}

export function declaredTextOption(
  component: WorkflowViewComponent,
  key: string,
): string | null {
  const raw = component.options?.[key]
  if (typeof raw !== 'string') return null
  const trimmed = raw.trim()
  return trimmed.length > 0 ? trimmed : null
}

/** `empty_text` is copy, never markup: the workflow cannot inject a component. */
export function emptyTextOption(component: WorkflowViewComponent): string | null {
  return declaredTextOption(component, 'empty_text')
}

export function metricCaption(component: WorkflowViewComponent): string | null {
  return declaredTextOption(component, 'caption')
}

export interface DeclaredBooleanOption {
  /** `undefined` when the option was not declared at all. */
  value: boolean | undefined
  refusal: string | null
}

export function declaredBooleanOption(
  component: WorkflowViewComponent,
  key: string,
): DeclaredBooleanOption {
  const raw = component.options?.[key]
  if (raw === undefined || raw === null) return { value: undefined, refusal: null }
  if (typeof raw === 'boolean') return { value: raw, refusal: null }
  if (typeof raw === 'string') {
    const normalized = raw.trim().toLowerCase()
    if (normalized === 'true') return { value: true, refusal: null }
    if (normalized === 'false') return { value: false, refusal: null }
  }
  return {
    value: undefined,
    refusal: `This panel sets ${key} to ${JSON.stringify(raw)}; only true or false are supported.`,
  }
}

export interface DeclaredNumberOption {
  value: number | null
  /** True when the declared number was clamped to the supported range. */
  clamped?: boolean
  refusal: string | null
}

export function declaredNumberOption(
  component: WorkflowViewComponent,
  key: string,
): DeclaredNumberOption {
  const raw = component.options?.[key]
  if (raw === undefined || raw === null) return { value: null, refusal: null }
  const parsed =
    typeof raw === 'number'
      ? raw
      : typeof raw === 'string' && raw.trim().length > 0
        ? Number(raw)
        : Number.NaN
  if (!Number.isFinite(parsed)) {
    return {
      value: null,
      refusal: `This panel sets ${key} to ${JSON.stringify(raw)}, which is not a number.`,
    }
  }
  return { value: parsed, refusal: null }
}

/** A board column may declare a pixel width; 180–420 keeps every column usable. */
export function columnWidthOption(component: WorkflowViewComponent): DeclaredNumberOption {
  const declared = declaredNumberOption(component, 'column_width')
  if (declared.value === null) return declared
  const clamped = Math.min(420, Math.max(180, Math.round(declared.value)))
  return {
    value: clamped,
    clamped: clamped !== declared.value,
    refusal: null,
  }
}

/** The note a board shows when the declared width was clamped, or null. */
export function columnWidthNote(component: WorkflowViewComponent): string | null {
  const width = columnWidthOption(component)
  if (width.value === null || !width.clamped) return null
  return `The workflow's declared column width was clamped to ${width.value} pixels; columns stay between 180 and 420 pixels so every one remains usable.`
}

export interface ListOptionDeclaration {
  showStatus: boolean
  showPriority: boolean
  /** `undefined` means the panel did not declare it. */
  showDependencies: boolean | undefined
  refusal: string | null
}

/** `list`/`task_list` visibility options, with one named refusal on a bad value. */
export function listOptionDeclaration(component: WorkflowViewComponent): ListOptionDeclaration {
  const status = declaredBooleanOption(component, 'show_status')
  const priority = declaredBooleanOption(component, 'show_priority')
  const dependencies = declaredBooleanOption(component, 'show_dependencies')
  return {
    showStatus: status.value ?? true,
    showPriority: priority.value ?? false,
    showDependencies: dependencies.value,
    refusal: status.refusal ?? priority.refusal ?? dependencies.refusal,
  }
}

/** A declared window value named in a refusal, e.g. `days`/`window`. */
export function declaredWindowOption(
  component: WorkflowViewComponent,
  key: 'days' | 'window',
): string | null {
  const raw = component.options?.[key]
  if (raw === undefined || raw === null) return null
  if (typeof raw === 'number' && Number.isFinite(raw)) return String(raw)
  if (typeof raw === 'string' && raw.trim().length > 0) return raw.trim()
  return JSON.stringify(raw)
}
