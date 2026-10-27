/**
 * I4 acceptance: the Agents workspace reads jobs this device owns.
 *
 * A review writes a decision and never moves the Orbit task. A missing or
 * foreign id answers 403 and is reported as "not visible", not as "does not
 * exist" — including for a job another enrolled device really started. Resuming
 * an interrupted job queues a successor; it does not launch one.
 *
 * Launching itself (the task drawer and Agents page dialogs, the executor
 * roster) is covered by `agents-launch.spec.ts`. Nothing on the job page starts
 * a model: the controls here are explicit review and resume only.
 */
import { expect, test } from './harness/fixtures'
import { readOverview, readJob, startJob } from './harness/agents'
import { getAuthority } from './harness/authority'
import { enrollDeviceWithAdminKey, type E2EServer } from './harness/server'
import { goto, signInViaApi } from './harness/session'

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
    // A running job is not resumable: the server requires a terminal
    // predecessor, so the control is replaced by an explanation.
    await expect(page.getByRole('button', { name: 'Resume as a new job' })).toHaveCount(0)
    await expect(
      page.getByText(/The server resumes only a completed, failed or interrupted job/i),
    ).toBeVisible()
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

  test('an unconfirmed review is discarded by a refresh that finds no review', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    await goto(page, server, '/agents/job-seed-4')
    await expect(page.getByRole('region', { name: 'Review' })).toBeVisible()

    // Drop the response without applying anything: the server still has no
    // review, so an explicit refresh can prove the attempt did not land.
    await page.route('**/v1/authoritative/agent-jobs/review', async (route) => {
      await route.abort('failed')
    })
    await page.getByLabel(/Reason/).fill('Checked the handoff; approve this time.')
    await page.getByRole('button', { name: 'Record review' }).click()
    await expect(page.getByText('The review was not confirmed.')).toBeVisible()

    // Changing the decision is blocked until the unknown attempt is resolved.
    await page.getByRole('button', { name: 'Request changes' }).click()
    await expect(
      page.getByText(/These fields no longer match the review whose outcome is unknown/i),
    ).toBeVisible()

    await page.getByRole('button', { name: 'Refresh the job' }).click()
    await expect(
      page.getByText(/no review is on record. The unconfirmed attempt was discarded/i),
    ).toBeVisible()
    await expect(
      page.getByText(/These fields no longer match the review whose outcome is unknown/i),
    ).toHaveCount(0)

    await page.unroute('**/v1/authoritative/agent-jobs/review')
    await page.getByRole('button', { name: 'Request changes' }).click()
    await page.getByRole('button', { name: 'Record review' }).click()
    await expect(
      page.getByText('Review recorded as changes requested. The task is unchanged.'),
    ).toBeVisible()

    const reviewResponse = await page.request.get(
      `${server.base}/v1/authoritative/agent-jobs/review?id=job-seed-4`,
    )
    expect(reviewResponse.status()).toBe(200)
    expect(((await reviewResponse.json()) as { decision: string }).decision).toBe(
      'changes_requested',
    )
  })

  test('an interrupted job resumes into a new job that appears in the overview', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    await goto(page, server, '/agents/job-seed-5')
    await expect(page.getByText('Interrupted').first()).toBeVisible()
    await expect(
      page.getByText(/The server resumes only a completed, failed or interrupted job/i),
    ).toHaveCount(0)
    await page.getByRole('button', { name: 'Resume as a new job' }).click()

    await page.waitForURL(
      (url) => /^\/agents\/[^/]+$/.test(url.pathname) && !url.pathname.endsWith('/job-seed-5'),
      { timeout: 15_000 },
    )
    const newId = decodeURIComponent(new URL(page.url()).pathname.split('/').pop() ?? '')
    expect(newId).not.toBe('job-seed-5')
    // Resume queues a new job; it does not launch it.
    await expect(page.getByText('Preparing').first()).toBeVisible()
    await expect(page.getByRole('button', { name: /launch|start agent|run agent/i })).toHaveCount(0)

    await goto(page, server, '/agents')
    await expect(page.locator(`[data-agent-job="${newId}"]`)).toBeVisible()
  })

  test('a foreign job id is reported as not visible, never as missing', async ({
    page,
    context,
    workspace,
  }) => {
    const { server, fixture } = workspace
    // An id that was never created: the ownership check answers before the lookup.
    await goto(page, server, '/agents/not-my-job-9f4c')
    await expect(page.getByText('This job is not visible to this device')).toBeVisible()
    await expect(page.getByText(/does not exist|no longer exists/i)).toHaveCount(0)
    await expect(page.getByRole('link', { name: 'Back to Agents' })).toBeVisible()

    // A job that really exists, owned by another enrolled device. Jobs belong to
    // the device that started them, so "not visible" must not be a polite
    // spelling of "not found".
    const other = {
      deviceId: 'e2e-foreign-executor',
      token: 'e2e-foreign-executor-token-0123456789abcdef',
    }
    await enrollDeviceWithAdminKey(server, other)
    const second = await context.browser()!.newContext({ baseURL: server.base })
    const otherPage = await second.newPage()
    try {
      await signInViaApi(otherPage, server, other)
      const started = await startJob(otherPage, server, {
        jobId: 'job-foreign-device',
        taskId: fixture.ids.orbitTask,
        machineRef: other.deviceId,
        repositoryRef: 'orbit',
        atMs: fixture.nowMs,
      })
      // The owner reads the job it queued...
      expect((await readJob(otherPage, server, started.job_id)).machine_ref).toBe(other.deviceId)
      // ...and this device is refused, without being told whether it exists.
      const foreign = await getAuthority(page, server, `agent-jobs?id=${started.job_id}`)
      expect(foreign.status).toBe(403)
      expect(JSON.stringify(foreign.body)).toContain('device identity mismatch')
      expect((await readOverview(page, server)).map((row) => row.job_id)).not.toContain(
        started.job_id,
      )

      await goto(page, server, `/agents/${started.job_id}`)
      await expect(page.getByText('This job is not visible to this device')).toBeVisible()
      await expect(page.getByText(/does not exist|no longer exists/i)).toHaveCount(0)
    } finally {
      await second.close()
    }
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
