/**
 * Inspect drawer: manifest-only archive detail, creation verification and an
 * on-demand verification run.
 *
 * Inspection is cheap (the manifest is the first archive entry) and never
 * extracts or mutates anything. "Verify now" re-hashes the archive on the
 * server; it still does not restore anything.
 */
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Drawer } from '@/components/ui/drawer'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { AlertTriangleIcon, DownloadIcon, RefreshIcon } from '@/components/ui/icons'
import { Section } from '@/components/ui/section'
import { SkeletonText } from '@/components/ui/skeleton'
import { backupErrorMessage, formatBytes, formatCount, formatTimestamp } from './backup-model'
import { ComponentTable, CountsGrid, ExclusionsList } from './backup-components'
import { VerificationReportView } from './verification-report-view'
import { useBackupInspection, useVerifyBackup } from './use-backup'
import type { BackupInspectionResponse, VerificationReport } from '@/api/contracts'

function InspectionBody({
  inspection,
  backupId,
  onDownload,
  externalReport,
}: {
  inspection: BackupInspectionResponse
  backupId: string
  onDownload: (backupId: string) => void
  externalReport?: VerificationReport | null
}) {
  const verify = useVerifyBackup()
  const creation = inspection.creation_verification
  const displayedReport = verify.data ?? externalReport ?? null

  return (
    <>
      <Section title="Summary" description="Read from the manifest inside the archive.">
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
          <div>
            <dt className="text-ink-muted">Created</dt>
            <dd className="mt-0.5 text-ink">
              {formatTimestamp(inspection.created_at, inspection.created_at_ms)}
            </dd>
          </div>
          <div>
            <dt className="text-ink-muted">Archive size</dt>
            <dd className="mt-0.5 tabular-nums text-ink">{formatBytes(inspection.archive.bytes)}</dd>
          </div>
          <div>
            <dt className="text-ink-muted">Format</dt>
            <dd className="mt-0.5 text-ink">
              v{inspection.format_version} (min reader v{inspection.min_reader_version})
            </dd>
          </div>
          <div>
            <dt className="text-ink-muted">Schema</dt>
            <dd className="mt-0.5 text-ink">{inspection.schema_version}</dd>
          </div>
          <div>
            <dt className="text-ink-muted">Produced by</dt>
            <dd className="mt-0.5 text-ink">
              Orbit {inspection.orbit_version}
              {inspection.orbit_build ? ` (${inspection.orbit_build})` : ''}
            </dd>
          </div>
          <div>
            <dt className="text-ink-muted">Complete</dt>
            <dd className="mt-0.5">
              <Badge tone={inspection.complete ? 'success' : 'danger'} dot>
                {inspection.complete ? 'Every claimed component present' : 'Incomplete'}
              </Badge>
            </dd>
          </div>
        </dl>
        <p className="mt-3 break-all font-mono text-[0.6875rem] text-ink-muted">
          {inspection.backup_id}
        </p>
      </Section>

      <Section title="Source" description="Human description only; the archive stores no source path.">
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
          <div>
            <dt className="text-ink-muted">Authority mode</dt>
            <dd className="mt-0.5 text-ink">{inspection.source.described_as}</dd>
          </div>
          <div>
            <dt className="text-ink-muted">Revision</dt>
            <dd className="mt-0.5 text-ink">
              {inspection.source.revision !== undefined ? inspection.source.revision : 'Not recorded'}
            </dd>
          </div>
          <div className="col-span-2">
            <dt className="text-ink-muted">Workspace id</dt>
            <dd className="mt-0.5 break-all font-mono text-ink-soft">
              {inspection.source.workspace_id ?? 'Not recorded'}
            </dd>
          </div>
          {inspection.source.excluded_client_pending_data ? (
            <div className="col-span-2 rounded-md border border-warning/30 bg-warning-soft/40 px-2.5 py-2 text-warning-ink">
              Unaccepted client data was known to be outside this snapshot.
            </div>
          ) : null}
        </dl>
        {inspection.config ? (
          <p className="mt-3 text-xs leading-5 text-ink-muted">
            Configuration snapshot: authority mode{' '}
            <span className="text-ink">{inspection.config.authority_mode}</span>
            {inspection.config.default_agent_provider
              ? `, default agent provider ${inspection.config.default_agent_provider}`
              : ''}
            . Credentials are never included.
          </p>
        ) : null}
      </Section>

      <Section title="Archive contents" description="Every inventoried file except the manifest itself.">
        <ComponentTable components={inspection.components} />
        <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-2 border-t border-line pt-3 text-xs">
          <div>
            <dt className="text-ink-muted">Inventory files</dt>
            <dd className="mt-0.5 tabular-nums text-ink">
              {formatCount(inspection.archive.inventory_files)}
            </dd>
          </div>
          <div>
            <dt className="text-ink-muted">Content bytes</dt>
            <dd className="mt-0.5 tabular-nums text-ink">
              {formatBytes(inspection.archive.content_bytes)}
            </dd>
          </div>
          <div>
            <dt className="text-ink-muted">Knowledge folders</dt>
            <dd className="mt-0.5 tabular-nums text-ink">
              {formatCount(inspection.archive.knowledge_directories)}
            </dd>
          </div>
          <div>
            <dt className="text-ink-muted">Archive bytes</dt>
            <dd className="mt-0.5 tabular-nums text-ink">
              {formatBytes(inspection.totals.archive_bytes)}
            </dd>
          </div>
        </dl>
      </Section>

      <Section title="Recorded counts" description="Compare before and after a restore.">
        <CountsGrid counts={inspection.counts} />
      </Section>

      <Section title="Excluded from this backup">
        <ExclusionsList exclusions={inspection.exclusions} />
      </Section>

      <Section
        title="Creation verification"
        description="The sidecar written beside the archive after create-and-verify."
      >
        {creation ? (
          <div className="flex flex-col gap-2">
            <div className="flex flex-wrap items-center gap-2">
              <Badge tone={creation.ok ? 'success' : 'danger'} dot>
                {creation.ok ? 'Passed at creation' : 'Failed at creation'}
              </Badge>
              <span className="text-xs text-ink-muted">
                {formatTimestamp(new Date(creation.verified_at_ms).toISOString())} ·{' '}
                {formatCount(creation.verified_files)} files · {formatBytes(creation.archive_bytes)}
              </span>
            </div>
            {creation.problems.length > 0 ? (
              <ul aria-label="Creation verification problems" className="flex flex-col gap-1">
                {creation.problems.map((problem, index) => (
                  <li key={index} className="text-xs leading-5 text-danger-ink">
                    {problem.check}: {problem.detail}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
        ) : (
          <p className="flex items-start gap-1.5 text-xs leading-5 text-warning-ink">
            <AlertTriangleIcon size={13} className="mt-0.5 shrink-0" />
            No creation verification sidecar is present. This copy was not verified by this tool; a
            copy that travelled between machines must be verified again before it is trusted.
          </p>
        )}
      </Section>

      <Section
        title="Verify now"
        description="Re-hashes the archive on the server. Verification is not a restore."
      >
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            leadingIcon={<RefreshIcon size={14} />}
            loading={verify.isPending}
            disabled={verify.isPending}
            onClick={() => verify.mutate(backupId)}
          >
            {verify.isPending ? 'Verifying…' : 'Verify archive'}
          </Button>
          <Button
            variant="quiet"
            size="sm"
            leadingIcon={<DownloadIcon size={14} />}
            onClick={() => onDownload(backupId)}
          >
            Download archive
          </Button>
        </div>
        {verify.error ? (
          <p role="alert" className="mt-3 text-xs leading-5 text-danger-ink">
            {backupErrorMessage(verify.error)}
          </p>
        ) : null}
        {displayedReport ? (
          <div className="mt-3">
            <VerificationReportView report={displayedReport} />
          </div>
        ) : null}
      </Section>
    </>
  )
}

export function BackupInspectDrawer({
  backupId,
  onClose,
  onDownload,
  externalReport = null,
}: {
  backupId: string | null
  onClose: () => void
  onDownload: (backupId: string) => void
  /** A verification report produced from the history row, shown before a re-run. */
  externalReport?: VerificationReport | null
}) {
  const inspection = useBackupInspection(backupId)
  return (
    <Drawer
      open={backupId !== null}
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
      title="Backup details"
      description="Manifest contents, creation verification and an on-demand verification run."
      side="right"
      closeLabel="Close backup details"
      className="w-full max-w-none border-l border-line sm:w-[min(34rem,94vw)]"
    >
      {backupId === null ? null : (
        <div className="flex h-full min-h-0 flex-col">
          <header className="border-b border-line px-5 pb-4 pr-12 pt-5">
            <p className="section-label">Recovery point</p>
            <p className="mt-1 break-all font-mono text-xs text-ink">{backupId}</p>
          </header>
          {/* The sheet is a fixed-height flex column: only this body scrolls, so
              the verify/download controls at the bottom stay reachable. */}
          <div className="scrollbar-slim flex-1 overflow-y-auto">
            {inspection.isPending && !inspection.data ? (
              <div className="px-5 py-5" aria-busy="true">
                <SkeletonText lines={6} />
              </div>
            ) : inspection.error ? (
              <div className="px-5 py-6">
                <ErrorState
                  compact
                  title="Backup could not be inspected"
                  description={backupErrorMessage(inspection.error)}
                  onRetry={() => void inspection.refetch()}
                  retrying={inspection.isFetching}
                />
              </div>
            ) : inspection.data ? (
              <div className="divide-y divide-line">
                <InspectionBody
                  inspection={inspection.data}
                  backupId={backupId}
                  onDownload={onDownload}
                  externalReport={externalReport}
                />
              </div>
            ) : (
              <div className="px-5 py-6">
                <EmptyState compact headingLevel="p" title="No backup selected" />
              </div>
            )}
          </div>
        </div>
      )}
    </Drawer>
  )
}
