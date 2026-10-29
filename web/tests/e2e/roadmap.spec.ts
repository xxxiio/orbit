/**
 * M33 — roadmap and milestone scheduling.
 *
 * Dates are authored through the same commands the rest of the app uses. The
 * roadmap never invents a bar for an item without dates; it lists it as
 * unscheduled instead.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'

async function readMilestones(page: import('@playwright/test').Page, server: E2EServer) {
  const response = await page.request.get(`${server.base}/v1/authoritative/milestones`)
  expect(response.status()).toBe(200)
  return (await response.json()) as {
    milestones: Array<{ id: string; name: string; target_at_ms?: number | null }>
  }
}

async function readTask(page: import('@playwright/test').Page, server: E2EServer, id: string) {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks/${id}`)
  expect(response.status()).toBe(200)
  return (await response.json()) as {
    deadline_at_ms: number | null
    earliest_start_at_ms: number | null
    status: string
  }
}

test.describe('roadmap', () => {
  test('a milestone target date is edited and rendered as a checkpoint', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const milestone = fixture.milestones.find((candidate) => candidate.projectId === 'p-development')
    expect(milestone, 'the fixture needs a Development milestone').toBeTruthy()

    await goto(page, server, '/projects/p-development/roadmap')
    await page.locator(`[data-unscheduled-milestone="${milestone!.id}"]`).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name: 'Reliability' })).toBeVisible()
    await dialog.getByLabel('Target date').fill('2026-11-20')
    await dialog.getByRole('button', { name: 'Save target' }).click()
    await expect(page.getByText('Milestone target updated')).toBeVisible()

    await expect
      .poll(async () => (await readMilestones(page, server)).milestones.find((m) => m.id === milestone!.id)?.target_at_ms)
      .toBe(new Date(2026, 10, 20).getTime())

    // The marker is a checkpoint, not a duration bar.
    await expect(page.getByRole('button', { name: /Reliability, milestone on/ })).toBeVisible()

    // Scales are explicit, not zoom gestures.
    for (const scale of ['week', 'quarter', 'month']) {
      await page.getByRole('button', { name: scale, exact: true }).click()
      await expect(page.getByRole('button', { name: scale, exact: true })).toHaveAttribute(
        'aria-pressed',
        'true',
      )
    }
  })

  test('an undated task is unscheduled, then a date edited in its drawer draws a bar', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) =>
        candidate.projectId === 'p-development' &&
        candidate.deadlineAtMs == null &&
        !['done', 'cancelled'].includes(candidate.status),
    )
    expect(task, 'the fixture needs an undated Development task').toBeTruthy()

    await goto(page, server, '/projects/p-development/roadmap')
    await expect(page.getByText(/These items have no authored dates/)).toBeVisible()
    const unscheduled = page.getByRole('button', { name: new RegExp(task!.title) }).last()
    await unscheduled.click()

    const drawer = page.getByRole('dialog')
    await drawer.getByLabel('Task deadline').fill('2026-11-15T17:00')
    await drawer.getByLabel('Task deadline').press('Enter')
    await expect
      .poll(async () => (await readTask(page, server, task!.id)).deadline_at_ms)
      .toBeGreaterThan(0)
    const stored = await readTask(page, server, task!.id)
    expect(stored.earliest_start_at_ms).toBeNull()
    await drawer.getByRole('button', { name: 'Close task details' }).click()

    // A one-date item is a marker; it is rendered, not hidden.
    await expect(page.getByRole('button', { name: new RegExp(`${task!.title}.*(derived|–)`) })).toBeVisible()
  })

  test('the roadmap filters by epic without leaving the timeline', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/roadmap')
    await expect(page.getByRole('region', { name: /roadmap/ }).or(page.getByText('Development roadmap'))).toBeVisible()
    await page.getByLabel('Filter roadmap by milestone').click()
    await expect(page.getByRole('option', { name: 'No milestone' })).toBeVisible()
    await page.keyboard.press('Escape')
  })
})
