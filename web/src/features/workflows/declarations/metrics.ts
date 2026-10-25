/**
 * Declared metric aggregates.
 *
 * A metric component names an aggregate, a field and (for a non-count number) a
 * unit. A declaration this build cannot evaluate is refused by name instead of
 * being rendered as a count under a "sum" title.
 */
import type { Task, WorkItem, Workflow, WorkflowViewComponent } from '@/api/adapters'
import { dataSourceName } from './filters'

export type MetricAggregate = 'count' | 'sum' | 'average' | 'min' | 'max'

const METRIC_AGGREGATES = new Set<string>(['count', 'sum', 'average', 'min', 'max'])

export interface MetricDeclaration {
  aggregate: MetricAggregate
  field: string | null
  /** The unit the value is shown in; `null` for a plain count. */
  unit: string | null
  /**
   * Multiplier from the stored field's unit to `unit`. 1 when no conversion is
   * needed. A declared display unit is never treated as a free label on a field
   * whose base unit this build knows (minutes): a mismatch is converted or the
   * panel is refused.
   */
  factor: number
  /** Non-null means the declaration cannot be honoured; render a refusal. */
  refusal: string | null
}

/** Fields whose name already states the unit this build can show. */
function isMinutesField(field: string): boolean {
  const normalized = field.toLowerCase()
  return (
    normalized === 'estimated_minutes' ||
    normalized === 'estimatedminutes' ||
    normalized === 'minutes' ||
    normalized.endsWith('_minutes')
  )
}

/** A declared display unit, when the manifest supplied one. */
function declaredUnit(component: WorkflowViewComponent): string | null {
  const raw = component.options?.unit
  if (typeof raw !== 'string') return null
  const trimmed = raw.trim()
  return trimmed.length > 0 ? trimmed : null
}

function isMetricAggregate(value: string): value is MetricAggregate {
  return METRIC_AGGREGATES.has(value)
}

/**
 * The numeric fields a source can aggregate. Tasks carry exactly one
 * (`estimated_minutes`); a work item carries the `integer` properties its kind
 * declares. Milestones and the schedule carry none.
 */
export function numericFieldsForSource(
  source: string,
  workflow: Workflow | undefined,
  filter: Record<string, string> = {},
): ReadonlySet<string> {
  if (source === 'tasks') return new Set(['estimated_minutes'])
  if (source === 'work_items') {
    const kindFilter = filter.kind?.toLowerCase()
    const fields = new Set<string>()
    for (const kind of workflow?.kinds ?? []) {
      if (kindFilter && kind.id.toLowerCase() !== kindFilter) continue
      for (const field of kind.fields) {
        if (field.kind.type === 'integer') fields.add(field.id)
      }
    }
    return fields
  }
  return new Set()
}

/**
 * Resolve a metric's declared aggregate and field. A declaration this build
 * cannot evaluate gets a named reason instead of a count under a "sum" title.
 */
export function metricDeclaration(
  component: WorkflowViewComponent,
  workflow: Workflow | undefined,
): MetricDeclaration {
  const source = component.source ?? 'tasks'
  const raw = (component.aggregate ?? 'count').trim().toLowerCase()
  if (!isMetricAggregate(raw)) {
    return {
      aggregate: 'count',
      field: null,
      unit: null,
      factor: 1,
      refusal: `This panel asks for the “${component.aggregate}” aggregate, which this build cannot compute.`,
    }
  }

  const field = component.field?.trim() || null
  const numericFields = numericFieldsForSource(source, workflow, component.filter ?? {})

  if (raw !== 'count' && numericFields.size === 0) {
    return {
      aggregate: raw,
      field: null,
      unit: null,
      factor: 1,
      refusal: `${dataSourceName(source)} has no numeric field this build can aggregate; only a count is supported.`,
    }
  }

  if (field && !numericFields.has(field)) {
    const label = source === 'tasks' ? 'tasks' : dataSourceName(source)
    return {
      aggregate: raw,
      field: null,
      unit: null,
      factor: 1,
      refusal: `This panel aggregates the “${field}” field, which is not a numeric field on ${label}.`,
    }
  }

  if (raw !== 'count' && field === null) {
    return {
      aggregate: raw,
      field: null,
      unit: null,
      factor: 1,
      refusal: `A ${raw} aggregate needs the numeric field to aggregate, and this panel does not declare one.`,
    }
  }

  // A non-count number is only honest with a unit. Estimated/minutes fields
  // carry theirs in the name; any other integer field must say so in
  // `options.unit` rather than being shown as minutes.
  const unit = declaredUnit(component)
  if (raw !== 'count' && field !== null && !isMinutesField(field) && unit === null) {
    return {
      aggregate: raw,
      field,
      unit: null,
      factor: 1,
      refusal: `This panel aggregates the “${field}” field, which has no declared unit; add options.unit so the number is not shown as minutes.`,
    }
  }

  // A field whose name states its unit (minutes) can be *converted* to another
  // unit, but a declared unit must not be used as a bare label: "180 hours" for
  // 180 minutes is a lie. Known conversions are applied; anything else refuses.
  if (raw !== 'count' && field !== null && isMinutesField(field)) {
    const conversion = minutesConversion(unit)
    return {
      aggregate: raw,
      field,
      unit: conversion.unit,
      factor: conversion.factor,
      refusal: conversion.refusal,
    }
  }

  return {
    aggregate: raw,
    field,
    unit: raw === 'count' ? null : unit,
    factor: 1,
    refusal: null,
  }
}

interface UnitConversion {
  unit: string
  factor: number
  refusal: string | null
}

/** Minutes → the declared display unit. Unknown units are refused by name. */
function minutesConversion(unit: string | null): UnitConversion {
  if (unit === null) return { unit: 'minutes', factor: 1, refusal: null }
  const normalized = unit.trim().toLowerCase()
  if (['minute', 'minutes', 'min', 'mins'].includes(normalized)) {
    return { unit: 'minutes', factor: 1, refusal: null }
  }
  if (['hour', 'hours', 'h', 'hr', 'hrs'].includes(normalized)) {
    return { unit: 'hours', factor: 1 / 60, refusal: null }
  }
  if (['second', 'seconds', 's', 'sec', 'secs'].includes(normalized)) {
    return { unit: 'seconds', factor: 60, refusal: null }
  }
  return {
    unit: unit,
    factor: 1,
    refusal: `This panel displays a minutes field in “${unit}”; this build converts minutes to minutes, hours or seconds only.`,
  }
}

function numericProperty(value: unknown): number | null {
  if (typeof value === 'number') return Number.isFinite(value) ? value : null
  if (typeof value === 'string' && value.trim().length > 0) {
    const parsed = Number(value)
    return Number.isFinite(parsed) ? parsed : null
  }
  return null
}

/**
 * Compute the declared aggregate over rows. `count` counts rows (or non-null
 * field values when a field is declared); the others ignore nulls and return
 * `null` when nothing is left to aggregate.
 */
export function aggregateMetric(
  declaration: MetricDeclaration,
  rows: ReadonlyArray<number | null>,
): number | null {
  if (declaration.aggregate === 'count') {
    return declaration.field
      ? rows.filter((value) => value !== null).length
      : rows.length
  }
  const values = rows.filter((value): value is number => value !== null && Number.isFinite(value))
  if (values.length === 0) return declaration.aggregate === 'sum' ? 0 : null
  const sum = values.reduce((total, value) => total + value, 0)
  if (declaration.aggregate === 'sum') return sum
  if (declaration.aggregate === 'average') return sum / values.length
  if (declaration.aggregate === 'min') return Math.min(...values)
  return Math.max(...values)
}

/** Convert an aggregated raw value into the declaration's display unit. */
export function metricDisplayValue(
  declaration: MetricDeclaration,
  value: number | null,
): number | null {
  if (value === null) return null
  if (declaration.aggregate === 'count') return value
  return value * declaration.factor
}

/** Task rows reduced to the declared numeric field (null when unestimated). */
export function taskMetricRows(tasks: readonly Task[], field: string | null): Array<number | null> {
  if (!field) return tasks.map(() => null)
  if (field === 'estimated_minutes' || field === 'estimatedMinutes') {
    return tasks.map((task) => task.estimatedMinutes)
  }
  return tasks.map(() => null)
}

export function workItemMetricRows(
  items: readonly WorkItem[],
  field: string | null,
): Array<number | null> {
  if (!field) return items.map(() => null)
  return items.map((item) => numericProperty(item.properties[field]))
}
