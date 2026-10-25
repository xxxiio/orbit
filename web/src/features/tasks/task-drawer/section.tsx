import type { ReactNode } from 'react'
import type { ApiError } from '@/api/errors'
import { InlineFieldError } from '@/components/forms'
import { userMessage } from '@/hooks'

/** A titled block in the drawer body. Layout-neutral so any resource can use it. */
export function Section({
  title,
  description,
  children,
  aside,
}: {
  title: string
  description?: string
  children: ReactNode
  aside?: ReactNode
}) {
  return (
    <section className="px-5 py-5" aria-label={title}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-ink">{title}</h3>
          {description ? <p className="mt-0.5 text-xs leading-5 text-ink-muted">{description}</p> : null}
        </div>
        {aside}
      </div>
      <div className="mt-3">{children}</div>
    </section>
  )
}

/** The server's rejection message, scoped to the field that caused it. */
export function FieldError({ error }: { error: ApiError | null }) {
  return <InlineFieldError error={error ? userMessage(error) : null} />
}

export function SavingNote({ visible }: { visible: boolean }) {
  if (!visible) return null
  return <p className="mt-1 text-2xs text-ink-muted">Saving…</p>
}
