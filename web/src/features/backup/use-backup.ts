/**
 * React Query bindings for the backup settings surface.
 *
 * Reads: one overview query (destination, schedule, history, last verified
 * backup) and one manifest inspection query per open drawer. Writes: create,
 * verify, preflight, isolated restore, replacement plan and schedule update.
 *
 * Rules kept from the rest of the app: nothing is shown as stored before the
 * server answered, and a backup/restore command invalidates the overview so
 * the history reflects what the server actually wrote.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  createBackup,
  fetchBackupInspection,
  fetchBackups,
  preflightRestore,
  requestReplacePlan,
  restoreIsolated,
  updateBackupSchedule,
  verifyBackup,
} from '@/api/endpoints/backup'
import type {
  BackupPolicyUpdate,
  CreateBackupRequest,
  IsolatedRestoreRequest,
  RestoreMode,
} from '@/api/contracts'

export const backupKeys = {
  overview: () => ['backups'] as const,
  inspection: (backupId: string) => ['backups', backupId, 'inspection'] as const,
}

export function useBackupOverview() {
  return useQuery({
    queryKey: backupKeys.overview(),
    queryFn: ({ signal }) => fetchBackups(signal),
  })
}

/** Manifest-only read for the inspect drawer; disabled until a row is chosen. */
export function useBackupInspection(backupId: string | null) {
  return useQuery({
    queryKey: backupKeys.inspection(backupId ?? 'none'),
    queryFn: ({ signal }) => fetchBackupInspection(backupId as string, signal),
    enabled: backupId !== null,
  })
}

export function useCreateBackup() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (body: CreateBackupRequest) => createBackup(body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: backupKeys.overview() })
    },
    onError: () => {
      // A busy/existing archive answer may have changed the history anyway.
      void queryClient.invalidateQueries({ queryKey: backupKeys.overview() })
    },
  })
}

export function useVerifyBackup() {
  return useMutation({
    mutationFn: (backupId: string) => verifyBackup(backupId),
  })
}

export function useRestorePreflight() {
  return useMutation({
    mutationFn: ({ backupId, mode }: { backupId: string; mode: RestoreMode }) =>
      preflightRestore({ backup_id: backupId, mode }),
  })
}

export function useIsolatedRestore() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (body: IsolatedRestoreRequest) => restoreIsolated(body),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: backupKeys.overview() })
    },
  })
}

export function useReplacePlan() {
  return useMutation({
    mutationFn: ({
      backupId,
      administratorKey,
    }: {
      backupId: string
      administratorKey: string
    }) => requestReplacePlan({ backup_id: backupId }, administratorKey),
  })
}

export function useUpdateSchedule() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (body: BackupPolicyUpdate) => updateBackupSchedule(body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: backupKeys.overview() })
    },
  })
}
