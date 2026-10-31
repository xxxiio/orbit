import { describe, expect, it } from 'vitest'
import {
  backupErrorMessage,
  backupMoment,
  backupName,
  countsSummary,
  formatBytes,
  formatCount,
  formatTimestamp,
  isBackupInProgress,
  isDownloadTooLarge,
  isRestoreConfirmationValid,
  isWorkspaceBusy,
  lastBackupCopy,
  planFacts,
  preflightBlockedCopy,
  RESTORE_CONFIRMATION,
  SCHEDULE_APPLIED_NOTE,
  SCHEDULE_LABELS,
  scheduleSummary,
  verificationCopy,
  verificationHint,
} from '@/features/backup/backup-model'
import type {
  BackupCounts,
  BackupPolicy,
  BackupSummary,
  RestorePlanSummary,
  RestorePreflightResponse,
  VerificationReport,
} from '@/api/contracts'

const NOW = 1_761_432_000_000

function backup(overrides: Partial<BackupSummary> = {}): BackupSummary {
  return {
    backup_id: 'orbit-20261031-0900-nightly.orbitbak',
    label: 'nightly',
    created_at: '2026-10-31T00:00:00Z',
    created_at_ms: NOW,
    size_bytes: 2 * 1024 * 1024,
    format_version: 1,
    schema_version: 42,
    source_mode: 'server',
    verification: 'verified',
    verification_checked_at_ms: NOW,
    complete: true,
    ...overrides,
  }
}

function counts(overrides: Partial<BackupCounts> = {}): BackupCounts {
  return {
    knowledge_notes: 12,
    knowledge_folders: 3,
    attachment_objects: 4,
    attachment_associations: 5,
    tables: { projects: 2, tasks: 20 },
    ...overrides,
  }
}

function report(overrides: Partial<VerificationReport> = {}): VerificationReport {
  return {
    ok: true,
    format_version: 1,
    schema_version: 42,
    archive_bytes: 2048,
    checks: [
      { name: 'structure', status: 'pass', detail: 'archive structure is intact' },
      { name: 'file_hashes', status: 'pass', detail: 'all hashes match', checked: 8 },
    ],
    problems: [],
    warnings: [],
    manifest: null,
    verified_files: [],
    ...overrides,
  }
}

function plan(overrides: Partial<RestorePlanSummary> = {}): RestorePlanSummary {
  return {
    mode: 'isolated',
    backup_id: 'orbit-20261031-0900-nightly.orbitbak',
    label: 'nightly',
    created_at: '2026-10-31T00:00:00Z',
    archive_bytes: 2048,
    backup_source_mode: 'server',
    backup_schema_version: 42,
    supported_schema_version: 42,
    schema_compatible: true,
    schema_upgrade_required: false,
    target: {
      kind: 'isolated',
      target_name: 'restore-orbit-20261031-0900-nightly',
      components: ['Database snapshot'],
      exists: false,
      active: false,
      pending_outbox_rows: 0,
    },
    replaces: [],
    preserves: [],
    downtime_required: false,
    connected_client_warnings: [],
    external_dependencies: [],
    excluded_components: [],
    totals: {
      database_bytes: 1024,
      knowledge_bytes: 512,
      attachment_bytes: 0,
      config_bytes: 0,
      content_bytes: 1536,
      archive_bytes: 2048,
    },
    counts: counts(),
    free_space_bytes: 10 * 1024 * 1024,
    ...overrides,
  }
}

function preflight(overrides: Partial<RestorePreflightResponse> = {}): RestorePreflightResponse {
  return {
    admissible: true,
    plan: plan(),
    blockers: [],
    warnings: [],
    verification: report(),
    ...overrides,
  }
}

describe('backup verification wording', () => {
  it('never presents unknown verification as verified', () => {
    expect(verificationCopy('verified')).toEqual({
      label: 'Verified at creation',
      tone: 'success',
    })
    expect(verificationCopy('failed').tone).toBe('danger')
    expect(verificationCopy('unknown')).toEqual({ label: 'Not verified', tone: 'warning' })
    expect(verificationHint('unknown')).toMatch(/not verified by this tool/i)
    expect(verificationHint('verified')).toMatch(/when it was created/i)
  })
})

describe('schedule policy summary', () => {
  const policy: BackupPolicy = {
    version: 1,
    schedule: 'daily',
    max_retained: 10,
    min_retention_days: 7,
    max_total_bytes: 2 * 1024 * 1024,
  }

  it('names the schedule and every stored retention limit', () => {
    expect(scheduleSummary(policy)).toBe('Daily · keep 10 · at least 7 days · up to 2.0 MB')
    expect(scheduleSummary({ version: 1, schedule: 'disabled' })).toBe('Disabled')
    expect(scheduleSummary({ version: 1, schedule: 'before_upgrade' })).toBe('Before each upgrade')
  })

  it('says the page does not execute the policy', () => {
    expect(SCHEDULE_APPLIED_NOTE).toMatch(/applies automatic backups and retention/i)
    expect(SCHEDULE_APPLIED_NOTE).toMatch(/this page never runs them/i)
    expect(SCHEDULE_LABELS.before_upgrade).toBe('Before each upgrade')
  })
})

describe('formatting helpers', () => {
  it('formats byte counts without pretending precision it does not have', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(2048)).toBe('2.0 KB')
    expect(formatBytes(2 * 1024 * 1024)).toBe('2.0 MB')
    expect(formatBytes(3 * 1024 * 1024 * 1024)).toBe('3.0 GB')
    expect(formatBytes(-1)).toBe('—')
    expect(formatCount(12345)).toMatch(/12[,.\s]345/)
  })

  it('formats recorded timestamps and falls back when the ISO value is odd', () => {
    expect(backupMoment(backup())).toBeTruthy()
    expect(formatTimestamp('2026-10-31T00:00:00Z')).toBeTruthy()
    expect(formatTimestamp('not-a-date', NOW)).toBe(backupMoment(backup()))
    expect(formatTimestamp('not-a-date')).toBe('not-a-date')
  })

  it('summarises counts and names a backup from its label or file name', () => {
    expect(countsSummary(counts())).toContain('12 notes')
    expect(countsSummary(counts({ knowledge_notes: 1 }))).toContain('1 note ·')
    expect(backupName(backup({ label: 'nightly' }))).toBe('nightly')
    expect(backupName(backup({ label: null }))).toBe('orbit-20261031-0900-nightly.orbitbak')
    expect(backupName(backup({ label: '  ' }))).toBe('orbit-20261031-0900-nightly.orbitbak')
  })

  it('describes the last verified recovery point honestly', () => {
    expect(lastBackupCopy(null)).toMatch(/No verified backup yet/i)
    expect(lastBackupCopy(backup())).toContain('nightly')
    expect(lastBackupCopy(backup())).toContain('2.0 MB')
  })
})

describe('restore confirmation gate', () => {
  it('requires the exact confirmation word', () => {
    expect(isRestoreConfirmationValid(RESTORE_CONFIRMATION)).toBe(true)
    expect(isRestoreConfirmationValid(`  ${RESTORE_CONFIRMATION}  `)).toBe(true)
    expect(isRestoreConfirmationValid('restore')).toBe(false)
    expect(isRestoreConfirmationValid('RESTORE now')).toBe(false)
    expect(isRestoreConfirmationValid('')).toBe(false)
  })
})

describe('restore plan facts', () => {
  it('renders the generated isolated target name and never a server path', () => {
    const facts = planFacts(preflight())
    const rendered = facts.map((fact) => `${fact.label}: ${fact.value}`).join('\n')
    expect(rendered).toContain('Isolated copy (active workspace untouched)')
    expect(rendered).toContain('restore-orbit-20261031-0900-nightly')
    expect(rendered).not.toMatch(/\/home\/|\/srv\/|\.db\b/)
    expect(facts[0]?.label).toBe('Mode')
  })

  it('renders replacement as maintenance with rollback preservation', () => {
    const facts = planFacts(
      preflight({
        plan: plan({
          mode: 'replace',
          downtime_required: true,
          replaces: ['the authoritative database', 'the knowledge vault'],
          preserves: ['the previous database', 'the previous knowledge vault'],
        }),
      }),
    )
    const rendered = facts.map((fact) => `${fact.label}: ${fact.value}`).join('\n')
    expect(rendered).toContain('Replace authority (offline maintenance)')
    expect(rendered).toContain('Required: the server must be stopped')
    expect(rendered).toContain('the previous database')
    expect(facts.find((fact) => fact.label === 'Downtime')?.tone).toBe('warning')
  })

  it('marks a newer backup schema as incompatible instead of promising a restore', () => {
    const facts = planFacts(
      preflight({
        plan: plan({ schema_compatible: false, backup_schema_version: 99, supported_schema_version: 42 }),
      }),
    )
    const schema = facts.find((fact) => fact.label === 'Schema compatibility')
    expect(schema?.tone).toBe('danger')
    expect(schema?.value).toMatch(/upgrade Orbit/i)
    expect(schema?.value).not.toMatch(/downgrade/i)
  })

  it('marks an older schema as an upgrade after restore', () => {
    const facts = planFacts(
      preflight({
        plan: plan({ schema_upgrade_required: true, backup_schema_version: 40, supported_schema_version: 42 }),
      }),
    )
    const schema = facts.find((fact) => fact.label === 'Schema compatibility')
    expect(schema?.tone).toBe('warning')
    expect(schema?.value).toMatch(/migrated/i)
  })

  it('flags insufficient free space as a danger', () => {
    const facts = planFacts(preflight({ plan: plan({ free_space_bytes: 100 }) }))
    const free = facts.find((fact) => fact.label === 'Free space')
    expect(free?.tone).toBe('danger')
    expect(free?.value).toContain('100 B')
  })

  it('refuses to hide blockers behind a green badge', () => {
    expect(preflightBlockedCopy(preflight())).toBeNull()
    const blocked = preflight({
      admissible: false,
      blockers: [{ code: 'incompatible_schema', detail: 'the backup is newer than this build' }],
    })
    expect(preflightBlockedCopy(blocked)).toContain('the backup is newer than this build')
  })
})

describe('error copy', () => {
  it('names an unavailable service honestly', () => {
    expect(backupErrorMessage({ status: 503, message: 'backup service unavailable: not implemented' })).toMatch(
      /may not include a working backup service yet/i,
    )
    expect(backupErrorMessage(new Error('offline'))).toBe('offline')
    expect(backupErrorMessage(null)).toBe('The backup request could not be completed.')
  })

  it('distinguishes a concurrent create, a busy workspace and a too-large download', () => {
    expect(isBackupInProgress({ status: 409, message: 'a backup is already in progress; wait' })).toBe(true)
    expect(isBackupInProgress({ status: 409, message: 'conflict: archive exists' })).toBe(false)
    expect(isWorkspaceBusy({ status: 409, message: 'workspace is busy, retry the backup' })).toBe(true)
    expect(isDownloadTooLarge({ status: 413 })).toBe(true)
    expect(isDownloadTooLarge({ status: 500 })).toBe(false)
  })
})
