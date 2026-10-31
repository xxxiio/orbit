/**
 * Wire contracts for the workspace backup and restore API.
 *
 * These mirror the Rust serialisation in `crates/orbit-server/src/backup_api.rs`
 * exactly. Three things are deliberate and must not be "cleaned up":
 *
 * 1. `BackupSummary.backup_id` is an archive *file name*, never a path. The
 *    server resolves it inside its configured backup directory, so the browser
 *    cannot ask for an arbitrary file.
 * 2. Optional Rust fields with `skip_serializing_if` are absent keys here, not
 *    `null`. Fields without that annotation (`label`, `verification_checked_at_ms`,
 *    `orbit_build`-aside) are always present and may be `null`.
 * 3. Restore plans carry component labels, not server paths. Operator-only
 *    replacement commands are the single exception and are returned only by the
 *    administrator route.
 */

/** Which automatic backups the server policy enables. */
export type BackupSchedule = 'disabled' | 'daily' | 'weekly' | 'before_upgrade'

/** Whether an archive was verified when it was created. */
export type BackupVerificationStatus = 'verified' | 'failed' | 'unknown'

/** Authority mode of the snapshotted database. */
export type BackupSourceMode = 'local' | 'server' | 'replica'

/** Which restore path is being planned. */
export type RestoreMode = 'isolated' | 'replace'

/** One row of the history table, derived from the archive's own manifest. */
export interface BackupSummary {
  /** Archive file name; the stable browser-facing identifier. */
  backup_id: string
  label: string | null
  created_at: string
  created_at_ms: number
  size_bytes: number
  format_version: number
  schema_version: number
  source_mode: BackupSourceMode
  verification: BackupVerificationStatus
  verification_checked_at_ms: number | null
  complete: boolean
}

export interface SkippedBackupSummary {
  /** File name only; the server never returns its directory. */
  file_name: string
  reason: string
}

export interface BackupDestinationSummary {
  label: string
  writable: boolean
}

/** `GET /v1/backups`. */
export interface BackupOverviewResponse {
  destination: BackupDestinationSummary
  schedule: BackupPolicy
  last_backup: BackupSummary | null
  backups: BackupSummary[]
  skipped: SkippedBackupSummary[]
}

/** `policy.json`, written by the shared backup service (`deny_unknown_fields`). */
export interface BackupPolicy {
  version: number
  schedule: BackupSchedule
  max_retained?: number
  min_retention_days?: number
  max_total_bytes?: number
}

/** `PUT /v1/backups/schedule` body: the policy without its schema version. */
export interface BackupPolicyUpdate {
  schedule: BackupSchedule
  max_retained?: number | null
  min_retention_days?: number | null
  max_total_bytes?: number | null
}

export interface BackupComponentSummary {
  component: 'database' | 'knowledge' | 'attachments' | 'configuration' | string
  label: string
  files: number
  bytes: number
}

export interface BackupTotals {
  database_bytes: number
  knowledge_bytes: number
  attachment_bytes: number
  config_bytes: number
  content_bytes: number
  archive_bytes: number
}

export interface BackupCounts {
  knowledge_notes: number
  knowledge_folders: number
  attachment_objects: number
  attachment_associations: number
  /** SQLite row count per table present in the snapshot. */
  tables: Record<string, number>
}

export interface BackupSource {
  mode: BackupSourceMode
  workspace_id?: string
  revision?: number
  /** Human description; never an absolute path. */
  described_as: string
  excluded_client_pending_data: boolean
}

export interface BackupConfigSnapshot {
  config_version: number
  authority_mode: string
  default_agent_provider?: string
  connected_server?: string
}

export interface BackupExclusion {
  category: string
  detail: string
}

export interface VerificationProblem {
  check: string
  detail: string
}

/** Recorded beside an archive after create-and-verify; not part of the archive. */
export interface VerificationSidecar {
  verified_at_ms: number
  ok: boolean
  archive_bytes: number
  format_version: number
  verified_files: number
  problems: VerificationProblem[]
}

export interface InventoryEntry {
  path: string
  kind: 'database' | 'knowledge_note' | 'attachment_object' | 'configuration' | string
  size_bytes: number
  sha256: string
}

export interface VerificationCheck {
  name: string
  status: 'pass' | 'fail' | 'skipped'
  detail: string
  checked?: number
}

/** `POST /v1/backups/{id}/verify`; also embedded in a restore preflight. */
export interface VerificationReport {
  ok: boolean
  format_version: number
  schema_version: number | null
  archive_bytes: number
  checks: VerificationCheck[]
  problems: VerificationProblem[]
  warnings: string[]
  manifest: unknown
  verified_files: InventoryEntry[]
}

/** `POST /v1/backups` body. */
export interface CreateBackupRequest {
  label?: string
}

/** `POST /v1/backups`; paths are never part of the response. */
export interface CreateBackupResponse {
  backup_id: string
  created_at: string
  label: string | null
  size_bytes: number
  format_version: number
  schema_version: number
  verified: boolean
  verification_summary: string
  components: BackupComponentSummary[]
  totals: BackupTotals
  counts: BackupCounts
  warnings: string[]
}

export interface BackupArchiveSummary {
  bytes: number
  inventory_files: number
  content_bytes: number
  knowledge_directories: number
}

/** `GET /v1/backups/{id}`; the server omits paths entirely. */
export interface BackupInspectionResponse {
  backup_id: string
  label: string | null
  created_at: string
  created_at_ms: number
  format_version: number
  min_reader_version: number
  orbit_version: string
  orbit_build?: string
  schema_version: number
  complete: boolean
  source: BackupSource
  config?: BackupConfigSnapshot
  exclusions: BackupExclusion[]
  totals: BackupTotals
  counts: BackupCounts
  archive: BackupArchiveSummary
  components: BackupComponentSummary[]
  creation_verification: VerificationSidecar | null
}

export interface RestorePreflightRequest {
  backup_id: string
  mode: RestoreMode
}

export interface RestoreTargetSummary {
  kind: RestoreMode
  /** Generated isolated-restore folder name; absent for replacement. */
  target_name?: string
  /** Component labels the target refers to; paths are never returned. */
  components: string[]
  exists: boolean
  active: boolean
  pending_outbox_rows: number
}

export interface RestoreProblemSummary {
  code: string
  detail: string
}

export interface RestorePlanSummary {
  mode: RestoreMode
  backup_id: string
  label: string | null
  created_at: string
  archive_bytes: number
  backup_source_mode: BackupSourceMode
  backup_schema_version: number
  supported_schema_version: number
  schema_compatible: boolean
  schema_upgrade_required: boolean
  target: RestoreTargetSummary
  replaces: string[]
  preserves: string[]
  downtime_required: boolean
  connected_client_warnings: string[]
  external_dependencies: string[]
  excluded_components: BackupExclusion[]
  totals: BackupTotals
  counts: BackupCounts
  free_space_bytes: number | null
}

export interface RestorePreflightResponse {
  /** False when any blocker is present; a restore must not be started then. */
  admissible: boolean
  plan: RestorePlanSummary
  blockers: RestoreProblemSummary[]
  warnings: RestoreProblemSummary[]
  verification: VerificationReport
}

export interface IsolatedRestoreRequest {
  backup_id: string
  name?: string
}

/** `POST /v1/restores/isolated`; `verified` is not a claim about the data. */
export interface IsolatedRestoreResponse {
  target_name: string
  backup_id: string
  verified: boolean
  activated: boolean
  counts: BackupCounts
  message: string
}

export interface ReplacePlanRequest {
  backup_id: string
}

export interface ReplacePlanStep {
  title: string
  /** Operator-only command; the single place a server path may appear. */
  command?: string
  detail: string
}

/** `POST /v1/restores/replace-plan` (administrator credential only). */
export interface ReplacePlanResponse {
  maintenance_required: boolean
  backup_id: string
  archive_sha256: string
  manifest_sha256: string
  created_at: string
  pending_plan_written: boolean
  steps: ReplacePlanStep[]
  note: string
}
