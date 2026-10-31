/**
 * M33 corrective — one full task creation.
 *
 * The full dialog writes title, description, status, priority, milestone,
 * deadline, estimate, acceptance criteria and dependencies in a single
 * `create` action. Every configured value must then be visible in the
 * inspector, the board and the roadmap *without* a follow-up edit command.
 *
 * The second test drops the response of a create the server really applied:
 * pressing Create again must replay the same attempt key, so the retry cannot
 * duplicate the task.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'
import type { Page } from '@playwright/test'

const TITLE = 'Ship the corrected workspace'
const DESCRIPTION =
  'The corrected Tasks tab must be created in one command and read back unchanged.'
const ESTIMATE_MINUTES = 90

const DAY = 86_400_000

interface TaskDetail {
  id: string
  title: string
  status: string
  project_id: string | null
  milestone_id: string | null
  description: string
  priority: string | null
  estimated_minutes: number | null
  earliest_start_at_ms: number | null
  deadline_at_ms: number | null
}

interface CriterionRecord {
  id: string
  text: string
  completed: boolean
  position: number
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

async function tasksNamed(page: Page, server: E2EServer, title: string): Promise<TaskDetail[]> {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks`)
  expect(response.status()).toBe(200)
  const body = (await response.json()) as { tasks: TaskDetail[] }
  return body.tasks.filter((task) => task.title === title)
}

async function taskDetail(page: Page, server: E2EServer, id: string): Promise<TaskDetail> {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks/${id}`)
  expect(response.status()).toBe(200)
  return (await response.json()) as TaskDetail
}

async function criteria(
  page: Page,
  server: E2EServer,
  id: string,
): Promise<CriterionRecord[]> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/tasks/${id}/acceptance-criteria`,
  )
  expect(response.status()).toBe(200)
  return (await response.json()) as CriterionRecord[]
}

async function prerequisitesOf(
  page: Page,
  server: E2EServer,
  taskId: string,
): Promise<string[]> {
  const response = await page.request.get(`${server.base}/v1/authoritative/task-dependencies`)
  expect(response.status()).toBe(200)
  const edges = ((await response.json()) as {
    edges: Array<{ task_id: string; prerequisite_id: string }>
  }).edges
  return edges.filter((edge) => edge.task_id === taskId).map((edge) => edge.prerequisite_id)
}

test.describe('full task creation', () => {
  test('one create command stores every configured value and all three surfaces show it', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const milestone = fixture.milestones.find((candidate) => candidate.id === 'm-dev-release')
    expect(milestone, 'the fixture needs the Release milestone').toBeTruthy()
    // A finished task: it can gate new work without ever creating a cycle.
    const prerequisite = fixture.tasks.find(
      (candidate) => candidate.projectId === 'p-development' && candidate.status === 'done',
    )
    expect(prerequisite, 'the fixture needs a done Development task').toBeTruthy()
    const deadline = dateTimeInputValue(fixture.nowMs + 20 * DAY + 90 * 60_000)

    const writes: string[] = []
    await page.route('**/v1/authoritative/task-commands', async (route) => {
      writes.push(route.request().postData() ?? '')
      await route.continue()
    })

    await goto(page, server, '/projects/p-development/tasks')
    await page.getByRole('main').getByRole('button', { name: 'New task', exact: true }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name: 'New task' })).toBeVisible()
    // Opened from a project, so the project is already chosen.
    await expect(dialog.getByLabel('Project for the new task')).toContainText('Development')

    await dialog.getByLabel('Title').fill(TITLE)
    await dialog.getByLabel('Description').fill(DESCRIPTION)
    await dialog.getByLabel('Starting status').click()
    await page.getByRole('option', { name: 'Planned', exact: true }).click()
    await dialog.getByLabel('Task priority').click()
    await page.getByRole('option', { name: 'Urgent', exact: true }).click()
    await dialog.getByLabel('Task milestone').click()
    await page.getByRole('option', { name: milestone!.name, exact: true }).click()
    await dialog.getByLabel('Deadline').fill(deadline)

    await dialog.getByRole('button', { name: /Advanced/ }).click()
    await dialog.getByLabel('Estimate (minutes)').fill(String(ESTIMATE_MINUTES))
    await dialog.getByLabel('Add a prerequisite').click()
    await page.getByRole('option', { name: prerequisite!.title, exact: true }).click()
    await dialog.getByRole('button', { name: 'Add', exact: true }).click()
    await expect(dialog.getByRole('list', { name: 'Prerequisites for the new task' })).toContainText(
      prerequisite!.title,
    )

    await dialog.getByRole('button', { name: 'Add criterion' }).click()
    await dialog
      .getByRole('textbox', { name: 'Acceptance criterion 1', exact: true })
      .fill('The setup flow accepts one payload.')
    await dialog.getByRole('button', { name: 'Add criterion' }).click()
    await dialog
      .getByRole('textbox', { name: 'Acceptance criterion 2', exact: true })
      .fill('The inspector shows the stored values.')

    await dialog.getByRole('button', { name: 'Create task' }).click()
    await expect(page.getByText('Task created', { exact: true })).toBeVisible()
    await expect(dialog).toBeHidden()

    // One accepted command, no follow-up patch.
    expect(writes, 'the whole draft travels in one create command').toHaveLength(1)
    const payload = JSON.parse(writes[0]) as {
      action: Record<string, unknown>
    }
    expect(payload.action).toMatchObject({
      type: 'create',
      title: TITLE,
      project_id: 'p-development',
      milestone_id: milestone!.id,
      description: DESCRIPTION,
      status: 'planned',
      priority: 'urgent',
      estimated_minutes: ESTIMATE_MINUTES,
      deadline_at_ms: localInstant(deadline),
      acceptance_criteria: [
        'The setup flow accepts one payload.',
        'The inspector shows the stored values.',
      ],
      prerequisite_ids: [prerequisite!.id],
    })

    const [created] = await tasksNamed(page, server, TITLE)
    expect(created, 'exactly one task carries the title').toBeTruthy()
    const record = await taskDetail(page, server, created.id)
    expect(record).toMatchObject({
      status: 'planned',
      project_id: 'p-development',
      milestone_id: milestone!.id,
      description: DESCRIPTION,
      priority: 'urgent',
      estimated_minutes: ESTIMATE_MINUTES,
      deadline_at_ms: localInstant(deadline),
      earliest_start_at_ms: null,
    })
    expect(await criteria(page, server, created.id)).toMatchObject([
      { text: 'The setup flow accepts one payload.', completed: false, position: 0 },
      { text: 'The inspector shows the stored values.', completed: false, position: 1 },
    ])
    expect(await prerequisitesOf(page, server, created.id)).toEqual([prerequisite!.id])

    // Inspector: the toast offers the created task directly, and every value is
    // already there — nothing needs a second edit.
    const openTask = page.getByRole('button', { name: 'Open task', exact: true })
    await expect(openTask).toBeVisible()
    await openTask.click()
    const drawer = page.getByRole('dialog')
    await expect(drawer.getByRole('heading', { name: TITLE })).toBeVisible()
    await expect(drawer.getByLabel('Task status')).toContainText('Planned')
    await expect(drawer.getByLabel('Task priority')).toContainText('Urgent')
    await expect(drawer.getByLabel('Task milestone')).toContainText(milestone!.name)
    await expect(drawer.getByLabel('Task estimate in minutes')).toHaveValue(
      String(ESTIMATE_MINUTES),
    )
    await expect(drawer.getByLabel('Task deadline')).toHaveValue(deadline)
    await expect(drawer.getByLabel('Task description')).toHaveValue(DESCRIPTION)
    await expect(drawer.getByRole('list', { name: /Acceptance criteria for task/ })).toContainText(
      'The setup flow accepts one payload.',
    )
    await expect(drawer.getByText('The inspector shows the stored values.')).toBeVisible()
    await expect(drawer.getByText('0/2 done')).toBeVisible()
    await expect(drawer.getByText(prerequisite!.title).first()).toBeVisible()
    await drawer.getByRole('button', { name: 'Close task details' }).click()

    // Board: the compact card names its milestone, priority and estimate.
    await page.getByRole('button', { name: 'Board', exact: true }).click()
    const card = page.locator(`[data-board-card="${created.id}"]`)
    await expect(card).toBeVisible()
    await expect(card).toContainText(TITLE)
    await expect(card).toContainText('Urgent priority')
    await expect(card).toContainText(milestone!.name)
    await expect(card).toContainText('1 h 30 min')
    // The authored deadline is rendered as a calendar cue on the card (the
    // label is locale-formatted, so only its presence and shape are asserted).
    const deadlineCue = card.locator('[title]')
    await expect(deadlineCue).toHaveCount(1)
    await expect(deadlineCue).toHaveAttribute('title', /,/)

    // Roadmap: the authored deadline places the task, as a checkpoint because
    // only one date was configured.
    await goto(page, server, '/projects/p-development/roadmap')
    const row = page.locator(`[data-roadmap-row="task"][data-roadmap-id="${created.id}"]`)
    await expect(row).toBeVisible()
    const label = await row
      .getByRole('button', { name: new RegExp(`^${TITLE}`) })
      .getAttribute('aria-label')
    expect(label, 'one authored date is a marker, not a window').not.toMatch(/–/)

    // Nothing above needed a second write.
    expect(writes, 'inspecting and reading never patch the created task').toHaveLength(1)
  })

  test('a dropped create response is retried with the same attempt key, not duplicated', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const title = 'Uncertain retry task'

    const writes: string[] = []
    let dropped = false
    await page.route('**/v1/authoritative/task-commands', async (route) => {
      writes.push(route.request().postData() ?? '')
      if (!dropped) {
        dropped = true
        // The server stores the task, then the answer is lost: the browser must
        // replay the same command id and task id instead of minting new ones.
        await route.fetch()
        await route.abort('failed')
        return
      }
      await route.continue()
    })

    await goto(page, server, '/projects/p-basic/tasks')
    await page.getByRole('main').getByRole('button', { name: 'New task', exact: true }).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByLabel('Title').fill(title)
    await dialog.getByRole('button', { name: 'Create task' }).click()

    await expect(page.getByText('The task may already exist')).toBeVisible()
    await expect(dialog).toBeVisible()
    await expect(page.getByText('Task created', { exact: true })).toHaveCount(0)

    // The write really landed before the response was dropped.
    expect(await tasksNamed(page, server, title)).toHaveLength(1)

    await dialog.getByRole('button', { name: 'Create task' }).click()
    await expect(page.getByText('Task created', { exact: true })).toBeVisible()
    await expect(dialog).toBeHidden()
    await page.unroute('**/v1/authoritative/task-commands')

    expect(writes, 'the retry is a replay, not a second create').toHaveLength(2)
    const first = JSON.parse(writes[0]) as { command_id: string; action: Record<string, unknown> }
    const second = JSON.parse(writes[1]) as { command_id: string; action: Record<string, unknown> }
    expect(second.command_id).toBe(first.command_id)
    expect(second.action).toEqual(first.action)

    const stored = await tasksNamed(page, server, title)
    expect(stored, 'one attempt key, one task').toHaveLength(1)
    expect(stored[0].id).toBe(first.action.task_id)
  })
})
