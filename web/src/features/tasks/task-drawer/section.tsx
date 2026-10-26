import type { ApiError } from '@/api/errors'
import { InlineFieldError } from '@/components/forms'
import { userMessage } from '@/hooks'

/**
 * The drawer's block is the shared `Section`; it is re-exported here so the
 * drawer's own section modules keep one import path. Field errors and the
 * saving note stay task-drawer-local.
 */
export { Section } from '@/components/ui/section'

/**
 * The server's rejection message, scoped to the field that caused it.
 *
 * This is the drawer's only refusal surface for a command: the footer no longer
 * repeats the same sentence, so the message appears exactly once beside the
 * control that failed. A conflict also carries its recovery here.
 */
export function FieldError({
  error,
  onRefresh,
}: {
  error: ApiError | null
  onRefresh?: () => void
}) {
  return (
    <InlineFieldError
      error={error ? userMessage(error) : null}
      action={
        onRefresh ? (
          <button
            type="button"
            onClick={onRefresh}
            className="font-medium underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          >
            Reload this task and try again
          </button>
        ) : null
      }
    />
  )
}

export function SavingNote({ visible }: { visible: boolean }) {
  if (!visible) return null
  return <p className="mt-1 text-2xs text-ink-muted">Saving…</p>
}
