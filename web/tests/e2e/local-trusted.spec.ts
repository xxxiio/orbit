/**
 * M33 local-trusted authentication acceptance.
 *
 * `ORBIT_AUTH_MODE=local-trusted` is an explicit opt-in for a single-user
 * machine: the server binds to loopback, the browser enters without a cookie,
 * and the synthetic local principal can use the product API. Everything that
 * needs a real credential (administrator, operator, executor, enrolled device)
 * keeps refusing, and the same database can be switched back to the default
 * authenticated policy with one `restart({ env })`.
 *
 * This spec starts its own server on purpose (like the TTL-expiry test in
 * `auth.spec.ts`) so a policy change never touches another spec's harness.
 *
 * Scenario A: the workspace is open, carries real work, survives a restart, and
 * revalidates when the policy switches back to authenticated mode.
 * Scenario C: the security boundary — privileged routes, CSRF, `Origin`,
 * `Sec-Fetch-Site` and the loopback `Host` rule.
 *
 * Evidence capture is opt-in: `ORBIT_CAPTURE=1` writes `web/evidence/m33-auth/`.
 */
import { expect, test } from './harness/fixtures'
import { hasSessionCookie, signIn } from './harness/session'
import { startServer, type E2EServer } from './harness/server'
import type { BrowserContext, Locator, Page } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const DAY = 86_400_000
const LOCAL_TRUSTED_ENV = { ORBIT_AUTH_MODE: 'local-trusted' } as const

const PROJECT_NAME = 'Local Trusted Workspace'
const MILESTONE_NAME = 'Local Trusted Milestone'
const TASK_TITLE = 'Ship the local-trusted acceptance lane'
const TASK_DESCRIPTION =
  'The local-trusted policy must open the workspace, keep real work, and never invent a device session.'
const TASK_CRITERION = 'A second browser enters without a cookie.'
const TASK_ESTIMATE_MINUTES = 90
const TASK_PRIORITY = 'Urgent'
const TASK_STATUS = 'Planned'

const CAPTURE = process.env.ORBIT_CAPTURE === '1'
const CAPTURE_DIR = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'evidence',
  'm33-auth',
)

interface TaskRecord {
  id: string
  title: string
  status: string
  project_id: string | null
  milestone_id: string | null
  description: string
  priority: string | null
  estimated_minutes: number | null
  deadline_at_ms: number | null
  earliest_start_at_ms: number | null
}

interface MilestoneRecord {
  id: string
  name: string
  start_at_ms: number | null
  target_at_ms: number | null
  task_ids: string[]
}

interface CriterionRecord {
  text: string
  completed: boolean
  position: number
}

function dateInputValue(ms: number): string {
  const date = new Date(ms)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function dateTimeInputValue(ms: number): string {
  const date = new Date(ms)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`
}

function localInstant(value: string): number {
  const [datePart, timePart] = value.split('T')
  const [year, month, day] = datePart.split('-').map(Number)
  const [hour, minute] = timePart.split(':').map(Number)
  return new Date(year, month - 1, day, hour, minute, 0, 0).getTime()
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function projectIdFromUrl(page: Page): string {
  const match = new URL(page.url()).pathname.match(/^\/projects\/([^/?#]+)/)
  if (!match) throw new Error(`not on a project URL: ${page.url()}`)
  return decodeURIComponent(match[1])
}

async function startLocalTrusted(): Promise<E2EServer> {
  return startServer({ env: { ...LOCAL_TRUSTED_ENV } })
}

async function authoritative<T>(page: Page, server: E2EServer, route: string): Promise<T> {
  const response = await page.request.get(`${server.base}${route}`)
  expect(response.status(), `GET ${route}`).toBe(200)
  return (await response.json()) as T
}

async function tasksNamed(
  page: Page,
  server: E2EServer,
  projectId: string,
  title: string,
): Promise<TaskRecord[]> {
  const body = await authoritative<{ tasks: TaskRecord[] }>(page, server, '/v1/authoritative/tasks')
  return body.tasks.filter((task) => task.project_id === projectId && task.title === title)
}

async function milestonesNamed(
  page: Page,
  server: E2EServer,
  name: string,
): Promise<MilestoneRecord[]> {
  const body = await authoritative<{ milestones: MilestoneRecord[] }>(
    page,
    server,
    '/v1/authoritative/milestones',
  )
  return body.milestones.filter((milestone) => milestone.name === name)
}

async function createProject(page: Page, server: E2EServer, name: string): Promise<string> {
  await page.goto(`${server.base}/projects`)
  await expect(page.getByRole('heading', { name: 'Projects', level: 1 })).toBeVisible()
  await page.getByRole('button', { name: /new project/i }).first().click()
  const dialog = page.getByRole('dialog')
  await expect(dialog).toBeVisible()
  await dialog.getByLabel(/project name/i).fill(name)
  await dialog.getByRole('button', { name: /create project/i }).click()

  await expect(page).toHaveURL(/\/projects\/[^/?#]+/)
  await expect(page.getByRole('heading', { name, level: 1 })).toBeVisible()
  return projectIdFromUrl(page)
}

async function createMilestone(
  page: Page,
  server: E2EServer,
  projectId: string,
  name: string,
  start: string,
  target: string,
): Promise<string> {
  await page.goto(`${server.base}/projects/${projectId}/milestones`)
  await page.getByRole('button', { name: 'New milestone' }).first().click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByRole('heading', { name: 'New milestone' })).toBeVisible()
  await dialog.getByLabel('Name').fill(name)
  await dialog.getByLabel('Start date').fill(start)
  await dialog.getByLabel('Target date').fill(target)
  await dialog.getByRole('button', { name: 'Create milestone' }).click()

  await expect(page.getByText('Milestone created', { exact: true })).toBeVisible()
  await expect(dialog).toBeHidden()
  const dismiss = page.getByRole('button', { name: 'Dismiss notification' })
  if ((await dismiss.count()) > 0) await dismiss.first().click()

  const [record] = await milestonesNamed(page, server, name)
  expect(record, 'the server must hold the created milestone').toBeTruthy()
  return record.id
}

async function createFullyConfiguredTask(
  page: Page,
  server: E2EServer,
  projectId: string,
  projectName: string,
  milestoneName: string,
  deadline: string,
): Promise<string> {
  await page.goto(`${server.base}/projects/${projectId}/tasks`)
  await expect(page.locator('table')).toBeVisible()
  await page.getByRole('main').getByRole('button', { name: 'New task', exact: true }).first().click()
  const dialog = page.getByRole('dialog')
  await expect(dialog.getByRole('heading', { name: 'New task' })).toBeVisible()
  // Opened from the project, so its project is already chosen.
  await expect(dialog.getByLabel('Project for the new task')).toContainText(projectName)

  await dialog.getByLabel('Title').fill(TASK_TITLE)
  await dialog.getByLabel('Description').fill(TASK_DESCRIPTION)
  await dialog.getByLabel('Starting status').click()
  await page.getByRole('option', { name: TASK_STATUS, exact: true }).click()
  await dialog.getByLabel('Task priority').click()
  await page.getByRole('option', { name: TASK_PRIORITY, exact: true }).click()
  // Assigning the milestone in the same create command.
  await dialog.getByLabel('Task milestone').click()
  await page.getByRole('option', { name: milestoneName, exact: true }).click()
  await dialog.getByLabel('Deadline').fill(deadline)

  await dialog.getByRole('button', { name: /Advanced/ }).click()
  await dialog.getByLabel('Estimate (minutes)').fill(String(TASK_ESTIMATE_MINUTES))
  await dialog.getByRole('button', { name: 'Add criterion' }).click()
  await dialog
    .getByRole('textbox', { name: 'Acceptance criterion 1', exact: true })
    .fill(TASK_CRITERION)

  await dialog.getByRole('button', { name: 'Create task' }).click()
  await expect(page.getByText('Task created', { exact: true })).toBeVisible()
  await expect(dialog).toBeHidden()

  const [created] = await tasksNamed(page, server, projectId, TASK_TITLE)
  expect(created, 'exactly one task carries the title').toBeTruthy()
  return created.id
}

function taskRow(page: Page, id: string): Locator {
  return page.locator(`[data-roadmap-row="task"][data-roadmap-id="${id}"]`)
}

function milestoneRow(page: Page, id: string): Locator {
  return page.locator(`[data-roadmap-row="milestone"][data-roadmap-id="${id}"]`)
}

test.describe('local trusted', () => {
  test('opens without a sign-in, keeps real work across a restart, and revalidates on policy switch', async ({
    page,
    browser,
  }) => {
    test.setTimeout(180_000)
    const server = await startLocalTrusted()
    try {
      // The policy is announced in the log, not inferred from the peer address.
      const startupLog = await server.logTail(40)
      expect(startupLog).toContain('Authentication: Local trusted mode')
      expect(startupLog).toContain('Access: Local machine only')

      // A fresh browser enters the workspace directly.
      await page.goto(`${server.base}/`)
      await expect(page).not.toHaveURL(/\/login/)
      await expect(page).toHaveTitle('Today · Orbit')
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible()
      await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toHaveCount(0)
      await expect(page.getByText('Local trusted mode')).toBeVisible()
      // The footer is a policy note: no device identity and nothing to sign out of.
      await expect(page.getByRole('button', { name: 'Sign out' })).toHaveCount(0)
      await expect(page.getByText(server.deviceId)).toHaveCount(0)
      expect(await hasSessionCookie(page.context(), server)).toBe(false)

      // `/login` cannot trap a browser the server already trusts.
      await page.goto(`${server.base}/login`)
      await expect(page).not.toHaveURL(/\/login/)
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible()

      // Real work: a project, a dated milestone, a fully configured task.
      const projectId = await createProject(page, server, PROJECT_NAME)
      const start = dateInputValue(Date.now() + 10 * DAY)
      const target = dateInputValue(Date.now() + 40 * DAY)
      const milestoneId = await createMilestone(
        page,
        server,
        projectId,
        MILESTONE_NAME,
        start,
        target,
      )
      const deadline = dateTimeInputValue(Date.now() + 25 * DAY)
      const taskId = await createFullyConfiguredTask(
        page,
        server,
        projectId,
        PROJECT_NAME,
        MILESTONE_NAME,
        deadline,
      )

      // The task carries the milestone and every authored field, with no
      // follow-up edit command.
      await page.getByRole('button', { name: TASK_TITLE, exact: true }).first().click()
      const drawer = page.getByRole('dialog')
      await expect(drawer.getByRole('heading', { name: TASK_TITLE })).toBeVisible()
      await expect(drawer.getByLabel('Task milestone')).toContainText(MILESTONE_NAME)
      await expect(drawer.getByLabel('Task status')).toContainText(TASK_STATUS)
      await expect(drawer.getByLabel('Task priority')).toContainText(TASK_PRIORITY)
      await expect(drawer.getByLabel('Task estimate in minutes')).toHaveValue(
        String(TASK_ESTIMATE_MINUTES),
      )
      await expect(drawer.getByLabel('Task deadline')).toHaveValue(deadline)
      await expect(drawer.getByLabel('Task description')).toHaveValue(TASK_DESCRIPTION)
      await expect(
        drawer.getByRole('list', { name: /Acceptance criteria for task/ }),
      ).toContainText(TASK_CRITERION)
      await drawer.getByRole('button', { name: 'Close task details' }).click()
      await expect(drawer).toBeHidden()

      const [storedTask] = await tasksNamed(page, server, projectId, TASK_TITLE)
      expect(storedTask).toMatchObject({
        status: 'planned',
        project_id: projectId,
        milestone_id: milestoneId,
        priority: 'urgent',
        estimated_minutes: TASK_ESTIMATE_MINUTES,
        deadline_at_ms: localInstant(deadline),
      })
      const [storedMilestone] = await milestonesNamed(page, server, MILESTONE_NAME)
      expect(storedMilestone.task_ids).toContain(taskId)

      // List -> Board -> Backlog are URL-backed views over the same rows.
      await page.getByRole('button', { name: 'Board', exact: true }).click()
      await expect(page).toHaveURL(/[?&]view=board/)
      await expect(page.getByRole('group', { name: 'Task board' })).toBeVisible()
      await expect(page.locator(`[data-board-card="${taskId}"]`)).toContainText(TASK_TITLE)

      await page.getByRole('button', { name: 'Backlog', exact: true }).click()
      await expect(page).toHaveURL(/[?&]view=backlog/)
      await expect(page.getByRole('heading', { name: 'Backlog', exact: true })).toBeVisible()
      await expect(page.getByRole('button', { name: TASK_TITLE, exact: true })).toBeVisible()

      await page.getByRole('button', { name: 'List', exact: true }).click()
      await expect(page.locator('table')).toBeVisible()
      await expect(page.getByRole('button', { name: TASK_TITLE, exact: true })).toBeVisible()

      // The milestone draws its authored start+target window on the roadmap.
      await page.goto(`${server.base}/projects/${projectId}/roadmap`)
      const bar = milestoneRow(page, milestoneId)
      await expect(bar).toBeVisible()
      await expect(
        bar.getByRole('button', {
          name: new RegExp(`^${escapeRegExp(MILESTONE_NAME)}, .*open milestone$`),
        }),
      ).toBeVisible()
      await expect(taskRow(page, taskId)).toBeVisible()

      // A reload keeps the same workspace with no prompt.
      await page.reload()
      await expect(milestoneRow(page, milestoneId)).toBeVisible()
      await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toHaveCount(0)

      // A second, independent cookie jar enters directly too.
      const second = await browser.newContext({ baseURL: server.base })
      try {
        const secondPage = await second.newPage()
        await secondPage.goto(`${server.base}/`)
        await expect(secondPage).toHaveTitle('Today · Orbit')
        await expect(secondPage.getByRole('heading', { level: 1 })).toBeVisible()
        await expect(secondPage).not.toHaveURL(/\/login/)
        expect(await hasSessionCookie(second, server)).toBe(false)
      } finally {
        await second.close()
      }

      // Same database, same policy: durable work survives a real process restart.
      await server.restart()
      await page.reload()
      await expect(milestoneRow(page, milestoneId)).toBeVisible()
      await expect(taskRow(page, taskId)).toBeVisible()

      const [taskAfterRestart] = await tasksNamed(page, server, projectId, TASK_TITLE)
      expect(taskAfterRestart?.id, 'the task survives a restart').toBe(taskId)
      const [milestoneAfterRestart] = await milestonesNamed(page, server, MILESTONE_NAME)
      expect(milestoneAfterRestart?.id, 'the milestone survives a restart').toBe(milestoneId)

      await page.goto(`${server.base}/projects`)
      await expect(
        page.getByRole('link', { name: new RegExp(`^${escapeRegExp(PROJECT_NAME)}`) }).first(),
      ).toBeVisible()

      // The session contract is a policy answer, not a cookie.
      const session = await page.request.get(`${server.base}/v1/web/session`)
      expect(session.status()).toBe(200)
      const sessionBody = (await session.json()) as {
        mode: string
        device_id: string
        display_name: string | null
        expires_at_ms: number | null
      }
      expect(sessionBody).toEqual({
        mode: 'local-trusted',
        device_id: 'local-trusted',
        display_name: 'Local browser',
        expires_at_ms: null,
      })
      expect(session.headers()['cache-control']).toBe('no-store')
      expect(session.headers()['set-cookie']).toBeUndefined()
      expect(await hasSessionCookie(page.context(), server)).toBe(false)

      // Policy switch on the same database: the cached local-trusted answer is
      // stale immediately, so a reload lands on the sign-in form.
      await server.restart({ env: { ORBIT_AUTH_MODE: '' } })
      expect(await server.logTail(20)).toContain('Authentication: Device authentication')

      await page.reload()
      await expect(page).toHaveURL(/\/login/, { timeout: 15_000 })
      await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toBeVisible()
      await expect(page.getByRole('complementary', { name: 'Workspace' })).toHaveCount(0)
      await expect(page.getByText('Local trusted mode')).toHaveCount(0)
      expect(await hasSessionCookie(page.context(), server)).toBe(false)

      const afterSwitch = await page.request.get(`${server.base}/v1/web/session`)
      expect(afterSwitch.status()).toBe(401)

      // Forwarded loopback claims are not a credential in authenticated mode.
      const forwarded = await page.request.get(`${server.base}/v1/authoritative/snapshot`, {
        headers: { 'x-forwarded-for': '203.0.113.9', 'x-real-ip': '127.0.0.1' },
      })
      expect(forwarded.status()).toBe(401)

      // Signing in with the enrolled harness device proves the switch kept the
      // data instead of replacing the database.
      await signIn(page, server)
      await page.goto(`${server.base}/projects`)
      await expect(
        page.getByRole('link', { name: new RegExp(`^${escapeRegExp(PROJECT_NAME)}`) }).first(),
      ).toBeVisible()
      const [taskAfterSwitch] = await tasksNamed(page, server, projectId, TASK_TITLE)
      expect(taskAfterSwitch?.id, 'the same database still holds the task').toBe(taskId)
    } finally {
      await server.stop()
    }
  })

  test('still refuses privileged credentials, CSRF, foreign origins and foreign hosts', async ({
    page,
  }) => {
    const server = await startLocalTrusted()
    try {
      const api = page.request
      const csrf = { 'x-orbit-csrf': '1' }

      // Administrator: enrolling a device needs the admin key, not local trust.
      const enroll = await api.post(`${server.base}/v1/devices`, {
        headers: csrf,
        data: { device_id: 'local-trusted-probe', token: 'local-trusted-probe-token-0123456789' },
      })
      expect(enroll.status()).toBe(403)
      expect(await enroll.text()).toContain('administrator credential required')

      // Operator: capability install/authorize/revoke are not browser actions.
      const install = await api.post(`${server.base}/v1/authoritative/capabilities/install`, {
        headers: csrf,
        data: { command_id: 'install-probe', capability_id: 'orbit.reference.work-item-note' },
      })
      expect(install.status()).toBe(403)
      expect(await install.text()).toContain('operator credential required')

      // Executor: announcing capabilities needs an executor credential.
      const announce = await api.post(
        `${server.base}/v1/authoritative/capability-executors/announce`,
        { headers: csrf, data: { capability_ids: [] } },
      )
      expect(announce.status()).toBe(403)
      expect(await announce.text()).toContain('executor credential required')

      // Enrolled device: machine transports never accept the local principal.
      const claim = await api.post(`${server.base}/v1/authoritative/agent-machines/claim`, {
        headers: csrf,
        data: {
          job_id: 'job-probe',
          expected_revision: 0,
          claim_token: 'token',
          at_ms: 1000,
        },
      })
      expect(claim.status()).toBe(403)
      expect(await claim.text()).toContain('enrolled device credential required')

      // Product mutations need browser proof even without a cookie…
      const projectCommand = {
        command_id: 'csrf-probe',
        expected_revision: 0,
        project_id: 'p-csrf-probe',
        name: 'CSRF probe',
      }
      const noHeader = await api.post(`${server.base}/v1/authoritative/project-commands`, {
        data: projectCommand,
      })
      expect(noHeader.status()).toBe(403)
      expect(await noHeader.text()).toContain('csrf check failed')

      // …a cross-site fetch is refused…
      const crossSite = await api.post(`${server.base}/v1/authoritative/project-commands`, {
        headers: { ...csrf, 'sec-fetch-site': 'cross-site' },
        data: projectCommand,
      })
      expect(crossSite.status()).toBe(403)

      // …and an Origin that is not this Host is refused.
      const foreignOrigin = await api.post(`${server.base}/v1/authoritative/project-commands`, {
        headers: { ...csrf, origin: 'https://evil.example' },
        data: projectCommand,
      })
      expect(foreignOrigin.status()).toBe(403)

      // Control: the same command with the browser proof is accepted, so the
      // refusals above are the guard and not a broken payload.
      const accepted = await api.post(`${server.base}/v1/authoritative/project-commands`, {
        headers: csrf,
        data: projectCommand,
      })
      expect(accepted.status()).toBe(200)

      // The Host header is the DNS-rebinding/reverse-proxy boundary. Playwright
      // request contexts forward an explicit Host verbatim, so this is still the
      // real HTTP listener answering (no browser rewrite).
      const foreignHost = await api.get(`${server.base}/v1/authoritative/projects`, {
        headers: { host: 'public.example.com' },
      })
      expect(foreignHost.status()).toBe(403)
      expect(await foreignHost.text()).toContain('host not allowed in local trusted mode')

      const rebindingHost = await api.get(`${server.base}/v1/authoritative/projects`, {
        headers: { host: '127.0.0.1.nip.io' },
      })
      expect(rebindingHost.status()).toBe(403)
      expect(await rebindingHost.text()).toContain('host not allowed in local trusted mode')

      // Forwarded headers are data, not authority: local mode still allows the
      // product read (the authenticated half is asserted after the policy switch).
      const forwarded = await api.get(`${server.base}/v1/authoritative/projects`, {
        headers: { 'x-forwarded-for': '203.0.113.9', 'x-real-ip': '127.0.0.1' },
      })
      expect(forwarded.status()).toBe(200)

      // …and the local-trusted session is always a no-cookie policy answer.
      const session = await api.get(`${server.base}/v1/web/session`)
      expect(session.status()).toBe(200)
      expect(session.headers()['set-cookie']).toBeUndefined()
      expect(await hasSessionCookie(page.context(), server)).toBe(false)
    } finally {
      await server.stop()
    }
  })
})

/* ------------------------------------------------------------------------- *
 * Evidence capture (opt-in).
 * ------------------------------------------------------------------------- */

async function settle(page: Page): Promise<void> {
  await page.addStyleTag({
    content:
      '*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}',
  })
  await page.evaluate(() => document.fonts.ready)
  await page.waitForTimeout(450)
}

async function shot(page: Page, name: string): Promise<void> {
  await settle(page)
  await page.screenshot({ path: path.join(CAPTURE_DIR, `${name}.png`) })
}

test.describe('local trusted evidence', () => {
  test.skip(!CAPTURE, 'set ORBIT_CAPTURE=1 to capture committed evidence')
  test.setTimeout(300_000)

  test('captures workspace, sidebar and roadmap for light and dark at both sizes', async ({
    browser,
  }) => {
    await mkdir(CAPTURE_DIR, { recursive: true })
    const server = await startLocalTrusted()
    const errors: string[] = []

    const watch = (page: Page) => {
      page.on('pageerror', (error) => errors.push(`pageerror ${page.url()}: ${error.message}`))
      page.on('console', (message) => {
        if (message.type() === 'error') errors.push(`console ${page.url()}: ${message.text()}`)
      })
    }

    const contexts: BrowserContext[] = []
    try {
      // Real content first: a project, its milestone and a configured task.
      const setupContext = await browser.newContext({
        viewport: { width: 1440, height: 900 },
      })
      contexts.push(setupContext)
      const setupPage = await setupContext.newPage()
      watch(setupPage)
      const projectId = await createProject(setupPage, server, PROJECT_NAME)
      await createMilestone(
        setupPage,
        server,
        projectId,
        MILESTONE_NAME,
        dateInputValue(Date.now() + 10 * DAY),
        dateInputValue(Date.now() + 40 * DAY),
      )
      await createFullyConfiguredTask(
        setupPage,
        server,
        projectId,
        PROJECT_NAME,
        MILESTONE_NAME,
        dateTimeInputValue(Date.now() + 25 * DAY),
      )

      for (const theme of ['light', 'dark'] as const) {
        for (const viewport of [
          { name: '1440x900', width: 1440, height: 900 },
          { name: '390x844', width: 390, height: 844 },
        ] as const) {
          const context = await browser.newContext({
            viewport: { width: viewport.width, height: viewport.height },
            deviceScaleFactor: 1,
            colorScheme: theme,
          })
          await context.addInitScript(
            ([key, value]) => {
              window.localStorage.setItem(key, value)
            },
            ['orbit.theme', theme],
          )
          const page = await context.newPage()
          watch(page)
          contexts.push(context)

          // Workspace: the shell with real work and the policy footer.
          await page.goto(`${server.base}/`)
          await expect(page).toHaveTitle('Today · Orbit')
          await expect(page.getByRole('heading', { level: 1 })).toBeVisible()
          // At phone width the sidebar (and its policy footer) lives in the drawer.
          if (viewport.width >= 1024) {
            await expect(page.getByText('Local trusted mode')).toBeVisible()
          }
          await shot(page, `workspace-local-trusted-${theme}-${viewport.name}`)

          // Sidebar: footer in view on desktop, navigation drawer on a phone.
          if (viewport.width < 1024) {
            await page.getByRole('button', { name: 'Open navigation' }).click()
            const drawer = page.getByRole('dialog')
            await expect(drawer).toBeVisible()
            await expect(drawer.getByText('Local trusted mode')).toBeVisible()
          } else {
            const footer = page.getByText('Local trusted mode')
            await footer.scrollIntoViewIfNeeded()
            await expect(footer).toBeVisible()
          }
          await shot(page, `sidebar-local-trusted-${theme}-${viewport.name}`)

          // Roadmap: the milestone bar drawn from authored dates.
          const [milestone] = await milestonesNamed(page, server, MILESTONE_NAME)
          await page.goto(`${server.base}/projects/${projectId}/roadmap`)
          const row = milestoneRow(page, milestone.id)
          await expect(row).toBeVisible()
          await expect(
            row.getByRole('button', {
              name: new RegExp(`^${escapeRegExp(MILESTONE_NAME)}, .*open milestone$`),
            }),
          ).toBeVisible()
          await shot(page, `roadmap-local-trusted-${theme}-${viewport.name}`)
        }
      }

      expect(errors, 'capture pages must not log errors').toEqual([])
    } finally {
      for (const context of contexts) await context.close()
      await server.stop()
    }
  })
})
