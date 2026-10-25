import type { ApiError } from '@/api/errors'
import { InlineFieldError } from '@/components/forms'
import { userMessage } from '@/hooks'

/**
 * The drawer's block is the shared `Section`; it is re-exported here so the
 * drawer's own section modules keep one import path. Field errors and the
 * saving note stay task-drawer-local.
 */
export { Section } from '@/components/ui/section'

/** The server's rejection message, scoped to the field that caused it. */
export function FieldError({ error }: { error: ApiError | null }) {
  return <InlineFieldError error={error ? userMessage(error) : null} />
}

export function SavingNote({ visible }: { visible: boolean }) {
  if (!visible) return null
  return <p className="mt-1 text-2xs text-ink-muted">Saving…</p>
}
