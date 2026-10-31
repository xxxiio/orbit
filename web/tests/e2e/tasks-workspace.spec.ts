/**
 * M33 corrective — the unified Tasks workspace.
 *
 * One Tasks tab owns three presentations over the same `useProjectTasks` read.
 * The search, status, priority and milestone filters are URL state shared by
 * List, Board and Backlog: switching a view keeps them, and each view applies
 * them to the same rows. The compact board card opens the inspector, movement
 * is the server's decision, and the density choice is remembered.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'
import type { Page } from '@playwright/test'

import { boardColumn, taskDetail } from './harness/workspace-reads'

async function taskStatus(page: Page, server: E2EServer, id: string): Promise<string> {
  return (await taskDetail(page, server, id)).status
}

function queryOf(page: Page): URLSearchParams {
  return new URL(page.url()).searchParams
}

test.describe('tasks workspace', () => {
  test('list, board and backlog share the filters in the URL and the same rows', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    // A milestone child that is backlog work with a priority: the four filters
    // below can all narrow to it, and it is visible in all three views.
    const task = fixture.tasks.find(
      (candidate) =>
        candidate.projectId === 'p-development' &&
        candidate.milestoneId === 'm-dev-reliability' &&
        ['inbox', 'planned', 'deferred'].includes(candidate.status) &&
        candidate.priority !== null,
    )
    expect(task, 'the fixture needs a prioritised backlog child of Reliability').toBeTruthy()
    const needle = task!.title
      .split(/\s+/)
      .find(
        (part) =>
          part.length > 4 &&
          fixture.tasks.filter(
            (candidate) =>
              candidate.projectId === 'p-development' &&
              candidate.title.toLowerCase().includes(part.toLowerCase()),
          ).length === 1,
      )
    expect(needle, 'the fixture needs a distinctive word in the child title').toBeTruthy()

    await goto(page, server, '/projects/p-development/tasks')

    // One control bar, one set of filters.
    await page.getByLabel('Search tasks in this project').fill(needle!)
    await page.getByLabel('Filter by status').click()
    await page.getByRole('option', { name: 'Planned', exact: true }).click()
    await page.getByLabel('Filter by priority').click()
    await page.getByRole('option', { name: 'Low', exact: true }).click()
    await page.getByLabel('Filter by milestone').click()
    await page.getByRole('option', { name: 'Reliability', exact: true }).click()

    let params = queryOf(page)
    expect(params.get('q')).toBe(needle)
    expect(params.get('status')).toBe('planned')
    expect(params.get('priority')).toBe('low')
    expect(params.get('milestone')).toBe('m-dev-reliability')
    expect(params.get('view'), 'the default list keeps no view parameter').toBeNull()

    // List: exactly the one task that matches all four filters.
    await expect(page.getByText('1 task · 1 open').first()).toBeVisible()
    await expect(page.getByRole('button', { name: task!.title, exact: true })).toBeVisible()

    // Board: same filters, same task, same Tasks tab.
    await page.getByRole('button', { name: 'Board', exact: true }).click()
    params = queryOf(page)
    expect(params.get('view')).toBe('board')
    expect([
      params.get('q'),
      params.get('status'),
      params.get('priority'),
      params.get('milestone'),
    ]).toEqual([needle, 'planned', 'low', 'm-dev-reliability'])
    const card = page.locator(`[data-board-card="${task!.id}"]`)
    await expect(card).toBeVisible()
    await expect(page.locator('[data-board-card]')).toHaveCount(1)
    await expect(
      page.getByRole('navigation', { name: 'Project sections' }).getByRole('link', { name: /^Tasks/ }),
    ).toHaveAttribute('aria-current', 'page')

    // Backlog: the planned task is backlog work, so the same filters still show it.
    await page.getByRole('button', { name: 'Backlog', exact: true }).click()
    params = queryOf(page)
    expect(params.get('view')).toBe('backlog')
    expect(params.get('q')).toBe(needle)
    expect(params.get('status')).toBe('planned')
    expect(params.get('priority')).toBe('low')
    expect(params.get('milestone')).toBe('m-dev-reliability')
    await expect(page.getByRole('heading', { name: 'Backlog', exact: true })).toBeVisible()
    await expect(page.getByText('1 task · 1 open').first()).toBeVisible()
    await expect(page.getByRole('button', { name: task!.title, exact: true })).toBeVisible()

    // Back to List: no filter was lost on the way around.
    await page.getByRole('button', { name: 'List', exact: true }).click()
    await expect(page.getByLabel('Search tasks in this project')).toHaveValue(needle!)
    await expect(page.getByLabel('Filter by status')).toContainText('Planned')
    await expect(page.getByLabel('Filter by priority')).toContainText('Low')
    await expect(page.getByLabel('Filter by milestone')).toContainText('Reliability')
    await expect(page.locator('table')).toBeVisible()

    await page.reload()
    await expect(page.getByLabel('Search tasks in this project')).toHaveValue(needle!)
    await expect(page.getByLabel('Filter by milestone')).toContainText('Reliability')
  })

  test('the milestone filter narrows the list, the board and the backlog', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const children = fixture.tasks.filter(
      (task) => task.projectId === 'p-development' && task.milestoneId === 'm-dev-reliability',
    )
    const backlogChildren = children.filter((task) =>
      ['inbox', 'planned', 'deferred'].includes(task.status),
    )
    // The Backlog header describes the project's backlog scope; the URL filter
    // narrows the rows beneath it.
    const projectBacklog = fixture.tasks.filter(
      (task) =>
        task.projectId === 'p-development' &&
        ['inbox', 'planned', 'deferred'].includes(task.status),
    )
    expect(children.length, 'the fixture needs children on Reliability').toBeGreaterThan(1)
    expect(backlogChildren.length, 'the fixture needs a backlog child').toBeGreaterThan(0)

    await goto(page, server, '/projects/p-development/tasks?milestone=m-dev-reliability')
    await expect(page.getByText(`${children.length} tasks ·`, { exact: false }).first()).toBeVisible()
    for (const child of children) {
      await expect(page.getByRole('button', { name: child.title, exact: true })).toBeVisible()
    }
    const outsider = fixture.tasks.find(
      (task) => task.projectId === 'p-development' && task.milestoneId === null,
    )
    expect(outsider).toBeTruthy()
    await expect(page.getByText(outsider!.title, { exact: true })).toHaveCount(0)

    await page.getByRole('button', { name: 'Board', exact: true }).click()
    await expect(page.locator('[data-board-card]')).toHaveCount(children.length)
    for (const child of children) {
      await expect(page.locator(`[data-board-card="${child.id}"]`)).toBeVisible()
    }

    await page.getByRole('button', { name: 'Backlog', exact: true }).click()
    await expect(
      page.getByText(`${projectBacklog.length} items`, { exact: false }).first(),
    ).toBeVisible()
    for (const child of backlogChildren) {
      await expect(page.getByRole('button', { name: child.title, exact: true })).toBeVisible()
    }
    // A backlog task outside the milestone is not smuggled in by the view switch.
    const otherBacklog = fixture.tasks.find(
      (task) =>
        task.projectId === 'p-development' &&
        task.milestoneId === null &&
        ['inbox', 'planned', 'deferred'].includes(task.status),
    )
    expect(otherBacklog).toBeTruthy()
    await expect(page.getByText(otherBacklog!.title, { exact: true })).toHaveCount(0)
  })

  test('a compact board card opens the inspector and hides raw ids', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) =>
        candidate.projectId === 'p-development' &&
        candidate.priority === 'high' &&
        !['done', 'cancelled'].includes(candidate.status),
    )
    expect(task, 'the fixture needs a high-priority Development task').toBeTruthy()

    await goto(page, server, '/projects/p-development/tasks?view=board')
    const card = page.locator(`[data-board-card="${task!.id}"]`)
    await expect(card).toBeVisible()
    // Compact by default: title, priority cue, maybe a date — never an id.
    await expect(card).toContainText(task!.title)
    await expect(card).not.toContainText(task!.id)
    await expect(card.getByRole('button', { name: new RegExp(`^Actions for `) })).toHaveCount(1)

    await card.getByRole('button', { name: task!.title, exact: true }).click()
    const drawer = page.getByRole('dialog')
    await expect(drawer.getByRole('heading', { name: task!.title })).toBeVisible()
    await expect(drawer.getByLabel('Task status')).toBeVisible()
    expect(new URL(page.url()).searchParams.get('task')).toBe(task!.id)
    await drawer.getByRole('button', { name: 'Close task details' }).click()
    await expect(drawer).toBeHidden()
  })

  test('a status change through the board is the server’s answer, not a local move', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) => candidate.projectId === 'p-development' && candidate.status === 'inbox',
    )
    expect(task, 'the fixture needs an inbox Development task').toBeTruthy()

    await goto(page, server, '/projects/p-development/tasks?view=board')
    const inbox = boardColumn(page, /^Inbox column/)
    await expect(inbox.locator(`[data-board-card="${task!.id}"]`)).toBeVisible()
    // Movement lives in the overflow, never on the card face.
    await expect(
      inbox.getByRole('button', { name: `Move to… ${task!.title}` }),
    ).toHaveCount(0)

    await page.getByRole('button', { name: `Actions for ${task!.title}` }).click()
    await page.getByRole('menuitem', { name: 'Planned', exact: true }).click()

    await expect(boardColumn(page, /^Planned column/).locator(`[data-board-card="${task!.id}"]`)).toBeVisible()
    await expect(inbox.locator(`[data-board-card="${task!.id}"]`)).toHaveCount(0)
    expect(await taskStatus(page, server, task!.id)).toBe('planned')

    await page.reload()
    await expect(
      boardColumn(page, /^Planned column/).locator(`[data-board-card="${task!.id}"]`),
    ).toBeVisible()
  })

  test('the compact/comfortable choice survives a reload', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) =>
        candidate.projectId === 'p-development' &&
        candidate.priority !== null &&
        !['done', 'cancelled'].includes(candidate.status),
    )
    expect(task).toBeTruthy()
    const priorityWord = { low: 'Low', medium: 'Medium', high: 'High', urgent: 'Urgent' }[
      task!.priority!
    ]

    await goto(page, server, '/projects/p-development/tasks?view=board')
    const density = page.getByRole('group', { name: 'Card density' })
    await expect(density.getByRole('button', { name: 'Compact', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    )

    await density.getByRole('button', { name: 'Comfortable', exact: true }).click()
    await expect(density.getByRole('button', { name: 'Comfortable', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(await page.evaluate(() => window.localStorage.getItem('orbit.board.density'))).toBe(
      'comfortable',
    )

    await page.reload()
    const reloaded = page.getByRole('group', { name: 'Card density' })
    await expect(
      reloaded.getByRole('button', { name: 'Comfortable', exact: true }),
    ).toHaveAttribute('aria-pressed', 'true')
    await expect(reloaded.getByRole('button', { name: 'Compact', exact: true })).toHaveAttribute(
      'aria-pressed',
      'false',
    )
    // Comfortable cards print the priority word; compact cards keep only the cue.
    const card = page.locator(`[data-board-card="${task!.id}"]`)
    await expect(card).toContainText(priorityWord)
  })
})
