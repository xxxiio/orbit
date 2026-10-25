/**
 * I2 evidence capture (opt-in).
 *
 *   ORBIT_I2_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
 *     tests/e2e/evidence-i2.spec.ts --project=chromium
 *
 * Every frame comes from the deterministic fixture seeded through the public
 * API (no mocked data, no preview server). Files land in `web/evidence/i2/`.
 * The suite skips itself in a normal run so a test pass never rewrites committed
 * evidence.
 */
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from './harness/fixtures'
import { goto } from './harness/session'

const OUT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', 'evidence', 'i2')
const ENABLED = process.env.ORBIT_I2_EVIDENCE === '1'

test.skip(!ENABLED, 'set ORBIT_I2_EVIDENCE=1 to capture committed evidence')
test.setTimeout(180_000)

async function settle(page: import('@playwright/test').Page): Promise<void> {
  await page.evaluate(() => document.fonts.ready)
  await page.waitForTimeout(450)
}

async function shot(page: import('@playwright/test').Page, name: string): Promise<void> {
  await settle(page)
  await page.screenshot({ path: path.join(OUT, `${name}.png`) })
}

test.describe('I2 evidence', () => {
  test('light captures at desktop and phone widths', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    await mkdir(OUT, { recursive: true })
    await page.addInitScript(([key, value]) => {
      window.localStorage.setItem(key, value)
    }, ['orbit.theme', 'light'])

    const desktop = { width: 1440, height: 900 }
    const phone = { width: 390, height: 844 }

    // Board: columns, counts, cards and the keyboard hint.
    await page.setViewportSize(desktop)
    await goto(page, server, '/projects/p-basic/board')
    await page.getByRole('group', { name: 'Task board' }).waitFor()
    await shot(page, 'board-light-1440x900')

    // Planning: authored capacity, a day plan from the server, then the
    // reviewed (not applied) approval gate.
    await goto(page, server, '/projects/p-development/planning?capacity=180')
    await page.getByText('Selected at minute', { exact: false }).first().waitFor()
    await shot(page, 'planning-light-1440x900')

    await page.getByRole('button', { name: 'Review this plan' }).click()
    await page.locator('[data-plan-review]').waitFor()
    await page.locator('[data-plan-review]').scrollIntoViewIfNeeded()
    await shot(page, 'planning-review-light-1440x900')

    // The applied receipt is part of the evidence: the same reviewed command,
    // now bound to stored block ids.
    const review = page.locator('[data-plan-review]')
    await review.getByRole('checkbox', { name: 'Approve the reviewed schedule' }).check()
    await page.getByRole('button', { name: 'Apply approved plan' }).click()
    await page.locator('[data-plan-receipt]').waitFor()
    await page.locator('[data-plan-receipt]').scrollIntoViewIfNeeded()
    await shot(page, 'planning-receipt-light-1440x900')

    await page.setViewportSize(desktop)
    await goto(page, server, '/projects/p-development/milestones')
    await page.getByRole('heading', { name: 'Milestones', exact: true }).first().waitFor()
    await shot(page, 'milestones-light-1440x900')

    await goto(page, server, '/projects/p-development/settings')
    await page.getByText('Not supported by the API yet').first().waitFor()
    await shot(page, 'settings-light-1440x900')

    // The project task list with a shareable URL: filters, readiness and a
    // selection (so the bulk bar is part of the capture).
    const selected = fixture.tasks.find(
      (task) => task.projectId === 'p-basic' && task.status === 'ready',
    )
    const query = new URLSearchParams({
      status: 'ready',
      cols: 'title,status,priority,due,readiness,dependencies',
      ...(selected ? { sel: selected.id } : {}),
    })
    await goto(page, server, `/projects/p-basic/tasks?${query.toString()}`)
    await page.locator('table').waitFor()
    await shot(page, 'task-list-light-1440x900')

    // Phone width: the same screens, not a separate simplified app.
    await page.setViewportSize(phone)
    await goto(page, server, '/projects/p-basic/board')
    await page.getByRole('group', { name: 'Task board' }).waitFor()
    await shot(page, 'board-light-390x844')

    await goto(page, server, '/projects/p-development/planning?capacity=120')
    await page.getByText('Selected at minute', { exact: false }).first().waitFor()
    await shot(page, 'planning-light-390x844')

    await goto(page, server, '/projects/p-development/milestones')
    await page.getByRole('heading', { name: 'Milestones', exact: true }).first().waitFor()
    await shot(page, 'milestones-light-390x844')

    await goto(page, server, '/projects/p-development/settings')
    await page.getByText('Not supported by the API yet').first().waitFor()
    await shot(page, 'settings-light-390x844')

    await goto(page, server, `/projects/p-basic/tasks?${query.toString()}`)
    await page.getByRole('button', { name: /^Columns/ }).waitFor()
    await shot(page, 'task-list-light-390x844')
  })

  test('dark captures for the board and planning', async ({ page, workspace }) => {
    const { server } = workspace
    await mkdir(OUT, { recursive: true })
    await page.addInitScript(([key, value]) => {
      window.localStorage.setItem(key, value)
    }, ['orbit.theme', 'dark'])
    await page.setViewportSize({ width: 1440, height: 900 })

    await goto(page, server, '/projects/p-basic/board')
    await page.getByRole('group', { name: 'Task board' }).waitFor()
    await shot(page, 'board-dark-1440x900')

    await goto(page, server, '/projects/p-development/planning?capacity=180')
    await page.getByText('Selected at minute', { exact: false }).first().waitFor()
    await shot(page, 'planning-dark-1440x900')

    await page.getByRole('button', { name: 'Review this plan' }).click()
    await page.locator('[data-plan-review]').waitFor()
    await page.locator('[data-plan-review]').scrollIntoViewIfNeeded()
    await shot(page, 'planning-review-dark-1440x900')
  })
})
