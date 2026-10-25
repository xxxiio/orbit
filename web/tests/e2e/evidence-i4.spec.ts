/**
 * I4 evidence capture (opt-in, capture-only).
 *
 *   ORBIT_I4_EVIDENCE=1 node node_modules/@playwright/test/cli.js test \
 *     tests/e2e/evidence-i4.spec.ts --project=chromium
 *
 * Every frame comes from the deterministic fixture seeded through the public
 * API. This spec only reads and opens surfaces: it never uploads a file, creates
 * a note or records a review, so a failed capture cannot be mistaken for a
 * failed product flow. Acceptance lives in knowledge.spec.ts, attachments.spec.ts
 * and agents.spec.ts.
 */
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from './harness/fixtures'
import { goto } from './harness/session'

const OUT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', 'evidence', 'i4')
const ENABLED = process.env.ORBIT_I4_EVIDENCE === '1'

test.skip(!ENABLED, 'set ORBIT_I4_EVIDENCE=1 to capture committed evidence')
test.setTimeout(240_000)

async function settle(page: import('@playwright/test').Page): Promise<void> {
  await page.evaluate(() => document.fonts.ready)
  await page.waitForTimeout(450)
}

async function shot(page: import('@playwright/test').Page, name: string): Promise<void> {
  await settle(page)
  await page.screenshot({ path: path.join(OUT, `${name}.png`) })
}

test.describe('I4 evidence', () => {
  test('knowledge, attachments, agents and the real registry panels', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    await mkdir(OUT, { recursive: true })
    await page.addInitScript(([key, value]) => {
      window.localStorage.setItem(key, value)
    }, ['orbit.theme', 'light'])

    const desktop = { width: 1440, height: 900 }
    const phone = { width: 390, height: 844 }

    // Knowledge: a note with resolvable and unresolved references.
    await page.setViewportSize(desktop)
    await goto(page, server, '/knowledge/notes/orbit-overview.md')
    await page.locator('[data-note-body]').waitFor()
    await page.getByRole('region', { name: 'Relations' }).waitFor()
    await shot(page, 'knowledge-note-light-1440x900')

    await page.setViewportSize(phone)
    await shot(page, 'knowledge-note-light-390x844')

    await page.setViewportSize(desktop)
    await page.getByRole('button', { name: 'New note' }).click()
    await page.getByRole('dialog').waitFor()
    await shot(page, 'knowledge-create-light-1440x900')
    await page.keyboard.press('Escape')

    await page.getByLabel('Search notes').fill('readiness')
    await page.getByRole('list', { name: 'Search results' }).waitFor()
    await shot(page, 'knowledge-search-light-1440x900')

    // Attachments: the task drawer list with a real image preview open.
    await goto(page, server, `/inbox?task=${encodeURIComponent(fixture.tasks[0].id)}`)
    const taskAttachments = page.getByRole('dialog').getByRole('region', { name: 'Attachments' })
    await taskAttachments.waitFor()
    const imageRow = taskAttachments.locator('[data-attachment-id="a-06"]')
    await imageRow.getByRole('button', { name: /Preview/i }).click()
    await imageRow.getByRole('img', { name: /Preview of status-card\.svg/ }).waitFor()
    await taskAttachments.scrollIntoViewIfNeeded()
    await shot(page, 'attachments-task-drawer-light-1440x900')

    await page.setViewportSize(phone)
    await shot(page, 'attachments-task-drawer-light-390x844')

    // The same component inside the work-item sheet.
    await page.setViewportSize(desktop)
    await goto(page, server, '/projects/p-orbit-development/board')
    await page.locator('[data-work-item-key="wi-orbit-feature-ready"]').getByRole('button').first().click()
    const itemAttachments = page.getByRole('dialog').getByRole('region', { name: 'Attachments' })
    await itemAttachments.waitFor()
    await itemAttachments.scrollIntoViewIfNeeded()
    await shot(page, 'attachments-work-item-sheet-light-1440x900')

    // Agents: overview, a job with a handoff ready for review, and the phone frame.
    await goto(page, server, '/agents')
    await page.getByRole('heading', { level: 1, name: 'Agents' }).waitFor()
    await shot(page, 'agents-overview-light-1440x900')

    await page.locator('[data-agent-job="job-seed-4"]').click()
    await page.getByRole('heading', { level: 1 }).waitFor()
    await page.evaluate(() => window.scrollTo(0, 0))
    await shot(page, 'agent-job-detail-light-1440x900')
    await page.getByRole('region', { name: 'Review' }).waitFor()
    await page.getByRole('region', { name: 'Review' }).scrollIntoViewIfNeeded()
    await shot(page, 'agent-job-review-light-1440x900')

    await page.setViewportSize(phone)
    await shot(page, 'agent-job-review-light-390x844')

    // Dark mode, same overview.
    await page.setViewportSize(desktop)
    await page.evaluate(() => window.localStorage.setItem('orbit.theme', 'dark'))
    await goto(page, server, '/agents')
    await page.getByRole('heading', { level: 1, name: 'Agents' }).waitFor()
    await shot(page, 'agents-overview-dark-1440x900')

    // Home's attention rail comes from the same attention read.
    await page.evaluate(() => window.localStorage.setItem('orbit.theme', 'light'))
    await goto(page, server, '/')
    const attention = page.getByRole('heading', { name: 'Agent jobs needing attention' })
    await attention.waitFor()
    await attention.scrollIntoViewIfNeeded()
    await shot(page, 'home-attention-light-1440x900')

    // Declared workflow panels, now over real reads.
    await goto(page, server, '/projects/p-research/evidence')
    const references = page.locator('[data-view-component="references"]')
    await references.waitFor()
    await references.locator('a[href^="/knowledge/"]').first().waitFor()
    await references.evaluate((element) => element.scrollIntoView({ block: 'start' }))
    await shot(page, 'registry-knowledge-refs-light-1440x900')

    await goto(page, server, '/projects/p-development/agents')
    const jobs = page.locator('[data-view-component="jobs"]')
    await jobs.waitFor()
    await jobs.evaluate((element) => element.scrollIntoView({ block: 'start' }))
    await shot(page, 'registry-agent-jobs-light-1440x900')
  })
})
