/**
 * Work-item presentation rules.
 *
 * A work item has identity, a kind and a state; its readable content lives in
 * `properties`, whose schema is declared by the kind. Nothing here decides what
 * a transition means or whether one is legal — that is the server's answer.
 * These helpers only turn the declaration into labels, columns, guards and
 * action offers so every work-item surface reads the same manifest the same way.
 */
import type {
  WorkItem,
  WorkItemField,
  WorkItemKind,
  WorkItemTransition,
  Workflow,
  WorkflowAction,
  WorkflowView,
  WorkflowViewComponent,
} from '@/api/adapters'
import type { TransitionGuard } from '@/api/contracts'
import { humanizeIdentifier } from '@/lib/humanize'

/* ------------------------------------------------------------------ */
/* Kinds, states, fields                                               */
/* ------------------------------------------------------------------ */

/** A kind id from the manifest, matched case-insensitively like view filters. */
export function kindFor(workflow: Workflow | undefined, kindId: string | undefined): WorkItemKind | undefined {
  if (!kindId) return undefined
  return workflow?.kinds.find((kind) => kind.id.toLowerCase() === kindId.toLowerCase())
}

export function kindLabel(kind: WorkItemKind | undefined, fallback: string): string {
  return kind?.name.trim() || humanizeIdentifier(fallback)
}

/** A state id is data: show it as words, never as a raw snake_case token. */
export function stateLabel(state: string): string {
  return humanizeIdentifier(state)
}

export function fieldLabel(field: WorkItemField | undefined, fallback: string): string {
  const id = field?.id ?? fallback
  return humanizeIdentifier(id)
}

/** The first text field, else the first choice field: the item's name fallback. */
export function titleFieldFor(kind: WorkItemKind | undefined): WorkItemField | undefined {
  if (!kind) return undefined
  return (
    kind.fields.find((field) => field.kind.type === 'text') ??
    kind.fields.find((field) => field.kind.type === 'choice')
  )
}

function fieldHasValue(field: WorkItemField, item: WorkItem): boolean {
  const value = item.properties[field.id]
  return value !== null && value !== undefined && String(value).trim().length > 0
}

/** The field a particular item actually uses as its name. */
export function titleFieldForItem(
  kind: WorkItemKind | undefined,
  item: WorkItem,
): WorkItemField | undefined {
  if (!kind) return undefined
  return (
    kind.fields.find((field) => field.kind.type === 'text' && fieldHasValue(field, item)) ??
    kind.fields.find((field) => field.kind.type === 'choice' && fieldHasValue(field, item)) ??
    titleFieldFor(kind)
  )
}

/**
 * Items have properties only, so the title fallback is the first text field and
 * then the first choice field. When neither carries a value the kind name is the
 * honest heading; the short id under it keeps two such items distinguishable.
 */
export function workItemTitle(item: WorkItem, kind: WorkItemKind | undefined): string {
  const field = titleFieldForItem(kind, item)
  const value = field ? item.properties[field.id] : undefined
  if (value !== null && value !== undefined) {
    const text = fieldValueText(field, value)
    if (text.trim().length > 0 && text !== '—') return text.trim()
  }
  return kindLabel(kind, item.kind)
}

/** A stable, short reference: full ids are useful in the sheet, not on a card. */
export function shortWorkItemId(id: string): string {
  return id.length > 16 ? `${id.slice(0, 8)}…${id.slice(-4)}` : id
}

export function propertyText(value: unknown): string {
  if (value === null || value === undefined) return '—'
  if (typeof value === 'boolean') return value ? 'Yes' : 'No'
  if (typeof value === 'number') return String(value)
  if (typeof value === 'string') return value.trim().length > 0 ? value : '—'
  return JSON.stringify(value)
}

/** A typed value as copy: choice tokens are humanized, free text is not. */
export function fieldValueText(field: WorkItemField | undefined, value: unknown): string {
  if (value === null || value === undefined) return 'Not set'
  if (field?.kind.type === 'boolean') return value === true ? 'Yes' : value === false ? 'No' : '—'
  if (field?.kind.type === 'choice' && typeof value === 'string') return humanizeIdentifier(value)
  return propertyText(value)
}

/** Fields a card or a row can show without repeating the title. */
export function displayFields(kind: WorkItemKind | undefined, item: WorkItem): WorkItemField[] {
  if (!kind) return []
  const title = titleFieldForItem(kind, item)
  return kind.fields.filter((field) => field.id !== title?.id).slice(0, 4)
}

/* ------------------------------------------------------------------ */
/* Transitions and guards                                              */
/* ------------------------------------------------------------------ */

export function transitionsFromState(
  kind: WorkItemKind | undefined,
  state: string,
): WorkItemTransition[] {
  return kind?.transitions.filter((transition) => transition.from === state) ?? []
}

/**
 * A guard is rendered as a sentence a person can act on. The words come from
 * the declaration (`field`, `value`, `state`); an unknown guard type is named,
 * never silently dropped.
 */
export function guardSentence(guard: TransitionGuard, kind: WorkItemKind | undefined): string {
  switch (guard.type) {
    case 'property_equals': {
      const field = kind?.fields.find((candidate) => candidate.id === guard.field)
      return `Requires ${fieldLabel(field, guard.field)} to be ${fieldValueText(field, guard.value)}.`
    }
    case 'property_present': {
      const field = kind?.fields.find((candidate) => candidate.id === guard.field)
      return `Requires ${fieldLabel(field, guard.field)} to be set.`
    }
    case 'all_prerequisites_in_state':
      return `All prerequisites must be in ${stateLabel(guard.state)}.`
    default: {
      const unknown = guard as { type?: string }
      return `Requires a guard this build cannot read${unknown.type ? ` (${unknown.type})` : ''}.`
    }
  }
}

export interface StateActionOffer {
  transition: WorkItemTransition
  action: WorkflowAction
}

/** Every declared action on a transition that leaves the current state. */
export function actionsForState(kind: WorkItemKind | undefined, state: string): StateActionOffer[] {
  return transitionsFromState(kind, state).flatMap((transition) =>
    transition.actions.map((action) => ({ transition, action })),
  )
}

export function parameterNames(action: WorkflowAction): string[] {
  return Object.keys(action.parameters ?? {})
}

export function parameterType(value: unknown): string {
  if (value === null) return 'value'
  if (Array.isArray(value)) return 'list'
  return typeof value === 'object' ? 'object' : typeof value
}

/* ------------------------------------------------------------------ */
/* Sorting and columns                                                 */
/* ------------------------------------------------------------------ */

export interface WorkItemSortDeclaration {
  compare: ((left: WorkItem, right: WorkItem) => number) | null
  refusal: string | null
}

function compareValues(left: unknown, right: unknown): number {
  if (typeof left === 'number' && typeof right === 'number') return left - right
  if (typeof left === 'boolean' && typeof right === 'boolean') return Number(left) - Number(right)
  const leftText = propertyText(left)
  const rightText = propertyText(right)
  return leftText.localeCompare(rightText, undefined, { numeric: true, sensitivity: 'base' })
}

/**
 * `title` resolves through each item's own kind, so a list that mixes kinds is
 * still ordered by what each row actually shows as its name.
 */
export function workItemSortDeclaration(
  component: WorkflowViewComponent,
  workflow: Workflow | undefined,
): WorkItemSortDeclaration {
  const declared = component.sort?.trim()
  if (!declared) return { compare: null, refusal: null }
  const key = declared.toLowerCase()

  const fieldIds = new Set<string>()
  for (const kind of workflow?.kinds ?? []) {
    for (const field of kind.fields) fieldIds.add(field.id)
  }

  if (key === 'title' || key === 'name') {
    return {
      compare: (left, right) => {
        const leftKind = kindFor(workflow, left.kind)
        const rightKind = kindFor(workflow, right.kind)
        return workItemTitle(left, leftKind).localeCompare(workItemTitle(right, rightKind), undefined, {
          numeric: true,
          sensitivity: 'base',
        })
      },
      refusal: null,
    }
  }
  if (key === 'kind') {
    return {
      compare: (left, right) => left.kind.localeCompare(right.kind),
      refusal: null,
    }
  }
  if (key === 'state' || key === 'status') {
    return {
      compare: (left, right) => left.state.localeCompare(right.state),
      refusal: null,
    }
  }
  if (fieldIds.has(declared)) {
    return {
      compare: (left, right) => compareValues(left.properties[declared], right.properties[declared]),
      refusal: null,
    }
  }
  return {
    compare: null,
    refusal: `This panel sorts work items by “${declared}”, which is not a property of its kinds.`,
  }
}

export function sortWorkItems(
  items: readonly WorkItem[],
  declaration: WorkItemSortDeclaration,
  descending: boolean,
): WorkItem[] {
  if (!declaration.compare) return [...items]
  const compare = declaration.compare
  return [...items].sort((left, right) => (descending ? compare(right, left) : compare(left, right)))
}

export interface WorkItemColumn {
  key: 'title' | 'kind' | 'state' | 'revision' | 'field'
  label: string
  field?: WorkItemField
}

export interface WorkItemColumnPlan {
  columns: WorkItemColumn[]
  /** Declared column names this build cannot show, named for a visible note. */
  unsupported: string[]
}

const DEFAULT_COLUMNS: WorkItemColumn[] = [
  { key: 'title', label: 'Work item' },
  { key: 'kind', label: 'Kind' },
  { key: 'state', label: 'State' },
]

/**
 * Map a declared `columns` list onto the fields of the view's kinds. `title`,
 * `kind` and `state` are the three virtual columns; anything else must name a
 * declared field. Unknown names are reported, not silently dropped.
 */
export function workItemColumnPlan(
  component: WorkflowViewComponent,
  workflow: Workflow | undefined,
): WorkItemColumnPlan {
  const declared = component.columns ?? []
  if (declared.length === 0) return { columns: DEFAULT_COLUMNS.map((column) => ({ ...column })), unsupported: [] }

  const fields = new Map<string, WorkItemField>()
  for (const kind of workflow?.kinds ?? []) {
    for (const field of kind.fields) {
      if (!fields.has(field.id)) fields.set(field.id, field)
    }
  }

  const columns: WorkItemColumn[] = []
  const unsupported: string[] = []
  for (const name of declared) {
    const key = name.trim()
    if (key.length === 0) continue
    const lower = key.toLowerCase()
    if (lower === 'title' || lower === 'name') {
      columns.push({ key: 'title', label: 'Work item' })
      continue
    }
    if (lower === 'kind' || lower === 'type') {
      columns.push({ key: 'kind', label: 'Kind' })
      continue
    }
    if (lower === 'state' || lower === 'status') {
      columns.push({ key: 'state', label: 'State' })
      continue
    }
    if (lower === 'revision') {
      columns.push({ key: 'revision', label: 'Revision' })
      continue
    }
    const field = fields.get(key)
    if (field) {
      columns.push({ key: 'field', label: fieldLabel(field, key), field })
      continue
    }
    unsupported.push(key)
  }

  // The title is the row's primary action and cannot be hidden.
  if (!columns.some((column) => column.key === 'title')) {
    columns.unshift({ key: 'title', label: 'Work item' })
  }
  return { columns, unsupported }
}

/* ------------------------------------------------------------------ */
/* View declarations                                                   */
/* ------------------------------------------------------------------ */

/**
 * The kind a view pins through a work-item component's `filter.kind`. The first
 * declaring component wins, so a create action opened from the view knows which
 * kind it is adding to.
 */
export function pinnedKindForView(view: WorkflowView | undefined): string | null {
  for (const component of view?.components ?? []) {
    if ((component.source ?? null) !== 'work_items') continue
    const declared = component.filter?.kind
    if (declared && declared.trim().length > 0) return declared.trim()
  }
  return null
}

/** The kind the view puts first, used when no filter pins one. */
export function defaultKindForComponent(
  component: WorkflowViewComponent,
  workflow: Workflow | undefined,
): WorkItemKind | undefined {
  return kindFor(workflow, component.filter?.kind) ?? workflow?.kinds[0]
}

export interface WorkItemKindResolution {
  kind: WorkItemKind | undefined
  /** True when the view names more than one kind and does not pin one. */
  needsChoice: boolean
  /** Set when the filter names a kind the manifest does not declare. */
  refusal: string | null
}

export function resolveBoardKind(
  component: WorkflowViewComponent,
  workflow: Workflow | undefined,
): WorkItemKindResolution {
  const declared = component.filter?.kind?.trim()
  if (declared) {
    const kind = kindFor(workflow, declared)
    if (!kind) {
      return {
        kind: undefined,
        needsChoice: false,
        refusal: `This board pins the work-item kind “${declared}”, which the pinned workflow does not declare.`,
      }
    }
    return { kind, needsChoice: false, refusal: null }
  }
  const kinds = workflow?.kinds ?? []
  return {
    kind: kinds[0],
    needsChoice: kinds.length > 1,
    refusal: null,
  }
}

/** The declared board states, in the manifest's canonical order. */
export function boardStatesForKind(kind: WorkItemKind | undefined): string[] {
  return kind ? [...kind.states] : []
}
