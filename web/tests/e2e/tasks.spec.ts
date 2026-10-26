/**
 * I1 acceptance: task capture, edit, status change, and rejection paths — all
 * against the authoritative server.
 *
 * The drawer commits field-by-field and always renders the server's answer: a
 * rejected command shows an error and the previous value, never the optimistic
 * one.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import { goto, signInViaApi } from './harness/session'
import type { E2EServer } from './harness/server'

async function captureTask(page: Page, server: E2EServer, title: string): Promise<void> {
  await goto(page, server, '/inbox')
  await expect(page.getByRole('heading', { name: 'Inbox', level: 1 })).toBeVisible()
  const input = page.getByLabel('Capture a task for the Inbox')
  await expect(input).toBeVisible()
  await input.fill(title)
  await input.press('Enter')
  await expect(page.getByText(title, { exact: true }).first()).toBeVisible()
  // The field clears so the next capture can start without a mouse.
  await expect(input).toHaveValue('')
}

/**
 * Open one task from a list by its title cell.
 *
 * The table rows are clickable but are not exposed as buttons/links today; the
 * missing keyboard-activatable row control is reported separately rather than
 * papered over here.
 */
async function openTask(page: Page, title: string): Promise<Locator> {
  await page.getByText(title, { exact: true }).first().click()
  const drawer = page.getByRole('dialog')
  await expect(drawer).toBeVisible()
  return drawer
}

async function renameTask(drawer: Locator, title: string): Promise<void> {
  await drawer.getByRole('button', { name: 'Edit task title' }).click()
  const input = drawer.getByLabel('Task title')
  await input.fill(title)
  await input.press('Enter')
  await expect(drawer.getByRole('button', { name: 'Edit task title' })).toContainText(title)
}

/** Pick a value from a Radix combobox trigger. */
async function choose(page: Page, control: Locator, visible: RegExp): Promise<void> {
  await control.click()
  await page.getByRole('option', { name: visible }).click()
}

test.describe('tasks', () => {
  test('capture → edit → status change survives a reload', async ({ page, workspace }) => {
    const { server } = workspace
    const title = 'Reconcile the acceptance walkthrough'
    const editedTitle = 'Reconcile the acceptance walkthrough (I1)'
    const description = 'Walk the I1 script end to end against a real server.'

    await captureTask(page, server, title)

    const drawer = await openTask(page, title)
    await renameTask(drawer, editedTitle)

    // Description: markdown editor, committed on blur.
    const descriptionField = drawer.getByLabel('Task description')
    await descriptionField.fill(description)
    await descriptionField.blur()

    // Priority and labels.
    await choose(page, drawer.getByLabel('Task priority'), /^high$/i)
    await expect(drawer.getByLabel('Task priority')).toContainText(/high/i)

    await drawer.getByRole('button', { name: 'Edit task labels' }).click()
    const labels = drawer.getByPlaceholder('design, docs')
    await labels.fill('acceptance')
    await labels.press('Enter')
    await expect(drawer.getByRole('button', { name: 'Edit task labels' })).toContainText('acceptance')

    // Status: inbox → planned is a legal server transition.
    await choose(page, drawer.getByLabel('Task status'), /^planned$/i)
    await expect(drawer.getByLabel('Task status')).toContainText(/planned/i)

    // Reload proves every field round-tripped through the server, and the drawer
    // is deep-linkable (`?task=<id>`), so it must reopen on the same task.
    await page.reload()
    const reopened = page.getByRole('dialog')
    await expect(reopened).toBeVisible()
    await expect(reopened.getByRole('button', { name: 'Edit task title' })).toContainText(editedTitle)
    await expect(reopened.getByLabel('Task description')).toHaveValue(description)
    await expect(reopened.getByLabel('Task priority')).toContainText(/high/i)
    await expect(reopened.getByRole('button', { name: 'Edit task labels' })).toContainText('acceptance')
    await expect(reopened.getByLabel('Task status')).toContainText(/planned/i)
  })

  test('the drawer offers only the status edges the domain accepts', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    // `inbox → done` is not a legal transition; the control must not offer it.
    const taskId = fixture.ids.b.inbox[0]
    const task = fixture.tasks.find((candidate) => candidate.id === taskId)!

    await signInViaApi(page, server)
    await page.goto(`${server.base}/inbox?task=${taskId}`)
    const drawer = page.getByRole('dialog')
    await expect(drawer).toBeVisible()
    await expect(drawer.getByRole('button', { name: 'Edit task title' })).toContainText(task.title)

    await expect(
      drawer.getByText(/Only the next states this task's workflow allows are offered/),
    ).toBeVisible()

    const status = drawer.getByLabel('Task status')
    await status.click()
    // The server's edge set for Inbox, and nothing else.
    await expect(page.getByRole('option', { name: 'Planned', exact: true })).toBeVisible()
    await expect(page.getByRole('option', { name: 'Done', exact: true })).toHaveCount(0)
    await expect(page.getByRole('option', { name: 'In progress', exact: true })).toHaveCount(0)
    await page.keyboard.press('Escape')

    // Nothing changed by opening the control.
    await expect(status).toContainText(/inbox/i)
    await page.reload()
    await expect(page.getByRole('dialog').getByLabel('Task status')).toContainText(/inbox/i)
  })

  test('a failed edge read falls back to every status and the server still refuses', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const taskId = fixture.ids.b.inbox[0]

    // The read is unavailable: the drawer must say so and keep the old
    // offer-everything behaviour rather than inventing an edge set.
    await page.route('**/authoritative/task-transitions*', (route) => route.abort())

    await signInViaApi(page, server)
    await page.goto(`${server.base}/inbox?task=${taskId}`)
    const drawer = page.getByRole('dialog')
    await expect(drawer).toBeVisible()
    await expect(
      drawer.getByText(/The allowed moves could not be read, so every status is offered/),
    ).toBeVisible()

    const status = drawer.getByLabel('Task status')
    await choose(page, status, /^done$/i)

    // The rejection is visible and the stored value is still the old one. The
    // drawer quotes the server's exact reason, not a generic "invalid".
    const alerts = drawer.getByRole('alert')
    await expect(alerts.first()).toBeVisible()
    await expect(alerts.first()).toHaveText('invalid task status transition: inbox -> done')
    // The footer never promises that edits save while a refusal is visible.
    await expect(drawer.locator('footer')).not.toContainText('Edits are sent when you leave a field.')
    await expect(status).toContainText(/inbox/i)
    await expect(drawer.getByRole('button', { name: 'Edit task title' })).toContainText(
      fixture.tasks.find((candidate) => candidate.id === taskId)!.title,
    )
  })

  test('bulk status change applies to every selected task and survives a reload', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const titles = ['Bulk target alpha', 'Bulk target beta']

    await goto(page, server, '/inbox')
    await expect(page.getByRole('heading', { name: 'Inbox', level: 1 })).toBeVisible()
    const capture = page.getByLabel('Capture a task for the Inbox')
    for (const title of titles) {
      await capture.fill(title)
      await capture.press('Enter')
      await expect(page.getByText(title, { exact: true }).first()).toBeVisible()
      await expect(capture).toHaveValue('')
    }

    // The desktop table and the stacked list are both in the DOM; only the table
    // is laid out at this viewport.
    const table = page.locator('table')
    for (const title of titles) {
      await table.getByRole('checkbox', { name: `Select ${title}` }).check()
    }

    const bulk = page.getByRole('region', { name: 'Bulk actions' })
    await expect(bulk).toBeVisible()
    await expect(bulk).toContainText('2 selected')

    // One status change covers both rows: the second command must fence on the
    // revision the first acceptance returned.
    await choose(page, bulk.getByLabel('Bulk action'), /^Set status$/i)
    await choose(page, bulk.getByLabel('Bulk value'), /^Planned$/i)
    await bulk.getByRole('button', { name: 'Apply' }).click()

    // No partial-failure message, and both rows render the stored status.
    await expect(bulk).toBeHidden()
    for (const title of titles) {
      await expect(table.getByLabel(`Status for ${title}`)).toContainText(/planned/i)
    }

    // Reload proves both commands reached the authoritative store.
    await page.reload()
    await expect(page.getByRole('heading', { name: 'Inbox', level: 1 })).toBeVisible()
    for (const title of titles) {
      await expect(page.locator('table').getByLabel(`Status for ${title}`)).toContainText(/planned/i)
    }
  })
})
