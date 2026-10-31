/**
 * Workspace backup and restore endpoints.
 *
 * The browser never names a server path: `backupId` is an archive file name and
 * every other input is a label, a schedule value or a restore folder name. The
 * server resolves the id inside its configured backup directory.
 *
 * The replacement plan is the only administrator route. Its operator commands
 * are the only response that may contain server paths; they are shown only in
 * the offline maintenance panel, never in a product summary.
 */
import { api, apiRequest, apiUrl } from '../client'
import type {
  BackupInspectionResponse,
  BackupOverviewResponse,
  BackupPolicy,
  BackupPolicyUpdate,
  CreateBackupRequest,
  CreateBackupResponse,
  IsolatedRestoreRequest,
  IsolatedRestoreResponse,
  ReplacePlanRequest,
  ReplacePlanResponse,
  RestorePreflightRequest,
  RestorePreflightResponse,
  VerificationReport,
} from '../contracts'

const BASE = '/backups'
const RESTORES = '/restores'

/** Overview: destination, schedule, last verified backup and full history. */
export function fetchBackups(signal?: AbortSignal): Promise<BackupOverviewResponse> {
  return api.get<BackupOverviewResponse>(BASE, { signal })
}

/** Create one backup synchronously; the response only lands after verification. */
export function createBackup(body: CreateBackupRequest): Promise<CreateBackupResponse> {
  return api.post<CreateBackupResponse>(BASE, body)
}

/** Manifest-only inspection. Use `verifyBackup` for content verification. */
export function fetchBackupInspection(
  backupId: string,
  signal?: AbortSignal,
): Promise<BackupInspectionResponse> {
  return api.get<BackupInspectionResponse>(`${BASE}/${encodeURIComponent(backupId)}`, { signal })
}

/** Re-verify the archive bytes on the server; never implies a restore. */
export function verifyBackup(backupId: string): Promise<VerificationReport> {
  return api.post<VerificationReport>(`${BASE}/${encodeURIComponent(backupId)}/verify`)
}

/**
 * Same-origin download URL. The archive response carries a
 * `Content-Disposition: attachment` header, so a plain anchor performs the
 * download with the session cookie and no JavaScript buffering.
 */
export function backupDownloadUrl(backupId: string): string {
  return apiUrl(`${BASE}/${encodeURIComponent(backupId)}/download`)
}

/** Check compatibility and build the exact restore plan without mutating anything. */
export function preflightRestore(body: RestorePreflightRequest): Promise<RestorePreflightResponse> {
  return api.post<RestorePreflightResponse>(`${RESTORES}/preflight`, body)
}

/** Restore into a new workspace copy; the active workspace is never modified. */
export function restoreIsolated(body: IsolatedRestoreRequest): Promise<IsolatedRestoreResponse> {
  return api.post<IsolatedRestoreResponse>(`${RESTORES}/isolated`, body)
}

/**
 * Write the offline replacement plan. Requires the administrator credential,
 * which is supplied per request and never stored.
 */
export function requestReplacePlan(
  body: ReplacePlanRequest,
  administratorKey: string,
): Promise<ReplacePlanResponse> {
  return apiRequest<ReplacePlanResponse>(`${RESTORES}/replace-plan`, {
    method: 'POST',
    body,
    headers: { Authorization: `Bearer ${administratorKey}` },
  })
}

export function fetchBackupSchedule(signal?: AbortSignal): Promise<BackupPolicy> {
  return api.get<BackupPolicy>(`${BASE}/schedule`, { signal })
}

/** Persist the policy; scheduling/retention execution belongs to a later iteration. */
export function updateBackupSchedule(body: BackupPolicyUpdate): Promise<BackupPolicy> {
  return apiRequest<BackupPolicy>(`${BASE}/schedule`, { method: 'PUT', body })
}
