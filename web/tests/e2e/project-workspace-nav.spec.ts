/**
 * M33 — the project workspace offers List, Board, Backlog and Roadmap as
 * first-class project sections while keeping workflow-declared extra views.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'

test.describe('project workspace navigation', () => {
  test('the canonical sections are present once and keep their deep links', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/list')
    const nav = page.getByRole('navigation', { name: 'Project sections' })
    for (const label of ['Overview', 'List', 'Board', 'Backlog', 'Roadmap', 'Milestones']) {
      await expect(nav.getByRole('link', { name: new RegExp(`^${label}`) })).toHaveCount(1)
    }
    // A declared extra view survives the canonical set.
    await expect(nav.getByRole('link', { name: /Agent jobs/ })).toBeVisible()
    await expect(nav.getByRole('link', { name: /^List/ })).toHaveAttribute('aria-current', 'page')

    // The list is a real table with the Epic column and the shared filter bar.
    await expect(page.getByRole('columnheader', { name: 'Epic' })).toBeVisible()
    await expect(page.getByLabel('Search tasks in this project')).toBeVisible()

    // Selecting a group is URL-backed and reloads unchanged.
    await page.getByLabel('Group tasks').click()
    await page.getByRole('option', { name: 'Group: Epic' }).click()
    await expect(page).toHaveURL(/group=epic/)
    await page.reload()
    await expect(page.getByLabel('Group tasks')).toContainText('Epic')
  })

  test('backlog scopes to unscheduled work and defaults to epic grouping', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    await goto(page, server, '/projects/p-development/backlog')
    await expect(page.getByRole('heading', { name: 'Backlog' })).toBeVisible()
    await expect(page.getByLabel('Group tasks')).toContainText('Epic')
    // Only inbox/planned/deferred work appears.
    const backlog = fixture.tasks.filter(
      (task) =>
        task.projectId === 'p-development' &&
        ['inbox', 'planned', 'deferred'].includes(task.status),
    )
    expect(backlog.length).toBeGreaterThan(0)
    await expect(page.getByText(`${backlog.length} items`)).toBeVisible()
    await expect(page.getByText('No epic', { exact: true }).first()).toBeVisible()
  })

  test('the desktop board is unchanged and keeps the canonical section link', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/board')
    await expect(page.getByText('Task board')).toBeVisible()
    const nav = page.getByRole('navigation', { name: 'Project sections' })
    await expect(nav.getByRole('link', { name: /^Board/ })).toHaveAttribute('aria-current', 'page')
  })
})
