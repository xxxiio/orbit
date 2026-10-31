/**
 * Backup history table.
 *
 * Every row comes from an archive's own manifest, so a copy carried in from
 * another machine is listed honestly with `unknown` verification. The
 * verification badge describes creation-time verification, not the current
 * bytes — a copied archive must be verified again.
 */
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import {
  AlertTriangleIcon,
  DownloadIcon,
  EyeIcon,
  LayersIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { Panel, PanelHeader } from '@/components/ui/panel'
import { TableHead, TableHeadCell, TableScrollRegion } from '@/components/surface'
import type { BackupSummary, SkippedBackupSummary } from '@/api/contracts'
import {
  backupMoment,
  backupName,
  formatBytes,
  verificationCopy,
  verificationHint,
} from './backup-model'

function HistoryRow({
  backup,
  verifying,
  onInspect,
  onVerify,
  onDownload,
}: {
  backup: BackupSummary
  verifying: boolean
  onInspect: (backupId: string) => void
  onVerify: (backupId: string) => void
  onDownload: (backupId: string) => void
}) {
  const verification = verificationCopy(backup.verification)
  return (
    <tr data-backup-id={backup.backup_id} className="border-b border-line last:border-b-0">
      <td className="whitespace-nowrap px-4 py-2.5 text-xs text-ink">{backupMoment(backup)}</td>
      <td className="max-w-[18rem] px-4 py-2.5">
        <p className="truncate text-sm text-ink">{backupName(backup)}</p>
        <p className="truncate font-mono text-[0.625rem] text-ink-faint">{backup.backup_id}</p>
      </td>
      <td className="whitespace-nowrap px-4 py-2.5 text-right text-xs tabular-nums text-ink-soft">
        {formatBytes(backup.size_bytes)}
      </td>
      <td className="whitespace-nowrap px-4 py-2.5 text-xs text-ink-soft">
        v{backup.format_version} · schema {backup.schema_version}
      </td>
      <td className="whitespace-nowrap px-4 py-2.5">
        <span title={verificationHint(backup.verification)}>
          <Badge tone={verification.tone} dot>
            {verification.label}
          </Badge>
        </span>
        {backup.complete ? null : (
          <Badge tone="danger" className="ml-1">
            Incomplete
          </Badge>
        )}
      </td>
      <td className="whitespace-nowrap px-4 py-2.5 text-right">
        <div className="flex items-center justify-end gap-1">
          <Button
            variant="quiet"
            size="sm"
            leadingIcon={<EyeIcon size={14} />}
            onClick={() => onInspect(backup.backup_id)}
          >
            Inspect
          </Button>
          <Button
            variant="quiet"
            size="sm"
            leadingIcon={<RefreshIcon size={14} />}
            loading={verifying}
            disabled={verifying}
            onClick={() => onVerify(backup.backup_id)}
          >
            Verify
          </Button>
          <Button
            variant="quiet"
            size="sm"
            leadingIcon={<DownloadIcon size={14} />}
            onClick={() => onDownload(backup.backup_id)}
          >
            Download
          </Button>
        </div>
      </td>
    </tr>
  )
}

export function BackupHistoryPanel({
  backups,
  skipped,
  verifyingId,
  onInspect,
  onVerify,
  onDownload,
}: {
  backups: BackupSummary[]
  skipped: SkippedBackupSummary[]
  verifyingId: string | null
  onInspect: (backupId: string) => void
  onVerify: (backupId: string) => void
  onDownload: (backupId: string) => void
}) {
  return (
    <div className="flex flex-col gap-3">
      <Panel as="section" data-backup-history="">
        <PanelHeader
          eyebrow="Recovery points"
          title="Backup history"
          description="Newest first. Verified at creation is not the same as verified now: a copied archive must be verified again."
          actions={
            <Badge tone="neutral">
              {backups.length} archive{backups.length === 1 ? '' : 's'}
            </Badge>
          }
        />
        {backups.length === 0 ? (
          <EmptyState
            icon={<LayersIcon size={18} />}
            headingLevel="p"
            title="No backups yet"
            description="Create the first verified recovery point above. Archives are listed from the server's configured backup folder."
            compact
          />
        ) : (
          <TableScrollRegion>
            <table className="w-full border-collapse text-left">
              <TableHead>
                <TableHeadCell>Created</TableHeadCell>
                <TableHeadCell>Label</TableHeadCell>
                <TableHeadCell>Size</TableHeadCell>
                <TableHeadCell>Format</TableHeadCell>
                <TableHeadCell>Verification</TableHeadCell>
                <TableHeadCell>
                  <span className="sr-only">Actions</span>
                </TableHeadCell>
              </TableHead>
              <tbody>
                {backups.map((backup) => (
                  <HistoryRow
                    key={backup.backup_id}
                    backup={backup}
                    verifying={verifyingId === backup.backup_id}
                    onInspect={onInspect}
                    onVerify={onVerify}
                    onDownload={onDownload}
                  />
                ))}
              </tbody>
            </table>
          </TableScrollRegion>
        )}
      </Panel>

      {skipped.length > 0 ? (
        <Panel data-skipped-backups="" className="border-warning/30">
          <PanelHeader
            title="Some files in the backup folder could not be read"
            description="They are not listed as recovery points. A corrupt or partial archive must not be silently treated as a backup."
            actions={
              <Badge tone="warning" dot>
                <AlertTriangleIcon size={11} /> {skipped.length} skipped
              </Badge>
            }
          />
          <ul aria-label="Skipped backup files" className="divide-y divide-line">
            {skipped.map((file) => (
              <li key={file.file_name} className="px-5 py-2.5 text-xs">
                <p className="font-mono text-ink-soft">{file.file_name}</p>
                <p className="mt-0.5 text-ink-muted">{file.reason}</p>
              </li>
            ))}
          </ul>
        </Panel>
      ) : null}
    </div>
  )
}
