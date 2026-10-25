import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { IconButton } from '@/components/ui/icon-button'
import { CloseIcon } from '@/components/ui/icons'
import { Input } from '@/components/ui/input'
import { useDirtyGuard } from './edit-guard'

export interface InlineTitleProps {
  value: string
  disabled: boolean
  /** Return `false` to keep editing (an async rename the server refused). */
  onCommit: (value: string) => void | boolean | Promise<boolean | void>
  /** Which resource names the control, e.g. "task" or "work item". */
  label?: string
  /**
   * `title` is click-to-edit on the title itself. `form` shows the caller's
   * display until they ask to rename, then an explicit Save/Cancel form.
   */
  variant?: 'title' | 'form'
  /** Form variant: the row's own display, given the control that starts editing. */
  renderDisplay?: (beginEdit: () => void) => ReactNode
  /** Form variant: a rename is in flight; Save is disabled until the server answers. */
  pending?: boolean
  className?: string
}

/**
 * One inline rename implementation. Enter (or blur, when the title itself is
 * the control) commits; an empty draft reverts. The caller only renders the
 * display and sends the command, so a milestone and a task title cannot drift
 * apart.
 */
export function InlineTitle({
  value,
  disabled,
  onCommit,
  label = 'task title',
  variant = 'title',
  renderDisplay,
  pending = false,
  className,
}: InlineTitleProps) {
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

  const commit = useCallback(async () => {
    const next = stateRef.current.draft.trim()
    if (next.length === 0 || next === valueRef.current) {
      setEditing(false)
      setDraft(valueRef.current)
      return
    }
    const accepted = await onCommit(next)
    if (accepted === false) return
    setEditing(false)
  }, [onCommit])

  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    'title',
    () => stateRef.current.editing && stateRef.current.draft.trim() !== valueRef.current,
    () => void commitRef.current(),
    () => stateRef.current.draft,
  )

  const beginEdit = () => {
    setDraft(valueRef.current)
    setEditing(true)
    requestAnimationFrame(() => inputRef.current?.select())
  }

  if (!editing) {
    if (variant === 'form') return <>{renderDisplay ? renderDisplay(beginEdit) : null}</>
    return (
      <button
        type="button"
        onClick={beginEdit}
        className={className ?? 'mt-2 block w-full rounded-sm text-left font-display text-xl font-medium leading-7 tracking-tight text-ink transition-colors hover:text-accent-soft-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60'}
        aria-label={`Edit ${label}`}
        disabled={disabled}
      >
        {value}
      </button>
    )
  }

  if (variant === 'form') {
    return (
      <form
        onSubmit={(event) => {
          event.preventDefault()
          void commit()
        }}
        className="flex max-w-md items-center gap-1.5"
      >
        <Input
          ref={inputRef}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          aria-label={label}
          autoFocus
          className="h-8 text-sm"
        />
        <Button type="submit" size="sm" variant="secondary" disabled={pending} loading={pending}>
          Save
        </Button>
        <IconButton
          label="Cancel rename"
          icon={<CloseIcon size={14} />}
          size="sm"
          variant="quiet"
          onClick={() => {
            setDraft(valueRef.current)
            setEditing(false)
          }}
        />
      </form>
    )
  }

  return (
    <input
      ref={inputRef}
      autoFocus
      value={draft}
      disabled={disabled}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={() => void commit()}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault()
          void commit()
        }
      }}
      aria-label={label.charAt(0).toUpperCase() + label.slice(1)}
      className="mt-2 w-full rounded-sm border border-accent/50 bg-surface px-1 py-0.5 font-display text-xl font-medium leading-7 tracking-tight text-ink focus:outline-none focus:ring-2 focus:ring-accent/25"
    />
  )
}

