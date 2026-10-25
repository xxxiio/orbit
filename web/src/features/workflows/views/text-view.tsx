import type { ViewRendererProps } from '../view-types'
import { componentTypeName } from '../view-metadata'
import { ComponentPanel, DeclaredActionsNote, ProvenanceNote } from './shared'

/**
 * Declared prose. A text component carries the workflow author's own wording;
 * it is shown as written, with its provenance, instead of being rewritten or
 * dropped. Empty declarations say so rather than rendering a blank card.
 * Declared `actions` are surfaced in the footer like every other panel: this
 * build does not wire them, and silence would read as support.
 */
export function TextView({ component, workflow }: ViewRendererProps) {
  const title = componentTypeName(component)
  const text = component.text?.trim() ?? ''
  const actionsNote =
    component.actions.length > 0 ? (
      <ProvenanceNote>
        <DeclaredActionsNote component={component} />
      </ProvenanceNote>
    ) : null

  if (text.length === 0) {
    return (
      <ComponentPanel title={title} eyebrow="Notes" footer={actionsNote}>
        <p className="text-sm leading-6 text-ink-muted">
          This panel is declared without any text, so there is nothing to read here.
        </p>
      </ComponentPanel>
    )
  }

  const paragraphs = text.split(/\n{2,}/)

  return (
    <ComponentPanel
      title={title}
      eyebrow="Notes"
      footer={
        <div className="flex flex-col gap-1">
          <ProvenanceNote>
            Declared by {workflow ? `${workflow.name} v${workflow.version}` : 'this project’s workflow'}.
            Wording comes from the workflow, not from this screen.
          </ProvenanceNote>
          {actionsNote}
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        {paragraphs.map((paragraph, index) => (
          <p key={index} className="text-pretty text-sm leading-6 text-ink-soft">
            {paragraph}
          </p>
        ))}
      </div>
    </ComponentPanel>
  )
}
