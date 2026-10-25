import type { ReactNode } from 'react'

/**
 * One titled block in a drawer or sheet body.
 *
 * Layout-neutral: the shell decides where sections stack, this only names the
 * block. Task drawer sections and the work-item sheet use the same component so
 * the two resources read as one product.
 */
export function Section({
  title,
  description,
  children,
  aside,
}: {
  title: string
  description?: ReactNode
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
