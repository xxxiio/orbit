/**
 * I4 acceptance: the Agents workspace reads jobs this device owns.
 *
 * A review writes a decision and never moves the Orbit task. A missing/foreign
 * id answers 403 and is reported as "not visible", not as "does not exist".
 * There is no launch control anywhere.
 */
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

async function taskStatus(
  page: import('@playwright/test').Page,
  server: E2EServer,
  taskId: string,
): Promise<string> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/tasks/${encodeURIComponent(taskId)}`,
  )
  expect(response.status()).toBe(200)
  return ((await response.json()) as { status: string }).status
}

test.describe('agent jobs', () => {
  test('overview → detail → review, and the task is still unchanged', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const job = fixture.agentJobs.find((candidate) => candidate.id === 'job-seed-4')
    expect(job, 'the fixture must seed a handoff with no review').toBeTruthy()

    await goto(page, server, '/agents')
    const before = await taskStatus(page, server, job!.taskId)
    await expect(page.getByRole('heading', { level: 1, name: 'Agents' })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Needs attention' })).toBeVisible()
    const row = page.locator('[data-agent-job="job-seed-4"]')
    await expect(row).toBeVisible()
    await expect(row.getByText('Needs attention')).toBeVisible()
    await row.click()

    await expect(page).toHaveURL(/\/agents\/job-seed-4$/)
    await expect(page.getByText('job-seed-4').first()).toBeVisible()
    await expect(page.getByRole('region', { name: 'Review' })).toBeVisible()
    await expect(page.getByText('Approving records a review. It does not complete the task.')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Resume as a new job' })).toBeVisible()
    // No launch/run affordance exists anywhere on the page.
    await expect(page.getByRole('button', { name: /launch|start agent|run agent/i })).toHaveCount(0)

    await page.getByLabel(/Reason/).fill('Checked the handoff receipt; the guard copy is accurate.')
    await page.getByRole('button', { name: 'Approve', exact: true }).click()
    await page.getByRole('button', { name: 'Record review' }).click()

    await expect(page.getByText('Review recorded as approved. The task was not completed.')).toBeVisible()
    await expect(page.getByText('Approved').first()).toBeVisible()
    await expect(page.getByText('Task not completed')).toBeVisible()

    const reviewResponse = await page.request.get(
      `${server.base}/v1/authoritative/agent-jobs/review?id=job-seed-4`,
    )
    expect(reviewResponse.status()).toBe(200)
    const review = (await reviewResponse.json()) as {
      decision: string
      task_completed: boolean
    }
    expect(review.decision).toBe('approved')
    expect(review.task_completed).toBe(false)

    // The task is untouched by a review.
    expect(await taskStatus(page, server, job!.taskId)).toBe(before)
  })

  test('a foreign job id is reported as not visible, never as missing', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/agents/not-my-job-9f4c')
    await expect(page.getByText('This job is not visible to this device')).toBeVisible()
    await expect(page.getByText(/does not exist|no longer exists/i)).toHaveCount(0)
    await expect(page.getByRole('link', { name: 'Back to Agents' })).toBeVisible()
  })

  test('Home lists the agent jobs that need attention', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, '/')
    await expect(page.getByRole('heading', { name: 'Agent jobs needing attention' })).toBeVisible()
    await expect(page.locator('a[href="/agents/job-seed-1"]').first()).toBeVisible()
    await expect(page.locator('a[href="/agents/job-seed-4"]').first()).toBeVisible()
    await page.locator('a[href="/agents/job-seed-1"]').first().click()
    await expect(page).toHaveURL(/\/agents\/job-seed-1$/)
  })
})
