/**
 * S2 acceptance: one refusal surface, and the toast never parks on the drawer.
 *
 * A stale-revision rejection is the noisiest failure path: it used to render
 * the same sentence inline, again in the footer, and once more in a toast that
 * covered the drawer's lower controls. The drawer now owns exactly one copy.
 */
import type { Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { signInViaApi } from './harness/session'

async function currentRevision(page: Page, server: E2EServer): Promise<number> {
  const response = await page.request.get(`${server.base}/v1/authoritative/projects`)
  expect(response.status()).toBe(200)
  return ((await response.json()) as { revision: number }).revision
}

function overlaps(
  left: { x: number; y: number; width: number; height: number },
  right: { x: number; y: number; width: number; height: number },
): boolean {
  return !(
    left.x + left.width <= right.x ||
    right.x + right.width <= left.x ||
    left.y + left.height <= right.y ||
    right.y + right.height <= left.y
  )
}

test('a stale description revision is shown once in the drawer', async ({
  page,
  context,
  workspace,
}) => {
  const { server, fixture } = workspace
  const taskId = fixture.ids.orbitTask

  await signInViaApi(page, server)
  await page.goto(`${server.base}/inbox?task=${taskId}`)
  const drawer = page.getByRole('dialog')
  await expect(drawer).toBeVisible()
  const description = drawer.getByLabel('Task description')
  await expect(description).toBeVisible()

  // A second session moves the ledger first, so the description save below is
  // a definitive 409 rather than an ambiguous transport failure.
  const second = await context.browser()!.newContext()
  const other = await second.newPage()
  try {
    await signInViaApi(other, server)
    const revision = await currentRevision(other, server)
    const response = await other.request.post(`${server.base}/v1/authoritative/task-commands`, {
      data: {
        command_id: `error-surface-description-${taskId}`,
        expected_revision: revision,
        action: { type: 'rename', task_id: taskId, title: 'Moved by the other window' },
      },
      headers: { 'content-type': 'application/json', 'x-orbit-csrf': '1' },
    })
    expect(response.status(), 'the second writer must be accepted').toBe(200)

    await description.fill('A description typed against a stale revision.')
    await description.blur()

    // Exactly one alert: the description section's own error. PropertiesSection
    // must not repeat the same `set_details` refusal.
    const alerts = drawer.getByRole('alert')
    await expect(alerts).toHaveCount(1)
    await expect(alerts).toContainText(/conflicted with a newer version/i)

    // Exactly one recovery action, beside the description, not duplicated.
    const reload = drawer.getByRole('button', { name: 'Reload this task and try again' })
    await expect(reload).toHaveCount(1)
  } finally {
    await second.close()
  }
})

test('a stale revision is shown once in the drawer and the toast clears it', async ({
  page,
  context,
  workspace,
}) => {
  const { server, fixture } = workspace
  const taskId = fixture.ids.orbitTask
  const original = fixture.tasks.find((task) => task.id === taskId)!.title

  await signInViaApi(page, server)
  await page.goto(`${server.base}/inbox?task=${taskId}`)
  const drawer = page.getByRole('dialog')
  await expect(drawer).toBeVisible()
  await expect(drawer.getByRole('button', { name: 'Edit task title' })).toContainText(original)

  // A second session moves the ledger first, so session A's next rename is a
  // definitive 409 rather than an ambiguous transport failure.
  const second = await context.browser()!.newContext()
  const other = await second.newPage()
  try {
    await signInViaApi(other, server)
    const revision = await currentRevision(other, server)
    const renamed = `${original} (from the other window)`
    const response = await other.request.post(`${server.base}/v1/authoritative/task-commands`, {
      data: {
        command_id: `error-surface-rename-${taskId}`,
        expected_revision: revision,
        action: { type: 'rename', task_id: taskId, title: renamed },
      },
      headers: { 'content-type': 'application/json', 'x-orbit-csrf': '1' },
    })
    expect(response.status(), 'the second writer must be accepted').toBe(200)

    await drawer.getByRole('button', { name: 'Edit task title' }).click()
    const title = drawer.getByLabel('Task title')
    await title.fill(`${original} (stale write)`)
    await title.press('Enter')

    // Exactly one alert inside the drawer: the inline message next to the
    // title. The footer must not repeat the server's sentence.
    const alerts = drawer.getByRole('alert')
    await expect(alerts).toHaveCount(1)
    await expect(alerts).toContainText(/conflicted with a newer version/i)
    await expect(drawer.locator('footer')).not.toContainText(/conflicted with a newer version/i)

    // Recovery lives on the same inline surface.
    const reload = drawer.getByRole('button', { name: 'Reload this task and try again' })
    await expect(reload).toBeVisible()

    // The notification is allowed to exist, but it must not cover the drawer's
    // panel (and therefore not its controls).
    const toast = page.locator('[aria-label="Notifications"] [data-state="open"]').first()
    await expect(toast).toBeVisible()
    const drawerBox = await drawer.boundingBox()
    const toastBox = await toast.boundingBox()
    expect(drawerBox, 'the drawer must have a layout box').not.toBeNull()
    expect(toastBox, 'the toast must have a layout box').not.toBeNull()
    expect(
      overlaps(drawerBox!, toastBox!),
      'the toast must be offset so it does not overlap the drawer',
    ).toBe(false)

    // The one inline action still recovers without a reload.
    await reload.click()
    await expect(drawer.getByRole('button', { name: 'Edit task title' })).toContainText(renamed)
    await expect(alerts).toHaveCount(0)
  } finally {
    await second.close()
  }
})
