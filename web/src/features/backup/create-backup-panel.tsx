/**
 * Create-backup panel.
 *
 * Creating a backup is safe: the shared service takes a fenced snapshot, writes
 * a temporary archive, verifies it and only then publishes. The success panel
 * therefore states exactly what the server reported ("verification passed") and
 * never claims the workspace was restored.
 */
import { useState, type FormEvent } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  AlertCircleIcon,
  AlertTriangleIcon,
  CheckCircleIcon,
  DownloadIcon,
  EyeIcon,
  LoaderIcon,
  PlusIcon,
} from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import {
  backupErrorMessage,
  formatBytes,
  isBackupInProgress,
  type ToneCopy,
} from './backup-model'
import { ComponentTable } from './backup-components'
import { useCreateBackup } from './use-backup'
import type { CreateBackupResponse } from '@/api/contracts'

function CreateResult({
  result,
  onInspect,
  onDownload,
}: {
  result: CreateBackupResponse
  onInspect: (backupId: string) => void
  onDownload: (backupId: string) => void
}) {
  const tone: ToneCopy = result.verified
    ? { label: 'Verification passed', tone: 'success' }
    : { label: 'Verification failed', tone: 'danger' }
  return (
    <div data-create-result={result.backup_id} className="rounded-lg border border-line bg-surface-sunken/40 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={tone.tone} dot>
          {tone.label}
        </Badge>
        <p className="min-w-0 flex-1 truncate text-sm font-medium text-ink">
          {result.label ?? result.backup_id}
        </p>
      </div>
      <p className="mt-1.5 break-all font-mono text-[0.6875rem] text-ink-muted">
        {result.backup_id}
      </p>
      <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-2 text-xs sm:grid-cols-4">
        <div>
          <dt className="text-ink-muted">Created</dt>
          <dd className="mt-0.5 text-ink">{formatIso(result.created_at)}</dd>
        </div>
        <div>
          <dt className="text-ink-muted">Size</dt>
          <dd className="mt-0.5 tabular-nums text-ink">{formatBytes(result.size_bytes)}</dd>
        </div>
        <div>
          <dt className="text-ink-muted">Format</dt>
          <dd className="mt-0.5 text-ink">
            v{result.format_version} · schema {result.schema_version}
          </dd>
        </div>
        <div>
          <dt className="text-ink-muted">Components</dt>
          <dd className="mt-0.5 text-ink">
            {result.components.reduce((total, component) => total + component.files, 0)} files
          </dd>
        </div>
      </dl>
      <p className="mt-2 text-xs leading-5 text-ink-muted">{result.verification_summary}</p>
      {result.warnings.length > 0 ? (
        <ul aria-label="Backup warnings" className="mt-2 flex flex-col gap-1">
          {result.warnings.map((warning, index) => (
            <li key={index} className="flex items-start gap-1.5 text-xs leading-5 text-warning-ink">
              <AlertTriangleIcon size={13} className="mt-0.5 shrink-0" />
              <span>{warning}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="mt-3 border-t border-line pt-3">
        <ComponentTable components={result.components} />
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button
          variant="secondary"
          size="sm"
          leadingIcon={<EyeIcon size={14} />}
          onClick={() => onInspect(result.backup_id)}
        >
          Inspect backup
        </Button>
        <Button
          variant="quiet"
          size="sm"
          leadingIcon={<DownloadIcon size={14} />}
          onClick={() => onDownload(result.backup_id)}
        >
          Download
        </Button>
      </div>
    </div>
  )
}

function formatIso(iso: string): string {
  const parsed = Date.parse(iso)
  if (Number.isNaN(parsed)) return iso
  return new Date(parsed).toLocaleString()
}

export function CreateBackupPanel({
  onInspect,
  onDownload,
}: {
  onInspect: (backupId: string) => void
  onDownload: (backupId: string) => void
}) {
  const create = useCreateBackup()
  const [label, setLabel] = useState('')

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const trimmed = label.trim()
    create.mutate(trimmed.length > 0 ? { label: trimmed } : {})
  }

  const result = create.data
  const error = create.error
  const inProgress = create.isPending

  return (
    <Panel as="section" data-create-backup="">
      <PanelHeader
        eyebrow="Safe action"
        title="Create a backup"
        description="One verified .orbitbak file containing the database, knowledge vault and attachments. Creating a backup never changes the workspace."
      />
      <form onSubmit={submit}>
        <PanelBody className="flex flex-col gap-4">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-end">
            <Field
              label="Label (optional)"
              hint="A short note recorded in the archive name and manifest."
              className="min-w-0 flex-1"
            >
              <Input
                placeholder="e.g. before upgrade"
                maxLength={64}
                value={label}
                onChange={(event) => setLabel(event.target.value)}
                disabled={inProgress}
              />
            </Field>
            <Button
              type="submit"
              loading={inProgress}
              disabled={inProgress}
              leadingIcon={inProgress ? <LoaderIcon size={15} /> : <PlusIcon size={15} />}
              className="sm:mb-6"
            >
              {inProgress ? 'Creating and verifying…' : 'Create backup'}
            </Button>
          </div>
          <p className="text-xs leading-5 text-ink-muted">
            Only one backup can run at a time. The workspace briefly pauses writes while the
            database snapshot is taken; a large workspace can make that pause noticeable. The
            archive is verified before it is published, and a failed backup never replaces a
            previous one.
          </p>
          {inProgress ? (
            <p role="status" aria-busy="true" className="flex items-center gap-2 text-xs text-ink-muted">
              <LoaderIcon size={13} className="animate-spin" />
              Taking a fenced snapshot, writing the archive and verifying it. Do not start another
              backup.
            </p>
          ) : null}
          {error ? (
            <div role="alert" className="rounded-md border border-danger/30 bg-danger-soft/50 px-3 py-2">
              <p className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
                <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                <span className="min-w-0 flex-1">
                  {isBackupInProgress(error)
                    ? 'A backup is already in progress on this server. Wait for it to finish, then retry.'
                    : backupErrorMessage(error)}
                </span>
              </p>
            </div>
          ) : null}
          {result ? (
            <CreateResult
              result={result}
              onInspect={onInspect}
              onDownload={onDownload}
            />
          ) : null}
        </PanelBody>
      </form>
      {!result ? (
        <p className="flex items-start gap-1.5 border-t border-line px-5 py-3 text-2xs leading-4 text-ink-faint">
          <CheckCircleIcon size={12} className="mt-0.5 shrink-0 text-success" />
          Safe to repeat: each run creates a new archive. Existing archives are never overwritten.
        </p>
      ) : null}
    </Panel>
  )
}
