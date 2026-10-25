import { useRef, useState, type FormEvent } from 'react'
import { useCreateTask, userMessage, type ApiError } from '@/hooks'
import { Button } from '@/components/ui/button'
import { AlertCircleIcon, LoaderIcon, PlusIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'

/**
 * Single-line capture. The text is cleared only after the server accepts the
 * task; a rejection keeps it in place with the server's message underneath.
 */
export function QuickCapture({ onCaptured }: { onCaptured?: (taskId: string) => void }) {
  const createTask = useCreateTask()
  const [title, setTitle] = useState('')
  const [announcement, setAnnouncement] = useState<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const error: ApiError | null = createTask.disposition === 'rejected' ? createTask.error : null
  const submitting = createTask.isPending

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (submitting) return
    const trimmed = title.trim()
    if (trimmed.length === 0) {
      inputRef.current?.focus()
      return
    }

    const submitted = trimmed
    const outcome = await createTask.mutateAsync({ title: submitted }).catch(() => null)
    if (!outcome) return
    setTitle((current) => (current === submitted ? '' : current))
    setAnnouncement(`Captured “${outcome.task.title}” in the Inbox.`)
    onCaptured?.(outcome.task.id)
    inputRef.current?.focus()
  }

  return (
    <section aria-label="Quick capture" className="rounded-lg border border-line bg-surface shadow-soft">
      <form onSubmit={submit} className="flex items-center gap-2.5 px-3 py-2 sm:pl-4">
        <span aria-hidden="true" className="text-ink-faint">
          <PlusIcon size={17} />
        </span>
        <input
          ref={inputRef}
          data-quick-capture=""
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder="Capture a task for the Inbox…"
          aria-label="Capture a task for the Inbox"
          maxLength={280}
          className="h-9 min-w-0 flex-1 bg-transparent text-base text-ink outline-none placeholder:text-ink-faint"
        />
        <kbd className="command-key hidden sm:inline-flex">Enter</kbd>
        <Button type="submit" size="sm" loading={submitting} disabled={title.trim().length === 0}>
          {submitting ? 'Capturing…' : 'Capture'}
        </Button>
      </form>

      {submitting ? (
        <p className="flex items-center gap-1.5 border-t border-line px-4 py-1.5 text-2xs text-ink-muted">
          <LoaderIcon size={11} className="animate-spin" />
          Sending to the server…
        </p>
      ) : null}

      {error ? (
        <div
          role="alert"
          className={cn(
            'flex items-start gap-2 border-t border-danger/25 bg-danger-soft px-4 py-2 text-xs leading-5 text-danger-ink',
          )}
        >
          <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
          <span className="min-w-0">
            The task was not captured, and your text is still here. {userMessage(error)}
          </span>
        </div>
      ) : null}

      <p className="sr-only" role="status" aria-live="polite">
        {announcement}
      </p>
    </section>
  )
}
