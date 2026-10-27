/**
 * I1 acceptance: a task's recorded Created and Updated timestamps.
 *
 * The values come from the server's own activity history (`created_at_ms`,
 * `updated_at_ms` on `GET /v1/authoritative/tasks/{id}`), and the drawer must
 * render exactly those numbers: a readable label plus the full timestamp in the
 * `datetime`/`title` attributes. Nothing is fabricated locally.
 *
 * The `null` → `Unknown` branch is *not* exercised here: a task created through
 * the API always has a `created` activity row, so an API-created task cannot
 * produce an unknown value. That branch is covered at unit level.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { signInViaApi } from './harness/session'
import {
  authorityRevision,
  postAuthority,
  readTaskRecord,
  type TaskDetailRecord,
} from './harness/authority'

const TASK_ID = 't-i1-metadata'
const PROJECT_ID = 'p-orbit-development'
const TASK_TITLE = 'Metadata acceptance task'

/** Create the subject task through the public command route, in this run. */
async function seedTask(page: Page, server: E2EServer): Promise<TaskDetailRecord> {
  await signInViaApi(page, server)
  const created = await postAuthority(page, server, 'task-commands', {
    command_id: 'e2e-metadata-task',
    expected_revision: await authorityRevision(page, server),
    action: { type: 'create', task_id: TASK_ID, title: TASK_TITLE, project_id: PROJECT_ID },
  })
  expect(created.status, 'seeding the metadata task').toBe(200)
  return readTaskRecord(page, server, TASK_ID)
}

/** The number the drawer shows for one timestamp, or null when it says Unknown. */
async function stampMs(page: Page, which: 'created' | 'updated'): Promise<number | null> {
  const value = await page.locator(`[data-task-${which}]`).getAttribute('datetime')
  return value ? Date.parse(value) : null
}

/** The task drawer, addressed by its own close control (its title changes). */
function taskDrawer(page: Page): Locator {
  return page
    .getByRole('dialog')
    .filter({ has: page.getByRole('button', { name: 'Close task details' }) })
}

async function openDrawer(page: Page, server: E2EServer): Promise<void> {
  await page.goto(`${server.base}/projects/${PROJECT_ID}?task=${TASK_ID}`)
  const drawer = taskDrawer(page)
  await expect(drawer).toBeVisible()
  await expect(drawer).toContainText(TASK_TITLE)
  await expect(page.locator('[data-task-timestamps]')).toBeVisible()
}

test.describe('task metadata', () => {
  test('shows the server-recorded Created and Updated values, with the full timestamp', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const record = await seedTask(page, server)
    expect(record.created_at_ms, 'the create command recorded a timestamp').toBeGreaterThan(0)
    expect(record.updated_at_ms).toBeGreaterThan(0)

    await openDrawer(page, server)

    const created = page.locator('[data-task-created]')
    const updated = page.locator('[data-task-updated]')

    // The exact milliseconds the server reports, not a rounded local guess.
    await expect(created).toHaveAttribute(
      'datetime',
      new Date(record.created_at_ms as number).toISOString(),
    )
    await expect(updated).toHaveAttribute(
      'datetime',
      new Date(record.updated_at_ms as number).toISOString(),
    )

    // Readable on screen, with the complete value available in the same place.
    await expect(created).toContainText(/Today, \d/)
    await expect(updated).toContainText(/Today, \d/)
    await expect(created).toHaveAttribute('title', /\d{4}/)
    await expect(updated).toHaveAttribute('title', /\d{4}/)
    await expect(created.locator('.sr-only')).toContainText('Full timestamp:')
    await expect(updated.locator('.sr-only')).toContainText('Full timestamp:')

    // A task with history never shows the unknown state.
    await expect(page.locator('[data-task-timestamp-unknown]')).toHaveCount(0)
  })

  test('advances Updated on a change and leaves Created where it was', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const before = await seedTask(page, server)
    await openDrawer(page, server)
    const createdBefore = await stampMs(page, 'created')
    expect(createdBefore, 'the drawer shows the recorded creation time').toBe(
      before.created_at_ms,
    )

    // inbox → planned is a declared move; the server accepts and records it.
    const drawer = taskDrawer(page)
    await drawer.getByLabel('Task status').click()
    await page.getByRole('option', { name: 'Planned', exact: true }).click()
    await expect(drawer.getByLabel('Task status')).toContainText('Planned')

    // Updated moves forward; Created does not.
    await expect.poll(() => stampMs(page, 'updated')).toBeGreaterThan(
      before.updated_at_ms as number,
    )
    const after = await readTaskRecord(page, server, TASK_ID)
    expect(after.created_at_ms, 'Created is the earliest created row').toBe(
      before.created_at_ms,
    )
    expect(after.updated_at_ms as number).toBeGreaterThan(before.updated_at_ms as number)
    expect(after.status).toBe('planned')

    // The drawer is showing the accepted record, to the millisecond.
    await expect(page.locator('[data-task-created]')).toHaveAttribute(
      'datetime',
      new Date(after.created_at_ms as number).toISOString(),
    )
    await expect(page.locator('[data-task-updated]')).toHaveAttribute(
      'datetime',
      new Date(after.updated_at_ms as number).toISOString(),
    )
    expect(await stampMs(page, 'created')).toBe(createdBefore)
    await expect(page.locator('[data-task-timestamp-unknown]')).toHaveCount(0)

    // Reopening from the URL reads the same stored history.
    await page.reload()
    await expect(taskDrawer(page)).toBeVisible()
    await expect(page.locator('[data-task-updated]')).toHaveAttribute(
      'datetime',
      new Date(after.updated_at_ms as number).toISOString(),
    )
    await expect(page.locator('[data-task-created]')).toHaveAttribute(
      'datetime',
      new Date(after.created_at_ms as number).toISOString(),
    )
  })
})
