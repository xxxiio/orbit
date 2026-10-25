import { useCallback, useEffect, useRef, useState } from 'react'
import { useDirtyGuard } from './use-edit-guard'

/** Click-to-edit title. Enter or blur commits; an empty draft reverts. */
export function InlineTitle({
  value,
  disabled,
  onCommit,
  label = 'task title',
}: {
  value: string
  disabled: boolean
  onCommit: (title: string) => void
  /** Which resource names the control, e.g. "task" or "work item". */
  label?: string
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(value)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!editing) setDraft(value)
  }, [value, editing])

  const stateRef = useRef({ editing, draft })
  stateRef.current = { editing, draft }
  const valueRef = useRef(value)
  valueRef.current = value

  const commit = useCallback(() => {
    const next = stateRef.current.draft.trim()
    setEditing(false)
    if (next.length === 0) {
      setDraft(valueRef.current)
      return
    }
    if (next !== valueRef.current) onCommit(next)
  }, [onCommit])

  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    'title',
    () => stateRef.current.editing && stateRef.current.draft.trim() !== valueRef.current,
    () => commitRef.current(),
  )

  if (!editing) {
    return (
      <button
        type="button"
        onClick={() => {
          setEditing(true)
          requestAnimationFrame(() => inputRef.current?.select())
        }}
        className="mt-2 block w-full rounded-sm text-left font-display text-xl font-medium leading-7 tracking-tight text-ink transition-colors hover:text-accent-soft-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        aria-label={`Edit ${label}`}
        disabled={disabled}
      >
        {value}
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
      aria-label={label.charAt(0).toUpperCase() + label.slice(1)}
      className="mt-2 w-full rounded-sm border border-accent/50 bg-surface px-1 py-0.5 font-display text-xl font-medium leading-7 tracking-tight text-ink focus:outline-none focus:ring-2 focus:ring-accent/25"
    />
  )
}
