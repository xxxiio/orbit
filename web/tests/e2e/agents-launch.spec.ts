/**
 * I3 acceptance: queueing an agent job from the browser, and what the authority
 * does with it afterwards.
 *
 * There is no model and no Herdr in this path. The fake executor is the fixture's
 * own enrolled device credential: the server's machine fence is
 * `job.machine_ref == authenticated device id`, so the device this browser is
 * signed in as *is* a real executor once it posts the `agent-machines/*` routes
 * (`harness/agents.ts`). Every spec owns one real server through the harness and
 * asserts both what the server stores (`page.request`) and what a person sees.
 *
 * The rules the specs below keep:
 *  - the browser queues; it never starts a model, and it never claims its own job;
 *  - a queued job with no claim is "awaiting an executor", not a person's work;
 *  - a review is a write that never completes the Orbit task;
 *  - an unknown executor is refused *before* a job exists;
 *  - a refused launch retries the same `job_id`, never a second one.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import {
  claim,
  jobClock,
  observe,
  readExecutors,
  readHandoffs,
  readJob,
  readOverview,
  readResumeContext,
  readReview,
  reportLaunch,
  review,
  startJob,
  submitHandoff,
  type OverviewItem,
} from './harness/agents'
import {
  authorityRevision,
  postAuthority,
  readTaskRecord,
} from './harness/authority'
import { enrollDeviceWithAdminKey, revokeDevice, type E2EServer } from './harness/server'
import { seedWorkspace, type WorkspaceFixture } from './harness/seed'
import { goto, signInViaApi } from './harness/session'

/** A job that no seeded fixture row uses, so counts stay easy to read. */
function jobId(suffix: string): string {
  return `job-k-${suffix}`
}

/** The one task these specs queue: ready, no prerequisites, no seeded job. */
function eligibleTask(fixture: WorkspaceFixture) {
  const task = fixture.tasks.find((candidate) => candidate.id === fixture.ids.orbitTask)
  expect(task, 'the fixture must expose an eligible orbit task').toBeTruthy()
  return task!
}

function drawerFor(page: Page, taskId: string): Locator {
  return page.locator(`[data-task-drawer="${taskId}"]`)
}

async function openTaskDrawer(
  page: Page,
  server: E2EServer,
  task: { id: string; projectId: string },
): Promise<Locator> {
  await goto(
    page,
    server,
    `/projects/${encodeURIComponent(task.projectId)}?task=${encodeURIComponent(task.id)}`,
  )
  const drawer = drawerFor(page, task.id)
  await expect(drawer).toBeVisible()
  return drawer
}

async function rowFor(
  page: Page,
  server: E2EServer,
  id: string,
): Promise<OverviewItem | undefined> {
  return (await readOverview(page, server)).find((row) => row.job_id === id)
}

/** The job page's own refresh, so assertions read the server, not a cache. */
async function refreshJobPage(page: Page): Promise<void> {
  await page.getByRole('button', { name: 'Refresh', exact: true }).click()
}

test.describe('agent launch', () => {
  test('queue → claim → launch → observe → handoff → review, and the task does not move', async ({
    page,
    server,
  }) => {
    /**
     * The shared `workspace` fixture also seeds agent jobs, and seeding claims
     * them with this device's own credential — which would make the browser's
     * device a device that *has* claimed before. This cycle needs the other
     * honest case, the one a fresh install is in: enrolled, with nothing ever
     * claimed on it. So it seeds the same workspace without the agent-job
     * extras, and the device the browser signs in as has no claim on record.
     */
    const fixture = await seedWorkspace(server, { extras: false })
    await page.clock.setFixedTime(new Date(fixture.nowMs))
    const task = eligibleTask(fixture)
    const objective = 'Take the drawer launch path end to end and report what changed.'
    const sessionRef = 'session-k-full-cycle'

    // The baseline: the task is queueable and the server says so.
    await goto(page, server, `/projects/${task.projectId}?task=${task.id}`)
    const before = await readTaskRecord(page, server, task.id)
    expect(before.status).toBe('ready')

    const drawer = drawerFor(page, task.id)
    await expect(drawer).toBeVisible()
    await expect(drawer.locator('header').getByText('Ready', { exact: true })).toBeVisible()
    const launchTrigger = drawer.locator('[data-launch-agent-job]')
    await expect(launchTrigger).toBeEnabled()
    await launchTrigger.click()

    const dialog = page.locator('[data-launch-dialog]')
    await expect(dialog).toBeVisible()
    await expect(dialog).toHaveAttribute('data-launch-step', 'form')
    // The task is fixed by the drawer, so there is no task chooser to operate.
    await expect(dialog.locator('[data-launch-task]')).toHaveAttribute('data-launch-task', task.id)
    // One active executor exists (the harness device) and is preselected.
    await expect(dialog.getByRole('combobox', { name: /^Executor/ })).toContainText(
      server.deviceId,
    )
    await dialog.locator('#agent-launch-objective').fill(objective)
    await dialog.locator('#agent-launch-repository').fill('orbit')

    await dialog.getByRole('button', { name: 'Review and queue' }).click()
    await expect(dialog).toHaveAttribute('data-launch-step', 'confirm')
    const pinnedId = (await dialog.locator('[data-launch-job-id]').textContent())?.trim() ?? ''
    expect(pinnedId, 'the confirmation must show the real launch identity').not.toBe('assigned when you queue')

    // The fixture's executor has never claimed a job, and the confirmation says
    // so for that device instead of promising that work has begun. The warning
    // is a caveat: the launch is still offered.
    const neverClaimed = dialog.locator('[data-launch-never-claimed]')
    await expect(neverClaimed).toBeVisible()
    await expect(neverClaimed).toContainText('This device has never claimed a job.')
    await expect(neverClaimed).toContainText(server.deviceId)
    await expect(neverClaimed).toContainText('has not claimed a job before')
    await expect(neverClaimed).toContainText('Preparing')
    await expect(dialog.getByRole('button', { name: 'Queue the job' })).toBeEnabled()

    await dialog.getByRole('button', { name: 'Queue the job' }).click()
    await page.waitForURL((url) => url.pathname === `/agents/${pinnedId}`, { timeout: 20_000 })

    // Landed on the job the server created: queued, unclaimed, nothing running.
    await expect(page.locator('[data-claim-state]')).toHaveAttribute('data-claim-state', 'none')
    await expect(page.getByText('Preparing', { exact: true }).first()).toBeVisible()
    const queued = await readJob(page, server, pinnedId)
    expect(queued.state).toBe('preparing')
    expect(queued.claim_state).toBeNull()
    expect(queued.machine_ref).toBe(server.deviceId)
    expect(queued.repository_ref).toBe('orbit')
    expect(queued.task_id).toBe(task.id)
    // An unclaimed job is waiting for its executor, not for a person.
    expect((await rowFor(page, server, pinnedId))?.next_action).toBe('await_executor')

    // The executor side. One clock for the job, starting at its own creation.
    const clock = jobClock(queued.created_at_ms)
    const claimToken = `claim-${pinnedId}`
    const claimed = await claim(page, server, {
      jobId: pinnedId,
      expectedRevision: 1,
      claimToken,
      atMs: clock.next(),
    })
    const launched = await reportLaunch(page, server, {
      jobId: pinnedId,
      claimToken,
      sessionRef,
      atMs: clock.next(),
    })
    const observed = await observe(page, server, {
      jobId: pinnedId,
      expectedRevision: launched.revision,
      claimToken,
      sessionRef,
      activity: 'working',
      atMs: clock.next(),
    })
    expect(claimed.revision).toBe(2)
    expect(observed.job_id).toBe(pinnedId)

    await refreshJobPage(page)
    await expect(page.locator('[data-claim-state]')).toHaveAttribute('data-claim-state', 'confirmed')
    await expect(page.getByText('Running', { exact: true }).first()).toBeVisible()
    expect((await readJob(page, server, pinnedId)).state).toBe('running')

    const handoffId = `h-${pinnedId}`
    const payload = {
      summary: 'Claimed the queued job, launched the executor session and reported back.',
      validation: 'the launch receipt and the observation are on record',
      next_steps: ['review the launch receipt', 'confirm the task did not move'],
    }
    await submitHandoff(page, server, {
      jobId: pinnedId,
      handoffId,
      claimToken,
      sessionRef,
      payload,
      atMs: clock.next(),
    })
    // Evidence alone is not a review, and it never moves the task.
    expect((await readTaskRecord(page, server, task.id)).status).toBe('ready')
    expect(await readReview(page, server, pinnedId)).toBeNull()

    await review(page, server, {
      jobId: pinnedId,
      handoffId,
      decision: 'approved',
      reason: 'The launch, claim and handoff are on record; the task is untouched.',
      atMs: clock.next(),
    })

    // The person's view of the finished cycle.
    await refreshJobPage(page)
    await expect(page.getByText('Running', { exact: true }).first()).toBeVisible()
    await expect(page.locator('[data-claim-state]')).toHaveAttribute('data-claim-state', 'confirmed')
    await expect(page.locator('ul[aria-label="Handoffs"]')).toContainText(payload.summary)
    const reviewRegion = page.getByRole('region', { name: 'Review' })
    await expect(reviewRegion).toContainText('Approved')
    await expect(reviewRegion).toContainText('Task not completed')
    await expect(reviewRegion).toContainText('the task is still Ready')
    // The frozen brief the executor was given carries the objective.
    const resumeContext = page.getByRole('region', { name: 'Resume context' })
    await expect(resumeContext).toContainText(objective)

    // Stored state, read back from the same server.
    const stored = await readReview(page, server, pinnedId)
    expect(stored?.decision).toBe('approved')
    expect(stored?.task_completed).toBe(false)
    const handoffs = await readHandoffs(page, server, pinnedId)
    expect(handoffs.map((entry) => entry.handoff_id)).toEqual([handoffId])
    expect(handoffs[0].payload).toMatchObject({ summary: payload.summary })
    const context = await readResumeContext(page, server, pinnedId)
    expect(context.original_brief.objective).toBe(objective)
    expect(context.review?.decision).toBe('approved')
    expect(context.task_completion_inferred).toBe(false)

    // The task never moved: not in the drawer, not in the record.
    expect((await readTaskRecord(page, server, task.id)).status).toBe('ready')
    await goto(page, server, `/projects/${task.projectId}?task=${task.id}`)
    const revisited = drawerFor(page, task.id)
    await expect(revisited.locator('header').getByText('Ready', { exact: true })).toBeVisible()
    // The launcher is gone because the task now has an active job.
    await expect(revisited.locator('[data-agent-job="' + pinnedId + '"]')).toBeVisible()
    await expect(revisited.locator('[data-launch-agent-job]')).toHaveCount(0)
  })

  test('with every enrolled executor revoked both launch triggers are disabled and nothing is posted', async ({
    page,
    server,
  }) => {
    /**
     * The harness always enrols its browser device, so a live session would
     * otherwise always contribute one *active* executor to the roster — the
     * roster is "every enrolled device except the server's own identity", and a
     * session's device is re-checked for liveness on every request. To reach the
     * server's own "every enrolled executor is revoked" answer honestly, the
     * session is a real enrolled device whose id the authority reserves for
     * itself, and the one real executor is revoked through the admin key.
     *
     * The enrolment has to happen before the first system-authored change: that
     * change inserts the reserved row with `INSERT OR IGNORE` (revoked, unusable
     * token), so an earlier real enrolment keeps the id signable. Everything
     * below still goes through the public admin/authoritative routes — no SQL,
     * no patched binary, no mock.
     */
    const reserved = 'orbit-server'
    const reservedToken = 'k-only-session-device-token-0123456789abcdef'
    await enrollDeviceWithAdminKey(server, { deviceId: reserved, token: reservedToken })
    const fixture = await seedWorkspace(server)
    await page.clock.setFixedTime(new Date(fixture.nowMs))
    const task = eligibleTask(fixture)
    await revokeDevice(server, server.deviceId)

    const starts: string[] = []
    page.on('request', (request) => {
      if (request.method() !== 'POST') return
      if (new URL(request.url()).pathname.endsWith('/agent-jobs/start')) starts.push(request.url())
    })

    await goto(page, server, '/agents', { deviceId: reserved, token: reservedToken })

    // The server's answer: one enrolled device, revoked, so no launch at all.
    const executors = await readExecutors(page, server)
    expect(executors.map((executor) => executor.device_id)).not.toContain(reserved)
    expect(executors.filter((executor) => executor.active)).toHaveLength(0)
    expect(executors.find((executor) => executor.device_id === server.deviceId)?.active).toBe(false)

    const panel = page.locator('[data-agent-executors]')
    await expect(panel).toBeVisible()
    await expect(panel.locator(`[data-executor="${server.deviceId}"]`)).toHaveAttribute(
      'data-executor-state',
      'revoked',
    )
    await expect(panel.locator('[data-executor-empty]')).toHaveCount(0)
    await expect(panel.locator('[data-executor-unsupported]')).toHaveCount(0)
    await expect(panel.locator('[data-launch-blocked]')).toContainText(
      'Every enrolled executor is revoked',
    )
    const panelTrigger = panel.locator('[data-launch-agent-job]')
    await expect(panelTrigger).toBeDisabled()
    await expect(panelTrigger).toHaveAttribute('aria-describedby', 'agents-launch-blocked')
    await expect(page.locator('#agents-launch-blocked')).toContainText(
      'Every enrolled executor is revoked',
    )

    // The task drawer offers the same control, disabled for the same reason.
    await goto(page, server, `/projects/${task.projectId}?task=${task.id}`, {
      deviceId: reserved,
      token: reservedToken,
    })
    const drawer = drawerFor(page, task.id)
    await expect(drawer).toBeVisible()
    const drawerTrigger = drawer.locator('[data-launch-agent-job]')
    await expect(drawerTrigger).toBeDisabled()
    await expect(drawer.locator('[data-launch-blocked]')).toContainText(
      'Every enrolled executor is revoked',
    )

    // Nothing was sent: no dialog opened, no start request, no stored job.
    await expect(page.locator('[data-launch-dialog]')).toHaveCount(0)
    expect(starts).toEqual([])
    expect(await readOverview(page, server)).toHaveLength(0)
    expect(await rowFor(page, server, jobId('never-posted'))).toBeUndefined()
  })

  test('an unknown executor is refused before a job exists', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = eligibleTask(fixture)
    await goto(page, server, '/agents')
    const before = await readOverview(page, server)

    const refused = await postAuthority(page, server, 'agent-jobs/start', {
      job_id: jobId('unknown-executor'),
      task_id: task.id,
      agent_kind: 'codex',
      machine_ref: 'nobody',
      repository_ref: 'orbit',
      worktree: false,
      at_ms: Date.now(),
    })
    expect(refused.status, 'an unenrolled machine is a client error').toBe(400)
    expect(refused.body).toEqual({ error: 'unknown agent executor' })

    // No row was created, directly and through the person's overview.
    const after = await readOverview(page, server)
    expect(after.map((row) => row.job_id)).toEqual(before.map((row) => row.job_id))
    expect(after.map((row) => row.job_id)).not.toContain(jobId('unknown-executor'))

    await goto(page, server, '/agents')
    await expect(page.locator(`[data-agent-job="${jobId('unknown-executor')}"]`)).toHaveCount(0)
    await expect(page.locator('[data-agent-job^="job-k-"]')).toHaveCount(0)
  })

  test('a refused launch retries the same job id and creates exactly one job', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = eligibleTask(fixture)
    const failedId = jobId('pinned-retry')

    const drawer = await openTaskDrawer(page, server, task)
    const before = await readOverview(page, server)
    expect(before.map((row) => row.job_id)).not.toContain(failedId)
    await drawer.locator('[data-launch-agent-job]').click()
    const dialog = page.locator('[data-launch-dialog]')
    await expect(dialog).toBeVisible()
    await dialog.locator('#agent-launch-objective').fill('Retry this launch without queueing twice.')
    await dialog.locator('#agent-launch-repository').fill('orbit')
    await dialog.getByRole('button', { name: 'Review and queue' }).click()
    await expect(dialog).toHaveAttribute('data-launch-step', 'confirm')
    const pinnedId = (await dialog.locator('[data-launch-job-id]').textContent())?.trim() ?? ''
    expect(pinnedId.length).toBeGreaterThan(0)

    // Another client makes the task ineligible between the confirmation and the
    // send. The dialog cannot know; the server must refuse, and the tool must
    // keep the launch identity for the retry.
    const moved = await postAuthority(page, server, 'task-commands', {
      command_id: `k-block-${task.id}`,
      expected_revision: await authorityRevision(page, server),
      action: { type: 'change_status', task_id: task.id, status: 'blocked' },
    })
    expect(moved.status, 'moving the task out of a launchable status').toBe(200)

    await dialog.getByRole('button', { name: 'Queue the job' }).click()
    const refusal = dialog.locator('[data-launch-refusal]')
    await expect(refusal).toContainText('task not eligible for connected start')
    await expect(dialog.getByRole('button', { name: 'Retry the same launch' })).toBeVisible()
    // The refused attempt stored nothing at all.
    expect((await readOverview(page, server)).map((row) => row.job_id)).not.toContain(pinnedId)
    await expect(dialog.locator('[data-launch-job-id]')).toHaveText(pinnedId)

    // Make it eligible again and retry: same key, same job id, one job row.
    const unblocked = await postAuthority(page, server, 'task-commands', {
      command_id: `k-unblock-${task.id}`,
      expected_revision: await authorityRevision(page, server),
      action: { type: 'change_status', task_id: task.id, status: 'ready' },
    })
    expect(unblocked.status).toBe(200)

    await dialog.getByRole('button', { name: 'Retry the same launch' }).click()
    await page.waitForURL((url) => url.pathname === `/agents/${pinnedId}`, { timeout: 20_000 })
    await expect(page.locator('[data-claim-state]')).toHaveAttribute('data-claim-state', 'none')

    const rows = (await readOverview(page, server)).filter((row) => row.task_id === task.id)
    expect(rows, 'a retried launch must never queue a second job').toHaveLength(1)
    expect(rows[0].job_id).toBe(pinnedId)
    expect((await readOverview(page, server)).map((row) => row.job_id)).toContain(pinnedId)
    expect(await readJob(page, server, pinnedId)).toMatchObject({
      id: pinnedId,
      task_id: task.id,
      state: 'preparing',
      claim_state: null,
    })
  })

  test('a job nobody has claimed is awaiting an executor, not attention', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = eligibleTask(fixture)
    const id = jobId('awaiting')
    await goto(page, server, '/agents')

    await startJob(page, server, {
      jobId: id,
      taskId: task.id,
      machineRef: server.deviceId,
      repositoryRef: 'orbit',
      atMs: fixture.nowMs,
    })

    const rows = await readOverview(page, server)
    const mine = rows.find((row) => row.job_id === id)
    expect(mine, 'the started job must be in this device\'s overview').toBeTruthy()
    expect(mine).toMatchObject({
      state: 'preparing',
      next_action: 'await_executor',
      needs_attention: false,
    })
    const awaiting = rows.filter((row) => row.next_action === 'await_executor' && !row.needs_attention)
    expect(awaiting.map((row) => row.job_id)).toContain(id)

    await goto(page, server, '/agents')
    const section = page.locator('[data-awaiting-executor]')
    await expect(section).toBeVisible()
    await expect(section.getByRole('heading', { name: 'Awaiting an executor' })).toBeVisible()
    await expect(section.getByText(/Nothing is running/)).toBeVisible()
    await expect(section.locator(`[data-agent-job="${id}"]`)).toBeVisible()
    // Every awaiting job is listed once, and none of them is attention.
    await expect(section.locator('ul[aria-label="Awaiting an executor"] > li')).toHaveCount(
      awaiting.length,
    )
    await expect(
      page.locator(`[aria-label="Needs attention"] [data-agent-job="${id}"]`),
    ).toHaveCount(0)
    // The two counts on the page are the server's own totals.
    const stats = page.locator('dl').first()
    await expect(stats.locator('dd').nth(1)).toHaveText(
      String(rows.filter((row) => row.needs_attention).length),
    )
    await expect(stats.locator('dd').nth(2)).toHaveText(String(awaiting.length))

    // The job page says the same thing, in words a person can act on.
    await page.locator(`[data-agent-job="${id}"]`).first().click()
    await expect(page).toHaveURL(new RegExp(`/agents/${id}$`))
    await expect(page.locator('[data-claim-state]')).toHaveAttribute('data-claim-state', 'none')
    await expect(page.getByText('Waiting for an executor').first()).toBeVisible()
    await expect(page.locator('[data-claim-state="none"]')).toContainText(
      'No executor has claimed this job',
    )
  })

  test('the job, its claim, handoff and review survive a server restart', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = eligibleTask(fixture)
    const id = jobId('restart')
    const sessionRef = 'session-k-restart'
    const claimToken = `claim-${id}`
    const handoffId = `h-${id}`
    const payload = {
      summary: 'Recorded a handoff that has to outlive the process.',
      validation: 'the claim, the observation and the review are all durable rows',
      next_steps: ['restart the server', 'read the same job back'],
    }

    await signInViaApi(page, server)
    const started = await startJob(page, server, {
      jobId: id,
      taskId: task.id,
      machineRef: server.deviceId,
      repositoryRef: 'orbit',
      objective: 'Prove the launch record is durable.',
      atMs: fixture.nowMs,
    })
    const queued = await readJob(page, server, id)
    expect(queued.created_at_ms).toBe(fixture.nowMs)
    const clock = jobClock(queued.created_at_ms)
    await claim(page, server, {
      jobId: id,
      expectedRevision: started.revision,
      claimToken,
      atMs: clock.next(),
    })
    const launched = await reportLaunch(page, server, {
      jobId: id,
      claimToken,
      sessionRef,
      atMs: clock.next(),
    })
    await observe(page, server, {
      jobId: id,
      expectedRevision: launched.revision,
      claimToken,
      sessionRef,
      activity: 'working',
      atMs: clock.next(),
    })
    await submitHandoff(page, server, {
      jobId: id,
      handoffId,
      claimToken,
      sessionRef,
      payload,
      atMs: clock.next(),
    })
    await review(page, server, {
      jobId: id,
      handoffId,
      decision: 'approved',
      reason: 'Everything the restart has to keep is already on record.',
      atMs: clock.next(),
    })

    const before = await readJob(page, server, id)
    expect(before).toMatchObject({ state: 'running', claim_state: 'confirmed' })

    // Stop the process and start a new one on the same database.
    await server.restart()
    // Web sessions live in memory, so sign in again with the same credential.
    await signInViaApi(page, server)

    const after = await readJob(page, server, id)
    expect(after.state).toBe('running')
    expect(after.claim_state).toBe('confirmed')
    expect(after.runtime_session_ref).toBe(sessionRef)
    expect(after.revision).toBe(before.revision)
    expect((await readHandoffs(page, server, id)).map((entry) => entry.handoff_id)).toEqual([
      handoffId,
    ])
    expect((await readReview(page, server, id))?.decision).toBe('approved')
    expect((await readResumeContext(page, server, id)).original_brief.objective).toBe(
      'Prove the launch record is durable.',
    )
    expect((await readOverview(page, server)).map((row) => row.job_id)).toContain(id)

    await page.goto(`${server.base}/agents/${id}`)
    await expect(page.locator('[data-claim-state]')).toHaveAttribute('data-claim-state', 'confirmed')
    await expect(page.getByText('Running', { exact: true }).first()).toBeVisible()
    await expect(page.locator('ul[aria-label="Handoffs"]')).toContainText(payload.summary)
    await expect(page.getByRole('region', { name: 'Review' })).toContainText('Approved')
    expect((await readTaskRecord(page, server, task.id)).status).toBe('ready')
  })
})
