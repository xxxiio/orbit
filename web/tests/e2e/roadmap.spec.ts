/**
 * M33 corrective — the milestone Roadmap.
 *
 * A milestone is the only grouping on the timeline. Its shape comes from the
 * authored fields and nothing else: start + target draws a duration bar, a
 * target alone is a checkpoint marker, and a milestone with no target is listed
 * as unscheduled rather than given an invented span. Child tasks sit under their
 * milestone and use their own authored window.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'
import type { Page } from '@playwright/test'

interface MilestoneRecord {
  id: string
  name: string
  status: string
  description: string
  owner: string | null
  start_at_ms: number | null
  target_at_ms: number | null
  task_ids: string[]
  work_item_ids: string[]
  progress: { total: number; completed: number; blocked: number; percent: number }
  activities: Array<{ id: string; summary: string }>
}

async function milestoneRecord(
  page: Page,
  server: E2EServer,
  id: string,
): Promise<MilestoneRecord> {
  const response = await page.request.get(`${server.base}/v1/authoritative/milestones`)
  expect(response.status()).toBe(200)
  const body = (await response.json()) as { milestones: MilestoneRecord[] }
  const record = body.milestones.find((milestone) => milestone.id === id)
  expect(record, `milestone ${id} must exist on the server`).toBeTruthy()
  return record as MilestoneRecord
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** `YYYY-MM-DD` in local time, the shape a date input reads. */
function dateInputValue(ms: number): string {
  const date = new Date(ms)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function localMidnight(value: string): number {
  const [year, month, day] = value.split('-').map(Number)
  return new Date(year, month - 1, day, 0, 0, 0, 0).getTime()
}

function milestoneRow(page: Page, id: string) {
  return page.locator(`[data-roadmap-row="milestone"][data-roadmap-id="${id}"]`)
}

function taskRow(page: Page, id: string) {
  return page.locator(`[data-roadmap-row="task"][data-roadmap-id="${id}"]`)
}

test.describe('roadmap', () => {
  test('milestones draw a duration, a checkpoint or nothing at all', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const reliability = fixture.milestones.find((milestone) => milestone.id === 'm-dev-reliability')
    const hardening = fixture.milestones.find((milestone) => milestone.id === 'm-dev-hardening')
    const docs = fixture.milestones.find((milestone) => milestone.id === 'm-dev-docs')
    expect(reliability?.startAtMs, 'the fixture needs a start+target milestone').not.toBeNull()
    expect(reliability?.targetAtMs).not.toBeNull()
    expect(hardening?.startAtMs, 'the fixture needs a target-only milestone').toBeNull()
    expect(hardening?.targetAtMs).not.toBeNull()
    expect(docs?.startAtMs, 'the fixture needs an undated milestone').toBeNull()
    expect(docs?.targetAtMs).toBeNull()

    await goto(page, server, '/projects/p-development/roadmap')

    // Start + target: one bar spanning two authored dates.
    const durationRow = milestoneRow(page, 'm-dev-reliability')
    await expect(durationRow).toBeVisible()
    const durationBar = durationRow.getByRole('button', { name: /^Reliability, .*open milestone$/ })
    await expect(durationBar, 'a start and a target draw one bar').toBeVisible()
    const durationLabel = await durationBar.getAttribute('aria-label')
    expect(durationLabel, 'a duration bar names both dates').toMatch(/–/)
    const record = await milestoneRecord(page, server, 'm-dev-reliability')
    expect(durationLabel).toContain(`${record.progress.percent}%`)
    await expect(durationBar).toHaveAttribute('title', /Reliability · .+ · /)

    // Target only: a marker on that day, never a one-day bar.
    const checkpointRow = milestoneRow(page, 'm-dev-hardening')
    await expect(checkpointRow).toBeVisible()
    await expect(
      checkpointRow.getByRole('button', { name: /^Hardening, milestone on .* change target date$/ }),
    ).toBeVisible()
    await expect(
      checkpointRow.getByRole('button', { name: /^Hardening, .*open milestone$/ }),
    ).toHaveCount(0)

    // No target: unscheduled. It keeps a row that says so, and an entry in the
    // unscheduled list; it is not drawn anywhere on the timeline.
    const undatedRow = milestoneRow(page, 'm-dev-docs')
    await expect(undatedRow).toBeVisible()
    await expect(undatedRow.getByText('No target')).toBeVisible()
    await expect(
      undatedRow.getByRole('button', { name: /^Documentation pass, .*open milestone$/ }),
    ).toHaveCount(0)
    await expect(page.locator('[data-unscheduled-milestone="m-dev-docs"]')).toContainText(
      'Documentation pass',
    )
    await expect(
      page.getByRole('button', { name: /^Documentation pass, milestone on/ }),
    ).toHaveCount(0)

    // The legend names the shapes the timeline can draw.
    for (const label of ['Duration', 'Checkpoint', 'Today']) {
      await expect(page.getByText(label, { exact: true }).first()).toBeVisible()
    }
  })

  test('child tasks sit under their milestone and use their own authored window', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const children = fixture.tasks.filter(
      (task) => task.projectId === 'p-development' && task.milestoneId !== null,
    )
    const windowed = children.find(
      (task) => task.earliestStartAtMs !== null && task.deadlineAtMs !== null,
    )
    const checkpointed = children.find(
      (task) => task.earliestStartAtMs === null && task.deadlineAtMs !== null,
    )
    expect(windowed, 'the fixture needs a milestone child with two authored dates').toBeTruthy()
    expect(checkpointed, 'the fixture needs a milestone child with one authored date').toBeTruthy()

    await goto(page, server, '/projects/p-development/roadmap')

    const windowRow = taskRow(page, windowed!.id)
    await expect(windowRow).toBeVisible()
    const windowLabel = await windowRow
      .getByRole('button', { name: new RegExp(`^${escapeRegExp(windowed!.title)}`) })
      .getAttribute('aria-label')
    expect(windowLabel, 'two authored dates make a task window').toMatch(/–/)

    const checkpointRow = taskRow(page, checkpointed!.id)
    await expect(checkpointRow).toBeVisible()
    const checkpointLabel = await checkpointRow
      .getByRole('button', { name: new RegExp(`^${escapeRegExp(checkpointed!.title)}`) })
      .getAttribute('aria-label')
    expect(checkpointLabel, 'one authored date is a marker, not a window').not.toMatch(/–/)

    // Hierarchy: each child sits after the milestone row it belongs to.
    const order = await page
      .locator('[data-roadmap-id]')
      .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-roadmap-id')))
    for (const child of [windowed!, checkpointed!]) {
      expect(order.indexOf(child.milestoneId!), `${child.title} needs its milestone row`).toBeGreaterThanOrEqual(0)
      expect(order.indexOf(child.id), `${child.title} must follow its milestone`).toBeGreaterThan(
        order.indexOf(child.milestoneId!),
      )
    }

    // A task with no authored dates is unscheduled: listed, never drawn.
    const unscheduled = fixture.tasks.find(
      (task) =>
        task.projectId === 'p-development' &&
        task.deadlineAtMs === null &&
        task.earliestStartAtMs === null,
    )
    expect(unscheduled, 'the fixture needs a dateless Development task').toBeTruthy()
    await expect(
      page.getByRole('button', { name: new RegExp(`^${escapeRegExp(unscheduled!.title)}`) }),
    ).toBeVisible()
    await expect(taskRow(page, unscheduled!.id)).toHaveCount(0)
    await expect(page.getByRole('main')).toContainText('These items have no authored dates')

    // A milestone with no dates stays unscheduled even when its child is drawn.
    await expect(page.locator('[data-unscheduled-milestone="m-dev-docs"]')).toBeVisible()
  })

  test('opening a milestone from the roadmap shows progress, children and activity', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/roadmap')
    const record = await milestoneRecord(page, server, 'm-dev-reliability')

    await page.getByRole('button', { name: /^Reliability, .*open milestone$/ }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name: 'Reliability' })).toBeVisible()

    const progress = dialog.getByRole('region', { name: 'Progress' })
    await expect(progress).toBeVisible()
    await expect(dialog.getByRole('progressbar', { name: 'Reliability progress' })).toHaveAttribute(
      'aria-valuenow',
      String(record.progress.percent),
    )
    await expect(progress).toContainText(`${record.progress.completed}/${record.progress.total} done`)

    // The child list is the server's membership, not a client-side filter.
    await expect(dialog.getByRole('heading', { name: `Tasks (${record.task_ids.length})` })).toBeVisible()
    await expect(dialog.getByRole('region', { name: 'Work items' })).toBeVisible()
    await expect(dialog.getByRole('region', { name: 'Description' })).toContainText(
      'Stabilise the command ledger',
    )
    await expect(dialog.getByText('Owner Alex')).toBeVisible()
    await expect(dialog.getByText('High priority')).toBeVisible()
    const activity = dialog.getByRole('region', { name: 'Activity' })
    await expect(activity.locator('li').first()).toBeVisible()

    await dialog.getByRole('button', { name: 'Close dialog' }).click()
    await expect(dialog).toBeHidden()
  })

  test('the roadmap filters by milestone and draws no epic model', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    await goto(page, server, '/projects/p-development/roadmap')
    const main = page.getByRole('main')
    await expect(main).not.toContainText(/Epic/i)
    await expect(main).not.toContainText('(derived')

    // Scales are explicit buttons, not zoom gestures.
    for (const scale of ['week', 'quarter', 'month'] as const) {
      await page.getByRole('button', { name: scale, exact: true }).click()
      await expect(page.getByRole('button', { name: scale, exact: true })).toHaveAttribute(
        'aria-pressed',
        'true',
      )
    }

    const release = fixture.milestones.find((milestone) => milestone.id === 'm-dev-release')
    expect(release).toBeTruthy()
    await page.getByLabel('Filter roadmap by milestone').click()
    await expect(page.getByRole('option', { name: 'No milestone' })).toBeVisible()
    await expect(page.getByRole('option', { name: release!.name })).toBeVisible()
    await expect(page.getByRole('option', { name: /Epic/i })).toHaveCount(0)
    await page.getByRole('option', { name: release!.name }).click()

    // Only the chosen milestone and its children remain; Unassigned is gone.
    await expect(page.locator('[data-roadmap-row="milestone"]')).toHaveCount(1)
    await expect(milestoneRow(page, release!.id)).toBeVisible()
    await expect(page.getByText('Unassigned', { exact: true })).toHaveCount(0)
    const children = fixture.tasks.filter(
      (task) => task.milestoneId === release!.id && task.deadlineAtMs !== null,
    )
    for (const child of children) {
      await expect(taskRow(page, child.id)).toBeVisible()
    }
  })

  test('a checkpoint marker edits the target date and stays a checkpoint', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const hardening = fixture.milestones.find((milestone) => milestone.id === 'm-dev-hardening')
    expect(hardening?.targetAtMs, 'the fixture needs a target-only milestone').not.toBeNull()
    const nextTarget = dateInputValue(hardening!.targetAtMs! + 21 * 86_400_000)

    await goto(page, server, '/projects/p-development/roadmap')
    await page.getByRole('button', { name: /^Hardening, milestone on/ }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name: 'Hardening' })).toBeVisible()
    await dialog.getByLabel('Target date').fill(nextTarget)
    await dialog.getByRole('button', { name: 'Save target' }).click()
    await expect(page.getByText('Milestone target updated')).toBeVisible()
    await expect(dialog).toBeHidden()

    await expect
      .poll(async () => (await milestoneRecord(page, server, 'm-dev-hardening')).target_at_ms)
      .toBe(localMidnight(nextTarget))

    // Still a marker: a target alone never becomes a duration bar.
    await expect(
      page.getByRole('button', { name: /^Hardening, milestone on .* change target date$/ }),
    ).toBeVisible()
    await expect(page.getByRole('button', { name: /^Hardening, .*open milestone$/ })).toHaveCount(0)
  })
})
