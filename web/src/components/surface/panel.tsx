/**
 * Shared chrome for one product panel.
 *
 * A renderer is a panel: it has a title, an optional declared caveat and a
 * body. Keeping the chrome here is what makes Basic, Development, Research,
 * Learning and Habits feel like one product even though their workflows declare
 * very different panels, and what lets Knowledge, Agent jobs and Attachments
 * use the same chrome without importing a workflow module.
 */
import type { ReactNode } from 'react'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { AlertCircleIcon, LayersIcon } from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { cn } from '@/components/ui/utils'
import { userMessage, type ApiError } from '@/hooks'
import { workflowText } from '@/domain'
import { humanizeIdentifier } from '@/lib/humanize'
import type { WorkflowViewComponent } from '@/api/adapters'

export function ComponentPanel({
  title,
  description,
  eyebrow,
  actions,
  children,
  bodyClassName,
  className,
  footer,
}: {
  title: ReactNode
  description?: ReactNode
  eyebrow?: ReactNode
  actions?: ReactNode
  children: ReactNode
  bodyClassName?: string
  className?: string
  footer?: ReactNode
}) {
  return (
    <Panel className={cn('flex min-w-0 flex-col', className)}>
      <PanelHeader
        title={title}
        description={description}
        eyebrow={eyebrow}
        actions={actions}
        className="flex-wrap gap-y-2"
      />
      <PanelBody className={cn('min-w-0 flex-1', bodyClassName)}>{children}</PanelBody>
      {footer ? <div className="border-t border-line px-5 py-3">{footer}</div> : null}
    </Panel>
  )
}

/**
 * The named refusal for a panel this build cannot render. It is never blank and
 * never pretends the panel succeeded: it names the thing and says why.
 */
export function UnavailablePanel({
  title,
  reason,
  hint,
  action,
}: {
  title: string
  reason: string
  hint?: ReactNode
  action?: ReactNode
}) {
  return (
    <ComponentPanel title={title} bodyClassName="p-0">
      <EmptyState
        compact
        headingLevel="p"
        icon={<LayersIcon size={18} />}
        title="Not available in the browser yet"
        description={
          <>
            {reason}
            {hint ? <span className="mt-1.5 block text-ink-muted">{hint}</span> : null}
          </>
        }
        action={action}
        className="min-h-[12rem]"
      />
    </ComponentPanel>
  )
}

/** A read that failed inside a panel: the panel's own retry, not a page error. */
export function InlineReadError({
  label,
  error,
  onRetry,
  retrying = false,
}: {
  label: string
  error: ApiError
  onRetry: () => void
  retrying?: boolean
}) {
  return (
    <ErrorState
      compact
      title={`${label} could not be read`}
      description={userMessage(error)}
      onRetry={onRetry}
      retrying={retrying}
    />
  )
}

/** The server's message as a refusal, verbatim, for a command that was rejected. */
export function RefusalNote({ message }: { message: string }) {
  return (
    <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
      <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
      <span className="min-w-0 flex-1">{message}</span>
    </p>
  )
}

/** A neutral provenance line: where a panel's data or wording comes from. */
export function ProvenanceNote({ children }: { children: ReactNode }) {
  return <p className="text-2xs leading-4 text-ink-faint">{children}</p>
}

/**
 * A declared `actions` list is view chrome, not a capability grant. This build
 * renders each panel's own controls (open, create, move, rename) and never adds
 * a button for an action it cannot complete. Say which actions were declared
 * where the declaration exists instead of dropping it silently.
 */
export function DeclaredActionsNote({
  component,
  ownLabel = 'controls',
}: {
  component: WorkflowViewComponent
  /** The noun the panel's own affordances go by ("controls", "links"). */
  ownLabel?: string
}) {
  if (component.actions.length === 0) return null
  const names = component.actions.map((action) => humanizeIdentifier(workflowText(action)))
  return (
    <span data-declared-actions={component.actions.join(',')}>
      The workflow also declares the {names.join(', ')} action
      {component.actions.length === 1 ? '' : 's'}; this panel renders its own {ownLabel} and does
      not attach an action it cannot complete.
    </span>
  )
}

/** A quiet panel action (usually Retry) that keeps the header compact. */
export function PanelRetry({ onClick, retrying }: { onClick: () => void; retrying?: boolean }) {
  return (
    <Button variant="quiet" size="sm" onClick={onClick} loading={retrying}>
      Retry
    </Button>
  )
}
