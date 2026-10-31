/**
 * Pure backup/restore presentation rules.
 *
 * Everything a person reads about a backup lives here so it can be unit-tested
 * without React or the network. The wording carries product promises and is
 * deliberately conservative:
 *
 * - verification is a statement about the archive, never about restored data;
 * - "verified at creation" is not "verified now" for a copied archive;
 * - replacement is maintenance, and the browser can only prepare a plan.
 */
import { formatMoment } from '@/domain'
import type {
  BackupCounts,
  BackupPolicy,
  BackupSchedule,
  BackupSummary,
  BackupVerificationStatus,
  RestorePreflightResponse,
  RestorePlanSummary,
} from '@/api/contracts'

/** Exact phrase a person must type before an isolated restore starts. */
export const RESTORE_CONFIRMATION = 'RESTORE'

/** Maximum retention values the server policy validates. */
export const RETENTION_LIMITS = {
  maxRetained: 365,
  minRetentionDays: 3650,
  maxTotalBytesMin: 1_048_576,
} as const

export const SCHEDULE_LABELS: Record<BackupSchedule, string> = {
  disabled: 'Disabled',
  daily: 'Daily',
  weekly: 'Weekly',
  before_upgrade: 'Before each upgrade',
}

export const SCHEDULE_OPTIONS: { value: BackupSchedule; label: string }[] = [
  { value: 'disabled', label: 'Disabled' },
  { value: 'daily', label: 'Daily' },
  { value: 'weekly', label: 'Weekly' },
  { value: 'before_upgrade', label: 'Before each upgrade' },
]

export interface ToneCopy {
  label: string
  tone: 'neutral' | 'success' | 'warning' | 'danger'
}

export function verificationCopy(status: BackupVerificationStatus): ToneCopy {
  switch (status) {
    case 'verified':
      return { label: 'Verified at creation', tone: 'success' }
    case 'failed':
      return { label: 'Verification failed', tone: 'danger' }
    default:
      return { label: 'Not verified', tone: 'warning' }
  }
}

/** A copied archive has no sidecar; say so instead of implying it is unsafe. */
export function verificationHint(status: BackupVerificationStatus): string {
  switch (status) {
    case 'verified':
      return 'Verified on this server when it was created.'
    case 'failed':
      return 'The creation verification recorded a failure. Do not restore from this file.'
    default:
      return 'No verification sidecar: this copy was not verified by this tool. Run Verify before trusting it.'
  }
}

export function scheduleSummary(policy: BackupPolicy): string {
  const schedule = SCHEDULE_LABELS[policy.schedule] ?? policy.schedule
  const limits: string[] = []
  if (policy.max_retained !== undefined) limits.push(`keep ${policy.max_retained}`)
  if (policy.min_retention_days !== undefined) {
    limits.push(`at least ${policy.min_retention_days} day${policy.min_retention_days === 1 ? '' : 's'}`)
  }
  if (policy.max_total_bytes !== undefined) limits.push(`up to ${formatBytes(policy.max_total_bytes)}`)
  return limits.length > 0 ? `${schedule} · ${limits.join(' · ')}` : schedule
}

export const SCHEDULE_APPLIED_NOTE =
  'Saved policy. The server applies automatic backups and retention on its next start; this page never runs them.'

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

export function formatCount(count: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(count)
}

/** A stable display name that never falls back to an empty string. */
export function backupName(backup: Pick<BackupSummary, 'label' | 'backup_id'>): string {
  return backup.label?.trim() || backup.backup_id
}

export function backupMoment(backup: Pick<BackupSummary, 'created_at' | 'created_at_ms'>): string {
  return formatMoment(backup.created_at_ms)
}

export function formatTimestamp(iso: string, fallbackMs?: number): string {
  const parsed = Date.parse(iso)
  if (Number.isNaN(parsed)) {
    return fallbackMs !== undefined ? formatMoment(fallbackMs) : iso
  }
  return formatMoment(parsed)
}

export function countsSummary(counts: BackupCounts): string {
  const tables = Object.keys(counts.tables ?? {}).length
  return [
    `${formatCount(counts.knowledge_notes)} note${counts.knowledge_notes === 1 ? '' : 's'}`,
    `${formatCount(counts.attachment_objects)} attachment object${counts.attachment_objects === 1 ? '' : 's'}`,
    `${formatCount(tables)} database table${tables === 1 ? '' : 's'}`,
  ].join(' · ')
}

/** The typed confirmation gate for an isolated restore. */
export function isRestoreConfirmationValid(input: string): boolean {
  return input.trim() === RESTORE_CONFIRMATION
}

export interface PlanFact {
  label: string
  value: string
  tone?: ToneCopy['tone']
}

function schemaFact(plan: RestorePlanSummary): PlanFact {
  if (!plan.schema_compatible) {
    return {
      label: 'Schema compatibility',
      value: `Backup schema ${plan.backup_schema_version} is newer than this build supports (schema ${plan.supported_schema_version}). Upgrade Orbit before restoring.`,
      tone: 'danger',
    }
  }
  if (plan.schema_upgrade_required) {
    return {
      label: 'Schema compatibility',
      value: `Backup schema ${plan.backup_schema_version} is older; the restored workspace is migrated to schema ${plan.supported_schema_version} on first open.`,
      tone: 'warning',
    }
  }
  return {
    label: 'Schema compatibility',
    value: `Backup schema ${plan.backup_schema_version} matches this build (schema ${plan.supported_schema_version}).`,
    tone: 'success',
  }
}

/** Facts rendered in the preflight panel; paths have already been redacted. */
export function planFacts(preflight: RestorePreflightResponse): PlanFact[] {
  const plan = preflight.plan
  const facts: PlanFact[] = [
    {
      label: 'Mode',
      value: plan.mode === 'isolated' ? 'Isolated copy (active workspace untouched)' : 'Replace authority (offline maintenance)',
    },
    {
      label: 'Backup created',
      value: formatTimestamp(plan.created_at),
    },
    { label: 'Archive size', value: formatBytes(plan.archive_bytes) },
    { label: 'Content', value: formatBytes(plan.totals.content_bytes) },
    schemaFact(plan),
  ]
  if (plan.mode === 'isolated') {
    facts.push({
      label: 'Restore folder name',
      value: plan.target.target_name ?? 'generated by the server',
    })
  }
  if (plan.mode === 'replace') {
    facts.push({
      label: 'Downtime',
      value: plan.downtime_required ? 'Required: the server must be stopped' : 'Not required',
      tone: plan.downtime_required ? 'warning' : 'neutral',
    })
    facts.push({ label: 'Replaces', value: plan.replaces.length > 0 ? plan.replaces.join(', ') : 'Nothing' })
    facts.push({ label: 'Preserves for rollback', value: plan.preserves.length > 0 ? plan.preserves.join(', ') : 'Nothing' })
  }
  if (plan.free_space_bytes !== null && plan.free_space_bytes < plan.totals.content_bytes) {
    facts.push({
      label: 'Free space',
      value: `${formatBytes(plan.free_space_bytes)} free, but ${formatBytes(plan.totals.content_bytes)} is needed`,
      tone: 'danger',
    })
  }
  return facts
}

/** Blocker/warning copy for a preflight that must not start a restore. */
export function preflightBlockedCopy(preflight: RestorePreflightResponse): string | null {
  if (preflight.admissible) return null
  if (preflight.blockers.length > 0) {
    return `This backup cannot be restored here: ${preflight.blockers.map((blocker) => blocker.detail).join(' ')}`
  }
  return 'This backup cannot be restored here; the preflight did not admit it.'
}

export const RESTORE_MODE_COPY = {
  isolated: {
    title: 'Isolated copy',
    description:
      'Restores into a new folder on the server and verifies it before it is opened. The active workspace is never modified. This is the recommended recovery drill.',
    action: 'Check isolated restore plan',
  },
  replace: {
    title: 'Replace authority',
    description:
      'Replaces the authoritative workspace after a server maintenance window. The browser can only prepare the offline plan: it never replaces the live database.',
    action: 'Check replacement plan',
  },
} as const

/** A short, path-free description of the last successful backup. */
export function lastBackupCopy(backup: BackupSummary | null): string {
  if (!backup) return 'No verified backup yet. Create one to have a recovery point.'
  return `${backupName(backup)} · ${backupMoment(backup)} · ${formatBytes(backup.size_bytes)}`
}

interface ErrorLike {
  status?: unknown
  message?: unknown
}

function errorStatus(error: unknown): number | null {
  if (!error || typeof error !== 'object') return null
  const status = (error as ErrorLike).status
  return typeof status === 'number' ? status : null
}

function errorMessageText(error: unknown): string {
  if (error && typeof error === 'object') {
    const message = (error as ErrorLike).message
    if (typeof message === 'string' && message.trim().length > 0) return message
  }
  return 'The backup request could not be completed.'
}

/** The server's message is authored human text; show it rather than hiding it. */
export function backupErrorMessage(error: unknown): string {
  if (errorStatus(error) === 503) {
    return `${errorMessageText(error)} The server build may not include a working backup service yet.`
  }
  return errorMessageText(error)
}

/** A concurrent create answered 409 "backup in progress"; not a failure to retry blindly. */
export function isBackupInProgress(error: unknown): boolean {
  return errorStatus(error) === 409 && /already in progress/i.test(errorMessageText(error))
}

/** The archive download was refused because it is above the server's limit. */
export function isDownloadTooLarge(error: unknown): boolean {
  return errorStatus(error) === 413
}

/** A restore preflight or command was refused because the workspace is busy. */
export function isWorkspaceBusy(error: unknown): boolean {
  const message = errorMessageText(error)
  return errorStatus(error) === 409 && /busy|fence/i.test(message)
}

