/**
 * The task drawer's Planning group.
 *
 * `set_planning` is one command for three fields, so the group holds one
 * explicit draft window (three input strings) and every commit parses that
 * whole window before building the patch. Nothing is shown as saved before the
 * server answers; a rejected command resets each draft to the stored record
 * (`error` is part of that reset), so a refused value can never linger next to
 * the server's truth.
 *
 * The window also registers with the drawer's close guard: a draft that has not
 * been sent is flushed through the same `setPlanning` path when the drawer is
 * asked to close, so Escape cannot silently discard an edit.
 */
import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import type { ApiError } from '@/api/errors'
import { useDirtyGuard } from '@/components/forms'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Section } from '@/components/ui/section'
import { FieldError, SavingNote } from './section'
import {
  parseDateTimeLocalInput,
  parseEstimateInput,
  planningPatchFromTask,
  toDateTimeLocalInput,
  type PlanningPatch,
} from './planning-fields'

export type PlanningField = 'estimate' | 'earliestStart' | 'deadline'

interface PlanningTask {
  estimatedMinutes: number | null
  earliestStartAtMs: number | null
  deadlineAtMs: number | null
}

/**
 * The whole planning window as the person sees it: strings while editing, so a
 * partly typed date is never coerced into a different instant.
 */
interface PlanningDraft {
  estimate: string
  earliestStart: string
  deadline: string
}

export interface PlanningSectionProps {
  task: PlanningTask
  pending: PlanningField | null
  disabled: boolean
  /** The server's refusal for this group, when the last command was rejected. */
  error: ApiError | null
  /** Offered beside a conflict so the person can recover without leaving. */
  onRefresh?: () => void
  onCommit: (field: PlanningField, patch: PlanningPatch) => void
}

const FIELD_ORDER: PlanningField[] = ['estimate', 'earliestStart', 'deadline']

function estimateInput(minutes: number | null): string {
  return minutes === null ? '' : String(minutes)
}

function draftFromTask(task: PlanningTask): PlanningDraft {
  return {
    estimate: estimateInput(task.estimatedMinutes),
    earliestStart: toDateTimeLocalInput(task.earliestStartAtMs),
    deadline: toDateTimeLocalInput(task.deadlineAtMs),
  }
}

function sameDraft(left: PlanningDraft, right: PlanningDraft): boolean {
  return (
    left.estimate === right.estimate &&
    left.earliestStart === right.earliestStart &&
    left.deadline === right.deadline
  )
}

function storedField(task: PlanningTask, field: PlanningField): string {
  switch (field) {
    case 'estimate':
      return estimateInput(task.estimatedMinutes)
    case 'earliestStart':
      return toDateTimeLocalInput(task.earliestStartAtMs)
    case 'deadline':
      return toDateTimeLocalInput(task.deadlineAtMs)
  }
}

export function PlanningSection({
  task,
  pending,
  disabled,
  error,
  onRefresh,
  onCommit,
}: PlanningSectionProps) {
  const [draft, setDraft] = useState<PlanningDraft>(() => draftFromTask(task))
  const [localError, setLocalError] = useState<string | null>(null)

  const draftRef = useRef(draft)
  draftRef.current = draft
  const taskRef = useRef(task)
  taskRef.current = task
  const onCommitRef = useRef(onCommit)
  onCommitRef.current = onCommit

  /**
   * The values the server has either stored or just been sent. A draft that
   * equals this baseline is not unsent: closing while a commit is in flight
   * waits for the answer instead of sending the same window twice.
   */
  const baselineRef = useRef<PlanningDraft>(draftFromTask(task))

  // Each stored field resets only its own draft, so an accepted estimate never
  // clobbers a deadline the person is still typing. A refusal resets every
  // draft: the task record does not change when the server refuses, so `error`
  // is the signal that the stored values are the truth again.
  useEffect(() => {
    const value = estimateInput(task.estimatedMinutes)
    setDraft((current) => ({ ...current, estimate: value }))
    baselineRef.current = { ...baselineRef.current, estimate: value }
  }, [task.estimatedMinutes])
  useEffect(() => {
    const value = toDateTimeLocalInput(task.earliestStartAtMs)
    setDraft((current) => ({ ...current, earliestStart: value }))
    baselineRef.current = { ...baselineRef.current, earliestStart: value }
  }, [task.earliestStartAtMs])
  useEffect(() => {
    const value = toDateTimeLocalInput(task.deadlineAtMs)
    setDraft((current) => ({ ...current, deadline: value }))
    baselineRef.current = { ...baselineRef.current, deadline: value }
  }, [task.deadlineAtMs])
  useEffect(() => {
    if (!error) return
    const reset = draftFromTask(taskRef.current)
    draftRef.current = reset
    baselineRef.current = reset
    setDraft(reset)
    setLocalError(null)
  }, [error, task.estimatedMinutes, task.earliestStartAtMs, task.deadlineAtMs])

  /**
   * Parse the explicit draft window and send it as one patch.
   *
   * `flush` (close) and a blur both come through here, so the close guard and a
   * normal blur cannot disagree about what the window currently holds. An
   * invalid field is rolled back to its stored value and the other drafts stay
   * dirty, so closing again still saves the valid half instead of losing it.
   */
  const commitDraft = (preferred?: PlanningField) => {
    const current = draftRef.current
    const stored = taskRef.current
    const storedDraft = draftFromTask(stored)
    // A field whose input still holds the stored spelling is untouched, so its
    // stored instant (which may carry seconds/milliseconds the input cannot
    // show) is preserved verbatim rather than being re-parsed at minute
    // precision. This is why the patch is built field by field instead of
    // spreading a partial object over the record.
    const touched = {
      estimate: current.estimate !== storedDraft.estimate,
      earliestStart: current.earliestStart !== storedDraft.earliestStart,
      deadline: current.deadline !== storedDraft.deadline,
    }
    const estimate = parseEstimateInput(current.estimate)
    const earliestStart = parseDateTimeLocalInput(current.earliestStart)
    const deadline = parseDateTimeLocalInput(current.deadline)

    const invalid: Array<{ field: PlanningField; message: string }> = []
    if (touched.estimate && estimate.error)
      invalid.push({ field: 'estimate', message: estimate.error })
    if (touched.earliestStart && earliestStart.error)
      invalid.push({ field: 'earliestStart', message: earliestStart.error })
    if (touched.deadline && deadline.error)
      invalid.push({ field: 'deadline', message: deadline.error })

    if (invalid.length > 0) {
      setLocalError(invalid[0].message)
      const nextDraft = { ...current }
      const nextBaseline = { ...baselineRef.current }
      for (const { field } of invalid) {
        const value = storedField(stored, field)
        nextDraft[field] = value
        nextBaseline[field] = value
      }
      draftRef.current = nextDraft
      baselineRef.current = nextBaseline
      setDraft(nextDraft)
      return
    }

    setLocalError(null)
    const patch: PlanningPatch = {
      estimatedMinutes: touched.estimate ? estimate.value : stored.estimatedMinutes,
      earliestStartAtMs: touched.earliestStart ? earliestStart.value : stored.earliestStartAtMs,
      deadlineAtMs: touched.deadline ? deadline.value : stored.deadlineAtMs,
    }
    const storedPatch = planningPatchFromTask(stored)
    const changed = FIELD_ORDER.filter((field) =>
      field === 'estimate'
        ? patch.estimatedMinutes !== storedPatch.estimatedMinutes
        : field === 'earliestStart'
          ? patch.earliestStartAtMs !== storedPatch.earliestStartAtMs
          : patch.deadlineAtMs !== storedPatch.deadlineAtMs,
    )

    if (changed.length === 0) {
      const reset = draftFromTask(stored)
      draftRef.current = reset
      baselineRef.current = reset
      setDraft(reset)
      return
    }

    // The draft stays on screen until the server answers; the baseline moves
    // with it so this exact window is not flushed a second time.
    baselineRef.current = { ...current }
    onCommitRef.current(preferred && changed.includes(preferred) ? preferred : changed[0], patch)
  }
  const flushRef = useRef(commitDraft)
  flushRef.current = commitDraft

  useDirtyGuard(
    'planning',
    () => !sameDraft(draftRef.current, baselineRef.current),
    () => flushRef.current(),
    () =>
      `${draftRef.current.estimate}\u0000${draftRef.current.earliestStart}\u0000${draftRef.current.deadline}`,
  )

  const onEnter = (field: PlanningField) => (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter') return
    event.preventDefault()
    commitDraft(field)
  }

  return (
    <Section
      title="Planning"
      description="The estimate feeds the server's day plan. Leaving a field empty clears it; the value above is always the stored one."
    >
      <div className="grid gap-x-4 gap-y-3 sm:grid-cols-3">
        <Field label="Estimate (minutes)" hint="Empty clears the estimate.">
          <Input
            type="number"
            min={1}
            step={5}
            inputMode="numeric"
            aria-label="Task estimate in minutes"
            value={draft.estimate}
            disabled={disabled}
            onChange={(event) => {
              setDraft((current) => ({ ...current, estimate: event.target.value }))
              if (localError) setLocalError(null)
            }}
            onBlur={() => commitDraft('estimate')}
            onKeyDown={onEnter('estimate')}
          />
        </Field>

        <Field label="Earliest start" hint="The server will not place it earlier.">
          <Input
            type="datetime-local"
            aria-label="Task earliest start"
            value={draft.earliestStart}
            disabled={disabled}
            onChange={(event) => {
              setDraft((current) => ({ ...current, earliestStart: event.target.value }))
              if (localError) setLocalError(null)
            }}
            onBlur={() => commitDraft('earliestStart')}
            onKeyDown={onEnter('earliestStart')}
          />
        </Field>

        <Field label="Deadline" hint="Deadlines come from the server's clock.">
          <Input
            type="datetime-local"
            aria-label="Task deadline"
            value={draft.deadline}
            disabled={disabled}
            onChange={(event) => {
              setDraft((current) => ({ ...current, deadline: event.target.value }))
              if (localError) setLocalError(null)
            }}
            onBlur={() => commitDraft('deadline')}
            onKeyDown={onEnter('deadline')}
          />
        </Field>
      </div>

      <div className="mt-2 flex flex-col gap-1">
        <FieldError error={error} onRefresh={onRefresh} />
        {localError ? (
          <p role="alert" className="text-xs leading-4 text-danger">
            {localError}
          </p>
        ) : null}
        <SavingNote visible={pending !== null} />
      </div>
    </Section>
  )
}
