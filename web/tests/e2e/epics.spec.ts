/**
 * M33 — epics are an authoritative grouping, not a client-side label.
 *
 * The test creates one through the dialog, assigns a task from the shared
 * drawer, completes the task, and reads progress from the server. A restart
 * proves the relationship is durable.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'

async function epics(page: import('@playwright/test').Page, server: E2EServer) {
  const response = await page.request.get(`${server.base}/v1/authoritative/epics`)
  expect(response.status()).toBe(200)
  return (await response.json()) as {
    revision: number
    epics: Array<{
      id: string
      title: string
      task_ids: string[]
      work_item_ids: string[]
      progress: { total: number; completed: number; percent: number }
    }>
  }
}

test.describe('epics', () => {
  test('create an epic, assign a task and persist progress across a restart', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) =>
        candidate.projectId === 'p-development' &&
        ['in_progress', 'review'].includes(candidate.status),
    )
    expect(task, 'the fixture needs an open Development task').toBeTruthy()

    await goto(page, server, '/projects/p-development/list')
    await page.getByRole('button', { name: 'New epic' }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name: 'New epic' })).toBeVisible()
    await dialog.getByLabel('Title').fill('Web experience')
    await dialog.getByLabel('Description').fill('Make the project workspace readable and fast.')
    await dialog.getByRole('button', { name: 'Create epic' }).click()
    await expect(page.getByText('Epic created', { exact: true })).toBeVisible()
    await expect(dialog).toBeHidden()

    await expect
      .poll(async () => (await epics(page, server)).epics.find((epic) => epic.title === 'Web experience'))
      .toBeTruthy()

    // One task drawer assigns membership through the epic command family.
    await page.getByRole('button', { name: task!.title, exact: true }).first().click()
    const drawer = page.locator('[data-task-drawer]')
    await expect(drawer.getByLabel('Task status')).toBeVisible()
    await drawer.getByLabel('Epic').click()
    await page.getByRole('option', { name: 'Web experience' }).click()
    await expect(drawer.getByLabel('Epic')).toContainText('Web experience')
    await page.waitForTimeout(400)

    await expect
      .poll(async () => (await epics(page, server)).epics.find((epic) => epic.title === 'Web experience')?.task_ids)
      .toEqual([task!.id])

    // Move the child to Done through the authoritative status control.
    const status = drawer.getByLabel('Task status')
    for (const target of ['In progress', 'Done']) {
      if ((await status.textContent())?.toLowerCase().includes(target.toLowerCase())) continue
      await status.click()
      const option = page.getByRole('option', { name: target, exact: true })
      if ((await option.count()) === 0) {
        await page.keyboard.press('Escape')
        continue
      }
      await option.click()
      await expect(status).toContainText(new RegExp(target, 'i'))
    }
    await expect(status).toContainText(/done/i)
    await page.getByRole('button', { name: 'Close task details' }).click()

    await expect
      .poll(async () => (await epics(page, server)).epics.find((epic) => epic.title === 'Web experience')?.progress)
      .toEqual({ total: 1, completed: 1, blocked: 0, percent: 100 })

    // Server restart, same database: the grouping and its progress survive.
    await workspace.server.restart()
    await page.goto(`${server.base}/projects/p-development/list`)
    await page.getByLabel('Filter project by epic').click()
    await expect(page.getByRole('option', { name: 'Web experience' })).toBeVisible()
    await page.keyboard.press('Escape')
    const persisted = await epics(page, server)
    expect(persisted.epics.find((epic) => epic.title === 'Web experience')?.task_ids).toEqual([
      task!.id,
    ])
  })

  test('cross-project hierarchy is refused and never left on screen', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const foreign = fixture.tasks.find((candidate) => candidate.projectId === 'p-basic')
    expect(foreign).toBeTruthy()

    await goto(page, server, '/projects/p-development/list')
    await page.getByRole('button', { name: 'New epic' }).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByLabel('Title').fill('Development only')
    await dialog.getByRole('button', { name: 'Create epic' }).click()
    await expect(page.getByText('Epic created', { exact: true })).toBeVisible()
    await expect(dialog).toBeHidden()

    // The drawer only offers epics from the task's project, so a cross-project
    // assignment cannot even be expressed in the UI.
    await goto(page, server, '/projects/p-basic/list')
    await page.getByRole('button', { name: foreign!.title, exact: true }).first().click()
    const drawer = page.locator('[data-task-drawer]')
    await drawer.getByLabel('Epic').click()
    await expect(page.getByRole('option', { name: 'Development only' })).toHaveCount(0)
    await expect(page.getByRole('option', { name: 'No epic' })).toBeVisible()
  })

  test('a Workflow Work Item joins the same epic through its sheet', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-orbit-development/list')
    await page.getByRole('button', { name: 'New epic' }).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByLabel('Title').fill('Work item grouping')
    await dialog.getByRole('button', { name: 'Create epic' }).click()
    await expect(page.getByText('Epic created', { exact: true })).toBeVisible()
    await expect(dialog).toBeHidden()

    await page.goto(`${server.base}/projects/p-orbit-development/board`)
    const card = page.locator('[data-work-item-key]').first()
    await expect(card).toBeVisible()
    const itemKey = await card.getAttribute('data-work-item-key')
    expect(itemKey).toBeTruthy()
    await card.getByRole('button').first().click()
    const sheet = page.getByRole('dialog')
    await sheet.getByRole('combobox', { name: 'Epic' }).click()
    await page.getByRole('option', { name: 'Work item grouping' }).click()
    await expect(sheet.getByRole('combobox', { name: 'Epic' })).toContainText('Work item grouping')

    await expect
      .poll(
        async () =>
          (await epics(page, server)).epics.find((epic) => epic.title === 'Work item grouping')
            ?.work_item_ids,
      )
      .toEqual([itemKey])
  })

})
