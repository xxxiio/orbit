import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { ToastProvider } from '@/components/ui/toast'
import { BackupSettingsPage } from '@/features/backup/backup-settings-page'
import { RestorePanel } from '@/features/backup/restore-panel'
import { jsonResponse, testQueryClient } from '../api/test-utils'
import type {
  BackupOverviewResponse,
  BackupSummary,
  CreateBackupResponse,
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

function overview(overrides: Partial<BackupOverviewResponse> = {}): BackupOverviewResponse {
  return {
    destination: { label: 'Server backup folder', writable: true },
    schedule: { version: 1, schedule: 'daily', max_retained: 10 },
    last_backup: null,
    backups: [],
    skipped: [],
    ...overrides,
  }
}

function createResponse(overrides: Partial<CreateBackupResponse> = {}): CreateBackupResponse {
  return {
    backup_id: 'orbit-20261031-0900-nightly.orbitbak',
    created_at: '2026-10-31T00:00:00Z',
    label: 'nightly',
    size_bytes: 2 * 1024 * 1024,
    format_version: 1,
    schema_version: 42,
    verified: true,
    verification_summary: 'verified: 8 file(s), schema 42',
    components: [
      { component: 'database', label: 'Database', files: 1, bytes: 1024 },
      { component: 'knowledge', label: 'Knowledge notes', files: 7, bytes: 1024 },
    ],
    totals: {
      database_bytes: 1024,
      knowledge_bytes: 1024,
      attachment_bytes: 0,
      config_bytes: 0,
      content_bytes: 2048,
      archive_bytes: 2048,
    },
    counts: {
      knowledge_notes: 7,
      knowledge_folders: 2,
      attachment_objects: 0,
      attachment_associations: 0,
      tables: { tasks: 10 },
    },
    warnings: [],
    ...overrides,
  }
}

function report(overrides: Partial<VerificationReport> = {}): VerificationReport {
  return {
    ok: true,
    format_version: 1,
    schema_version: 42,
    archive_bytes: 2048,
    checks: [],
    problems: [],
    warnings: [],
    manifest: null,
    verified_files: [],
    ...overrides,
  }
}

function preflight(overrides: Partial<RestorePreflightResponse> = {}): RestorePreflightResponse {
  return {
    admissible: true,
    plan: {
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
        components: ['Database snapshot', 'Knowledge vault', 'Attachments'],
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
        knowledge_bytes: 1024,
        attachment_bytes: 0,
        config_bytes: 0,
        content_bytes: 2048,
        archive_bytes: 2048,
      },
      counts: {
        knowledge_notes: 7,
        knowledge_folders: 2,
        attachment_objects: 0,
        attachment_associations: 0,
        tables: { tasks: 10 },
      },
      free_space_bytes: 10 * 1024 * 1024,
    },
    blockers: [],
    warnings: [],
    verification: report(),
    ...overrides,
  }
}

function stubFetch(handler: (url: string, init?: RequestInit) => Response | undefined) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      return handler(url, init) ?? jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )
}

function renderPage() {
  return render(
    <MemoryRouter>
      <QueryClientProvider client={testQueryClient()}>
        <ToastProvider>
          <BackupSettingsPage />
        </ToastProvider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('backup settings page', () => {
  it('shows the last verified point, destination and history with honest badges', async () => {
    stubFetch((url, init) => {
      if (url.endsWith('/v1/backups') && (init?.method ?? 'GET') === 'GET') {
        return jsonResponse(
          overview({
            last_backup: backup(),
            backups: [
              backup(),
              backup({
                backup_id: 'orbit-20261030-copied.orbitbak',
                label: 'copied',
                verification: 'unknown',
              }),
              backup({
                backup_id: 'orbit-20261029-broken.orbitbak',
                label: 'broken',
                verification: 'failed',
              }),
            ],
          }),
        )
      }
      return undefined
    })
    renderPage()

    expect(await screen.findByRole('heading', { name: 'Backup & Restore' })).toBeInTheDocument()
    expect(
      screen.getByText(/A copy on this disk does not survive disk failure/),
    ).toBeInTheDocument()
    expect((await screen.findAllByText('nightly')).length).toBeGreaterThan(0)
    expect(screen.getByText('Writable')).toBeInTheDocument()
    expect(screen.getAllByText('Verified at creation').length).toBeGreaterThan(0)

    const table = await screen.findByRole('table')
    expect(within(table).getByText('Not verified')).toBeInTheDocument()
    expect(within(table).getByText('Verification failed')).toBeInTheDocument()
    expect(within(table).getByText('orbit-20261030-copied.orbitbak')).toBeInTheDocument()
  })

  it('says no verified backup exists instead of implying one does', async () => {
    stubFetch(() => jsonResponse(overview()))
    renderPage()
    expect(await screen.findByText('No verified backup yet')).toBeInTheDocument()
    expect(screen.getByText('No backups yet')).toBeInTheDocument()
    expect(
      screen.getAllByText(/Nothing is restored implicitly|Restore never runs against an unknown file/)
        .length,
    ).toBeGreaterThan(0)
  })

  it('creates a backup and reports verification passed without claiming a restore', async () => {
    const user = userEvent.setup()
    let created = false
    stubFetch((url, init) => {
      if (url.endsWith('/v1/backups') && init?.method === 'POST') {
        created = true
        return jsonResponse(createResponse(), 200)
      }
      if (url.endsWith('/v1/backups')) {
        return jsonResponse(overview({ backups: created ? [backup()] : [] }))
      }
      return undefined
    })
    renderPage()
    await screen.findByText('No backups yet')

    await user.type(screen.getByLabelText(/Label/), 'nightly')
    await user.click(screen.getByRole('button', { name: 'Create backup' }))

    const result = await screen.findByText('Verification passed')
    expect(result).toBeInTheDocument()
    expect(screen.getByText('verified: 8 file(s), schema 42')).toBeInTheDocument()
    expect(screen.queryByText(/restored successfully/i)).not.toBeInTheDocument()
  })

  it('names an unavailable backup service instead of showing an empty history', async () => {
    stubFetch(() => jsonResponse({ error: 'backup service unavailable: not implemented' }, 503))
    renderPage()
    expect(await screen.findByText(/Backups could not be read/)).toBeInTheDocument()
    expect(
      screen.getByText(/may not include a working backup service yet/),
    ).toBeInTheDocument()
  })

  it('shows skipped archive files and names why they are not recovery points', async () => {
    stubFetch(() =>
      jsonResponse(
        overview({
          skipped: [{ file_name: 'orbit-broken.orbitbak', reason: 'manifest is unreadable' }],
        }),
      ),
    )
    renderPage()
    const skipped = await screen.findByRole('list', { name: 'Skipped backup files' })
    expect(within(skipped).getByText('orbit-broken.orbitbak')).toBeInTheDocument()
    expect(within(skipped).getByText('manifest is unreadable')).toBeInTheDocument()
  })
})

describe('restore panel', () => {
  function renderRestore(backups: BackupSummary[]) {
    return render(
      <QueryClientProvider client={testQueryClient()}>
        <RestorePanel backups={backups} />
      </QueryClientProvider>,
    )
  }

  it('gates an isolated restore behind a preflight and the typed confirmation word', async () => {
    const user = userEvent.setup()
    stubFetch((url, init) => {
      if (url.endsWith('/v1/restores/preflight') && init?.method === 'POST') {
        return jsonResponse(preflight())
      }
      return undefined
    })
    renderRestore([backup()])

    await user.click(screen.getByRole('button', { name: 'Check isolated restore plan' }))
    expect(await screen.findByText('Preflight allows this restore')).toBeInTheDocument()
    expect(screen.getByText('restore-orbit-20261031-0900-nightly')).toBeInTheDocument()

    const restore = screen.getByRole('button', { name: 'Create isolated copy' })
    expect(restore).toBeDisabled()

    await user.type(screen.getByLabelText(/Type RESTORE to confirm/), 'restore')
    expect(restore).toBeDisabled()

    await user.clear(screen.getByLabelText(/Type RESTORE to confirm/))
    await user.type(screen.getByLabelText(/Type RESTORE to confirm/), 'RESTORE')
    await waitFor(() => expect(restore).toBeEnabled())
  })

  it('never offers a restore button when the preflight is blocked', async () => {
    const user = userEvent.setup()
    stubFetch((url, init) => {
      if (url.endsWith('/v1/restores/preflight') && init?.method === 'POST') {
        return jsonResponse(
          preflight({
            admissible: false,
            blockers: [{ code: 'active_workspace', detail: 'the target is the active workspace' }],
          }),
        )
      }
      return undefined
    })
    renderRestore([backup()])

    await user.click(screen.getByRole('button', { name: 'Check isolated restore plan' }))
    expect(await screen.findByText('Preflight blocks this restore')).toBeInTheDocument()
    expect(screen.getAllByText(/the target is the active workspace/).length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'Create isolated copy' })).toBeDisabled()
  })

  it('requires an administrator key and shows operator commands without replacing the workspace', async () => {
    const user = userEvent.setup()
    stubFetch((url, init) => {
      if (url.endsWith('/v1/restores/preflight') && init?.method === 'POST') {
        return jsonResponse(
          preflight({
            plan: {
              ...preflight().plan,
              mode: 'replace',
              downtime_required: true,
              replaces: ['the authoritative database'],
              preserves: ['the previous database'],
              target: {
                kind: 'replace',
                components: ['Authoritative database', 'Knowledge vault', 'Attachment store'],
                exists: true,
                active: true,
                pending_outbox_rows: 0,
              },
            },
          }),
        )
      }
      if (url.endsWith('/v1/restores/replace-plan') && init?.method === 'POST') {
        return jsonResponse({
          maintenance_required: true,
          backup_id: 'orbit-20261031-0900-nightly.orbitbak',
          archive_sha256: 'a'.repeat(64),
          manifest_sha256: 'b'.repeat(64),
          created_at: '2026-10-31T00:00:00Z',
          pending_plan_written: true,
          steps: [
            { title: 'Stop the Orbit server', detail: 'Stop it first.' },
            {
              title: 'Replace the workspace offline',
              command:
                "orbit backup restore '/srv/backups/one.orbitbak' --replace --confirm '/srv/orbit.db'",
              detail: 'Run with the server stopped.',
            },
          ],
          note: 'Plan only. The live workspace was NOT replaced from the browser.',
        })
      }
      return undefined
    })
    renderRestore([backup()])

    await user.click(screen.getByRole('radio', { name: /Replace authority/ }))
    await user.click(screen.getByRole('button', { name: 'Check replacement plan' }))
    expect(await screen.findByText('Required: the server must be stopped')).toBeInTheDocument()

    const prepare = screen.getByRole('button', { name: 'Prepare replacement plan' })
    expect(prepare).toBeDisabled()
    await user.type(screen.getByLabelText('Administrator key'), 'admin-secret')
    await waitFor(() => expect(prepare).toBeEnabled())
    await user.click(prepare)

    expect(await screen.findByText('Offline replacement plan')).toBeInTheDocument()
    expect(screen.getByText(/was NOT replaced from the browser/)).toBeInTheDocument()
    expect(
      screen.getByText(/orbit backup restore .* --replace --confirm .*/),
    ).toBeInTheDocument()
    expect(screen.getByText('Operator command')).toBeInTheDocument()
  })
})
