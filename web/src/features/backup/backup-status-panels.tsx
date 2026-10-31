/**
 * The overview row of the Backup & Restore settings page: the last verified
 * recovery point, the destination, and the automatic-backup policy.
 *
 * The policy editor stores what the shared backup service persists; it does not
 * pretend to run scheduling or retention (a later iteration owns execution).
 */
import { useEffect, useState, type FormEvent } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  AlertTriangleIcon,
  CheckCircleIcon,
  ClockIcon,
  DownloadIcon,
  FolderIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader, PanelFooter } from '@/components/ui/panel'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type {
  BackupPolicy,
  BackupSchedule,
  BackupSummary,
  BackupOverviewResponse,
} from '@/api/contracts'
import type { ApiError } from '@/api/client'
import {
  backupName,
  backupMoment,
  formatBytes,
  SCHEDULE_APPLIED_NOTE,
  SCHEDULE_OPTIONS,
  scheduleSummary,
  verificationCopy,
} from './backup-model'
import { useUpdateSchedule } from './use-backup'

function errorText(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === 'string' && message.trim().length > 0) return message
  }
  return 'The request could not be completed.'
}

function LastBackupPanel({
  lastBackup,
  onInspect,
  onDownload,
}: {
  lastBackup: BackupSummary | null
  onInspect: (backupId: string) => void
  onDownload: (backupId: string) => void
}) {
  const verification = lastBackup ? verificationCopy(lastBackup.verification) : null
  return (
    <Panel className="flex flex-col" data-last-backup="">
      <PanelHeader
        eyebrow="Recovery point"
        title="Last verified backup"
        description="The newest archive that was verified on this server at creation."
        actions={
          lastBackup ? (
            <Badge tone={verification?.tone ?? 'neutral'} dot>
              {verification?.label ?? 'Verified'}
            </Badge>
          ) : (
            <Badge tone="warning">None yet</Badge>
          )
        }
      />
      <PanelBody className="flex-1">
        {lastBackup ? (
          <>
            <p className="flex items-center gap-2 text-sm font-medium text-ink">
              <CheckCircleIcon size={15} className="shrink-0 text-success" />
              <span className="min-w-0 truncate">{backupName(lastBackup)}</span>
            </p>
            <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
              <div>
                <dt className="text-ink-muted">Created</dt>
                <dd className="mt-0.5 flex items-center gap-1 text-ink">
                  <ClockIcon size={11} className="text-ink-faint" />
                  {backupMoment(lastBackup)}
                </dd>
              </div>
              <div>
                <dt className="text-ink-muted">Size</dt>
                <dd className="mt-0.5 tabular-nums text-ink">{formatBytes(lastBackup.size_bytes)}</dd>
              </div>
              <div>
                <dt className="text-ink-muted">Format</dt>
                <dd className="mt-0.5 text-ink">
                  v{lastBackup.format_version} · schema {lastBackup.schema_version}
                </dd>
              </div>
              <div>
                <dt className="text-ink-muted">Archive</dt>
                <dd className="mt-0.5 break-all font-mono text-[0.6875rem] text-ink-muted">
                  {lastBackup.backup_id}
                </dd>
              </div>
            </dl>
          </>
        ) : (
          <div className="flex h-full flex-col justify-center">
            <p className="text-sm font-medium text-ink">No verified backup yet</p>
            <p className="mt-1 text-xs leading-5 text-ink-muted">
              Create a backup to have a recovery point. A backup on this disk does not survive disk
              failure — copy verified archives to another device.
            </p>
          </div>
        )}
      </PanelBody>
      {lastBackup ? (
        <PanelFooter>
          <div className="flex gap-2">
            <Button variant="secondary" size="sm" onClick={() => onInspect(lastBackup.backup_id)}>
              Inspect
            </Button>
            <Button
              variant="quiet"
              size="sm"
              leadingIcon={<DownloadIcon size={14} />}
              onClick={() => onDownload(lastBackup.backup_id)}
            >
              Download
            </Button>
          </div>
        </PanelFooter>
      ) : null}
    </Panel>
  )
}

function DestinationPanel({ destination }: { destination: BackupOverviewResponse['destination'] }) {
  return (
    <Panel data-backup-destination="">
      <PanelHeader
        eyebrow="Destination"
        title={destination.label}
        description="Archives are written to the server's configured backup folder."
        actions={
          <Badge tone={destination.writable ? 'success' : 'danger'} dot>
            {destination.writable ? 'Writable' : 'Not writable'}
          </Badge>
        }
      />
      <PanelBody className="flex flex-col gap-3">
        <p className="flex items-start gap-2 text-xs leading-5 text-ink-muted">
          <FolderIcon size={14} className="mt-0.5 shrink-0" />
          <span>
            {destination.writable
              ? 'The server can write archives here. Copying a verified archive to another disk or trusted location is still the only protection against disk failure.'
              : 'The server cannot write to its backup folder. Creating a backup will fail until an operator fixes the directory permissions.'}
          </span>
        </p>
        <p className="flex items-start gap-2 border-t border-line pt-3 text-xs leading-5 text-ink-muted">
          <AlertTriangleIcon size={14} className="mt-0.5 shrink-0 text-warning" />
          <span>
            A backup briefly pauses workspace writes while the database snapshot is taken. Very
            large workspaces can make that pause noticeable; concurrent writes may be asked to
            retry.
          </span>
        </p>
      </PanelBody>
    </Panel>
  )
}

function numericOrUndefined(raw: string): number | undefined {
  const trimmed = raw.trim()
  if (trimmed === '') return undefined
  const parsed = Number(trimmed)
  return Number.isFinite(parsed) ? parsed : undefined
}

function SchedulePanel({ policy }: { policy: BackupPolicy }) {
  const update = useUpdateSchedule()
  const [schedule, setSchedule] = useState<BackupSchedule>(policy.schedule)
  const [maxRetained, setMaxRetained] = useState(
    policy.max_retained !== undefined ? String(policy.max_retained) : '',
  )
  const [minRetentionDays, setMinRetentionDays] = useState(
    policy.min_retention_days !== undefined ? String(policy.min_retention_days) : '',
  )
  const [maxTotalMb, setMaxTotalMb] = useState(
    policy.max_total_bytes !== undefined
      ? String(Math.round(policy.max_total_bytes / (1024 * 1024)))
      : '',
  )

  useEffect(() => {
    setSchedule(policy.schedule)
    setMaxRetained(policy.max_retained !== undefined ? String(policy.max_retained) : '')
    setMinRetentionDays(
      policy.min_retention_days !== undefined ? String(policy.min_retention_days) : '',
    )
    setMaxTotalMb(
      policy.max_total_bytes !== undefined
        ? String(Math.round(policy.max_total_bytes / (1024 * 1024)))
        : '',
    )
  }, [policy])

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const megabytes = numericOrUndefined(maxTotalMb)
    update.mutate({
      schedule,
      max_retained: numericOrUndefined(maxRetained) ?? null,
      min_retention_days: numericOrUndefined(minRetentionDays) ?? null,
      max_total_bytes: megabytes !== undefined ? megabytes * 1024 * 1024 : null,
    })
  }

  return (
    <Panel as="section" data-backup-schedule="">
      <PanelHeader
        eyebrow="Automation"
        title="Automatic backups"
        description="Stored now; the server applies scheduling and retention on its next start."
        actions={<Badge tone={policy.schedule === 'disabled' ? 'neutral' : 'info'}>{scheduleSummary(policy)}</Badge>}
      />
      <form onSubmit={submit}>
        <PanelBody className="flex flex-col gap-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Schedule">
              <Select value={schedule} onValueChange={(value) => setSchedule(value as BackupSchedule)}>
                <SelectTrigger aria-label="Automatic backup schedule">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {SCHEDULE_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label="Keep at most" hint="Archives; the newest verified backup is always kept.">
              <Input
                inputMode="numeric"
                placeholder="e.g. 10"
                value={maxRetained}
                onChange={(event) => setMaxRetained(event.target.value)}
              />
            </Field>
            <Field label="Keep at least (days)" hint="Minimum age before an archive may be removed.">
              <Input
                inputMode="numeric"
                placeholder="e.g. 7"
                value={minRetentionDays}
                onChange={(event) => setMinRetentionDays(event.target.value)}
              />
            </Field>
            <Field label="Maximum total size (MB)" hint="Oldest archives are removed first.">
              <Input
                inputMode="numeric"
                placeholder="e.g. 2048"
                value={maxTotalMb}
                onChange={(event) => setMaxTotalMb(event.target.value)}
              />
            </Field>
          </div>
          <p className="text-xs leading-5 text-ink-muted">{SCHEDULE_APPLIED_NOTE}</p>
          {update.error ? (
            <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
              <AlertTriangleIcon size={13} className="mt-0.5 shrink-0" />
              <span>{errorText(update.error as ApiError)}</span>
            </p>
          ) : null}
          {update.isSuccess ? (
            <p role="status" className="text-xs leading-5 text-success-ink">
              Policy saved. The server will read it on its next start.
            </p>
          ) : null}
        </PanelBody>
        <PanelFooter>
          <p className="text-2xs text-ink-faint">This page never runs a scheduled backup.</p>
          <Button type="submit" size="sm" variant="secondary" loading={update.isPending}>
            <RefreshIcon size={14} />
            Save policy
          </Button>
        </PanelFooter>
      </form>
    </Panel>
  )
}

export function BackupStatusPanels({
  overview,
  onInspect,
  onDownload,
}: {
  overview: BackupOverviewResponse
  onInspect: (backupId: string) => void
  onDownload: (backupId: string) => void
}) {
  return (
    <div className="grid gap-5 lg:grid-cols-2 xl:grid-cols-3">
      <LastBackupPanel
        lastBackup={overview.last_backup}
        onInspect={onInspect}
        onDownload={onDownload}
      />
      <DestinationPanel destination={overview.destination} />
      <SchedulePanel policy={overview.schedule} />
    </div>
  )
}
