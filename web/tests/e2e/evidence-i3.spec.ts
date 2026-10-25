/**
 * I3 evidence capture (opt-in).
 *
 *   ORBIT_I3_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
 *     tests/e2e/evidence-i3.spec.ts --project=chromium
 *
 * Every frame comes from the deterministic fixture seeded through the public
 * API (no mocked data, no preview server). Files land in `web/evidence/i3/`.
 * The spec skips itself in a normal run so a test pass never rewrites committed
 * evidence.
 */
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'

const OUT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', 'evidence', 'i3')
const ENABLED = process.env.ORBIT_I3_EVIDENCE === '1'

test.skip(!ENABLED, 'set ORBIT_I3_EVIDENCE=1 to capture committed evidence')
test.setTimeout(240_000)

async function settle(page: import('@playwright/test').Page): Promise<void> {
  await page.evaluate(() => document.fonts.ready)
  await page.waitForTimeout(450)
}

async function shot(page: import('@playwright/test').Page, name: string): Promise<void> {
  await settle(page)
  await page.screenshot({ path: path.join(OUT, `${name}.png`) })
}

function bearer(server: E2EServer) {
  return {
    authorization: `Bearer ${server.token}`,
    'x-orbit-device-id': server.deviceId,
    'content-type': 'application/json',
  }
}

interface Manifest {
  id: string
  version: number
  name: string
  work_items: Array<{
    id: string
    fields: Array<{ id: string; kind: Record<string, unknown>; required: boolean; default: unknown }>
  }>
  views: Array<{
    id: string
    title: string
    description: string
    layout: { type: string }
    components: Array<Record<string, unknown>>
  }>
}

async function readManifest(
  page: import('@playwright/test').Page,
  server: E2EServer,
): Promise<Manifest> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/blueprints/show?id=development&version=2`,
    { headers: bearer(server) },
  )
  if (response.status() !== 200) throw new Error(`manifest read failed: ${response.status()}`)
  return (await response.json()) as Manifest
}

async function publish(
  page: import('@playwright/test').Page,
  server: E2EServer,
  definition: Manifest,
): Promise<void> {
  const response = await page.request.post(`${server.base}/v1/authoritative/blueprints`, {
    headers: bearer(server),
    data: { definition },
  })
  if (response.status() !== 200) throw new Error(`publish failed: ${response.status()} ${await response.text()}`)
}

test.describe('I3 evidence', () => {
  test('work items, sheet, create dialog and upgrade states', async ({ page, workspace }) => {
    const { server } = workspace
    await mkdir(OUT, { recursive: true })
    await page.addInitScript(([key, value]) => {
      window.localStorage.setItem(key, value)
    }, ['orbit.theme', 'light'])

    const desktop = { width: 1440, height: 900 }
    const phone = { width: 390, height: 844 }

    // Board with seeded items in declared states.
    await page.setViewportSize(desktop)
    await goto(page, server, '/projects/p-orbit-development/board')
    await page.getByRole('group', { name: 'Work item board' }).waitFor()
    await page.locator('[data-view-component="work_board"]').scrollIntoViewIfNeeded()
    await shot(page, 'work-items-board-light-1440x900')

    // The sheet: typed properties, declared moves and the honest action panel.
    await page
      .locator('[data-work-item-key="wi-orbit-feature-ready"]')
      .getByRole('button')
      .first()
      .click()
    await page.getByRole('region', { name: 'Actions' }).waitFor()
    await shot(page, 'work-item-sheet-light-1440x900')

    await page.getByRole('region', { name: 'Actions' }).scrollIntoViewIfNeeded()
    await shot(page, 'work-item-actions-light-1440x900')

    // A server refusal, from a declared move whose guard is not met.
    await page.getByRole('button', { name: 'Close work item details' }).click()
    const before = new Set(
      (
        (await (
          await page.request.get(
            `${server.base}/v1/authoritative/blueprints/items?project_id=p-guarded`,
          )
        ).json()) as Array<{ id: string }>
      ).map((item) => item.id),
    )
    await goto(page, server, '/projects/p-guarded/overview')
    await page.getByRole('button', { name: 'New work item' }).first().click()
    await page.getByRole('dialog').getByRole('button', { name: 'Create work item' }).click()
    const items = (await (
      await page.request.get(`${server.base}/v1/authoritative/blueprints/items?project_id=p-guarded`)
    ).json()) as Array<{ id: string }>
    const created = items.find((item) => !before.has(item.id))
    if (!created) throw new Error('the guarded ticket was not created')
    await page.locator(`[data-work-item-key="${created.id}"]`).getByRole('button').first().click()
    await page.getByRole('button', { name: 'Move work item to Approved' }).click()
    await page.getByRole('alert').filter({ hasText: /server refused/i }).first().waitFor()
    // Let the create toast clear so it cannot cover the refusal in the frame.
    await page.waitForTimeout(6500)
    await shot(page, 'work-item-refusal-light-1440x900')
    await page.getByRole('button', { name: 'Close work item details' }).click()

    // Create dialog with typed controls (the Development view pins one kind).
    await goto(page, server, '/projects/p-orbit-development/board')
    await page.getByRole('button', { name: 'New work item' }).first().click()
    await page.getByRole('dialog').waitFor()
    await shot(page, 'work-item-create-light-1440x900')
    await page.keyboard.press('Escape')

    // Phone width: the same board and sheet, not a separate simplified app.
    await page.setViewportSize(phone)
    await goto(page, server, '/projects/p-orbit-development/board')
    await page.getByRole('group', { name: 'Work item board' }).waitFor()
    await page.locator('[data-view-component="work_board"]').scrollIntoViewIfNeeded()
    await shot(page, 'work-items-board-light-390x844')
    await page
      .locator('[data-work-item-key="wi-orbit-feature-ready"]')
      .getByRole('button')
      .first()
      .click()
    await page.getByRole('region', { name: 'Actions' }).waitFor()
    await shot(page, 'work-item-sheet-light-390x844')
    await page.getByRole('button', { name: 'Close work item details' }).click()

    // An applicable upgrade: publish v3, review it, apply it, show the receipt.
    await page.setViewportSize(desktop)
    const base = await readManifest(page, server)
    const target = structuredClone(base)
    target.version = 3
    target.work_items[0].fields.push({
      id: 'notes',
      kind: { type: 'text' },
      required: false,
      default: null,
    })
    target.views.push({
      id: 'release',
      title: 'Release notes',
      description: 'Work items planned for the next release.',
      layout: { type: 'vertical' },
      components: [
        {
          id: 'release_board',
          type: 'kanban',
          source: 'work_items',
          group_by: 'state',
          limit: 100,
          title: 'Release board',
        },
      ],
    })
    await publish(page, server, target)
    await goto(page, server, '/projects/p-orbit-development/settings')
    await page.getByText(/A newer Development version is published/).waitFor()
    await page.getByText('Release notes').first().waitFor()
    await page.locator('[data-upgrade-panel]').scrollIntoViewIfNeeded()
    await shot(page, 'upgrade-preview-light-1440x900')

    await page.getByRole('button', { name: 'Upgrade workflow…' }).click()
    await page.getByRole('checkbox').check()
    await page.getByRole('button', { name: 'Apply upgrade' }).click()
    await page.locator('[data-upgrade-receipt]').waitFor()
    await page.waitForTimeout(6500)
    await shot(page, 'upgrade-receipt-light-1440x900')

    // A blocked target: v4 removes a field every seeded item still stores.
    const blocked = structuredClone(target)
    blocked.version = 4
    blocked.work_items[0].fields = blocked.work_items[0].fields.filter(
      (field) => field.id !== 'priority',
    )
    await publish(page, server, blocked)
    await goto(page, server, '/projects/p-orbit-development/settings')
    await page.locator('[data-upgrade-notice="field_removed_with_values"]').waitFor()
    await page.locator('[data-upgrade-panel]').scrollIntoViewIfNeeded()
    await shot(page, 'upgrade-blocked-light-1440x900')

    // Dark mode, same board.
    await page.addInitScript(([key, value]) => {
      window.localStorage.setItem(key, value)
    }, ['orbit.theme', 'dark'])
    await page.evaluate(() => window.localStorage.setItem('orbit.theme', 'dark'))
    await page.reload()
    await goto(page, server, '/projects/p-orbit-development/board')
    await page.getByRole('group', { name: 'Work item board' }).waitFor()
    await page.locator('[data-view-component="work_board"]').scrollIntoViewIfNeeded()
    await shot(page, 'work-items-board-dark-1440x900')
  })
})
