/**
 * A note's `[[id]]` relations: outgoing links, backlinks and the references the
 * vault cannot resolve.
 *
 * The panel is read-only. A link is a stable note id, so an unresolved id is
 * named as such instead of being turned into a path the vault never published.
 */
import { Link } from 'react-router-dom'
import type { ApiError } from '@/hooks'
import { useMarkdownRelations, userMessage } from '@/hooks'
import { ErrorState } from '@/components/ui/error-state'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { SkeletonText } from '@/components/ui/skeleton'
import { noteHref, splitRelations } from '@/features/knowledge/knowledge-model'

/** The relations read as `NoteView` holds it; the panel never re-reads it. */
export type RelationsRead = ReturnType<typeof useMarkdownRelations>

export interface NoteRelationsProps {
  noteId: string | null
  relations: RelationsRead
  /** A failed identity index: ids are still shown, just not resolved to paths. */
  inventoryError: ApiError | null
  idToPath: Map<string, string>
}

export function NoteRelations({ noteId, relations, inventoryError, idToPath }: NoteRelationsProps) {
  const relationRows = relations.data ? splitRelations(relations.data) : null

  return (
    <Panel className="mt-4 overflow-hidden" aria-label="Relations">
      <PanelHeader
        title="Relations"
        description="Links are explicit [[note-id]] references, resolved against the vault's stable ids."
        actions={
          <span className="text-2xs text-ink-muted">
            {relations.data
              ? `${relations.data.outgoing.length} outgoing · ${relations.data.backlinks.length} backlinks`
              : ''}
          </span>
        }
      />
      <PanelBody>
        {inventoryError ? (
          <p
            role="alert"
            className="mb-3 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
          >
            The note identity index could not be read, so relation ids are shown without resolving
            them to paths. {userMessage(inventoryError)}
          </p>
        ) : null}
        {!noteId ? (
          <p className="text-xs leading-5 text-ink-muted">
            Relations are unavailable without an <span className="font-mono">orbit_id</span>.
          </p>
        ) : relations.isPending && !relations.data ? (
          <SkeletonText lines={2} />
        ) : relations.error && !relations.data ? (
          <ErrorState
            compact
            title="Relations could not be read"
            description={userMessage(relations.error)}
            onRetry={() => void relations.refetch()}
            retrying={relations.isFetching}
          />
        ) : relationRows ? (
          <div className="flex flex-col gap-4">
            <RelationGroup
              label="Links to"
              ids={relationRows.resolvedOutgoing}
              idToPath={idToPath}
              empty="No resolvable outgoing links."
            />
            <RelationGroup
              label="Linked from"
              ids={relations.data?.backlinks ?? []}
              idToPath={idToPath}
              empty="No other note links here yet."
            />
            {relationRows.unresolved.length > 0 ? (
              <div>
                <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">
                  Unresolved references
                </p>
                <ul className="mt-1.5 flex flex-wrap gap-1.5" aria-label="Unresolved references">
                  {relationRows.unresolved.map((id) => (
                    <li key={id}>
                      <span
                        className="inline-flex items-center rounded-full border border-dashed border-line-strong bg-surface-sunken px-2 py-0.5 font-mono text-2xs text-ink-muted"
                        title="No note in this vault has this id"
                      >
                        [[{id}]]
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
          </div>
        ) : null}
      </PanelBody>
    </Panel>
  )
}

function RelationGroup({
  label,
  ids,
  idToPath,
  empty,
}: {
  label: string
  ids: string[]
  idToPath: Map<string, string>
  empty: string
}) {
  return (
    <div>
      <p className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</p>
      {ids.length === 0 ? (
        <p className="mt-1 text-xs leading-5 text-ink-muted">{empty}</p>
      ) : (
        <ul className="mt-1.5 flex flex-wrap gap-1.5" aria-label={label}>
          {ids.map((id) => {
            const target = idToPath.get(id)
            return (
              <li key={id}>
                {target ? (
                  <Link
                    to={noteHref(target)}
                    className="inline-flex items-center gap-1.5 rounded-full border border-accent/30 bg-accent-soft px-2 py-0.5 font-mono text-2xs text-accent-soft-ink transition-colors hover:border-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                    title={target}
                  >
                    {id}
                  </Link>
                ) : (
                  <span className="inline-flex items-center rounded-full border border-line bg-surface-sunken px-2 py-0.5 font-mono text-2xs text-ink-muted">
                    {id}
                  </span>
                )}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
