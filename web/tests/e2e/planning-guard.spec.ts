/**
 * S4 acceptance: a Planning draft is never lost by closing the drawer.
 *
 * Before this, estimate/earliest start/deadline were the only drawer editors
 * that did not register with the close guard: typing a value and pressing
 * Escape discarded it. These tests drive the real close paths and the server
 * record, so "saved" means the value round-tripped rather than merely painting.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

async function captureTask(page: Page, server: E2EServer, title: string): Promise<void> {
  await goto(page, server, '/inbox')
  await expect(page.getByRole('heading', { name: 'Inbox', level: 1 })).toBeVisible()
  const input = page.getByLabel('Capture a task for the Inbox')
  await input.fill(title)
  await input.press('Enter')
  await expect(page.getByText(title, { exact: true }).first()).toBeVisible()
}

async function openTask(page: Page, title: string): Promise<Locator> {
  await page.getByText(title, { exact: true }).first().click()
  const drawer = page.getByRole('dialog')
  await expect(drawer).toBeVisible()
  return drawer
}

async function taskRecord(page: Page, server: E2EServer, taskId: string) {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks/${taskId}`)
  expect(response.status()).toBe(200)
  return (await response.json()) as {
    estimated_minutes: number | null
    earliest_start_at_ms: number | null
    deadline_at_ms: number | null
  }
}

async function closeDrawer(drawer: Locator): Promise<void> {
  // A close is refused while the guard's flush is in flight; retry once the
  // server has answered. The drawer staying open is itself part of the test.
  await expect(drawer.getByLabel('Task estimate in minutes')).toBeEnabled()
  await drawer.getByRole('button', { name: 'Close task details' }).click()
  await expect(drawer).toBeHidden()
}

test.describe('planning drafts and the close guard', () => {
  test('typing an estimate then pressing Escape saves it instead of discarding it', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const title = 'Escape keeps the estimate'
    await captureTask(page, server, title)
    const drawer = await openTask(page, title)
    const taskId = new URL(page.url()).searchParams.get('task')
    expect(taskId).toBeTruthy()

    const estimate = drawer.getByLabel('Task estimate in minutes')
    await estimate.fill('60')
    await estimate.press('Escape')

    // The close is refused while the dirty window flushes; the drawer stays up.
    await expect(drawer).toBeVisible()
    await expect
      .poll(async () => (await taskRecord(page, server, taskId!)).estimated_minutes)
      .toBe(60)
    await expect(estimate).toHaveValue('60')
    await expect(estimate).toBeEnabled()

    await closeDrawer(drawer)

    // Reopen from the URL: the value is the server's, not the old local draft.
    await page.goto(`${server.base}/inbox?task=${taskId}`)
    const reopened = page.getByRole('dialog')
    await expect(reopened).toBeVisible()
    await expect(reopened.getByLabel('Task estimate in minutes')).toHaveValue('60')
  })

  test('clearing an estimate then pressing Escape saves the clear', async ({ page, workspace }) => {
    const { server } = workspace
    const title = 'Escape keeps the cleared estimate'
    await captureTask(page, server, title)
    const drawer = await openTask(page, title)
    const taskId = new URL(page.url()).searchParams.get('task')
    expect(taskId).toBeTruthy()

    const estimate = drawer.getByLabel('Task estimate in minutes')
    await estimate.fill('60')
    await estimate.press('Enter')
    await expect
      .poll(async () => (await taskRecord(page, server, taskId!)).estimated_minutes)
      .toBe(60)
    await expect(estimate).toBeEnabled()

    await estimate.fill('')
    await estimate.press('Escape')
    await expect
      .poll(async () => (await taskRecord(page, server, taskId!)).estimated_minutes)
      .toBeNull()
    await expect(estimate).toHaveValue('')

    await closeDrawer(drawer)
    await page.goto(`${server.base}/inbox?task=${taskId}`)
    await expect(page.getByRole('dialog').getByLabel('Task estimate in minutes')).toHaveValue('')
  })

  test('two quick estimate edits are both accepted and the last one wins', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const title = 'Two quick estimate edits'
    await captureTask(page, server, title)
    const drawer = await openTask(page, title)
    const taskId = new URL(page.url()).searchParams.get('task')
    expect(taskId).toBeTruthy()
    const estimate = drawer.getByLabel('Task estimate in minutes')

    await estimate.fill('60')
    await estimate.press('Enter')
    // The first edit reached the server before the second is allowed to start;
    // the field is disabled while its command is in flight.
    await expect
      .poll(async () => (await taskRecord(page, server, taskId!)).estimated_minutes)
      .toBe(60)

    await estimate.fill('90')
    await estimate.press('Enter')
    await expect
      .poll(async () => (await taskRecord(page, server, taskId!)).estimated_minutes)
      .toBe(90)
    await expect(estimate).toHaveValue('90')
  })
})
