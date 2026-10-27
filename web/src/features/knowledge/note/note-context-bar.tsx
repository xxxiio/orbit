/**
 * The strip that says which task a note is being linked to.
 *
 * On a knowledge route `?task=` is a *note context*, not an open task drawer.
 * The parameter would otherwise be invisible state, so the note names the task
 * (title when the workspace knows it, always the id) and offers the two honest
 * ways out: open the task in its own surface, or stop linking this note to it.
 *
 * The note itself keeps working while the task is a context: the reference panel
 * below adds and removes the typed reference that stores the link in the body.
 */
import { Link } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import { ArrowRightIcon, LinkIcon } from '@/components/ui/icons'

export interface NoteContextBarProps {
  taskId: string
  /** The task's title when the workspace snapshot has it. */
  taskTitle?: string
  /** Where the task lives, so opening it lands on a surface that accepts it. */
  taskHref: string
  /** Drops `?task=` from the address without touching the note. */
  onClear: () => void
}

export function NoteContextBar({ taskId, taskTitle, taskHref, onClear }: NoteContextBarProps) {
  const label = taskTitle?.trim() ? taskTitle : taskId
  return (
    <section
      aria-label="Task context"
      data-note-context
      data-context-task={taskId}
      className="relative flex flex-col gap-3 overflow-hidden rounded-lg border border-line bg-surface px-4 py-3 sm:flex-row sm:items-center sm:justify-between"
    >
      <span aria-hidden="true" className="absolute inset-y-0 left-0 w-[3px] bg-accent/70" />
      <div className="min-w-0">
        <p className="section-label">Task context</p>
        <p className="mt-0.5 flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5 text-sm text-ink">
          <LinkIcon size={13} aria-hidden="true" className="shrink-0 self-center text-ink-muted" />
          <span className="min-w-0 truncate font-medium" data-context-task-title>
            {label}
          </span>
          <span className="font-mono text-2xs text-ink-muted" data-context-task-id>
            <span className="sr-only">Task id </span>
            {taskId}
          </span>
        </p>
        <p className="mt-0.5 text-xs leading-5 text-ink-muted">
          The task comes from the address. The reference panel below keeps the link in the note.
        </p>
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-1.5">
        <Button asChild variant="secondary" size="sm">
          <Link to={taskHref} data-action="open-context-task">
            Open the task
            <ArrowRightIcon size={13} aria-hidden="true" className="shrink-0" />
          </Link>
        </Button>
        <Button
          variant="quiet"
          size="sm"
          onClick={onClear}
          data-action="clear-task-context"
          title="Keep the note open and drop the task from the address"
        >
          Stop linking
        </Button>
      </div>
    </section>
  )
}
