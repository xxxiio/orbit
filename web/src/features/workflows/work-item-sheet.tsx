/**
 * The work-item sheet.
 *
 * A work item has no title row of its own: its readable content is the property
 * set its kind declares. The sheet therefore shows the manifest's fields as
 * typed controls, offers exactly the declared transitions for the current
 * state, and lists the actions declared on those transitions without a run
 * control — capability execution is a local-authority feature in this product,
 * and this server exposes neither status nor an execution route.
 *
 * Every command reads the item's revision at mutate time and writes the
 * accepted response back into the list cache inside the hook, so two quick
 * edits can never fence against a revision the person has not seen. The sheet
 * itself writes nothing to the cache.
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import type { WorkItem, WorkItemField, WorkItemKind, Workflow } from '@/api/adapters'
import {
  uncertainMessage,
  useUpdateWorkItemProperties,
  useWorkItemTransition,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Drawer } from '@/components/ui/drawer'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { AlertCircleIcon, CheckCircleIcon, LoaderIcon } from '@/components/ui/icons'
import { Switch } from '@/components/ui/switch'
import {
  EditGuardProvider,
  InlineFieldError,
  LabeledSelect,
  PropertyField,
  useDirtyGuard,
  type EditorHandle,
} from '@/components/forms'
import { humanizeIdentifier } from '@/lib/humanize'
import { workflowText } from '@/domain'
import {
  actionsForState,
  fieldLabel,
  fieldValueText,
  guardSentence,
  kindLabel,
  parameterNames,
  parameterType,
  shortWorkItemId,
  stateLabel,
  transitionsFromState,
  workItemTitle,
} from './work-item-model'

const INTEGER_PATTERN = /^-?\d+$/

function Section({
  title,
  description,
  aside,
  children,
}: {
  title: string
  description?: ReactNode
  aside?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="px-5 py-5" aria-label={title}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-ink">{title}</h3>
          {description ? <p className="mt-0.5 text-xs leading-5 text-ink-muted">{description}</p> : null}
        </div>
        {aside}
      </div>
      <div className="mt-3">{children}</div>
    </section>
  )
}

/** A text property: the draft flushes on blur, Enter, or a guarded close. */
function TextProperty({
  field,
  value,
  pending,
  disabled,
  onCommit,
}: {
  field: WorkItemField
  value: unknown
  pending: boolean
  disabled: boolean
  onCommit: (value: string) => void
}) {
  const committed = typeof value === 'string' ? value : ''
  const [draft, setDraft] = useState(committed)
  const submitted = useRef<string | null>(null)
  useEffect(() => {
    setDraft(committed)
    submitted.current = committed
  }, [committed])
  // Dirty means "not yet sent", not merely "different from the server". The
  // server's value only changes after it answers, so comparing against it alone
  // would flush the same draft again while the write is still in flight.
  const dirty = draft !== committed && draft !== submitted.current

  const commit = () => {
    if (!dirty) return
    submitted.current = draft
    onCommit(draft)
  }

  useDirtyGuard(field.id, () => draft !== committed && draft !== submitted.current, commit)

  return (
    <PropertyField label={fieldLabel(field, field.id)} pending={pending}>
      <Input
        value={draft}
        disabled={disabled}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key !== 'Enter') return
          event.preventDefault()
          commit()
        }}
        placeholder={`Add ${fieldLabel(field, field.id).toLowerCase()}`}
      />
    </PropertyField>
  )
}

/** An integer property. Empty is not sent: the API has no "clear property". */
function IntegerProperty({
  field,
  value,
  pending,
  disabled,
  onCommit,
  onInvalid,
}: {
  field: WorkItemField
  value: unknown
  pending: boolean
  disabled: boolean
  onCommit: (value: number) => void
  onInvalid: (message: string | null) => void
}) {
  const committed = typeof value === 'number' ? String(value) : ''
  const [draft, setDraft] = useState(committed)
  const [error, setError] = useState<string | null>(null)
  const [clearingNote, setClearingNote] = useState<string | null>(null)
  const submitted = useRef<string | null>(null)
  useEffect(() => {
    setDraft(committed)
    submitted.current = committed
  }, [committed])
  const dirty = draft !== committed && draft !== submitted.current

  const commit = () => {
    const trimmed = draft.trim()
    if (trimmed.length === 0) {
      // The API has no "clear property" command. Revert rather than leaving a
      // draft that can never be sent (and would make a close flush forever).
      setDraft(committed)
      submitted.current = committed
      setError(null)
      setClearingNote(
        committed.length > 0
          ? 'Clearing a stored number is not supported by this server; the saved value is kept.'
          : null,
      )
      onInvalid(null)
      return
    }
    if (!INTEGER_PATTERN.test(trimmed)) {
      const message = `${fieldLabel(field, field.id)} must be a whole number.`
      setError(message)
      onInvalid(message)
      return
    }
    setError(null)
    setClearingNote(null)
    onInvalid(null)
    if (dirty) {
      submitted.current = draft
      onCommit(Number(trimmed))
    }
  }

  useDirtyGuard(field.id, () => draft !== committed && draft !== submitted.current, commit)

  return (
    <Field
      label={fieldLabel(field, field.id)}
      error={error ?? undefined}
      hint={
        pending
          ? 'Saving…'
          : (clearingNote ?? 'Whole numbers only. Clearing a stored number is not supported.')
      }
    >
      <Input
        type="number"
        inputMode="numeric"
        step={1}
        value={draft}
        disabled={disabled}
        onChange={(event) => {
          setDraft(event.target.value)
          setClearingNote(null)
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key !== 'Enter') return
          event.preventDefault()
          commit()
        }}
      />
    </Field>
  )
}

function BooleanProperty({
  field,
  value,
  pending,
  disabled,
  onCommit,
}: {
  field: WorkItemField
  value: unknown
  pending: boolean
  disabled: boolean
  onCommit: (value: boolean) => void
}) {
  return (
    <PropertyField label={fieldLabel(field, field.id)} pending={pending}>
      <Switch
        label={value === true ? 'Yes' : 'No'}
        checked={value === true}
        disabled={disabled}
        onCheckedChange={onCommit}
        aria-label={`${fieldLabel(field, field.id)}: ${value === true ? 'yes' : 'no'}`}
      />
    </PropertyField>
  )
}

function ChoiceProperty({
  field,
  value,
  pending,
  disabled,
  onCommit,
}: {
  field: WorkItemField
  value: unknown
  pending: boolean
  disabled: boolean
  onCommit: (value: string) => void
}) {
  const options =
    field.kind.type === 'choice'
      ? field.kind.options.map((option) => ({ value: option, label: humanizeIdentifier(option) }))
      : []
  return (
    <LabeledSelect
      label={fieldLabel(field, field.id)}
      ariaLabel={fieldLabel(field, field.id)}
      value={typeof value === 'string' ? value : ''}
      placeholder="Not set"
      options={options}
      disabled={disabled}
      hint={pending ? 'Saving…' : undefined}
      onValueChange={onCommit}
    />
  )
}

/** One declared transition, with its guards and capability requirement. */
function TransitionRow({
  transition,
  kind,
  item,
  pending,
  disabled,
  error,
  onMove,
}: {
  transition: WorkItemKind['transitions'][number]
  kind: WorkItemKind
  item: WorkItem
  pending: boolean
  disabled: boolean
  error: ApiError | null
  onMove: () => void
}) {
  return (
    <li
      data-work-item-transition={`${transition.from}->${transition.to}`}
      className="rounded-md border border-line bg-surface-sunken/25 px-3 py-3"
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="flex flex-wrap items-center gap-1.5 text-sm font-medium text-ink">
            Move to {stateLabel(transition.to)}
            <Badge tone="neutral">{stateLabel(transition.to)}</Badge>
          </p>
          {transition.guards.length > 0 ? (
            <ul className="mt-1.5 flex flex-col gap-1">
              {transition.guards.map((guard, index) => {
                const holds = guardHolds(item, guard)
                return (
                  <li key={`${guard.type}-${index}`} className="flex items-start gap-1.5">
                    <span
                      aria-hidden="true"
                      className={
                        holds === false
                          ? 'mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-warning'
                          : 'mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-line-strong'
                      }
                    />
                    <span
                      className={
                        holds === false
                          ? 'text-xs leading-5 text-warning-ink'
                          : 'text-xs leading-5 text-ink-muted'
                      }
                    >
                      {guardSentence(guard, kind)}
                      {holds === false ? ' This is not true for the item right now.' : ''}
                    </span>
                  </li>
                )
              })}
            </ul>
          ) : (
            <p className="mt-1 text-xs leading-5 text-ink-muted">
              The workflow declares no guard for this move.
            </p>
          )}
          {transition.requiredCapability ? (
            <p className="mt-1.5 flex flex-wrap items-center gap-1.5 text-2xs leading-4 text-warning-ink">
              <Badge tone="warning">Requires capability {transition.requiredCapability}</Badge>
              <span>
                Browser sessions are granted no capabilities, so the server will refuse this move.
              </span>
            </p>
          ) : null}
        </div>
        <Button
          variant="secondary"
          size="sm"
          disabled={disabled}
          loading={pending}
          onClick={onMove}
          aria-label={`Move work item to ${stateLabel(transition.to)}`}
        >
          Move to {stateLabel(transition.to)}
        </Button>
      </div>
      {error ? <InlineFieldError error={userMessage(error)} /> : null}
    </li>
  )
}

/**
 * Whether a guard is visibly satisfied by the item's own properties. `null`
 * means "this build cannot tell" (a relationship guard); the server still owns
 * the decision and the button is never disabled by this.
 */
function guardHolds(item: WorkItem, guard: WorkItemKind['transitions'][number]['guards'][number]): boolean | null {
  if (guard.type === 'property_equals') {
    return item.properties[guard.field] === guard.value
  }
  if (guard.type === 'property_present') {
    return Object.prototype.hasOwnProperty.call(item.properties, guard.field)
  }
  return null
}

export interface WorkItemSheetProps {
  item: WorkItem | null
  workflow: Workflow | undefined
  onClose: () => void
  onRefresh?: () => void
}

export function WorkItemSheet({ item, workflow, onClose, onRefresh }: WorkItemSheetProps) {
  if (!item) return null
  return (
    <WorkItemSheetPanel
      key={item.id}
      item={item}
      workflow={workflow}
      onClose={onClose}
      onRefresh={onRefresh}
    />
  )
}

function WorkItemSheetPanel({ item, workflow, onClose, onRefresh }: WorkItemSheetProps & { item: WorkItem }) {
  const transition = useWorkItemTransition()
  const properties = useUpdateWorkItemProperties()

  const kind = workflow?.kinds.find((candidate) => candidate.id === item.kind)
  const kindName = kindLabel(kind, item.kind)
  const title = workItemTitle(item, kind)

  const [announcement, setAnnouncement] = useState<string | null>(null)
  const [closeNotice, setCloseNotice] = useState<string | null>(null)
  const [localError, setLocalError] = useState<string | null>(null)
  const editorsRef = useRef(new Map<string, EditorHandle>())
  const flushRequestedRef = useRef(false)
  const register = useCallback((handle: EditorHandle) => {
    editorsRef.current.set(handle.id, handle)
    return () => {
      editorsRef.current.delete(handle.id)
    }
  }, [])

  const transitionError = transition.disposition === 'rejected' ? transition.error : null
  const propertyError = properties.disposition === 'rejected' ? properties.error : null
  const transitionUncertain = transition.disposition === 'uncertain'
  const propertyUncertain = properties.disposition === 'uncertain'
  const busy = transition.isPending || properties.isPending

  useEffect(() => {
    if (!announcement) return
    const timer = window.setTimeout(() => setAnnouncement(null), 6000)
    return () => window.clearTimeout(timer)
  }, [announcement])

  const requestClose = useCallback(() => {
    // Never flush while a write is in flight: the draft may already be the one
    // being written, and a second copy would fence on the same revision.
    if (busy) {
      setCloseNotice('Waiting for the server to answer the last change. Close again when it settles.')
      return
    }
    const dirty = [...editorsRef.current.values()].filter((editor) => editor.isDirty())
    if (dirty.length > 0) {
      if (flushRequestedRef.current) {
        // A previous close flushed and nothing is pending: the draft has either
        // been rejected or cannot be sent (an invalid number). Discard it.
        flushRequestedRef.current = false
        setCloseNotice(null)
        onClose()
        return
      }
      flushRequestedRef.current = true
      setCloseNotice('Your edit is being saved first. Close again when the server answers.')
      for (const editor of dirty) editor.flush()
      return
    }
    flushRequestedRef.current = false
    setCloseNotice(null)
    onClose()
  }, [busy, onClose])

  const updateProperty = async (fieldId: string, value: unknown) => {
    setLocalError(null)
    try {
      await properties.mutateAsync({ id: item.id, changes: { [fieldId]: value } })
      flushRequestedRef.current = false
      const field = kind?.fields.find((candidate) => candidate.id === fieldId)
      setAnnouncement(`${fieldLabel(field, fieldId)} saved.`)
    } catch {
      // `properties.error` carries the server's message; the item stays as stored.
    }
  }

  const moveTo = async (to: string) => {
    setLocalError(null)
    try {
      const accepted = await transition.mutateAsync({ id: item.id, to })
      flushRequestedRef.current = false
      setAnnouncement(`The server moved this item to ${stateLabel(accepted.state)}.`)
    } catch {
      // `transition.error` is shown next to the transition and in the footer.
    }
  }

  const stateTransitions = kind ? transitionsFromState(kind, item.state) : []
  const stateActions = kind ? actionsForState(kind, item.state) : []

  return (
    <Drawer
      open
      onOpenChange={(next) => {
        if (!next) requestClose()
      }}
      side="right"
      title={`${title} — work item details`}
      description="Work item properties, moves and declared actions."
      closeLabel="Close work item details"
      className="w-full max-w-none border-l border-line sm:w-[min(42rem,94vw)]"
    >
      <EditGuardProvider register={register}>
        <div className="flex h-full min-h-0 flex-col">
          <header className="border-b border-line px-5 pb-4 pr-12 pt-5">
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge tone="accent">{kindName}</Badge>
              <Badge tone="neutral" dot>
                {stateLabel(item.state)}
              </Badge>
              <span className="font-mono text-2xs text-ink-muted">Revision {item.revision}</span>
              <span className="font-mono text-2xs text-ink-faint" title={item.id}>
                {shortWorkItemId(item.id)}
              </span>
            </div>
            <h2 className="mt-2 font-display text-xl font-medium leading-7 tracking-tight text-ink">
              {title}
            </h2>
            {!kind ? (
              <p className="mt-2 rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5 text-xs leading-5 text-warning-ink">
                The pinned workflow does not declare the kind “{item.kind}”, so its properties,
                moves and actions cannot be named. The stored values are unchanged.
              </p>
            ) : null}
          </header>

          <div className="scrollbar-slim flex-1 overflow-y-auto">
            <div className="divide-y divide-line">
              <Section
                title="Properties"
                description="Typed by the workflow's declaration. A value is saved when you leave the field, and the server's answer is shown here."
                aside={
                  properties.isPending ? (
                    <LoaderIcon size={14} className="animate-spin text-accent" aria-label="Saving a property" />
                  ) : (
                    <span className="whitespace-nowrap font-mono text-2xs text-ink-faint">revision {item.revision}</span>
                  )
                }
              >
                {kind && kind.fields.length > 0 ? (
                  <div className="grid gap-x-4 gap-y-3 sm:grid-cols-2">
                    {kind.fields.map((field) => {
                      const value = item.properties[field.id]
                      const pending =
                        properties.isPending &&
                        Boolean(properties.variables?.changes && field.id in properties.variables.changes)
                      const fieldError =
                        propertyError &&
                        properties.variables?.changes &&
                        field.id in properties.variables.changes
                          ? propertyError
                          : null
                      const control =
                        field.kind.type === 'text' ? (
                          <TextProperty
                            field={field}
                            value={value}
                            pending={pending}
                            disabled={busy}
                            onCommit={(next) => void updateProperty(field.id, next)}
                          />
                        ) : field.kind.type === 'integer' ? (
                          <IntegerProperty
                            field={field}
                            value={value}
                            pending={pending}
                            disabled={busy}
                            onCommit={(next) => void updateProperty(field.id, next)}
                            onInvalid={setLocalError}
                          />
                        ) : field.kind.type === 'boolean' ? (
                          <BooleanProperty
                            field={field}
                            value={value}
                            pending={pending}
                            disabled={busy}
                            onCommit={(next) => void updateProperty(field.id, next)}
                          />
                        ) : (
                          <ChoiceProperty
                            field={field}
                            value={value}
                            pending={pending}
                            disabled={busy}
                            onCommit={(next) => void updateProperty(field.id, next)}
                          />
                        )
                      return (
                        <div key={field.id} data-work-item-field={field.id}>
                          {control}
                          {fieldError ? <InlineFieldError error={userMessage(fieldError)} /> : null}
                        </div>
                      )
                    })}
                  </div>
                ) : kind ? (
                  <p className="text-xs leading-5 text-ink-muted">
                    This kind declares no fields. The item stores only its state and revision.
                  </p>
                ) : null}
                <div className="mt-3 flex flex-col gap-1">
                  <InlineFieldError error={localError} />
                  <InlineFieldError error={propertyError ? userMessage(propertyError) : null} />
                  {propertyUncertain ? (
                    <p role="alert" className="text-xs leading-5 text-warning-ink">
                      {uncertainMessage(properties.error ?? undefined)}
                    </p>
                  ) : null}
                </div>
              </Section>

              <Section
                title="Moves"
                description="Only the transitions the workflow declares for this state are offered. The server re-checks the move; a refusal is quoted below and the item stays where it is."
                aside={
                  <Badge tone="neutral">
                    {stateTransitions.length} declared
                  </Badge>
                }
              >
                {kind ? (
                  stateTransitions.length > 0 ? (
                    <ul className="flex flex-col gap-2">
                      {stateTransitions.map((candidate) => (
                        <TransitionRow
                          key={`${candidate.from}->${candidate.to}`}
                          transition={candidate}
                          kind={kind}
                          item={item}
                          pending={transition.isPending && transition.variables?.to === candidate.to}
                          disabled={busy}
                          error={
                            transitionError && transition.variables?.to === candidate.to
                              ? transitionError
                              : null
                          }
                          onMove={() => void moveTo(candidate.to)}
                        />
                      ))}
                    </ul>
                  ) : (
                    <p className="text-xs leading-5 text-ink-muted">
                      The workflow declares no move out of {stateLabel(item.state)}.
                    </p>
                  )
                ) : (
                  <p className="text-xs leading-5 text-ink-muted">
                    The kind is not in the pinned manifest, so its moves cannot be named.
                  </p>
                )}
                <div className="mt-2 flex flex-col gap-1">
                  {transitionError && !transition.variables?.to ? (
                    <p role="alert" className="text-xs leading-5 text-danger-ink">
                      {userMessage(transitionError)}
                    </p>
                  ) : null}
                  {transitionUncertain ? (
                    <p role="alert" className="text-xs leading-5 text-warning-ink">
                      {uncertainMessage(transition.error ?? undefined)}
                    </p>
                  ) : null}
                </div>
              </Section>

              <Section
                title="Actions"
                description={`Actions the workflow declares for ${stateLabel(item.state)}.`}
                aside={<Badge tone="neutral">{stateActions.length} declared</Badge>}
              >
                <div className="rounded-md border border-info/30 bg-info-soft/50 px-3 py-2 text-xs leading-5 text-info-ink">
                  <p className="font-medium">Execution requires the local Orbit authority.</p>
                  <p className="mt-0.5">
                    This server does not execute capabilities, and it exposes no capability status,
                    so the browser lists what is declared and does not offer a run control.
                  </p>
                </div>
                <p className="mt-1.5 text-2xs leading-4 text-ink-muted">
                  Capability status is not exposed by this server.
                </p>

                {stateActions.length > 0 ? (
                  <ul className="mt-3 flex flex-col gap-2">
                    {stateActions.map(({ transition: edge, action }) => (
                      <li
                        key={`${edge.from}->${edge.to}#${action.id}`}
                        data-work-item-action={action.id}
                        className="rounded-md border border-line bg-surface px-3 py-2.5"
                      >
                        <div className="flex flex-wrap items-start justify-between gap-2">
                          <div className="min-w-0">
                            <p className="text-sm font-medium text-ink">
                              {humanizeIdentifier(workflowText(action.id))}
                            </p>
                            <p className="mt-0.5 text-xs leading-5 text-ink-muted">
                              {workflowText(action.description).trim() ||
                                'The workflow gives this action no description.'}
                            </p>
                            <p className="mt-1 text-2xs text-ink-faint">
                              Declared on {stateLabel(edge.from)} → {stateLabel(edge.to)}
                            </p>
                          </div>
                          <Badge tone="neutral">Not executable here</Badge>
                        </div>
                        <div className="mt-2 flex flex-wrap items-center gap-1.5">
                          <Badge tone="info">Requires capability {action.capability}</Badge>
                          {edge.requiredCapability && edge.requiredCapability !== action.capability ? (
                            <Badge tone="warning">Move requires {edge.requiredCapability}</Badge>
                          ) : null}
                        </div>
                        <div className="mt-2 border-t border-line pt-2">
                          <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                            Parameters
                          </p>
                          {parameterNames(action).length > 0 ? (
                            <ul className="mt-1 flex flex-wrap gap-1.5">
                              {parameterNames(action).map((name) => (
                                <li
                                  key={name}
                                  className="rounded-sm border border-line bg-surface-sunken px-1.5 py-0.5 font-mono text-2xs text-ink-soft"
                                >
                                  {name}
                                  <span className="ml-1 text-ink-faint">
                                    {parameterType(action.parameters[name])}
                                  </span>
                                </li>
                              ))}
                            </ul>
                          ) : (
                            <p className="mt-0.5 text-xs text-ink-muted">No parameters declared.</p>
                          )}
                        </div>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="mt-3 text-xs leading-5 text-ink-muted">
                    This state declares no actions. Capability status is not exposed by this server,
                    so no installed-capability state is shown.
                  </p>
                )}
              </Section>

              <Section title="Declared values" description="What the server stores for this item, in declaration order.">
                {kind ? (
                  <dl className="grid gap-x-4 gap-y-2 sm:grid-cols-2">
                    {kind.fields.map((field) => (
                      <div key={field.id} className="min-w-0">
                        <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                          {fieldLabel(field, field.id)}
                        </dt>
                        <dd className="break-words text-sm text-ink-soft">
                          {fieldValueText(field, item.properties[field.id])}
                        </dd>
                      </div>
                    ))}
                    <div>
                      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                        State
                      </dt>
                      <dd className="text-sm text-ink-soft">{stateLabel(item.state)}</dd>
                    </div>
                    <div>
                      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                        Revision
                      </dt>
                      <dd className="font-mono text-sm text-ink-soft">{item.revision}</dd>
                    </div>
                  </dl>
                ) : null}
              </Section>
            </div>
          </div>

          <footer className="flex flex-col gap-1.5 border-t border-line bg-surface px-5 py-3">
            <div className="flex items-center gap-2 text-xs">
              {busy ? (
                <>
                  <LoaderIcon size={13} className="animate-spin text-accent" />
                  <span className="text-ink-muted">Waiting for the server…</span>
                </>
              ) : announcement ? (
                <>
                  <CheckCircleIcon size={13} className="text-success" />
                  <span className="text-ink-muted" aria-live="polite">
                    {announcement}
                  </span>
                </>
              ) : (
                <span className="text-ink-faint">
                  Property edits save when you leave the field. Moves are checked by the server.
                </span>
              )}
            </div>
            {closeNotice ? (
              <p role="status" className="rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5 text-xs leading-5 text-warning-ink">
                {closeNotice}
              </p>
            ) : null}
            {(propertyError || transitionError) ? (
              <p
                role="alert"
                className="flex items-start gap-1.5 rounded-sm border border-danger/30 bg-danger-soft px-2 py-1.5 text-xs leading-5 text-danger-ink"
              >
                <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                <span className="min-w-0">
                  The server refused the change: {userMessage(propertyError ?? transitionError)}
                  {onRefresh ? (
                    <button
                      type="button"
                      onClick={onRefresh}
                      className="ml-1 font-medium underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                    >
                      Refresh the item and try again
                    </button>
                  ) : null}
                </span>
              </p>
            ) : null}
            {propertyUncertain || transitionUncertain ? (
              <p
                role="alert"
                className="flex items-start gap-1.5 rounded-sm border border-warning/40 bg-warning-soft px-2 py-1.5 text-xs leading-5 text-warning-ink"
              >
                <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                <span className="min-w-0">
                  {uncertainMessage((propertyUncertain ? properties.error : transition.error) ?? undefined)}
                  {onRefresh ? (
                    <button
                      type="button"
                      onClick={onRefresh}
                      className="ml-1 font-medium underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                    >
                      Refresh the item list
                    </button>
                  ) : null}
                </span>
              </p>
            ) : null}
            <p role="status" aria-live="polite" className="sr-only">
              {announcement}
            </p>
          </footer>
        </div>
      </EditGuardProvider>
    </Drawer>
  )
}
