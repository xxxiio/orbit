/**
 * I3 evidence capture (opt-in, capture-only).
 *
 *   ORBIT_I3_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
 *     tests/e2e/evidence-i3.spec.ts --project=chromium
 *
 * Every frame comes from the deterministic fixture seeded through the public
 * API (no mocked data, no preview server). Files land in `web/evidence/i3/`.
 *
 * This spec only *captures*: it navigates to states the fixture already seeded
 * and takes screenshots. It never creates an item, moves one, or publishes a
 * workflow version, because which commands succeed is acceptance coverage and
 * belongs in `work-items.spec.ts` / `upgrades.spec.ts` — running the same flows
 * here twice would make a failed capture look like a failed product. It skips
 * itself in a normal run so a test pass never rewrites committed evidence.
 */
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from './harness/fixtures'
import { goto } from './harness/session'

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

test.describe('I3 evidence', () => {
  test('work items, sheet and create dialog from the seeded state', async ({ page, workspace }) => {
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
    await page.getByRole('button', { name: 'Close work item details' }).click()

    // Create dialog with typed controls (the Development view pins one kind).
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

    // Dark mode, same board.
    await page.setViewportSize(desktop)
    await page.evaluate(() => window.localStorage.setItem('orbit.theme', 'dark'))
    await page.reload()
    await goto(page, server, '/projects/p-orbit-development/board')
    await page.getByRole('group', { name: 'Work item board' }).waitFor()
    await page.locator('[data-view-component="work_board"]').scrollIntoViewIfNeeded()
    await shot(page, 'work-items-board-dark-1440x900')
  })
})
