/**
 * The task drawer's Planning group.
 *
 * `set_planning` is one command for three fields, so every commit sends the
 * whole window: the value being edited plus the two stored values. Empty input
 * means `null`, which the server's contract defines as “clear this field”.
 * Nothing is shown as saved before the server answers; a rejected command
 * resets each draft to the stored record (`error` is part of that reset), so a
 * refused value can never linger next to the server's truth.
 */
import { useEffect, useState, type KeyboardEvent } from 'react'
import type { ApiError } from '@/api/errors'
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

export interface PlanningSectionProps {
  task: {
    estimatedMinutes: number | null
    earliestStartAtMs: number | null
    deadlineAtMs: number | null
  }
  pending: PlanningField | null
  disabled: boolean
  /** The server's refusal for this group, when the last command was rejected. */
  error: ApiError | null
  onCommit: (field: PlanningField, patch: PlanningPatch) => void
}

function estimateInput(minutes: number | null): string {
  return minutes === null ? '' : String(minutes)
}

export function PlanningSection({
  task,
  pending,
  disabled,
  error,
  onCommit,
}: PlanningSectionProps) {
  const [estimate, setEstimate] = useState(() => estimateInput(task.estimatedMinutes))
  const [earliestStart, setEarliestStart] = useState(() =>
    toDateTimeLocalInput(task.earliestStartAtMs),
  )
  const [deadline, setDeadline] = useState(() => toDateTimeLocalInput(task.deadlineAtMs))
  const [localError, setLocalError] = useState<string | null>(null)

  // Each stored field resets only its own draft, so an accepted estimate never
  // clobbers a deadline the person is still typing. A refusal resets every
  // draft: the task record does not change when the server refuses, so `error`
  // is the signal that the stored values are the truth again.
  useEffect(() => {
    setEstimate(estimateInput(task.estimatedMinutes))
  }, [task.estimatedMinutes])
  useEffect(() => {
    setEarliestStart(toDateTimeLocalInput(task.earliestStartAtMs))
  }, [task.earliestStartAtMs])
  useEffect(() => {
    setDeadline(toDateTimeLocalInput(task.deadlineAtMs))
  }, [task.deadlineAtMs])
  useEffect(() => {
    if (!error) return
    setEstimate(estimateInput(task.estimatedMinutes))
    setEarliestStart(toDateTimeLocalInput(task.earliestStartAtMs))
    setDeadline(toDateTimeLocalInput(task.deadlineAtMs))
    setLocalError(null)
  }, [error, task.estimatedMinutes, task.earliestStartAtMs, task.deadlineAtMs])

  const stored = planningPatchFromTask(task)

  const commitEstimate = () => {
    const parsed = parseEstimateInput(estimate)
    if (parsed.error) {
      setLocalError(parsed.error)
      setEstimate(estimateInput(task.estimatedMinutes))
      return
    }
    setLocalError(null)
    if (parsed.value !== stored.estimatedMinutes) {
      onCommit('estimate', { ...stored, estimatedMinutes: parsed.value })
    }
    setEstimate(estimateInput(task.estimatedMinutes))
  }

  const commitInstant = (field: 'earliestStart' | 'deadline', raw: string, reset: () => void) => {
    const parsed = parseDateTimeLocalInput(raw)
    if (parsed.error) {
      setLocalError(parsed.error)
      reset()
      return
    }
    setLocalError(null)
    const next: PlanningPatch =
      field === 'earliestStart'
        ? { ...stored, earliestStartAtMs: parsed.value }
        : { ...stored, deadlineAtMs: parsed.value }
    if (
      (field === 'earliestStart' && parsed.value !== stored.earliestStartAtMs) ||
      (field === 'deadline' && parsed.value !== stored.deadlineAtMs)
    ) {
      onCommit(field, next)
    }
    reset()
  }

  const resetEarliest = () => setEarliestStart(toDateTimeLocalInput(task.earliestStartAtMs))
  const resetDeadline = () => setDeadline(toDateTimeLocalInput(task.deadlineAtMs))

  const onEnter = (commit: () => void) => (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter') return
    event.preventDefault()
    commit()
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
            value={estimate}
            disabled={disabled}
            onChange={(event) => {
              setEstimate(event.target.value)
              if (localError) setLocalError(null)
            }}
            onBlur={commitEstimate}
            onKeyDown={onEnter(commitEstimate)}
          />
        </Field>

        <Field label="Earliest start" hint="The server will not place it earlier.">
          <Input
            type="datetime-local"
            aria-label="Task earliest start"
            value={earliestStart}
            disabled={disabled}
            onChange={(event) => {
              setEarliestStart(event.target.value)
              if (localError) setLocalError(null)
            }}
            onBlur={() => commitInstant('earliestStart', earliestStart, resetEarliest)}
            onKeyDown={onEnter(() => commitInstant('earliestStart', earliestStart, resetEarliest))}
          />
        </Field>

        <Field label="Deadline" hint="Deadlines come from the server's clock.">
          <Input
            type="datetime-local"
            aria-label="Task deadline"
            value={deadline}
            disabled={disabled}
            onChange={(event) => {
              setDeadline(event.target.value)
              if (localError) setLocalError(null)
            }}
            onBlur={() => commitInstant('deadline', deadline, resetDeadline)}
            onKeyDown={onEnter(() => commitInstant('deadline', deadline, resetDeadline))}
          />
        </Field>
      </div>

      <div className="mt-2 flex flex-col gap-1">
        <FieldError error={error} />
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
