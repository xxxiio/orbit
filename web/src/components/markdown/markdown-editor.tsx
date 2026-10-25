/**
 * The one Markdown draft editor.
 *
 * A Markdown field is a draft, a preview and a way to leave: the note body and
 * the task description were two implementations of that (one raw textarea with
 * a manual stale check, one tabbed field with a dirty guard), so a fix to one
 * never reached the other.
 *
 * The editor owns the draft, the Write/Preview switch and the dirty-guard
 * registration. It does not own the command: the caller decides what "save"
 * means (commit on blur, or an explicit Save button with its own fences) and
 * keeps its read baseline, because a note's baseline is a server byte fence and
 * a task description has no baseline at all.
 */
import { useState } from 'react'
import type { EditorHandle } from '@/components/forms'
import { useDirtyGuard } from '@/components/forms'
import { Markdown } from '@/components/markdown/markdown'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/components/ui/utils'

export interface MarkdownEditorProps {
  value: string
  onChange: (value: string) => void
  /** Accessible name of the draft field. */
  label: string
  placeholder?: string
  disabled?: boolean
  spellCheck?: boolean
  /** Classes for the draft field: a note body is mono and tall, a description is not. */
  fieldClassName?: string
  /** Rows for the draft field; the preview keeps the same minimum height. */
  rows?: number
  /** Classes for the preview box. */
  previewClassName?: string
  /** Called when the field loses focus, before the draft is discarded. */
  onCommit?: () => void
  /** Enables Cmd/Ctrl+Enter as an explicit save. */
  onSaveShortcut?: () => void
  /**
   * Registers the draft with the surrounding `EditGuardProvider` so closing the
   * shell flushes it first. Omitted outside a guard (a page, not a drawer).
   */
  guard?: EditorHandle
}

const noGuard: EditorHandle = {
  id: 'markdown-editor',
  isDirty: () => false,
  flush: () => {},
  draftKey: () => '',
}

export function MarkdownEditor({
  value,
  onChange,
  label,
  placeholder,
  disabled = false,
  spellCheck,
  fieldClassName,
  rows,
  previewClassName,
  onCommit,
  onSaveShortcut,
  guard,
}: MarkdownEditorProps) {
  const [mode, setMode] = useState<'write' | 'preview'>('write')
  const handle = guard ?? noGuard
  useDirtyGuard(handle.id, handle.isDirty, handle.flush, handle.draftKey)

  return (
    <Tabs value={mode} onValueChange={(next) => setMode(next as 'write' | 'preview')}>
      <TabsList className="gap-4">
        <TabsTrigger value="write" className="pb-1.5 text-xs">
          Write
        </TabsTrigger>
        <TabsTrigger value="preview" className="pb-1.5 text-xs">
          Preview
        </TabsTrigger>
      </TabsList>
      <TabsContent value="write" className="mt-2">
        <Textarea
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onBlur={onCommit}
          disabled={disabled}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
              event.preventDefault()
              onSaveShortcut?.()
            }
          }}
          rows={rows}
          placeholder={placeholder}
          aria-label={label}
          spellCheck={spellCheck}
          className={fieldClassName}
        />
      </TabsContent>
      <TabsContent value="preview" className="mt-2">
        <div
          className={cn(
            'rounded-md border border-line bg-surface px-3 py-2',
            previewClassName ?? 'min-h-[8rem]',
          )}
        >
          {value.trim().length === 0 ? (
            <p className="text-sm italic text-ink-faint">Nothing to preview yet.</p>
          ) : (
            <Markdown source={value} />
          )}
        </div>
      </TabsContent>
    </Tabs>
  )
}
