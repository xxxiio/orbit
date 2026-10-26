import { useCallback, useEffect, useRef, useState } from 'react'
import type { ApiError } from '@/api/errors'
import { MarkdownEditor } from '@/components/markdown'
import { LoaderIcon } from '@/components/ui/icons'
import { FieldError, Section } from './section'

/** Markdown draft with a write/preview switch; saves when the field loses focus. */
export function DescriptionEditor({
  description,
  pending,
  disabled = false,
  error,
  onRefresh,
  onCommit,
}: {
  description: string
  pending: boolean
  /** Any task command is in flight: the editor must not start a second one. */
  disabled?: boolean
  error: ApiError | null
  /** Offered beside a conflict so the person can recover without leaving. */
  onRefresh?: () => void
  onCommit: (description: string) => void
}) {
  const [draft, setDraft] = useState(description)

  useEffect(() => {
    setDraft(description)
  }, [description])

  const draftRef = useRef(draft)
  draftRef.current = draft
  const descriptionRef = useRef(description)
  descriptionRef.current = description

  const commit = useCallback(() => {
    if (draftRef.current === descriptionRef.current) return
    onCommit(draftRef.current)
  }, [onCommit])
  const commitRef = useRef(commit)
  commitRef.current = commit

  return (
    <Section
      title="Description"
      description="Markdown is rendered as text. Saves when you leave the field."
      aside={
        pending ? (
          <span className="inline-flex items-center gap-1 text-2xs text-ink-muted">
            <LoaderIcon size={11} className="animate-spin" /> Saving
          </span>
        ) : null
      }
    >
      <MarkdownEditor
        value={draft}
        onChange={setDraft}
        label="Task description"
        placeholder="What does done look like? Markdown is fine."
        disabled={disabled}
        rows={8}
        fieldClassName="scrollbar-slim min-h-[8rem] w-full resize-y rounded-md border border-line bg-surface px-3 py-2 font-sans text-sm leading-6 text-ink placeholder:text-ink-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20 transition-none hover:border-line disabled:bg-surface disabled:text-ink-soft"
        onCommit={commit}
        onSaveShortcut={commit}
        guard={{
          id: 'description',
          isDirty: () => draftRef.current !== descriptionRef.current,
          flush: () => commitRef.current(),
          draftKey: () => draftRef.current,
        }}
      />
      <FieldError error={error} onRefresh={onRefresh} />
    </Section>
  )
}
