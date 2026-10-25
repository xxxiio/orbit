import { useCallback, useEffect, useRef, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { useDirtyGuard } from './use-edit-guard'

/**
 * Comma-separated labels with a read view and an edit view. The server owns
 * de-duplication rules; the editor only trims and removes exact repeats.
 */
export function LabelsEditor({
  labels,
  pending,
  disabled,
  onCommit,
  inputId = 'task-labels',
  heading = 'Labels',
  ariaLabel = 'Edit task labels',
}: {
  labels: string[]
  pending: boolean
  disabled: boolean
  onCommit: (labels: string[]) => void
  inputId?: string
  heading?: string
  ariaLabel?: string
}) {
  const [draft, setDraft] = useState(labels.join(', '))
  const [editing, setEditing] = useState(false)

  useEffect(() => {
    if (!editing) setDraft(labels.join(', '))
  }, [labels, editing])

  const draftRef = useRef(draft)
  draftRef.current = draft
  const labelsRef = useRef(labels)
  labelsRef.current = labels

  const parse = (value: string) =>
    value
      .split(',')
      .map((label) => label.trim())
      .filter((label, index, all) => label.length > 0 && all.indexOf(label) === index)

  const commit = useCallback(() => {
    setEditing(false)
    const next = parse(draftRef.current)
    if (next.join('\u0000') === labelsRef.current.join('\u0000')) return
    onCommit(next)
  }, [onCommit])

  const commitRef = useRef(commit)
  commitRef.current = commit

  useDirtyGuard(
    'labels',
    () => parse(draftRef.current).join('\u0000') !== labelsRef.current.join('\u0000'),
    () => commitRef.current(),
  )

  if (!editing) {
    return (
      <div>
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-medium text-ink-muted">{heading}</span>
          {pending ? <span className="text-2xs text-ink-muted">Saving…</span> : null}
        </div>
        <button
          type="button"
          disabled={disabled}
          onClick={() => setEditing(true)}
          className="mt-1 flex min-h-8 w-full flex-wrap items-center gap-1.5 rounded-sm border border-dashed border-line px-2 py-1 text-left transition-colors hover:border-line-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/50"
          aria-label={ariaLabel}
        >
          {labels.length === 0 ? (
            <span className="text-xs text-ink-faint">No labels yet</span>
          ) : (
            labels.map((label) => (
              <Badge key={label} tone="neutral" className="lowercase">
                {label}
              </Badge>
            ))
          )}
        </button>
      </div>
    )
  }

  return (
    <div>
      <label className="text-xs font-medium text-ink-muted" htmlFor={inputId}>
        {heading}
      </label>
      <input
        id={inputId}
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
        placeholder="design, docs"
        className="mt-1 h-8 w-full rounded-sm border border-line bg-surface px-2 text-sm text-ink placeholder:text-ink-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
      />
      <p className="mt-1 text-2xs text-ink-muted">Separate labels with commas.</p>
    </div>
  )
}
