/**
 * Backup & Restore settings page (`/settings/backup`).
 *
 * The page is one honest story: what the last verified recovery point is,
 * where the archive lives, which policy is stored, how to create a new one, and
 * how to restore safely. Creating is safe; restoring is destructive. Only an
 * isolated restore can be started here (after typing the confirmation word), and
 * replacement can only produce an offline maintenance plan.
 */
import { useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ErrorState } from '@/components/ui/error-state'
import { PageHeader } from '@/components/ui/page-header'
import { Panel } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { RefreshIcon } from '@/components/ui/icons'
import { useDocumentTitle } from '@/hooks'
import { backupDownloadUrl } from '@/api/endpoints/backup'
import { backupErrorMessage, isDownloadTooLarge } from './backup-model'
import { BackupStatusPanels } from './backup-status-panels'
import { CreateBackupPanel } from './create-backup-panel'
import { BackupHistoryPanel } from './backup-history-panel'
import { BackupInspectDrawer } from './backup-inspect-drawer'
import { RestorePanel } from './restore-panel'
import { useBackupOverview, useVerifyBackup } from './use-backup'
import type { VerificationReport } from '@/api/contracts'

function OverviewSkeleton() {
  return (
    <div className="grid gap-5 lg:grid-cols-2 xl:grid-cols-3" aria-busy="true" aria-label="Loading backups">
      <Skeleton className="h-52 rounded-lg" />
      <Skeleton className="h-52 rounded-lg" />
      <Skeleton className="h-52 rounded-lg" />
    </div>
  )
}

export function BackupSettingsPage() {
  useDocumentTitle('Backup & Restore')
  const overview = useBackupOverview()
  const verify = useVerifyBackup()
  const { toast } = useToast()
  const [inspectId, setInspectId] = useState<string | null>(null)
  const [rowReports, setRowReports] = useState<Record<string, VerificationReport>>({})

  const download = async (backupId: string) => {
    try {
      const response = await fetch(backupDownloadUrl(backupId), { credentials: 'include' })
      if (!response.ok) {
        const payload = (await response.json().catch(() => null)) as { error?: unknown } | null
        const message =
          payload && typeof payload.error === 'string'
            ? payload.error
            : `The archive could not be downloaded (HTTP ${response.status}).`
        toast({
          title: isDownloadTooLarge({ status: response.status })
            ? 'Archive too large for a browser download'
            : 'Download failed',
          description: message,
          tone: 'danger',
        })
        return
      }
      const blob = await response.blob()
      const objectUrl = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = objectUrl
      anchor.download = backupId
      anchor.rel = 'noopener'
      document.body.append(anchor)
      anchor.click()
      anchor.remove()
      URL.revokeObjectURL(objectUrl)
      toast({ title: 'Download started', description: backupId, tone: 'success' })
    } catch (error) {
      toast({
        title: 'Download failed',
        description: backupErrorMessage(error),
        tone: 'danger',
      })
    }
  }

  const runVerify = (backupId: string) => {
    verify.mutate(backupId, {
      onSuccess: (report) => {
        setRowReports((current) => ({ ...current, [backupId]: report }))
        toast({
          title: report.ok ? 'Verification passed' : 'Verification failed',
          description: `${report.verified_files.length} file(s) checked${
            report.problems.length > 0 ? ` · ${report.problems.length} problem(s)` : ''
          }`,
          tone: report.ok ? 'success' : 'danger',
        })
      },
      onError: (error) => {
        toast({ title: 'Verification failed', description: backupErrorMessage(error), tone: 'danger' })
      },
    })
  }

  const data = overview.data

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Settings"
        title="Backup & Restore"
        description="A backup is one verified file containing the database, knowledge vault and attachments. A copy on this disk does not survive disk failure."
        actions={
          <Button
            variant="secondary"
            size="sm"
            leadingIcon={<RefreshIcon size={15} />}
            loading={overview.isFetching}
            onClick={() => void overview.refetch()}
          >
            Refresh
          </Button>
        }
      />

      {overview.isPending && !data ? (
        <OverviewSkeleton />
      ) : overview.error && !data ? (
        <Panel>
          <ErrorState
            title="Backups could not be read"
            description={backupErrorMessage(overview.error)}
            onRetry={() => void overview.refetch()}
            retrying={overview.isFetching}
          />
        </Panel>
      ) : data ? (
        <>
          {overview.error ? (
            <p role="alert" className="rounded-md border border-danger/30 bg-danger-soft/40 px-3 py-2 text-xs leading-5 text-danger-ink">
              The latest refresh failed; showing the last successful read. {backupErrorMessage(overview.error)}
            </p>
          ) : null}

          <BackupStatusPanels
            overview={data}
            onInspect={(backupId) => setInspectId(backupId)}
            onDownload={(backupId) => void download(backupId)}
          />

          <CreateBackupPanel
            onInspect={(backupId) => setInspectId(backupId)}
            onDownload={(backupId) => void download(backupId)}
          />

          <BackupHistoryPanel
            backups={data.backups}
            skipped={data.skipped}
            verifyingId={verify.isPending ? (verify.variables ?? null) : null}
            onInspect={(backupId) => setInspectId(backupId)}
            onVerify={runVerify}
            onDownload={(backupId) => void download(backupId)}
          />

          <RestorePanel backups={data.backups} />

          <p className="flex flex-wrap items-center gap-2 px-1 text-2xs leading-4 text-ink-faint">
            <Badge tone="neutral">Scope</Badge>
            Archives contain the database, the Markdown vault, referenced attachment objects and
            normalized non-secret configuration. Credentials, administrator keys and external agent
            sessions are never included.
          </p>
        </>
      ) : null}

      <BackupInspectDrawer
        backupId={inspectId}
        onClose={() => setInspectId(null)}
        onDownload={(backupId) => void download(backupId)}
        externalReport={inspectId ? (rowReports[inspectId] ?? null) : null}
      />
    </div>
  )
}
