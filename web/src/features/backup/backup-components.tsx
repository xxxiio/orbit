/**
 * Small read-only renderers shared by the create result, the inspect drawer and
 * the restore plan: component sizes, recorded counts and explicit exclusions.
 */
import { formatBytes, formatCount } from './backup-model'
import type { BackupComponentSummary, BackupCounts, BackupExclusion } from '@/api/contracts'

export function ComponentTable({ components }: { components: BackupComponentSummary[] }) {
  if (components.length === 0) {
    return <p className="text-xs text-ink-muted">No components were recorded in the manifest.</p>
  }
  return (
    <table className="w-full text-left text-xs">
      <thead>
        <tr className="text-ink-muted">
          <th scope="col" className="pb-1.5 pr-3 font-medium">
            Component
          </th>
          <th scope="col" className="pb-1.5 pr-3 text-right font-medium">
            Files
          </th>
          <th scope="col" className="pb-1.5 text-right font-medium">
            Bytes
          </th>
        </tr>
      </thead>
      <tbody>
        {components.map((component) => (
          <tr key={component.component} className="border-t border-line">
            <td className="py-1.5 pr-3 text-ink">{component.label}</td>
            <td className="py-1.5 pr-3 text-right tabular-nums text-ink-soft">
              {formatCount(component.files)}
            </td>
            <td className="py-1.5 text-right tabular-nums text-ink-soft">
              {formatBytes(component.bytes)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

export function CountsGrid({ counts }: { counts: BackupCounts }) {
  const tables = Object.keys(counts.tables ?? {}).length
  return (
    <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
      <div>
        <dt className="text-ink-muted">Knowledge notes</dt>
        <dd className="mt-0.5 tabular-nums text-ink">{formatCount(counts.knowledge_notes)}</dd>
      </div>
      <div>
        <dt className="text-ink-muted">Knowledge folders</dt>
        <dd className="mt-0.5 tabular-nums text-ink">{formatCount(counts.knowledge_folders)}</dd>
      </div>
      <div>
        <dt className="text-ink-muted">Attachment objects</dt>
        <dd className="mt-0.5 tabular-nums text-ink">{formatCount(counts.attachment_objects)}</dd>
      </div>
      <div>
        <dt className="text-ink-muted">Database tables</dt>
        <dd className="mt-0.5 tabular-nums text-ink">{formatCount(tables)}</dd>
      </div>
    </dl>
  )
}

export function ExclusionsList({ exclusions }: { exclusions: BackupExclusion[] }) {
  if (exclusions.length === 0) {
    return <p className="text-xs text-ink-muted">The manifest declares no exclusions.</p>
  }
  return (
    <ul aria-label="Excluded from this backup" className="flex flex-col gap-1.5">
      {exclusions.map((exclusion) => (
        <li key={exclusion.category} className="text-xs leading-5">
          <span className="font-medium text-ink-soft">{exclusion.category}:</span>{' '}
          <span className="text-ink-muted">{exclusion.detail}</span>
        </li>
      ))}
    </ul>
  )
}
