import { useCallback, useEffect, useRef, useState } from 'react'
import type { AcceptanceCriterion } from '@/api/adapters'
import type { ApiError } from '@/api/errors'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { IconButton } from '@/components/ui/icon-button'
import { ArrowDownIcon, ArrowUpIcon, PlusIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { FieldError, SavingNote, Section } from './section'
import type { TaskCommand } from './task-command'
import { useDirtyGuard } from '@/components/forms'

export function CriteriaSection({
  taskId,
  criteria,
  pending,
  error,
  command,
  onAdd,
  onChangeText,
  onToggle,
  onReorder,
  onRemove,
}: {
  taskId: string
  criteria: AcceptanceCriterion[]
  pending: boolean
  error: ApiError | null
  command: TaskCommand
  onAdd: (text: string) => Promise<boolean>
  onChangeText: (criterion: AcceptanceCriterion, text: string) => void
  onToggle: (criterion: AcceptanceCriterion) => void
  onReorder: (criterion: AcceptanceCriterion, position: number) => void
  onRemove: (criterion: AcceptanceCriterion) => void
}) {
  const [newText, setNewText] = useState('')
  const [adding, setAdding] = useState(false)
  const newTextRef = useRef(newText)
  newTextRef.current = newText

  const add = useCallback(async () => {
    const text = newTextRef.current.trim()
    if (text.length === 0) return
    setAdding(true)
    const accepted = await onAdd(text)
    setAdding(false)
    if (accepted) setNewText('')
  }, [onAdd])
  const addRef = useRef(add)
  addRef.current = add

  useDirtyGuard('new-criterion', () => newTextRef.current.trim().length > 0, () => void addRef.current())

  const ordered = [...criteria].sort((left, right) => left.position - right.position)
  const completed = ordered.filter((criterion) => criterion.completed).length

  return (
    <Section
      title="Acceptance criteria"
      description="Checking a criterion updates the checklist only — it never changes the task's status."
      aside={
        ordered.length > 0 ? (
          <span className="text-2xs tabular-nums text-ink-muted">
            {completed}/{ordered.length} done
          </span>
        ) : null
      }
    >
      {ordered.length === 0 ? (
        <p className="text-xs text-ink-muted">No criteria yet. Add the first check below.</p>
      ) : (
        <ul className="flex flex-col gap-1" aria-label={`Acceptance criteria for task ${taskId}`}>
          {ordered.map((criterion, index) => (
            <li key={criterion.id} className="group flex items-start gap-2 rounded-sm px-1 py-1 hover:bg-surface-hover/50">
              <span className="mt-0.5">
                <Checkbox
                  checked={criterion.completed}
                  disabled={command.isPending}
                  onCheckedChange={() => onToggle(criterion)}
                  aria-label={`${criterion.completed ? 'Uncheck' : 'Check'} ${criterion.text}`}
                />
              </span>
              <CriterionText
                criterion={criterion}
                disabled={command.isPending}
                onCommit={(text) => {
                  if (text !== criterion.text) onChangeText(criterion, text)
                }}
              />
              <span className="ml-auto flex shrink-0 items-center gap-0.5">
                <IconButton
                  label={`Move ${criterion.text} up`}
                  icon={<ArrowUpIcon size={14} />}
                  size="sm"
                  variant="quiet"
                  disabled={command.isPending || index === 0}
                  onClick={() => onReorder(criterion, index - 1)}
                />
                <IconButton
                  label={`Move ${criterion.text} down`}
                  icon={<ArrowDownIcon size={14} />}
                  size="sm"
                  variant="quiet"
                  disabled={command.isPending || index === ordered.length - 1}
                  onClick={() => onReorder(criterion, index + 1)}
                />
                <IconButton
                  label={`Remove ${criterion.text}`}
                  icon={<span aria-hidden="true">×</span>}
                  size="sm"
                  variant="quiet"
                  disabled={command.isPending}
                  onClick={() => onRemove(criterion)}
                  className="text-ink-muted hover:text-danger-ink"
                />
              </span>
            </li>
          ))}
        </ul>
      )}

      <form
        className="mt-2 flex items-center gap-1.5"
        onSubmit={(event) => {
          event.preventDefault()
          void add()
        }}
      >
        <input
          value={newText}
          onChange={(event) => setNewText(event.target.value)}
          placeholder="Add a criterion"
          aria-label="New acceptance criterion"
          disabled={command.isPending}
          className="h-8 min-w-0 flex-1 rounded-sm border border-line bg-surface px-2 text-sm text-ink placeholder:text-ink-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
        />
        <Button
          type="submit"
          size="sm"
          variant="secondary"
          loading={adding}
          disabled={newText.trim().length === 0 || command.isPending}
          leadingIcon={<PlusIcon size={14} />}
        >
          Add
        </Button>
      </form>

      <SavingNote visible={pending} />
      <FieldError error={error} />
    </Section>
  )
}

function CriterionText({
  criterion,
  disabled,
  onCommit,
}: {
  criterion: AcceptanceCriterion
  disabled: boolean
  onCommit: (text: string) => void
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(criterion.text)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!editing) setDraft(criterion.text)
  }, [criterion.text, editing])

  const stateRef = useRef({ editing, draft })
  stateRef.current = { editing, draft }
  const textRef = useRef(criterion.text)
  textRef.current = criterion.text

  const commit = useCallback(() => {
    const next = stateRef.current.draft.trim()
    setEditing(false)
    if (next.length === 0) {
      setDraft(textRef.current)
      return
    }
    if (next !== textRef.current) onCommit(next)
  }, [onCommit])
  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    `criterion-${criterion.id}`,
    () => stateRef.current.editing && stateRef.current.draft.trim() !== textRef.current,
    () => commitRef.current(),
  )

  if (!editing) {
    return (
      <button
        type="button"
        disabled={disabled}
        onClick={() => {
          setEditing(true)
          requestAnimationFrame(() => inputRef.current?.select())
        }}
        className={cn(
          'min-w-0 flex-1 rounded-sm text-left text-sm leading-5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50',
          criterion.completed ? 'text-ink-muted line-through' : 'text-ink',
        )}
        aria-label={`Edit criterion ${criterion.text}`}
      >
        {criterion.text}
      </button>
    )
  }

  return (
    <input
      ref={inputRef}
      autoFocus
      value={draft}
      disabled={disabled}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault()
          commit()
        }
      }}
      aria-label="Criterion text"
      className="min-w-0 flex-1 rounded-sm border border-accent/50 bg-surface px-1 py-0.5 text-sm text-ink focus:outline-none focus:ring-2 focus:ring-accent/20"
    />
  )
}
