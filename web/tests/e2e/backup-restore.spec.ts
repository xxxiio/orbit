/**
 * M34 I3 — Backup & Restore browser acceptance.
 *
 * Real `orbit-server`, real `orbit-backup` service, real Chromium: each test
 * creates an archive through the product routes and then exercises the settings
 * page. Nothing is mocked, and no test touches a developer's workspace (each
 * server owns a throwaway database and its own backup/restore folders).
 *
 * Covered:
 *  - create → verification passed state, archive listed with its sidecar state;
 *  - inspect drawer (manifest, components, creation verification);
 *  - row Verify → live verification report (verification is not restoration);
 *  - download: the browser keeps the page and the server answers an attachment;
 *  - isolated restore: preflight, generated target name, typed confirmation,
 *    created copy verified, active workspace untouched;
 *  - replacement: administrator credential, offline plan with operator commands,
 *    live database unchanged;
 *  - corrupted archive: verification failure is shown, no restore offered.
 */
import { readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'
import type { Page } from '@playwright/test'

const BACKUP_DIR = (server: E2EServer) => join(server.dataDir, 'data', 'backups')
const RESTORE_DIR = (server: E2EServer) => join(server.dataDir, 'data', 'restores')
const PENDING_PLAN = (server: E2EServer) => join(server.dataDir, 'data', 'server.db.restore-pending.json')

/** Create an archive through the product API using the browser's own cookie. */
async function createBackup(
  page: Page,
  server: E2EServer,
  label: string,
): Promise<{ backup_id: string }> {
  const response = await page.request.post(`${server.base}/v1/backups`, {
    data: { label },
    headers: { 'x-orbit-csrf': '1' },
  })
  expect(response.status(), await response.text()).toBe(200)
  return (await response.json()) as { backup_id: string }
}

async function readOverview(page: Page, server: E2EServer) {
  const response = await page.request.get(`${server.base}/v1/backups`)
  expect(response.status()).toBe(200)
  return (await response.json()) as {
    destination: { label: string; writable: boolean }
    last_backup: { backup_id: string; verification: string } | null
    backups: { backup_id: string; verification: string }[]
  }
}

async function archivePath(server: E2EServer): Promise<string> {
  const entries = await readdir(BACKUP_DIR(server))
  const file = entries.find((name) => name.endsWith('.orbitbak'))
  expect(file, 'create must publish an archive in the destination').toBeTruthy()
  return join(BACKUP_DIR(server), file as string)
}

test('creates, inspects, verifies and downloads a backup', async ({ page, server }) => {
  await goto(page, server, '/settings/backup')
  await expect(page.getByRole('heading', { name: 'Backup & Restore' })).toBeVisible()
  await expect(
    page.getByText(/A copy on this disk does not survive disk failure/),
  ).toBeVisible()
  // Honest empty state before anything exists.
  await expect(page.getByText('No verified backup yet')).toBeVisible()
  await expect(page.getByText('No backups yet')).toBeVisible()

  await page.getByLabel(/Label/).fill('acceptance')
  await page.getByRole('button', { name: 'Create backup' }).click()

  // The success panel only appears after the server finished create-and-verify.
  await expect(page.getByText('Verification passed')).toBeVisible({ timeout: 60_000 })
  await expect(page.getByText(/verified: \d+ file\(s\)/)).toBeVisible()

  const overview = await readOverview(page, server)
  expect(overview.backups).toHaveLength(1)
  expect(overview.backups[0]?.verification).toBe('verified')
  expect(overview.last_backup?.backup_id).toBe(overview.backups[0]?.backup_id)
  const backupId = overview.backups[0]!.backup_id
  expect(overview.destination.writable).toBe(true)

  const row = page.locator(`tr[data-backup-id="${backupId}"]`)
  await expect(row).toHaveCount(1)
  await expect(row.getByText('Verified at creation')).toBeVisible()

  // Inspect: manifest detail and creation verification sidecar.
  await row.getByRole('button', { name: 'Inspect' }).click()
  const drawer = page.getByRole('dialog')
  await expect(drawer.getByText('Recovery point')).toBeVisible()
  await expect(drawer.getByText(backupId).first()).toBeVisible()
  await expect(drawer.getByText('Archive contents')).toBeVisible()
  await expect(drawer.getByText('Passed at creation')).toBeVisible()
  await expect(drawer.getByText('Knowledge notes')).toBeVisible()

  // Verify now: a live report appears in the same drawer, with named checks.
  await drawer.getByRole('button', { name: 'Verify archive' }).click()
  await expect(drawer.getByText('Verification passed')).toBeVisible({ timeout: 60_000 })
  await expect(drawer.getByText('file_hashes')).toBeVisible()
  await expect(drawer.getByText(/Verification is not restoration/)).toBeVisible()
  await drawer.getByRole('button', { name: 'Close backup details' }).click()
  await expect(page.getByRole('dialog')).toHaveCount(0)

  // Download: the server answers an attachment and the browser receives it.
  const downloadPromise = page.waitForEvent('download')
  await page
    .locator(`tr[data-backup-id="${backupId}"]`)
    .getByRole('button', { name: 'Download' })
    .click()
  const download = await downloadPromise
  expect(download.suggestedFilename()).toBe(backupId)

  const archive = await page.request.get(`${server.base}/v1/backups/${backupId}/download`)
  expect(archive.status()).toBe(200)
  expect(archive.headers()['content-type']).toContain('application/octet-stream')
  expect(archive.headers()['content-disposition']).toContain('attachment')
  const bytes = await archive.body()
  expect(bytes.length).toBeGreaterThan(0)
})

test('runs an isolated restore drill behind a typed confirmation', async ({ page, server }) => {
  await goto(page, server, '/settings/backup')
  await expect(page.getByRole('heading', { name: 'Backup & Restore' })).toBeVisible()
  const { backup_id: backupId } = await createBackup(page, server, 'drill')
  await page.reload()
  await expect(page.locator(`tr[data-backup-id="${backupId}"]`)).toHaveCount(1)

  const restore = page.locator('[data-restore-panel]')
  await restore.scrollIntoViewIfNeeded()
  await restore.getByRole('button', { name: 'Check isolated restore plan' }).click()

  await expect(restore.getByText('Preflight allows this restore')).toBeVisible({ timeout: 60_000 })
  const targetName = await restore
    .getByText(/^restore-/)
    .first()
    .textContent()
  expect(targetName, 'the preflight must name the generated restore folder').toBeTruthy()

  const start = restore.getByRole('button', { name: 'Create isolated copy' })
  await expect(start).toBeDisabled()
  await restore.getByLabel(/Type RESTORE to confirm/).fill('restore')
  await expect(start).toBeDisabled()
  await restore.getByLabel(/Type RESTORE to confirm/).fill('RESTORE')
  await expect(start).toBeEnabled()
  await start.click()

  await expect(restore.getByText('Isolated copy verified')).toBeVisible({ timeout: 60_000 })
  await expect(restore.getByText(/active workspace was not modified/)).toBeVisible()

  // The copy is a real directory under the server's restore root.
  const restored = await stat(join(RESTORE_DIR(server), targetName!.trim()))
  expect(restored.isDirectory()).toBe(true)

  // The active workspace is still serving its own data.
  const snapshot = await page.request.get(`${server.base}/v1/authoritative/snapshot`)
  expect(snapshot.status()).toBe(200)
})

test('prepares an offline replacement plan only, never replacing from the browser', async ({
  page,
  server,
}) => {
  await goto(page, server, '/settings/backup')
  await expect(page.getByRole('heading', { name: 'Backup & Restore' })).toBeVisible()
  const { backup_id: backupId } = await createBackup(page, server, 'replace-plan')
  await page.reload()
  await expect(page.locator(`tr[data-backup-id="${backupId}"]`)).toHaveCount(1)

  const restore = page.locator('[data-restore-panel]')
  await restore.getByRole('radio', { name: /Replace authority/ }).check()
  await restore.getByRole('button', { name: 'Check replacement plan' }).click()
  await expect(restore.getByText(/Preflight (allows|blocks) this restore/)).toBeVisible({
    timeout: 60_000,
  })
  await expect(restore.getByText('Required: the server must be stopped')).toBeVisible()

  const prepare = restore.getByRole('button', { name: 'Prepare replacement plan' })
  await expect(prepare).toBeDisabled()
  await restore.getByLabel('Administrator key').fill(server.adminKey)
  await expect(prepare).toBeEnabled()
  await prepare.click()

  await expect(restore.getByText('Offline replacement plan')).toBeVisible({ timeout: 30_000 })
  await expect(
    restore.getByText(/The live workspace was NOT replaced from the browser/),
  ).toBeVisible()
  const operatorCommand = restore.getByText(/orbit backup restore .* --replace --confirm/)
  await expect(operatorCommand).toBeVisible()
  expect(await operatorCommand.textContent()).toContain('--confirm')

  // The plan was written beside the database, and the running server was not replaced.
  const pending = await stat(PENDING_PLAN(server))
  expect(pending.isFile()).toBe(true)
  const snapshot = await page.request.get(`${server.base}/v1/authoritative/snapshot`)
  expect(snapshot.status()).toBe(200)
})

test('shows verification failure for a corrupted archive and offers no restore', async ({
  page,
  server,
}) => {
  await goto(page, server, '/settings/backup')
  await expect(page.getByRole('heading', { name: 'Backup & Restore' })).toBeVisible()
  const { backup_id: backupId } = await createBackup(page, server, 'corrupt-me')

  // Flip one content byte in the middle of the archive: the manifest still
  // reads, so the entry stays listed, but the inventory hash cannot match.
  const path = await archivePath(server)
  const bytes = await readFile(path)
  const offset = Math.floor(bytes.length / 2)
  bytes[offset] = bytes[offset]! ^ 0xff
  await writeFile(path, bytes)

  await page.reload()
  const row = page.locator(`tr[data-backup-id="${backupId}"]`)
  await expect(row).toHaveCount(1)
  await row.getByRole('button', { name: 'Verify' }).click()

  const toast = page.locator('ol[aria-label="Notifications"]')
  await expect(toast.getByText('Verification failed')).toBeVisible({ timeout: 60_000 })

  // Opening the row shows the failing report recorded by the row action.
  await row.getByRole('button', { name: 'Inspect' }).click()
  const drawer = page.getByRole('dialog')
  await expect(drawer.getByText('Verification failed')).toBeVisible({ timeout: 30_000 })
  await expect(drawer.getByText(/Verification is not restoration/)).toBeVisible()
  await drawer.getByRole('button', { name: 'Close backup details' }).click()

  // A restore preflight must refuse rather than offer a restore button.
  const restore = page.locator('[data-restore-panel]')
  await restore.getByRole('button', { name: 'Check isolated restore plan' }).click()
  await expect(restore.getByText('Preflight blocks this restore')).toBeVisible({ timeout: 60_000 })
  await expect(restore.getByRole('button', { name: 'Create isolated copy' })).toBeDisabled()
})
