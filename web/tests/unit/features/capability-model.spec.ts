import { describe, expect, it } from 'vitest'
import type { WorkItemKind } from '@/api/adapters'
import { ApiError } from '@/api/errors'
import {
  CAPABILITY_AVAILABILITY_CODES,
  CAPABILITY_RECONCILE_DECISIONS,
  availabilityCopy,
  canReconcile,
  capabilityAttemptKey,
  capabilityConfirmation,
  capabilityInputsFromSchema,
  capabilityInputsPayload,
  capabilityRefusal,
  declaredCapabilitiesForState,
  emptyCapabilityInputs,
  hasCapabilityErrors,
  isUnsettledRequestState,
  parseAvailabilityRefusal,
  permissionLabel,
  receiptDetailText,
  reconcileCopy,
  requestStateCopy,
  requiresConfirmation,
  validateCapabilityInputs,
  validateReconcile,
  type ReconcileCopy,
} from '@/features/workflows/capability-model'

/**
 * The pure half of the capability panel: how an availability code reads, how the
 * request form is derived from the declared schema, what each request state
 * means, and what may be reconciled. Nothing here touches the DOM, so every rule
 * the panel obeys is pinned in one place.
 */

describe('availability copy', () => {
  it('gives every code its own sentence, and only executor_missing says an executor is missing', () => {
    const rendered = CAPABILITY_AVAILABILITY_CODES.map((code) => availabilityCopy(code, null))
    const titles = rendered.map((copy) => copy.title)
    expect(new Set(titles).size).toBe(CAPABILITY_AVAILABILITY_CODES.length)

    const executor = availabilityCopy('executor_missing', 'no active enrolled executor announced x')
    expect(executor.title).toBe('No executor has announced this capability.')
    expect(executor.hint).toMatch(/request would wait in the queue/i)
    // The distinction the server insists on: this is not an authorization fact.
    expect(executor.hint).toMatch(/not an authorization problem/i)
    expect(executor.hint).not.toMatch(/not authorized/i)

    const unauthorized = availabilityCopy('not_authorized', null)
    expect(unauthorized.title).toBe('This project has not been granted this capability.')
    expect(unauthorized.hint).toMatch(/operator/i)
    expect(unauthorized.title).not.toMatch(/executor/i)
  })

  it('keeps the server reason verbatim and names an unknown code instead of guessing', () => {
    const copy = availabilityCopy(
      'executor_missing',
      'no active enrolled executor announced orbit.reference.work-item-note',
    )
    expect(copy.serverReason).toBe(
      'no active enrolled executor announced orbit.reference.work-item-note',
    )

    const unknown = availabilityCopy('something_new', 'the server knows better')
    expect(unknown.title).toBe('The server refused this action (something_new).')
    expect(unknown.serverReason).toBe('the server knows better')

    const unnamed = availabilityCopy(null, null)
    expect(unnamed.title).toMatch(/without naming a reason/)
    expect(unnamed.serverReason).toBeNull()
  })

  it('reads the structured refusal back into the same vocabulary', () => {
    expect(
      parseAvailabilityRefusal(
        'capability action unavailable: executor_missing: no active enrolled executor announced orbit.reference.work-item-note',
      ),
    ).toEqual({
      code: 'executor_missing',
      reason: 'no active enrolled executor announced orbit.reference.work-item-note',
    })
    expect(parseAvailabilityRefusal('capability action requires confirmation')).toBeNull()

    const unavailable = capabilityRefusal(
      new ApiError(
        400,
        'capability action unavailable: not_authorized: capability x is not authorized for project p',
      ),
    )
    expect(unavailable.title).toBe('This project has not been granted this capability.')
    expect(unavailable.detail).toContain('capability action unavailable: not_authorized')

    const invalid = capabilityRefusal(
      new ApiError(400, 'invalid capability action input: invalid value for note'),
    )
    expect(invalid.title).toBe('The server refused one of the inputs.')
    expect(invalid.detail).toBe('invalid capability action input: invalid value for note')

    const conflict = capabilityRefusal(new ApiError(409, 'capability request conflict'))
    expect(conflict.title).toMatch(/changed on the server/)
    expect(conflict.detail).toMatch(/never sent twice/)

    const forbidden = capabilityRefusal(new ApiError(403, 'executor credential required'))
    expect(forbidden.title).toMatch(/browser session may not request/)

    const unknownFailure = capabilityRefusal(new ApiError(0, 'Could not reach the Orbit server.'))
    expect(unknownFailure.title).toBe('The server did not record this.')
  })
})

describe('permission and confirmation', () => {
  it('requires confirmation for a mutating or externally effective action', () => {
    expect(requiresConfirmation(true, 'read')).toBe(true)
    expect(requiresConfirmation(false, 'mutate')).toBe(true)
    expect(requiresConfirmation(false, 'external_effect')).toBe(true)
    expect(requiresConfirmation(false, 'read')).toBe(false)
    expect(permissionLabel('mutate')).toBe('changes stored values')
  })

  it('says an executor runs the action, not the browser', () => {
    const confirmation = capabilityConfirmation({
      actionLabel: 'Append Note',
      externalEffect: false,
      permission: 'mutate',
    })
    expect(confirmation.title).toBe('Append Note will be recorded as a request.')
    expect(confirmation.effect).toMatch(/changes this work item/i)
    expect(confirmation.executor).toMatch(/waits for an executor/i)
    expect(confirmation.executor).toMatch(/This browser does not run the action/i)
    expect(confirmation.executor).toMatch(/nothing here says the item moved/i)

    const external = capabilityConfirmation({
      actionLabel: 'Publish',
      externalEffect: true,
      permission: 'read',
    })
    expect(external.effect).toMatch(/outside Orbit/)
    expect(external.effect).toMatch(/may not be reversible/)
  })
})

describe('request form from the declared schema', () => {
  const schema = {
    type: 'object',
    properties: {
      note: { type: 'string', minLength: 1, maxLength: 20, description: 'Note body' },
      tag: { type: 'string' },
      count: { type: 'integer', minimum: 1, maximum: 9, default: 2 },
      ratio: { type: 'number' },
      visible: { type: 'boolean', default: true },
      mode: { type: 'string', enum: ['fast', 'careful'], default: 'careful' },
    },
    required: ['note'],
  }

  it('derives one typed field per declared property, with its default', () => {
    const fields = capabilityInputsFromSchema(schema)
    expect(fields.map((field) => [field.name, field.kind, field.required])).toEqual([
      ['note', 'text', true],
      ['tag', 'text', false],
      ['count', 'integer', false],
      ['ratio', 'number', false],
      ['visible', 'boolean', false],
      ['mode', 'choice', false],
    ])
    expect(fields[0].minLength).toBe(1)
    expect(fields[0].maxLength).toBe(20)
    expect(fields[0].description).toBe('Note body')
    expect(fields[2].minimum).toBe(1)
    expect(fields[2].maximum).toBe(9)

    const values = emptyCapabilityInputs(fields)
    expect(values).toEqual({
      note: '',
      tag: '',
      count: '2',
      ratio: '',
      visible: true,
      mode: 'careful',
    })
    expect(fields[5].options.map((option) => option.label)).toEqual(['Fast', 'Careful'])
  })

  it('validates required, length, type, range and enum before sending', () => {
    const fields = capabilityInputsFromSchema(schema)
    const empty = emptyCapabilityInputs(fields)
    empty.count = 'abc'
    empty.mode = 'invented'
    const errors = validateCapabilityInputs(fields, empty)
    expect(hasCapabilityErrors(errors)).toBe(true)
    expect(errors.note).toBe('Note is required.')
    expect(errors.count).toBe('Count must be a whole number.')
    expect(errors.mode).toMatch(/Choose one of the listed values/)

    const tooLong = { ...empty, note: 'x'.repeat(21), count: '2', mode: 'fast' }
    expect(validateCapabilityInputs(fields, tooLong).note).toBe(
      'Note is limited to 20 characters.',
    )

    const outOfRange = { ...tooLong, note: 'ok', count: '12' }
    expect(validateCapabilityInputs(fields, outOfRange).count).toBe('Count must be at most 9.')

    const blank = { ...tooLong, note: '   ' }
    expect(validateCapabilityInputs(fields, blank).note).toBe('Note is required.')

    const valid = { ...tooLong, note: 'ok' }
    expect(validateCapabilityInputs(fields, valid)).toEqual({})
  })

  it('sends typed values and omits an empty optional text field', () => {
    const fields = capabilityInputsFromSchema(schema)
    const inputs = capabilityInputsPayload(fields, {
      note: 'hello',
      tag: '',
      count: '4',
      ratio: '1.5',
      visible: false,
      mode: 'fast',
    })
    expect(inputs).toEqual({
      note: 'hello',
      count: 4,
      ratio: 1.5,
      visible: false,
      mode: 'fast',
    })
    expect('tag' in inputs).toBe(false)
  })

  it('bounds an optional field exactly as the schema declares it', () => {
    // The reference action's shape: a bounded note and a bounded optional tag.
    // An optional input with no declared bound is one the effect contract refuses
    // only after an executor has already run.
    const fields = capabilityInputsFromSchema({
      type: 'object',
      properties: {
        note: { type: 'string', minLength: 1, maxLength: 2000 },
        tag: { type: 'string', maxLength: 64 },
      },
      required: ['note'],
    })
    const tag = fields.find((field) => field.name === 'tag')
    expect(tag?.maxLength).toBe(64)
    expect(
      validateCapabilityInputs(fields, { note: 'hello', tag: 't'.repeat(65) }).tag,
    ).toBe('Tag is limited to 64 characters.')
    expect(validateCapabilityInputs(fields, { note: 'hello', tag: 't'.repeat(64) })).toEqual({})
  })

  it('survives a schema it cannot fully read', () => {
    expect(capabilityInputsFromSchema(null)).toEqual([])
    expect(capabilityInputsFromSchema({ type: 'object' })).toEqual([])
    const unknown = capabilityInputsFromSchema({
      type: 'object',
      properties: { mystery: { description: 'no type' }, list: { type: 'array' } },
    })
    // A property without a readable type still gets a control: text, never dropped.
    expect(unknown.map((field) => [field.name, field.kind])).toEqual([
      ['mystery', 'text'],
      ['list', 'text'],
    ])
  })

  it('pins the attempt key to the action, revision and payload', () => {
    expect(capabilityAttemptKey('append_note', 3, { note: 'a', tag: 'b' })).toBe(
      capabilityAttemptKey('append_note', 3, { tag: 'b', note: 'a' }),
    )
    expect(capabilityAttemptKey('append_note', 3, { note: 'a' })).not.toBe(
      capabilityAttemptKey('append_note', 4, { note: 'a' }),
    )
  })
})

describe('request state copy', () => {
  const request = (state: string) => ({ state, recordedState: state }) as never

  it('describes each state without ever claiming the item moved', () => {
    const pending = requestStateCopy(request('pending'))
    expect(pending.label).toBe('Pending')
    expect(pending.description).toMatch(/Waiting for an executor to claim it/)
    expect(isUnsettledRequestState('pending')).toBe(true)

    const claimed = requestStateCopy(request('claimed'))
    expect(claimed.description).toBe('An executor has taken it; no result yet.')
    expect(isUnsettledRequestState('claimed')).toBe(true)

    const succeeded = requestStateCopy(request('succeeded'))
    expect(succeeded.description).toMatch(/does not move anything by itself/)

    for (const state of ['pending', 'claimed', 'succeeded', 'failed', 'uncertain']) {
      expect(requestStateCopy(request(state)).description).not.toMatch(
        /(moved|is now in|completed the item)/i,
      )
    }
  })

  it('names an unrecognised state instead of inventing one', () => {
    const copy = requestStateCopy({ state: 'unknown', recordedState: 'escalated' })
    expect(copy.label).toBe('Unrecognised state')
    expect(copy.description).toContain('escalated')
  })
})

describe('reconcile allow-list', () => {
  it('offers a reconcile for exactly the two states nothing settled', () => {
    expect(canReconcile('uncertain')).toBe(true)
    expect(canReconcile('claimed')).toBe(true)
    for (const state of ['pending', 'succeeded', 'failed', 'unknown', '']) {
      expect(canReconcile(state)).toBe(false)
    }
  })

  it('states the two reconcilable cases as the different questions they are', () => {
    const copy = (state: string) => {
      const value = reconcileCopy(state)
      expect(value, `${state} must be reconcilable`).not.toBeNull()
      return value as ReconcileCopy
    }

    // `uncertain`: the executor said it could not tell.
    const uncertain = copy('uncertain')
    expect(uncertain.title).toBe('The executor could not tell whether its effect landed.')
    expect(uncertain.description).toMatch(/could not tell whether the effect happened/i)
    expect(uncertain.description).not.toMatch(/answer may have been lost/i)

    // `claimed`: nothing recorded a result, so the answer may have been lost.
    const claimed = copy('claimed')
    expect(claimed.title).toBe('An executor took this request, and no result was recorded.')
    expect(claimed.description).toMatch(/no result|nothing recorded/i)
    expect(claimed.description).toMatch(/answer may have been lost/i)
    expect(claimed.description).not.toMatch(/could not tell/i)

    // Neither one claims to know the effect happened, and both say a decision is
    // recorded rather than executed.
    for (const value of [uncertain, claimed]) {
      expect(value.title).not.toBe('')
      expect(value.description).toMatch(/records your decision/i)
      expect(value.description).toMatch(/does not run the action again/i)
      expect(`${value.title} ${value.description}`).not.toMatch(/retry|re-?run/i)
    }

    // A state this panel may not settle has no copy to show.
    for (const state of ['pending', 'succeeded', 'failed', 'unknown']) {
      expect(reconcileCopy(state)).toBeNull()
    }
  })

  it('offers both reconcilable states the same two decisions, and no retry', () => {
    expect(CAPABILITY_RECONCILE_DECISIONS.map((option) => option.label)).toEqual([
      'Confirm it succeeded',
      'Confirm it failed',
    ])
    expect(
      CAPABILITY_RECONCILE_DECISIONS.map((option) => option.label).join(' '),
    ).not.toMatch(/retry|re-?run|re-?send/i)
  })

  it('needs a reason to confirm a failure, and nothing to confirm a success', () => {
    expect(validateReconcile('confirm_failed', '  ')).toBe(
      'Say what happened before confirming that it failed.',
    )
    expect(validateReconcile('confirm_failed', 'the process died before writing')).toBeNull()
    expect(validateReconcile('confirm_succeeded', '')).toBeNull()
    expect(validateReconcile('confirm_succeeded', 'x'.repeat(1001))).toBe(
      'Keep the reason to 1000 characters or fewer.',
    )
  })

  it('renders a receipt detail as the stored JSON document', () => {
    expect(receiptDetailText({ note: 'a', appended: true })).toBe(
      '{\n  "note": "a",\n  "appended": true\n}',
    )
    expect(receiptDetailText('plain text')).toBe('plain text')
    expect(receiptDetailText(null)).toBe('null')
  })
})

describe('declared capabilities of a state', () => {
  const kind: WorkItemKind = {
    id: 'ticket',
    name: 'Ticket',
    initialState: 'open',
    states: ['open', 'closed'],
    fields: [],
    transitions: [
      {
        from: 'open',
        to: 'closed',
        requiredCapability: 'orbit.release.approve',
        guards: [],
        actions: [
          {
            id: 'append_note',
            capability: 'orbit.reference',
            description: 'Append',
            parameters: {},
          },
          {
            id: 'append_tag',
            capability: 'orbit.reference',
            description: 'Tag',
            parameters: {},
          },
        ],
      },
    ],
  }

  it('lists each declared capability once, in declaration order', () => {
    const declared = declaredCapabilitiesForState(kind, 'open')
    expect(declared.map((entry) => entry.capabilityId)).toEqual([
      'orbit.release.approve',
      'orbit.reference',
    ])
    expect(declared[1].actionIds).toEqual(['append_note', 'append_tag'])
    expect(declared[0].requiredByTransition).toBe(true)
    expect(declared[0].actionIds).toEqual([])
  })

  it('declares nothing for a state with no outgoing transition', () => {
    expect(declaredCapabilitiesForState(kind, 'closed')).toEqual([])
    expect(declaredCapabilitiesForState(undefined, 'open')).toEqual([])
  })
})
