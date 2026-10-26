/**
 * I1 acceptance: stale `expected_revision` is a first-class outcome.
 *
 * Two browser sessions (separate cookie jars, same device) race on one task. The
 * second writer wins on the server; the first one must be told, must not keep its
 * optimistic value, and must recover by refetching — never by silently
 * overwriting the other writer.
 */
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto, signInViaApi } from './harness/session'

async function currentRevision(page: import('@playwright/test').Page, server: E2EServer) {
  const response = await page.request.get(`${server.base}/v1/authoritative/projects`)
  expect(response.status()).toBe(200)
  return (await response.json()) as { revision: number }
}

test.describe('conflict', () => {
  test('a stale revision is refused explicitly and recovers by refetch', async ({
    page,
    context,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const taskId = fixture.ids.orbitTask
    const original = fixture.tasks.find((task) => task.id === taskId)!.title

    // Session A: the human's view, opened on the task drawer.
    await signInViaApi(page, server)
    await page.goto(`${server.base}/inbox?task=${taskId}`)
    const drawer = page.getByRole('dialog')
    await expect(drawer).toBeVisible()

    // Session B: another browser, same device, writes first.
    const second = await context.browser()!.newContext()
    const other = await second.newPage()
    try {
      await signInViaApi(other, server)
      const { revision } = await currentRevision(other, server)
      const renamed = `${original} (from the other window)`
      const response = await other.request.post(
        `${server.base}/v1/authoritative/task-commands`,
        {
          data: {
            command_id: `conflict-rename-${taskId}`,
            expected_revision: revision,
            action: { type: 'rename', task_id: taskId, title: renamed },
          },
          headers: { 'content-type': 'application/json', 'x-orbit-csrf': '1' },
        },
      )
      expect(
        response.status(),
        'the second writer must be accepted while session A still holds the old revision',
      ).toBe(200)

      // Session A now edits with its stale revision: the server must reject it.
      await expect(drawer.getByRole('button', { name: 'Edit task title' })).toContainText(original)
      await drawer.getByRole('button', { name: 'Edit task title' }).click()
      const titleField = drawer.getByLabel('Task title')
      await expect(titleField).toBeVisible()
      const staleTitle = `${original} (stale write)`
      await titleField.fill(staleTitle)
      await titleField.press('Enter')

      const conflict = page
        .getByRole('alert')
        .or(page.getByRole('status'))
        .filter({ hasText: /changed|conflict|reload|refresh|newer/i })
        .first()
      await expect(conflict, 'a 409 must be surfaced, not swallowed').toBeVisible()

      // No silent overwrite: the server still holds B's title.
      const after = await other.request.get(`${server.base}/v1/authoritative/tasks/${taskId}`)
      expect(after.status()).toBe(200)
      expect((await after.json()).title).toBe(renamed)

      // The drawer offers the recovery the message promises: reload this task
      // without leaving the page, then edit against the new revision.
      const reload = drawer.getByRole('button', { name: 'Reload this task and try again' })
      await expect(reload).toBeVisible()
      await reload.click()
      await expect(
        drawer.getByRole('button', { name: 'Edit task title' }),
      ).toContainText(renamed)
      await expect(page.getByText(staleTitle, { exact: true })).toHaveCount(0)

      const edit = drawer.getByRole('button', { name: 'Edit task title' })
      await edit.click()
      const recoveredTitle = `${renamed} (recovered here)`
      await drawer.getByLabel('Task title').fill(recoveredTitle)
      await drawer.getByLabel('Task title').press('Enter')
      await expect(edit).toContainText(recoveredTitle)

      const accepted = await other.request.get(`${server.base}/v1/authoritative/tasks/${taskId}`)
      expect(accepted.status()).toBe(200)
      expect((await accepted.json()).title).toBe(recoveredTitle)
    } finally {
      await second.close()
    }
  })

  test('a task write followed by a milestone write does not conflict', async ({ page, workspace }) => {
    const { server } = workspace
    const projectId = 'p-development'

    await goto(page, server, `/projects/${projectId}/overview`)
    await page.getByRole('main').getByRole('button', { name: 'New task', exact: true }).click()
    const taskDialog = page.getByRole('dialog')
    await taskDialog.getByLabel('Title').fill('Cross-cache milestone seed')
    await taskDialog.getByRole('button', { name: 'Create task' }).click()
    await expect(taskDialog).toBeHidden()

    // The task write advanced the global ledger revision. The milestone cache
    // must have learned it, or this second command fences on the old revision.
    await goto(page, server, `/projects/${projectId}/milestones`)
    await page.getByRole('button', { name: 'New milestone' }).first().click()
    const milestoneDialog = page.getByRole('dialog')
    await milestoneDialog.getByLabel('Milestone name').fill('Cross-cache checkpoint')
    await milestoneDialog.getByRole('button', { name: 'Create milestone' }).click()
    await expect(milestoneDialog).toBeHidden()

    await expect(page.getByText('Cross-cache checkpoint', { exact: true }).first()).toBeVisible()
  })
})
