/**
 * Capability presentation rules: availability copy, the request form derived
 * from `input_schema`, request-state copy and the reconcile allow-list.
 *
 * Nothing here decides authority. The server computes availability from durable
 * facts, decides whether an action needs confirmation, and validates inputs
 * again; these helpers only make the server's answer sayable and keep the
 * browser from offering something it already knows is refused.
 *
 * Two rules are load-bearing:
 *
 * - An availability code is rendered as its own sentence. `executor_missing`
 *   says no executor has announced the capability — it must never be reworded as
 *   "not authorized", which is a different, durable refusal with its own fix.
 * - `uncertain` and `claimed` are the two reconcilable states, and they are not
 *   the same question. `uncertain` means an executor said it could not tell
 *   whether the effect happened; `claimed` means an executor took the request and
 *   nothing recorded a result, which may mean the answer was lost. `pending` has
 *   not run, and a settled state is already recorded, so neither gets a control.
 */
import type { CapabilityRequest, CapabilityStatus, KnownCapabilityRequestState, WorkItemKind } from '@/api/adapters'
import type { CapabilityReconcileDecision } from '@/api/contracts'
import { isApiError, scrubInternalNames, userMessage } from '@/api/errors'
import { humanizeIdentifier } from '@/lib/humanize'
import { transitionsFromState } from './work-item-model'

/* ------------------------------------------------------------------ */
/* Availability                                                        */
/* ------------------------------------------------------------------ */

/** The vocabulary and its order, as the server documents it. */
export const CAPABILITY_AVAILABILITY_CODES = [
  'capability_missing',
  'capability_disabled',
  'capability_not_installed',
  'executor_missing',
  'requirement_missing',
  'not_authorized',
] as const

export type CapabilityAvailabilityCode = (typeof CAPABILITY_AVAILABILITY_CODES)[number]

export function isCapabilityAvailabilityCode(value: string): value is CapabilityAvailabilityCode {
  return (CAPABILITY_AVAILABILITY_CODES as readonly string[]).includes(value)
}

export interface AvailabilityCopy {
  /** What the refusal means, in one sentence. */
  title: string
  /** What would change it, or why this browser cannot. */
  hint: string
  /** The server's own reason, quoted verbatim; `null` when it gave none. */
  serverReason: string | null
}

const AVAILABILITY_COPY: Record<CapabilityAvailabilityCode, { title: string; hint: string }> = {
  capability_missing: {
    title: 'The server does not have this capability.',
    hint: 'The workflow still declares the action, but no contract for it exists here. An operator cannot add one from a running server.',
  },
  capability_disabled: {
    title: 'This capability is switched off on the server.',
    hint: 'An operator can enable it. Changing the workflow does not.',
  },
  capability_not_installed: {
    title: 'This capability is not installed on the server.',
    hint: 'Installation is an operator step on the server. It cannot be installed from this browser.',
  },
  executor_missing: {
    title: 'No executor has announced this capability.',
    hint: 'A request would wait in the queue. Start the Orbit executor on an enrolled device and let it announce this capability. This is not an authorization problem.',
  },
  requirement_missing: {
    title: 'A capability this one requires is not installed.',
    hint: 'The capability status above names the ids it requires; every one of them must be installed.',
  },
  not_authorized: {
    title: 'This project has not been granted this capability.',
    hint: 'A grant is an operator step for this project. Signing in from this browser does not grant it.',
  },
}

/**
 * The copy for one availability code. An unavailable offer the server did not
 * name a code for is reported as exactly that, and a code this build does not
 * know is quoted rather than folded onto a wrong reason.
 */
export function availabilityCopy(code: string | null, reason: string | null): AvailabilityCopy {
  const serverReason = reason && reason.trim().length > 0 ? reason.trim() : null
  if (code === null) {
    return {
      title: 'The server marked this action unavailable without naming a reason.',
      hint: 'Reload the item to ask again. Nothing can be requested until the server offers it.',
      serverReason,
    }
  }
  if (isCapabilityAvailabilityCode(code)) {
    return { ...AVAILABILITY_COPY[code], serverReason }
  }
  return {
    title: `The server refused this action (${code}).`,
    hint: 'This build does not know that code. The server’s own reason is quoted below.',
    serverReason,
  }
}

/** `read` / `mutate` / `external_effect`, as words a person can act on. */
export function permissionLabel(permission: string): string {
  switch (permission) {
    case 'read':
      return 'reads data'
    case 'mutate':
      return 'changes stored values'
    case 'external_effect':
      return 'acts outside Orbit'
    case '':
      return 'no declared permission'
    default:
      return `permission “${permission}”`
  }
}

/**
 * Whether the server will demand `confirm: true`. The server's own rule is
 * "destructive action or `external_effect` permission"; the panel adds `mutate`
 * because a stored-value change is what a person is consenting to here.
 */
export function requiresConfirmation(externalEffect: boolean, permission: string): boolean {
  return externalEffect || permission === 'mutate' || permission === 'external_effect'
}

export interface CapabilityConfirmation {
  title: string
  effect: string
  executor: string
}

/** What the confirmation step states, before anything is sent. */
export function capabilityConfirmation(input: {
  actionLabel: string
  externalEffect: boolean
  permission: string
}): CapabilityConfirmation {
  const { actionLabel, externalEffect, permission } = input
  return {
    title: `${actionLabel} will be recorded as a request.`,
    effect: externalEffect
      ? 'When an executor runs it, it changes something outside Orbit. That effect may not be reversible.'
      : permission === 'external_effect'
        ? 'This capability acts outside Orbit. What the executor changes may not be reversible.'
        : 'When an executor runs it, it changes this work item’s stored values.',
    executor:
      'The request waits for an executor to claim it. This browser does not run the action, and nothing here says the item moved until the server’s own item read says so.',
  }
}

/* ------------------------------------------------------------------ */
/* Request form, built from the declared input schema                  */
/* ------------------------------------------------------------------ */

export type CapabilityInputKind = 'text' | 'number' | 'integer' | 'boolean' | 'choice'

export interface CapabilityInputOption {
  /** What the form holds and compares. */
  value: string
  /** What a person reads. */
  label: string
  /** What the server receives for this option. */
  payload: unknown
}

export interface CapabilityInputField {
  name: string
  label: string
  kind: CapabilityInputKind
  required: boolean
  description: string | null
  /** Initial text value for text/number/integer/choice fields. */
  defaultText: string
  /** Initial checked value for a boolean field. */
  defaultChecked: boolean
  options: CapabilityInputOption[]
  minLength: number | null
  maxLength: number | null
  minimum: number | null
  maximum: number | null
}

export type CapabilityFormValue = string | boolean
export type CapabilityFormValues = Record<string, CapabilityFormValue>
export type CapabilityFormErrors = Record<string, string>

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function finiteNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function scalarText(value: unknown): string {
  if (typeof value === 'string') return value
  if (typeof value === 'number' && Number.isFinite(value)) return String(value)
  return ''
}

function optionFor(value: unknown, index: number): CapabilityInputOption {
  const text = scalarText(value)
  return {
    value: text.length > 0 ? text : `option-${index}`,
    label: humanizeIdentifier(scalarText(value)),
    payload: value,
  }
}

/**
 * The declared inputs of one action, in the schema's own order.
 *
 * Supports the subset the server validates: `string` (with `minLength` and
 * `maxLength`), `integer`/`number` (with `minimum` and `maximum`), `boolean`,
 * and any `enum`. A property without a readable `type` becomes a text field, so
 * a future schema still gets a control instead of being silently dropped.
 */
export function capabilityInputsFromSchema(
  schema: Record<string, unknown> | null | undefined,
): CapabilityInputField[] {
  const properties = asRecord(schema?.properties)
  if (!properties) return []
  const required = new Set(
    Array.isArray(schema?.required)
      ? (schema?.required as unknown[]).filter((key): key is string => typeof key === 'string')
      : [],
  )

  return Object.entries(properties).map(([name, raw]) => {
    const property = asRecord(raw) ?? {}
    const declaredType = typeof property.type === 'string' ? property.type : 'string'
    const enumValues = Array.isArray(property.enum) ? property.enum : []
    const defaultValue = property.default
    const kind: CapabilityInputKind =
      enumValues.length > 0
        ? 'choice'
        : declaredType === 'boolean'
          ? 'boolean'
          : declaredType === 'integer'
            ? 'integer'
            : declaredType === 'number'
              ? 'number'
              : 'text'
    const options = enumValues.map(optionFor)
    const defaultOption =
      kind === 'choice'
        ? options.find((option) => option.payload === defaultValue)
        : undefined
    return {
      name,
      label: humanizeIdentifier(name),
      kind,
      required: required.has(name),
      description:
        typeof property.description === 'string' && property.description.trim().length > 0
          ? property.description.trim()
          : null,
      defaultText: defaultOption?.value ?? (kind === 'boolean' ? '' : scalarText(defaultValue)),
      defaultChecked: defaultValue === true,
      options,
      minLength: finiteNumber(property.minLength),
      maxLength: finiteNumber(property.maxLength),
      minimum: finiteNumber(property.minimum),
      maximum: finiteNumber(property.maximum),
    }
  })
}

/** The form's starting values: every declared default, nothing invented. */
export function emptyCapabilityInputs(fields: CapabilityInputField[]): CapabilityFormValues {
  const values: CapabilityFormValues = {}
  for (const field of fields) {
    values[field.name] = field.kind === 'boolean' ? field.defaultChecked : field.defaultText
  }
  return values
}

const INTEGER_PATTERN = /^-?\d+$/

/** Validate before sending. The server validates again; this only saves a round trip. */
export function validateCapabilityInputs(
  fields: CapabilityInputField[],
  values: CapabilityFormValues,
): CapabilityFormErrors {
  const errors: CapabilityFormErrors = {}
  for (const field of fields) {
    const value = values[field.name]
    if (field.kind === 'boolean') continue
    const text = typeof value === 'string' ? value : ''
    if (text.trim().length === 0) {
      if (field.required) errors[field.name] = `${field.label} is required.`
      continue
    }
    if (field.kind === 'choice') {
      if (!field.options.some((option) => option.value === text)) {
        errors[field.name] = `Choose one of the listed values for ${field.label.toLowerCase()}.`
      }
      continue
    }
    if (field.kind === 'integer' || field.kind === 'number') {
      if (field.kind === 'integer' && !INTEGER_PATTERN.test(text.trim())) {
        errors[field.name] = `${field.label} must be a whole number.`
        continue
      }
      const number = Number(text)
      if (!Number.isFinite(number)) {
        errors[field.name] = `${field.label} must be a number.`
        continue
      }
      if (field.minimum !== null && number < field.minimum) {
        errors[field.name] = `${field.label} must be at least ${field.minimum}.`
        continue
      }
      if (field.maximum !== null && number > field.maximum) {
        errors[field.name] = `${field.label} must be at most ${field.maximum}.`
        continue
      }
      continue
    }
    const length = [...text].length
    if (field.minLength !== null && length < field.minLength) {
      errors[field.name] = `${field.label} needs at least ${field.minLength} characters.`
      continue
    }
    if (field.maxLength !== null && length > field.maxLength) {
      errors[field.name] = `${field.label} is limited to ${field.maxLength} characters.`
    }
  }
  return errors
}

export function hasCapabilityErrors(errors: CapabilityFormErrors): boolean {
  return Object.keys(errors).length > 0
}

/**
 * The body's `inputs`: only declared keys, typed as the schema declares them.
 * An empty optional text field is omitted rather than sent as `""`, because the
 * server's schema may refuse a blank value the person never meant to supply.
 */
export function capabilityInputsPayload(
  fields: CapabilityInputField[],
  values: CapabilityFormValues,
): Record<string, unknown> {
  const inputs: Record<string, unknown> = {}
  for (const field of fields) {
    const value = values[field.name]
    if (field.kind === 'boolean') {
      inputs[field.name] = value === true
      continue
    }
    const text = typeof value === 'string' ? value : ''
    if (text.trim().length === 0) continue
    if (field.kind === 'choice') {
      const option = field.options.find((candidate) => candidate.value === text)
      inputs[field.name] = option ? option.payload : text
      continue
    }
    if (field.kind === 'integer' || field.kind === 'number') {
      inputs[field.name] = Number(text)
      continue
    }
    inputs[field.name] = text
  }
  return inputs
}

/** A stable key for "the same payload", so a retry reuses its command id. */
export function capabilityAttemptKey(
  actionId: string,
  itemRevision: number,
  inputs: Record<string, unknown>,
): string {
  const ordered = Object.keys(inputs)
    .sort()
    .map((key) => [key, inputs[key]] as const)
  return `${actionId}@${itemRevision}:${JSON.stringify(ordered)}`
}

/* ------------------------------------------------------------------ */
/* Request states                                                      */
/* ------------------------------------------------------------------ */

export type RequestStateTone = 'neutral' | 'info' | 'success' | 'danger' | 'warning'

export interface RequestStateCopy {
  label: string
  description: string
  tone: RequestStateTone
}

/**
 * What each recorded state means. Nothing here says the item moved: only the
 * work-item read can say that.
 */
export function requestStateCopy(
  request: Pick<CapabilityRequest, 'state' | 'recordedState'>,
): RequestStateCopy {
  const state: KnownCapabilityRequestState = request.state
  switch (state) {
    case 'pending':
      return {
        label: 'Pending',
        description: 'Waiting for an executor to claim it. Nothing has run.',
        tone: 'info',
      }
    case 'claimed':
      return {
        label: 'Claimed',
        description: 'An executor has taken it; no result yet.',
        tone: 'info',
      }
    case 'succeeded':
      return {
        label: 'Succeeded',
        description:
          'An executor reported success. The item’s stored values are read from the server; this report does not move anything by itself.',
        tone: 'success',
      }
    case 'failed':
      return {
        label: 'Failed',
        description: 'An executor reported that the action failed. Orbit does not retry it.',
        tone: 'danger',
      }
    case 'uncertain':
      return {
        label: 'Uncertain',
        description:
          'An executor reported that it could not tell whether the effect happened. Orbit will not retry it; confirm what happened below.',
        tone: 'warning',
      }
    default:
      return {
        label: 'Unrecognised state',
        description: `The server recorded the state “${request.recordedState}”, which this build cannot describe. Nothing is retried.`,
        tone: 'neutral',
      }
  }
}

/** `pending` and `claimed` have no recorded outcome yet. */
export function isUnsettledRequestState(state: string): boolean {
  return state === 'pending' || state === 'claimed'
}

/**
 * A receipt's detail, verbatim. The server stores a JSON document; it is shown
 * as JSON, never summarised into "success" or "a note was added".
 */
export function receiptDetailText(detail: unknown): string {
  if (detail === null || detail === undefined) return 'null'
  if (typeof detail === 'string') return detail
  try {
    return JSON.stringify(detail, null, 2)
  } catch {
    return String(detail)
  }
}

/* ------------------------------------------------------------------ */
/* Reconcile                                                           */
/* ------------------------------------------------------------------ */

/**
 * The two states an explicit settlement may answer.
 *
 * `uncertain`: the executor reported that it could not tell.
 * `claimed`: the executor took the request and no result was recorded — the
 * answer may have been lost. Never `pending` (nothing ran) and never a settled
 * state (the outcome is already recorded).
 */
export type ReconciliableRequestState = 'claimed' | 'uncertain'

export function canReconcile(state: string): state is ReconciliableRequestState {
  return state === 'claimed' || state === 'uncertain'
}

export interface ReconcileCopy {
  /** What this state means, stated as the question the person answers. */
  title: string
  /** Why a decision is needed, and that it records rather than re-runs. */
  description: string
}

const RECONCILE_COPY: Record<ReconciliableRequestState, ReconcileCopy> = {
  uncertain: {
    title: 'The executor could not tell whether its effect landed.',
    description:
      'An executor reported that it could not tell whether the effect happened. Confirm what happened: this records your decision and does not run the action again.',
  },
  claimed: {
    title: 'An executor took this request, and no result was recorded.',
    description:
      'Nothing recorded what the executor did, so the answer may have been lost after the effect happened. Confirm what happened: this records your decision and does not run the action again.',
  },
}

/**
 * The copy for one reconcilable state, or `null` for a state this panel must not
 * settle. The two cases are deliberately worded differently: `uncertain` is an
 * executor admitting it does not know, `claimed` is silence after a claim.
 */
export function reconcileCopy(state: string): ReconcileCopy | null {
  return canReconcile(state) ? RECONCILE_COPY[state] : null
}

export const RECONCILE_DETAIL_MAX = 1000

export interface ReconcileDecisionOption {
  value: CapabilityReconcileDecision
  label: string
  /** What the decision records, in the person's own terms. */
  description: string
  reasonRequired: boolean
}

export const CAPABILITY_RECONCILE_DECISIONS: readonly ReconcileDecisionOption[] = [
  {
    value: 'confirm_succeeded',
    label: 'Confirm it succeeded',
    description:
      'Record that the effect happened. This records your decision; it does not run the action again.',
    reasonRequired: false,
  },
  {
    value: 'confirm_failed',
    label: 'Confirm it failed',
    description:
      'Record that the effect did not happen. This records your decision; it does not run the action again.',
    reasonRequired: true,
  },
]

/** `null` when the reconciliation can be sent; otherwise the reason it cannot. */
export function validateReconcile(
  decision: CapabilityReconcileDecision,
  detail: string,
): string | null {
  const trimmed = detail.trim()
  if ([...trimmed].length > RECONCILE_DETAIL_MAX) {
    return `Keep the reason to ${RECONCILE_DETAIL_MAX} characters or fewer.`
  }
  const option = CAPABILITY_RECONCILE_DECISIONS.find((candidate) => candidate.value === decision)
  if (option?.reasonRequired && trimmed.length === 0) {
    return 'Say what happened before confirming that it failed.'
  }
  return null
}

/* ------------------------------------------------------------------ */
/* Requests already recorded for one action                            */
/* ------------------------------------------------------------------ */

/**
 * The request already recorded for one action on one item revision.
 *
 * The server records an action once per item revision: a second request for the
 * same `(item, revision, action)` is refused as `capability request conflict`,
 * whatever state the first one settled into. The panel therefore shows the
 * recorded request instead of offering the action again — the availability read
 * is the server's own answer and stays untouched.
 */
export function requestForActionRevision(
  requests: CapabilityRequest[] | undefined,
  actionId: string,
  itemRevision: number,
): CapabilityRequest | undefined {
  const recorded = requests ?? []
  for (let index = recorded.length - 1; index >= 0; index -= 1) {
    const request = recorded[index]
    if (request.actionId === actionId && request.itemRevision === itemRevision) return request
  }
  return undefined
}

export interface RecordedActionCopy {
  /** What the recorded request means for this control, in one sentence. */
  title: string
  /** The state it reads, and why there is no second request to make. */
  description: string
}

/**
 * What to say instead of a request control once this action is already recorded
 * for this item revision. The state comes from the history read, so the copy
 * never claims more than the server said.
 */
export function recordedActionCopy(request: CapabilityRequest): RecordedActionCopy {
  const state = requestStateCopy(request).label
  return {
    title: `This action is already requested for item revision ${request.itemRevision}.`,
    description: `It reads ${state}. One action is requested once per item revision, so there is no second request to make here; a later revision of this item can be requested again. The request history below has the record and its receipt.`,
  }
}

/* ------------------------------------------------------------------ */
/* Status summaries                                                    */
/* ------------------------------------------------------------------ */

/** Who announced a capability, as one sentence. Never inferred from a provider. */
export function executorSummary(executors: CapabilityStatus['executors']): string {
  if (executors.length === 0) return 'No executor has announced it.'
  const names = executors.map((executor) => executor.deviceId)
  const listed = names.length > 3 ? `${names.slice(0, 3).join(', ')} and ${names.length - 3} more` : names.join(', ')
  return `${names.length} executor${names.length === 1 ? '' : 's'}: ${listed}`
}

/** The revision the panel asked about, when the answer disagrees with the item. */
export function offersMatchRevision(offersRevision: number, itemRevision: number): boolean {
  return offersRevision === itemRevision
}

/* ------------------------------------------------------------------ */
/* Declared capabilities of one state                                  */
/* ------------------------------------------------------------------ */

export interface DeclaredCapability {
  capabilityId: string
  /** Actions the manifest declares for it on this state's transitions. */
  actionIds: string[]
  /** True when a transition out of this state needs it to move at all. */
  requiredByTransition: boolean
}

/**
 * Every capability the workflow declares for this state, in declaration order.
 * These are the ids the status read is asked about; a capability the workflow
 * does not name is not shown, because nothing here uses it.
 */
export function declaredCapabilitiesForState(
  kind: WorkItemKind | undefined,
  state: string,
): DeclaredCapability[] {
  const declared = new Map<string, DeclaredCapability>()
  const entryFor = (capabilityId: string): DeclaredCapability => {
    const existing = declared.get(capabilityId)
    if (existing) return existing
    const created: DeclaredCapability = {
      capabilityId,
      actionIds: [],
      requiredByTransition: false,
    }
    declared.set(capabilityId, created)
    return created
  }
  for (const transition of transitionsFromState(kind, state)) {
    if (transition.requiredCapability) {
      entryFor(transition.requiredCapability).requiredByTransition = true
    }
    for (const action of transition.actions) {
      const entry = entryFor(action.capability)
      if (!entry.actionIds.includes(action.id)) entry.actionIds.push(action.id)
    }
  }
  return [...declared.values()]
}

/* ------------------------------------------------------------------ */
/* Refusals                                                            */
/* ------------------------------------------------------------------ */

export interface AvailabilityRefusal {
  code: string
  reason: string
}

/**
 * The server's structured refusal, `capability action unavailable: <code>:
 * <reason>`. The browser quotes it verbatim and uses the code to say what would
 * change it, instead of parsing prose for a meaning.
 */
export function parseAvailabilityRefusal(message: string): AvailabilityRefusal | null {
  const match = /^capability action unavailable: ([A-Za-z0-9_]+): ([\s\S]*)$/.exec(message.trim())
  if (!match) return null
  return { code: match[1], reason: match[2] }
}

export interface CapabilityRefusal {
  title: string
  /** The server's own message, quoted; never a rewrite of it. */
  detail: string
}

/** What to say when a request or a reconcile was refused. */
export function capabilityRefusal(error: unknown): CapabilityRefusal {
  const message = isApiError(error) ? scrubInternalNames(error.message) : ''
  const status = isApiError(error) ? error.status : 0

  const unavailable = parseAvailabilityRefusal(message)
  if (unavailable) {
    return { title: availabilityCopy(unavailable.code, unavailable.reason).title, detail: message }
  }
  if (message.startsWith('invalid capability action input:')) {
    return { title: 'The server refused one of the inputs.', detail: message }
  }
  if (message === 'capability action requires confirmation') {
    return {
      title: 'The server needs an explicit confirmation for this action.',
      detail: message,
    }
  }
  if (status === 409) {
    return {
      title: 'The item or this request changed on the server.',
      detail: `${message || 'The server refused the request as a conflict.'} Re-read this item and check the request history; the same intent is never sent twice.`,
    }
  }
  if (status === 403) {
    return {
      title: 'This browser session may not request capability actions.',
      detail: message || 'The server refused this session.',
    }
  }
  if (status === 404) {
    return {
      title: 'The server does not know this work item any more.',
      detail: message || 'It may have been removed from another device.',
    }
  }
  return { title: 'The server did not record this.', detail: userMessage(error) }
}
