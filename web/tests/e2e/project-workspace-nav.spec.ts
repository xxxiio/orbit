/**
 * M33 corrective — one Tasks tab, three URL views, declared views intact.
 *
 * The corrected workspace has four canonical sections (Overview, Tasks,
 * Roadmap, Milestones) plus whatever the pinned workflow declares. List, Board
 * and Backlog are not top-level entries: they are the `?view=` of the one Tasks
 * route. A workflow `board`/`tasks` view becomes that tab's configuration only
 * when *every* one of its components reads tasks; a mixed view (the Development
 * board also has a Work Item board) keeps its own section and address. The
 * legacy `/list`, `/board` and `/backlog` addresses redirect to the matching
 * view only when no declared section owns the address.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'
import type { Page } from '@playwright/test'

/** A legacy address as a Tasks address, without dropping unrelated params. */
function tasksUrl(pageUrl: string, server: E2EServer, projectId: string): URL {
  const url = new URL(pageUrl)
  expect(url.origin).toBe(new URL(server.base).origin)
  expect(url.pathname).toBe(`/projects/${projectId}/tasks`)
  return url
}

function projectUrl(page: Page, server: E2EServer, projectId: string, section: string): URL {
  const url = new URL(page.url())
  expect(url.origin).toBe(new URL(server.base).origin)
  expect(url.pathname).toBe(`/projects/${projectId}/${section}`)
  return url
}

function sectionNav(page: Page) {
  return page.getByRole('navigation', { name: 'Project sections' })
}

test.describe('project workspace navigation', () => {
  test('the canonical sections are present once and the declared Board keeps its entry', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/tasks')
    const nav = sectionNav(page)

    for (const label of ['Overview', 'Tasks', 'Roadmap', 'Milestones']) {
      await expect(
        nav.getByRole('link', { name: new RegExp(`^${label}`) }),
        `${label} must be one canonical section`,
      ).toHaveCount(1)
    }
    await expect(nav.locator('a[href$="/tasks"]')).toHaveCount(1)
    await expect(nav.getByRole('link', { name: /^Tasks/ })).toHaveAttribute('aria-current', 'page')

    // Development declares a board that also carries a Work Item board. It is a
    // mixed view, so it is not consumed into the Tasks tab: the Board section
    // keeps its own entry and address beside the one Tasks entry.
    const boardLink = nav.getByRole('link', { name: /^Board/ })
    await expect(boardLink).toHaveCount(1)
    await expect(boardLink).toHaveAttribute('href', '/projects/p-development/board')
    // List and Backlog are URL views of Tasks, never sections.
    await expect(nav.locator('a[href$="/list"], a[href$="/backlog"]')).toHaveCount(0)
    await expect(nav.getByRole('link', { name: /^(List|Backlog)\b/ })).toHaveCount(0)

    // A workflow-declared extra view survives the canonical set.
    await expect(nav.getByRole('link', { name: /Agent jobs/ })).toBeVisible()

    // The default view is the list: real table, no Epic column, shared filters.
    await expect(page.getByRole('columnheader', { name: 'Milestone' })).toBeVisible()
    await expect(page.getByRole('columnheader', { name: 'Epic' })).toHaveCount(0)
    await expect(page.getByLabel('Search tasks in this project')).toBeVisible()
    await expect(page.getByRole('button', { name: 'List', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    )

    // Grouping is milestone/status/priority only, and still URL-backed.
    await page.getByLabel('Group tasks').click()
    await expect(page.getByRole('option', { name: 'Group: Epic' })).toHaveCount(0)
    await page.getByRole('option', { name: 'Group: Milestone' }).click()
    await expect(page).toHaveURL(/group=milestone/)
    await page.reload()
    await expect(page.getByLabel('Group tasks')).toContainText('Milestone')
  })

  test('the Development board stays a declared section with both panels', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/board')

    // No redirect: `/board` is the Board section for this project.
    const url = projectUrl(page, server, 'p-development', 'board')
    expect(url.search).toBe('')
    await expect(sectionNav(page).getByRole('link', { name: /^Board/ })).toHaveAttribute(
      'aria-current',
      'page',
    )

    // Both declared surfaces render: the task board and the Work Item board.
    const taskBoard = page.locator('[data-view-component="task_board"]')
    const workBoard = page.locator('[data-view-component="work_board"]')
    await expect(taskBoard).toHaveCount(1)
    await expect(workBoard).toHaveCount(1)
    await expect(taskBoard.getByRole('heading', { name: 'Task board', exact: true })).toBeVisible()
    await expect(taskBoard.locator('[data-board-action]').first()).toBeVisible()
    await expect(workBoard.getByRole('heading', { name: 'Work items', exact: true })).toBeVisible()
    // The seed deliberately has no Work Items in Development; the honest board
    // says so instead of rendering a blank surface.
    await expect(workBoard.getByText('No work items here yet', { exact: true })).toBeVisible()

    // The Tasks tab is still a single entry beside the declared Board.
    const nav = sectionNav(page)
    await expect(nav.locator('a[href$="/tasks"]')).toHaveCount(1)
    await expect(nav.getByRole('link', { name: /^Board/ })).toHaveCount(1)

    // The declared Board is reachable from its own entry too, and that never
    // turns into a Tasks view.
    await page.getByRole('link', { name: /^Tasks/ }).click()
    await expect(page).toHaveURL(/\/projects\/p-development\/tasks$/)
    await page.getByRole('link', { name: /^Board/ }).click()
    await expect(page).toHaveURL(/\/projects\/p-development\/board$/)
    await expect(page.locator('[data-view-component="work_board"]')).toBeVisible()
  })

  test('legacy list and backlog redirect while a pure task board still redirects', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const base = '/projects/p-development'

    // `/list` is the default view, so the redirect leaves a clean Tasks address.
    await goto(page, server, `${base}/list`)
    await expect(page).toHaveURL(new RegExp(`${base}/tasks$`))
    const listUrl = tasksUrl(page.url(), server, 'p-development')
    expect(listUrl.search, 'the default view keeps no `view` parameter').toBe('')
    await expect(page.locator('table')).toBeVisible()
    await expect(sectionNav(page).getByRole('link', { name: /^Tasks/ })).toHaveAttribute(
      'aria-current',
      'page',
    )

    // `/backlog` becomes `?view=backlog`; the other filters are preserved.
    await page.goto(`${server.base}${base}/backlog?status=inbox&q=replay&priority=high`)
    await expect(page).toHaveURL(/\/tasks\?/)
    const backlogUrl = tasksUrl(page.url(), server, 'p-development')
    expect(backlogUrl.searchParams.get('view')).toBe('backlog')
    expect(backlogUrl.searchParams.get('status')).toBe('inbox')
    expect(backlogUrl.searchParams.get('q')).toBe('replay')
    expect(backlogUrl.searchParams.get('priority')).toBe('high')
    await expect(page.getByRole('heading', { name: 'Backlog', exact: true })).toBeVisible()
    await expect(page.getByLabel('Search tasks in this project')).toHaveValue('replay')
    await expect(page.getByLabel('Filter by status')).toContainText('Inbox')
    await expect(page.getByLabel('Filter by priority')).toContainText('High')

    // `p-basic` declares a pure task board: it *is* consumed, so its `/board`
    // address redirects to the Tasks board view.
    await page.goto(`${server.base}/projects/p-basic/board`)
    await expect(page).toHaveURL(/\/projects\/p-basic\/tasks\?/)
    const basicUrl = tasksUrl(page.url(), server, 'p-basic')
    expect(basicUrl.searchParams.get('view')).toBe('board')
    await expect(page.getByRole('group', { name: 'Card density' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Board', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    await expect(page.locator('table')).toHaveCount(0)
    await expect(sectionNav(page).getByRole('link', { name: /^Tasks/ })).toHaveAttribute(
      'aria-current',
      'page',
    )
    // A consumed board is a Tasks view, not a second section.
    await expect(sectionNav(page).getByRole('link', { name: /^Board/ })).toHaveCount(0)
  })

  test('switching task views never adds a section entry or loses the Tasks tab', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/tasks')
    const nav = sectionNav(page)
    // The workspace first paints the fallback sections and swaps in the pinned
    // workflow's views when that read resolves; wait for the declared entry so
    // the baseline is the final section list.
    await expect(nav.getByRole('link', { name: /^Tasks/ })).toBeVisible()
    await expect(nav.getByRole('link', { name: /^Board/ })).toBeVisible()
    const sectionNames = async () =>
      nav.locator('a').evaluateAll((nodes) =>
        nodes.map((node) => (node.textContent ?? '').trim().replace(/\s+/g, ' ')),
      )
    const before = await sectionNames()
    expect(before.join(' '), 'the declared Board is part of the section list').toMatch(/Board/)

    for (const view of ['Board', 'Backlog', 'List'] as const) {
      await page.getByRole('button', { name: view, exact: true }).click()
      await expect(page.getByRole('button', { name: view, exact: true })).toHaveAttribute(
        'aria-pressed',
        'true',
      )
      await expect(nav.getByRole('link', { name: /^Tasks/ })).toHaveAttribute('aria-current', 'page')
      expect(await sectionNames(), `${view} must not add a section entry`).toEqual(before)
    }

    // The Tasks board is the compact task board only: the mixed declared board
    // is not silently pulled in as its configuration.
    await page.getByRole('button', { name: 'Board', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Board', exact: true })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    await expect(page.locator('[data-view-component="work_board"]')).toHaveCount(0)
    await expect(page.locator('[data-board-action]').first()).toBeVisible()
  })
})
