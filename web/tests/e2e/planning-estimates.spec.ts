/**
 * I5-01 acceptance: a person can author the inputs the planner needs.
 *
 * The drawer's Planning group writes `set_planning` with the same revision
 * fence, read-back and refusal treatment as every other property. A sized task
 * then survives a reload, is selected by the server's day plan, and its
 * reviewed/applied schedule block survives another reload.
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

const TITLE = 'Sized by the drawer'
const DEADLINE = '2030-01-15T12:30'

async function createProject(page: Page, server: E2EServer, name: string): Promise<string> {
  await page.goto(`${server.base}/projects`)
  await page.getByRole('button', { name: 'New project' }).first().click()
  const dialog = page.getByRole('dialog')
  await dialog.getByLabel('Project name').fill(name)
  // The Development workflow declares the Milestones/Schedule panel this test
  // reads the stored block from. The seeded workspace has it published; wait
  // for the workflow list rather than racing its first read.
  const development = dialog.getByRole('radio', { name: /Development/ })
  await expect(development).toBeVisible({ timeout: 20_000 })
  await development.check()
  await dialog.getByRole('button', { name: 'Create project' }).click()
  await expect(page).toHaveURL(/\/projects\/[^/]+$/, { timeout: 20_000 })
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible()
  return new URL(page.url()).pathname.split('/').filter(Boolean)[1]
}

async function setStatus(page: Page, drawer: ReturnType<Page['getByRole']>, status: string) {
  await drawer.getByLabel('Task status').click()
  await page.getByRole('option', { name: status, exact: true }).click()
  await expect(drawer.getByLabel('Task status')).toContainText(status)
}

async function taskRecord(page: Page, server: E2EServer, taskId: string) {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks/${taskId}`)
  expect(response.status()).toBe(200)
  return (await response.json()) as {
    estimated_minutes: number | null
    earliest_start_at_ms: number | null
    deadline_at_ms: number | null
  }
}

async function scheduleBlocks(page: Page, server: E2EServer) {
  const response = await page.request.get(`${server.base}/v1/authoritative/schedule`)
  expect(response.status()).toBe(200)
  return ((await response.json()) as {
    blocks: Array<{ id: string; title: string; target_kind: string; target_id: string | null }>
  }).blocks
}

test.describe('planning estimates and deadlines', () => {
  // This flow creates a project, a task, three planning writes, a day plan and an
  // applied block, then reloads twice. Solo it runs at ~26s; the default 30s
  // budget leaves no headroom for a seeded server under two workers.
  test.setTimeout(90_000)

  test('a sized task survives reload and its applied schedule block does too', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/')
    const projectId = await createProject(page, server, 'Estimate gate')

    // One task, created through the browser, then opened from the board.
    await page.goto(`${server.base}/projects/${projectId}/board`)
    await expect(page.getByRole('navigation', { name: 'Project sections' })).toBeVisible()
    await page.getByRole('button', { name: 'New task', exact: true }).first().click()
    const create = page.getByRole('dialog')
    await create.getByLabel('Title').fill(TITLE)
    await create.getByRole('button', { name: 'Create task', exact: true }).click()
    await expect(create).toBeHidden()

    const card = page.getByRole('button', { name: TITLE, exact: true }).first()
    await card.click()
    const drawer = page.getByRole('dialog')
    await expect(drawer.getByLabel('Task description')).toBeVisible()
    const taskId = new URL(page.url()).searchParams.get('task')
    expect(taskId).toBeTruthy()

    // Inbox → Ready is an allowed transition; the day plan needs actionable work.
    await setStatus(page, drawer, 'Ready')

    const estimate = drawer.getByLabel('Task estimate in minutes')
    await expect(estimate).toBeVisible()
    await estimate.fill('60')
    await estimate.press('Tab')
    await expect(estimate).toHaveValue('60')
    await expect
      .poll(async () => (await taskRecord(page, server, taskId!)).estimated_minutes)
      .toBe(60)

    const deadline = drawer.getByLabel('Task deadline')
    await expect(deadline).toBeEnabled()
    await deadline.fill(DEADLINE)
    await deadline.blur()
    await expect(deadline).toHaveValue(DEADLINE)
    await expect
      .poll(async () => (await taskRecord(page, server, taskId!)).deadline_at_ms)
      .not.toBeNull()

    // Read-back after a full reload: the stored record is the source.
    await page.reload()
    const reopened = page.getByRole('dialog')
    await expect(reopened.getByLabel('Task description')).toBeVisible()
    await expect(reopened.getByLabel('Task estimate in minutes')).toHaveValue('60')
    await expect(reopened.getByLabel('Task deadline')).toHaveValue(DEADLINE)
    await reopened.getByRole('button', { name: 'Close task details' }).click()
    await expect(reopened).toBeHidden()

    // The server's day plan selects the sized task.
    await page.goto(`${server.base}/projects/${projectId}/planning`)
    await page.getByRole('heading', { name: /^Plan / }).waitFor()
    const capacity = page.getByLabel('Available minutes today')
    await capacity.fill('120')
    await page.getByRole('button', { name: 'Build day plan' }).click()
    const dayPlan = panelByHeading(page, 'Day plan')
    await expect(dayPlan.getByRole('button', { name: TITLE, exact: true })).toBeVisible()
    await expect(dayPlan.getByText('Selected', { exact: true }).first()).toBeVisible()

    // The default window is tomorrow's 09:00–12:00; the 60-minute task fits.
    const schedulePanel = panelByHeading(page, 'Schedule a window')
    await expect(schedulePanel.locator('[data-schedule-slot]').first()).toBeVisible()
    await expect(schedulePanel.getByText(TITLE, { exact: true })).toBeVisible()

    await schedulePanel.getByRole('button', { name: 'Review this plan' }).click()
    const review = page.locator('[data-plan-review]')
    await expect(review).toBeVisible()
    await review.getByRole('checkbox', { name: 'Approve the reviewed schedule' }).check()
    await schedulePanel.getByRole('button', { name: 'Apply approved plan' }).click()
    await expect(page.locator('[data-plan-receipt]')).toBeVisible()

    expect((await scheduleBlocks(page, server)).map((block) => block.title)).toEqual([TITLE])

    // Reload and read the saved schedule back from the server, then from the
    // Timeline surface; nothing is trusted from the pre-reload React state.
    await page.reload()
    const after = await scheduleBlocks(page, server)
    expect(after.map((block) => block.title)).toEqual([TITLE])

    await page.goto(`${server.base}/projects/${projectId}/milestones`)
    const schedule = panelByHeading(page, 'Schedule')
    await expect(schedule.getByText(TITLE, { exact: true })).toBeVisible()
  })

  test('an unsized project points at the missing estimate and offers the explicit default duration', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/')
    const projectId = await createProject(page, server, 'Unsized gate')

    await page.goto(`${server.base}/projects/${projectId}/board`)
    await expect(page.getByRole('navigation', { name: 'Project sections' })).toBeVisible()
    await page.getByRole('button', { name: 'New task', exact: true }).first().click()
    const create = page.getByRole('dialog')
    await create.getByLabel('Title').fill('Needs an estimate')
    await create.getByRole('button', { name: 'Create task', exact: true }).click()
    await expect(create).toBeHidden()
    await page.getByRole('button', { name: 'Needs an estimate', exact: true }).first().click()
    const drawer = page.getByRole('dialog')
    await expect(drawer.getByLabel('Task description')).toBeVisible()
    await setStatus(page, drawer, 'Ready')
    await drawer.getByRole('button', { name: 'Close task details' }).click()
    await expect(drawer).toBeHidden()

    await page.goto(`${server.base}/projects/${projectId}/planning`)
    const capacity = page.getByLabel('Available minutes today')
    await capacity.fill('120')
    await page.getByRole('button', { name: 'Build day plan' }).click()
    const dayPlan = panelByHeading(page, 'Day plan')
    await expect(dayPlan.getByText('Missing an estimate').first()).toBeVisible()
    await expect(dayPlan.getByText('Add an estimate to these tasks')).toBeVisible()

    // The empty state opens the unsized task, where the Planning group lives.
    await dayPlan
      .getByRole('button', { name: 'Open “Needs an estimate” to add an estimate' })
      .click()
    await expect(page.getByRole('dialog').getByLabel('Task estimate in minutes')).toBeVisible()
    await page.getByRole('dialog').getByRole('button', { name: 'Close task details' }).click()
    await expect(page.getByRole('dialog')).toBeHidden()

    // The API's explicit escape hatch: include unsized work at an authored
    // default instead of silently leaving Review disabled.
    await dayPlan
      .getByRole('checkbox', { name: 'Plan unsized tasks with a default duration' })
      .check()
    await expect(
      dayPlan.getByRole('button', { name: 'Needs an estimate', exact: true }),
    ).toBeVisible()
    await expect(dayPlan.getByText('Selected', { exact: true }).first()).toBeVisible()

    // A schedule block reserves real time, so the server still needs a stored
    // estimate. The page says why Review is unavailable and points at the task
    // instead of leaving a disabled button unexplained.
    const schedulePanel = panelByHeading(page, 'Schedule a window')
    await expect(schedulePanel.getByText(/no duration estimate/i).first()).toBeVisible()
    await expect(
      schedulePanel.getByRole('button', { name: 'Review this plan' }),
    ).toBeDisabled()
    await expect(
      schedulePanel.getByRole('button', { name: /to add an estimate$/ }),
    ).toBeVisible()
  })
})
