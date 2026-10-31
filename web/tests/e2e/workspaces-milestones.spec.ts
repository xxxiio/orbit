/**
 * M33 corrective — rich milestones.
 *
 * A milestone is a project checkpoint with a description, priority, owner and
 * optional dates. Creation authorises every field in one command; the detail
 * dialog reads progress, child tasks, Work Items and activity from the server;
 * and a milestone that still owns work cannot be deleted.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'
import type { Page } from '@playwright/test'

interface MilestoneRecord {
  id: string
  name: string
  description: string
  status: string
  priority: string | null
  owner: string | null
  start_at_ms: number | null
  target_at_ms: number | null
  task_ids: string[]
  work_item_ids: string[]
  progress: { total: number; completed: number; blocked: number; percent: number }
  activities: Array<{ id: string; summary: string }>
}

async function milestoneRecords(page: Page, server: E2EServer): Promise<MilestoneRecord[]> {
  const response = await page.request.get(`${server.base}/v1/authoritative/milestones`)
  expect(response.status()).toBe(200)
  return ((await response.json()) as { milestones: MilestoneRecord[] }).milestones
}

async function milestoneByName(
  page: Page,
  server: E2EServer,
  name: string,
): Promise<MilestoneRecord> {
  const record = (await milestoneRecords(page, server)).find((entry) => entry.name === name)
  expect(record, `the server must hold milestone ${name}`).toBeTruthy()
  return record as MilestoneRecord
}

function dateInputValue(ms: number): string {
  const date = new Date(ms)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function localMidnight(value: string): number {
  const [year, month, day] = value.split('-').map(Number)
  return new Date(year, month - 1, day, 0, 0, 0, 0).getTime()
}

test.describe('milestones', () => {
  test('create a rich milestone and see progress, children and activity in its details', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const name = 'Acceptance checkpoint'
    const description = 'The corrected workspace passes its browser acceptance run.'
    const start = dateInputValue(fixture.nowMs + 30 * 86_400_000)
    const target = dateInputValue(fixture.nowMs + 60 * 86_400_000)
    const child = fixture.tasks.find(
      (task) => task.projectId === 'p-development' && task.status === 'ready',
    )
    expect(child, 'the fixture needs a ready Development task to assign').toBeTruthy()

    await goto(page, server, '/projects/p-development/milestones')
    await expect(page.getByRole('heading', { name: 'Development', level: 1 })).toBeVisible()

    await page.getByRole('button', { name: 'New milestone' }).first().click()
    const createDialog = page.getByRole('dialog')
    await expect(createDialog.getByRole('heading', { name: 'New milestone' })).toBeVisible()

    // The client mirrors the server: a start after the target never leaves the form.
    await createDialog.getByLabel('Name').fill(name)
    await createDialog.getByLabel('Start date').fill(target)
    await createDialog.getByLabel('Target date').fill(start)
    await createDialog.getByRole('button', { name: 'Create milestone' }).click()
    await expect(createDialog.getByRole('alert')).toContainText(
      'The start date must be on or before the target date.',
    )

    await createDialog.getByLabel('Description').fill(description)
    await createDialog.getByLabel('Milestone priority').click()
    await page.getByRole('option', { name: 'High', exact: true }).click()
    await createDialog.getByLabel('Owner').fill('Dana')
    await createDialog.getByLabel('Start date').fill(start)
    await createDialog.getByLabel('Target date').fill(target)
    await createDialog.getByRole('button', { name: 'Create milestone' }).click()
    await expect(page.getByText('Milestone created', { exact: true })).toBeVisible()
    await expect(createDialog).toBeHidden()
    // Dismiss the notification so it cannot sit over the card's own actions.
    await page.getByRole('button', { name: 'Dismiss notification' }).click()
    await expect(page.getByText('Milestone created', { exact: true })).toBeHidden()

    // Every configured field is stored, in one create command.
    const created = await milestoneByName(page, server, name)
    expect(created).toMatchObject({
      description,
      status: 'planned',
      priority: 'high',
      owner: 'Dana',
      start_at_ms: localMidnight(start),
      target_at_ms: localMidnight(target),
    })

    const card = page.locator(`[data-milestone-card="${created.id}"]`)
    await expect(card).toBeVisible()
    await expect(card).toContainText(name)
    await expect(card).toContainText('Planned')
    await expect(card.getByRole('progressbar', { name: `${name} progress` })).toHaveAttribute(
      'aria-valuenow',
      '0',
    )
    await expect(card).toContainText('No work yet')
    await expect(card).toContainText('Owner Dana')
    await expect(card).not.toContainText('No dates')

    // Details: the record's own progress, children and history.
    await card.getByRole('button', { name: `Open details for ${name}` }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name })).toBeVisible()
    await expect(dialog.getByRole('region', { name: 'Description' })).toContainText(description)
    await expect(dialog.getByText('Owner Dana')).toBeVisible()
    await expect(dialog.getByText('High priority')).toBeVisible()
    await expect(dialog.getByRole('region', { name: 'Progress' })).toContainText('No work yet')
    await expect(dialog.getByText('No tasks are assigned yet.')).toBeVisible()
    const activityItems = dialog.getByRole('region', { name: 'Activity' }).locator('li')
    const activityBefore = await activityItems.count()
    expect(activityBefore, 'creating a milestone records activity').toBeGreaterThan(0)

    // Assigning a child is a milestone command; the dialog shows the server's
    // count afterwards rather than assuming the assignment.
    await dialog.getByLabel('Task to assign to this milestone').click()
    await page.getByRole('option', { name: child!.title, exact: true }).click()
    await dialog.getByRole('button', { name: 'Assign', exact: true }).click()
    await expect(dialog.getByRole('heading', { name: 'Tasks (1)' })).toBeVisible()
    await expect(dialog.getByRole('region', { name: 'Progress' })).toContainText('0/1 done')
    await expect
      .poll(async () => (await milestoneByName(page, server, name)).task_ids)
      .toEqual([child!.id])
    await expect
      .poll(async () => (await milestoneByName(page, server, name)).activities.length)
      .toBeGreaterThan(activityBefore)

    await dialog.getByRole('button', { name: 'Close dialog' }).click()
    await expect(dialog).toBeHidden()

    // The card behind the dialog moved with the server's progress.
    await expect(card.getByRole('progressbar', { name: `${name} progress` })).toHaveAttribute(
      'aria-valuenow',
      '0',
    )
    await page.reload()
    await expect(page.locator(`[data-milestone-card="${created.id}"]`)).toContainText('0/1 done')
  })

  test('a legal status change is the server’s, and survives a reload', async ({ page, workspace }) => {
    const { server } = workspace
    const hardening = workspace.fixture.milestones.find(
      (milestone) => milestone.id === 'm-dev-hardening',
    )
    expect(hardening).toBeTruthy()

    await goto(page, server, '/projects/p-development/milestones')
    const card = page.locator(`[data-milestone-card="${hardening!.id}"]`)
    await expect(card).toContainText('Planned')
    await card.getByRole('button', { name: `Open details for ${hardening!.name}` }).click()

    const dialog = page.getByRole('dialog')
    const status = dialog.getByLabel('Milestone status')
    await expect(status).toContainText('Planned')
    await status.click()
    await page.getByRole('option', { name: 'In progress', exact: true }).click()
    await expect(status).toContainText('In progress')
    await expect
      .poll(
        async () =>
          (await milestoneRecords(page, server)).find((entry) => entry.id === hardening!.id)?.status,
      )
      .toBe('in_progress')

    await dialog.getByRole('button', { name: 'Close dialog' }).click()
    await page.reload()
    await expect(page.locator(`[data-milestone-card="${hardening!.id}"]`)).toContainText(
      'In progress',
    )
  })

  test('deleting a milestone that still owns work is refused by the server', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const reliability = workspace.fixture.milestones.find(
      (milestone) => milestone.id === 'm-dev-reliability',
    )
    expect(reliability).toBeTruthy()

    // Signing in is what gives `page.request` a session; read the record after it.
    await goto(page, server, '/projects/p-development/milestones')
    const record = await milestoneByName(page, server, reliability!.name)
    expect(record.task_ids.length, 'the guard needs a milestone with children').toBeGreaterThan(0)

    const card = page.locator(`[data-milestone-card="${record.id}"]`)
    await card.getByRole('button', { name: `Open details for ${record.name}` }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name: record.name })).toBeVisible()

    await dialog.getByRole('button', { name: 'Delete milestone' }).click()
    await dialog.getByRole('button', { name: 'Delete milestone' }).click()

    await expect(dialog.getByRole('alert')).toContainText(
      /cannot delete milestone with attached work/i,
    )
    // Nothing was applied: the record and its children are still there.
    expect((await milestoneByName(page, server, record.name)).task_ids).toEqual(record.task_ids)
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: 'Close dialog' }).click()
    await expect(card).toBeVisible()
  })
})
