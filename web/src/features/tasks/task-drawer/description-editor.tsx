import { useCallback, useEffect, useRef, useState } from 'react'
import type { ApiError } from '@/api/errors'
import { Markdown } from '@/components/markdown'
import { LoaderIcon } from '@/components/ui/icons'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { FieldError, Section } from './section'
import { useDirtyGuard } from './use-edit-guard'

/** Markdown draft with a write/preview switch; saves when the field loses focus. */
export function DescriptionEditor({
  description,
  pending,
  disabled = false,
  error,
  onCommit,
}: {
  description: string
  pending: boolean
  /** Any task command is in flight: the editor must not start a second one. */
  disabled?: boolean
  error: ApiError | null
  onCommit: (description: string) => void
}) {
  const [draft, setDraft] = useState(description)
  const [mode, setMode] = useState<'write' | 'preview'>('write')

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

  useDirtyGuard(
    'description',
    () => draftRef.current !== descriptionRef.current,
    () => commitRef.current(),
  )

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
      <Tabs value={mode} onValueChange={(value) => setMode(value as 'write' | 'preview')}>
        <TabsList className="gap-4">
          <TabsTrigger value="write" className="pb-1.5 text-xs">
            Write
          </TabsTrigger>
          <TabsTrigger value="preview" className="pb-1.5 text-xs">
            Preview
          </TabsTrigger>
        </TabsList>
        <TabsContent value="write" className="mt-2">
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onBlur={commit}
            disabled={disabled}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
                event.preventDefault()
                commit()
              }
            }}
            rows={8}
            placeholder="What does done look like? Markdown is fine."
            aria-label="Task description"
            className="scrollbar-slim min-h-[8rem] w-full resize-y rounded-md border border-line bg-surface px-3 py-2 font-sans text-sm leading-6 text-ink placeholder:text-ink-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
          />
        </TabsContent>
        <TabsContent value="preview" className="mt-2">
          <div className="min-h-[8rem] rounded-md border border-line bg-surface px-3 py-2">
            {draft.trim().length === 0 ? (
              <p className="text-sm italic text-ink-faint">Nothing to preview yet.</p>
            ) : (
              <Markdown source={draft} />
            )}
          </div>
        </TabsContent>
      </Tabs>
      <FieldError error={error} />
    </Section>
  )
}
