/**
 * I2 acceptance — the task list and declared counts.
 *
 * Everything that changes the list is URL state; a declared count/filter must
 * read the same real rows its panel renders, never the project total.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'

test.describe('task list URL state', () => {
  test('filter, sort, columns and selection survive reload and a deep link', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const basic = fixture.tasks.filter((task) => task.projectId === 'p-basic')
    const blocked = basic.filter((task) => task.status === 'blocked')
    expect(blocked.length).toBeGreaterThan(0)
    const selected = blocked[0]
    // A word from a real blocked title, so the search filter is meaningful.
    const needle = selected.title.split(' ').at(-1)!
    const expected = blocked.filter((task) =>
      task.title.toLowerCase().includes(needle.toLowerCase()),
    )
    expect(expected.length).toBeGreaterThan(0)

    const query = new URLSearchParams({
      q: needle,
      status: 'blocked',
      sort: 'title',
      dir: 'desc',
      cols: 'title,status,priority,due,readiness,dependencies',
      sel: selected.id,
    })
    await goto(page, server, `/projects/p-basic/tasks?${query.toString()}`)

    const search = page.getByLabel('Search tasks in this project')
    await expect(search).toHaveValue(needle)
    await expect(page.getByLabel('Filter by status')).toContainText('Blocked')
    const table = page.locator('table')
    await expect(table.getByRole('columnheader', { name: 'Readiness' })).toBeVisible()
    await expect(table.getByRole('columnheader', { name: 'Dependencies' })).toBeVisible()

    const expectedTitles = expected.map((task) => task.title).sort((a, b) => b.localeCompare(a))
    const renderedTitles = await table.locator('tbody tr td button[data-row-action]').allTextContents()
    expect(renderedTitles).toEqual(expectedTitles)

    // The URL selection is real: the bulk bar is already active.
    await expect(page.getByRole('region', { name: 'Bulk actions' })).toContainText('1 selected')

    // The server is authoritative for readiness cells; the reason is named in
    // the product's vocabulary.
    const anyReason = table.locator('tbody td [data-readiness-reason]').first()
    await expect(anyReason).toBeVisible()

    await page.reload()
    await expect(page.getByLabel('Search tasks in this project')).toHaveValue(needle)
    await expect(page.getByLabel('Filter by status')).toContainText('Blocked')
    await expect(page.getByRole('region', { name: 'Bulk actions' })).toContainText('1 selected')
    await expect(page.locator('table').getByRole('columnheader', { name: 'Readiness' })).toBeVisible()

    // Changing the column set is also URL state.
    await page.getByRole('button', { name: /^Columns/ }).click()
    await page.getByRole('checkbox', { name: 'Show Readiness column' }).click()
    await expect(page.locator('table').getByRole('columnheader', { name: 'Readiness' })).toHaveCount(0)
    await expect(page).toHaveURL(/cols=[^&]*dependencies/)
    await page.reload()
    await expect(page.locator('table').getByRole('columnheader', { name: 'Readiness' })).toHaveCount(0)
    await expect(page.locator('table').getByRole('columnheader', { name: 'Dependencies' })).toBeVisible()
  })
})

test.describe('declared counts and filters', () => {
  test('a declared count and filter read the real rows, not the project total', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const basic = fixture.tasks.filter((task) => task.projectId === 'p-basic')
    const expected = {
      inProgress: basic.filter((task) => task.status === 'in_progress').length,
      done: basic.filter((task) => task.status === 'done').length,
    }
    expect(expected.inProgress, 'the fixture needs an in-progress p-basic task').toBeGreaterThan(0)
    expect(expected.done, 'the fixture needs a done p-basic task').toBeGreaterThan(0)
    expect(expected.inProgress).toBeLessThan(basic.length)

    await goto(page, server, '/projects/p-basic/overview')

    // Basic declares `count` metrics filtered by status. A count of the whole
    // project would overstate both; the declared filter must narrow real rows.
    const inProgressPanel = page.locator('[data-view-component="in_progress"]')
    await expect(
      inProgressPanel.getByRole('heading', { name: 'In progress', exact: true }),
    ).toBeVisible()
    await expect(
      inProgressPanel.getByText(
        `${expected.inProgress} ${expected.inProgress === 1 ? 'task' : 'tasks'}`,
        { exact: true },
      ),
    ).toBeVisible()

    const completedPanel = page.locator('[data-view-component="completed"]')
    await expect(
      completedPanel.getByRole('heading', { name: 'Completed', exact: true }),
    ).toBeVisible()
    await expect(
      completedPanel.getByText(`${expected.done} ${expected.done === 1 ? 'task' : 'tasks'}`, {
        exact: true,
      }),
    ).toBeVisible()

    // "Next actions" declares `filter: { actionable: true }` with a limit of 6.
    // The rows it shows must be exactly the server-actionable slice of the
    // project, cut to the declared limit — an ignored filter would list six
    // arbitrary tasks instead.
    const nextActions = page.locator('[data-view-component="next_actions"]')
    await expect(
      nextActions.getByRole('heading', { name: 'Next actions', exact: true }),
    ).toBeVisible()

    const readinessResponse = await page.request.get(
      `${server.base}/v1/authoritative/task-readiness?now_ms=${fixture.nowMs}`,
    )
    expect(readinessResponse.status()).toBe(200)
    const readiness = (await readinessResponse.json()) as {
      tasks: Array<{ task_id: string; actionable: boolean }>
    }
    const actionable = new Set(
      readiness.tasks.filter((entry) => entry.actionable).map((entry) => entry.task_id),
    )
    const expectedActionable = basic.filter((task) => actionable.has(task.id))
    expect(
      expectedActionable.length,
      'the fixture must leave some p-basic tasks outside the actionable filter',
    ).toBeLessThan(basic.length)

    const titles = await nextActions.locator('ul > li button').allTextContents()
    const expectedRows = Math.min(6, expectedActionable.length)
    expect(titles, 'the declared filter and limit decide the visible rows').toHaveLength(expectedRows)
    for (const title of titles) {
      const task = basic.find((candidate) => candidate.title === title)
      expect(task, `${title} must be a seeded p-basic task`).toBeTruthy()
      expect(actionable.has(task!.id), `${title} must be server-actionable`).toBe(true)
    }
    if (expectedRows === 0) {
      await expect(
        nextActions.getByText('No task matches actionable is true right now.'),
      ).toBeVisible()
    }
  })
})
