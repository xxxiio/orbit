/**
 * I2 acceptance: planning is server-authoritative.
 *
 * A day plan only exists once capacity is authored; a schedule is a proposal
 * until a reviewed command is explicitly approved; the receipt is the
 * fingerprint and command id the server bound the write to. Nothing in this
 * spec recomputes a slot or a disposition in the test.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

function panelByHeading(page: Page, name: string): Locator {
  return page
    .locator('section')
    .filter({ has: page.getByRole('heading', { name, exact: true }) })
    .last()
}

async function scheduleBlocks(page: Page, server: E2EServer): Promise<Array<{ id: string; title: string }>> {
  const response = await page.request.get(`${server.base}/v1/authoritative/schedule`)
  expect(response.status()).toBe(200)
  return ((await response.json()) as { blocks: Array<{ id: string; title: string }> }).blocks
}

test.describe('planning', () => {
  test('a day plan needs authored capacity, then shows the server dispositions', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const dayPlanRequests: string[] = []
    page.on('request', (request) => {
      if (request.url().includes('/v1/authoritative/day-plan')) dayPlanRequests.push(request.url())
    })

    await goto(page, server, '/projects/p-development/planning')
    const dayPanel = panelByHeading(page, 'Day plan')
    await expect(dayPanel).toBeVisible()
    await expect(dayPanel.getByText('Tell the server how much time you have')).toBeVisible()
    // No capacity, no request: the client never guesses a working day.
    expect(dayPlanRequests).toEqual([])

    const capacity = dayPanel.getByLabel('Available minutes today')
    await capacity.fill('120')
    await capacity.press('Enter')

    await expect.poll(() => dayPlanRequests.length).toBeGreaterThan(0)
    expect(dayPlanRequests[0]).toContain('available_minutes=120')
    expect(dayPlanRequests[0]).toContain('project_id=p-development')
    await expect(page).toHaveURL(/capacity=120/)

    // The stat and every explanation below are the server's answer.
    await expect(dayPanel.getByText('Capacity', { exact: true }).first()).toBeVisible()
    await expect(dayPanel.getByText('2 h', { exact: true }).first()).toBeVisible()
    await expect(
      dayPanel.getByText('Selected at minute 0 of available 120', { exact: false }).first(),
    ).toBeVisible()
    await expect(dayPanel.getByText('selected', { exact: true }).first()).toBeVisible()

    // A disposition the UI did not choose: the server evaluated every bucket.
    await expect(dayPanel.getByText(/actionable and \d+ blocked tasks were evaluated/)).toBeVisible()

    // Changing capacity is authored again, and the URL keeps it.
    await capacity.fill('240')
    await capacity.press('Enter')
    await expect.poll(() => dayPlanRequests.length).toBeGreaterThan(1)
    expect(dayPlanRequests.at(-1)).toContain('available_minutes=240')
    await expect(page).toHaveURL(/capacity=240/)
  })

  test('review is read-only; an explicit approval applies the exact fingerprinted command and survives reload', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/planning')

    const schedulePanel = panelByHeading(page, 'Schedule a window')
    await expect(schedulePanel).toBeVisible()

    const slots = schedulePanel.locator('[data-schedule-slot]')
    await expect(slots.first()).toBeVisible()
    const slotCount = await slots.count()
    expect(slotCount).toBeGreaterThan(0)
    const firstSlotTitle = (await slots.first().locator('p').first().textContent())?.trim()
    expect(firstSlotTitle?.length).toBeGreaterThan(0)

    // The window is an authored input, not "now + 1 day".
    await expect(schedulePanel.getByLabel('Schedule window date')).toHaveValue(/^\d{4}-\d{2}-\d{2}$/)
    await expect(schedulePanel.getByLabel('Schedule window start time')).toHaveValue(/^\d{2}:\d{2}$/)
    await expect(schedulePanel.getByLabel('Schedule window end time')).toHaveValue(/^\d{2}:\d{2}$/)

    expect(await scheduleBlocks(page, server)).toHaveLength(0)

    await schedulePanel.getByRole('button', { name: 'Review this plan' }).click()
    const review = page.locator('[data-plan-review]')
    await expect(review).toBeVisible()
    await expect(review.getByText('Reviewed · not applied')).toBeVisible()

    const fingerprint = (await page.locator('[data-review-fingerprint]').textContent())?.trim() ?? ''
    const commandId = (await page.locator('[data-review-command]').textContent())?.trim() ?? ''
    expect(fingerprint).toMatch(/^[0-9a-f]{64}$/)
    expect(commandId.length).toBeGreaterThan(0)

    // Review must not have written anything.
    expect(await scheduleBlocks(page, server)).toHaveLength(0)

    const apply = schedulePanel.getByRole('button', { name: 'Apply approved plan' })
    await expect(apply).toBeDisabled()
    await review.getByRole('checkbox', { name: 'Approve the reviewed schedule' }).check()
    await expect(apply).toBeEnabled()
    await apply.click()

    const receipt = page.locator('[data-plan-receipt]')
    await expect(receipt).toBeVisible()
    await expect(receipt.getByText(/Plan applied at revision \d+/)).toBeVisible()
    // Receipt identity is exactly the reviewed command, never regenerated.
    expect((await receipt.locator('[data-receipt-fingerprint]').textContent())?.trim()).toBe(fingerprint)
    expect((await receipt.locator('[data-receipt-command]').textContent())?.trim()).toBe(commandId)

    const blocks = await scheduleBlocks(page, server)
    expect(blocks).toHaveLength(slotCount)

    // Reload and read the saved schedule from the Timeline view, not from the
    // planning page's in-memory state.
    await goto(page, server, '/projects/p-development/milestones')
    const schedule = panelByHeading(page, 'Schedule')
    await expect(schedule).toBeVisible()
    await expect(schedule.getByText(firstSlotTitle!, { exact: true })).toBeVisible()

    const afterReload = await scheduleBlocks(page, server)
    expect(afterReload.map((block) => block.title)).toContain(firstSlotTitle)
  })

  test('the session is re-read when an open tab becomes visible again', async ({ page, workspace }) => {
    const { server } = workspace
    const sessionReads: string[] = []
    page.on('request', (request) => {
      if (request.url().endsWith('/v1/web/session') && request.method() === 'GET') {
        sessionReads.push(request.url())
      }
    })

    await goto(page, server, '/')
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible()
    await expect(page.getByText(/Updated just now|Updated \d+ min ago/).first()).toBeVisible()
    const before = sessionReads.length

    // A tab restored after hours is visible again; the app must slide the
    // server session instead of waiting for a request to discover a 401.
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')))
    await expect.poll(() => sessionReads.length).toBeGreaterThan(before)
    await expect(page).not.toHaveURL(/\/login/)
  })
})
