/**
 * I1 acceptance: saved views are personal presentation preferences stored on
 * the server.
 *
 * The subject is a view of `p-basic`, the one seeded project whose workflow
 * declares both a full task list (`/tasks`, display `list`) and a task board
 * (`/board`, display `board`). That makes the display half of a saved view
 * observable: applying the list view from the board has to move to the list.
 *
 * Every stored claim is a `GET /v1/authoritative/saved-views` read through the
 * browser's session; the UI assertions only cover what a person sees.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import { goto, openSecondBrowser } from './harness/session'
import {
  authorityRevision,
  blueprintBinding,
  readSavedViews,
  renameViewViaApi,
  type SavedViewRecord,
} from './harness/authority'

const PROJECT_ID = 'p-basic'
const LIST_PATH = `/projects/${PROJECT_ID}/list`
const BOARD_PATH = `/projects/${PROJECT_ID}/board`
/** The blocked task with high priority in the fixture (i=0 of the basic plan). */
const FILTERED_TASK = 'Tidy the downloads folder'
/** The other blocked task: medium priority, so the priority filter hides it. */
const HIDDEN_TASK = 'Tidy the bookmarks bar'

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

async function openSavedViews(page: Page): Promise<Locator> {
  await expect(page.locator('[data-saved-views-trigger]')).toBeVisible()
  await page.locator('[data-saved-views-trigger]').click()
  const dialog = page.locator('[data-saved-views]')
  await expect(dialog).toBeVisible()
  return dialog
}

/** Status + priority + sort through the toolbar; returns the URL's query string. */
async function applyFilters(page: Page): Promise<string> {
  await page.getByLabel('Filter by status').click()
  await page.getByRole('option', { name: 'Blocked', exact: true }).click()
  await page.getByLabel('Filter by priority').click()
  await page.getByRole('option', { name: 'High', exact: true }).click()
  await page.getByLabel('Sort tasks').click()
  await page.getByRole('option', { name: 'Sort: Title' }).click()

  // The filters are URL state: that string is what a saved view stores.
  const query = new URL(page.url()).search.replace(/^\?/, '')
  expect(query, 'status, priority and sort must all be in the URL').toBe(
    'status=blocked&priority=high&sort=title',
  )
  return query
}

/** Filter, save under `name`, and return the stored query string. */
async function saveFilteredView(page: Page, name: string): Promise<string> {
  const query = await applyFilters(page)
  const dialog = await openSavedViews(page)

  await dialog.locator('[data-saved-view-new]').click()
  await expect(dialog).toHaveAttribute('data-saved-view-mode', 'save')
  await dialog.locator('[data-saved-view-name-input]').fill(name)
  await dialog.locator('[data-saved-view-submit]').click()

  await expect(page.getByText('Saved view created', { exact: true })).toBeVisible()
  await expect(dialog).toHaveAttribute('data-saved-view-mode', 'list')
  const rows = dialog.locator('[data-saved-view]')
  await expect(rows).toHaveCount(1)
  await expect(rows.first()).toContainText(name)

  // Close it again: every test opens the control it is asserting on.
  await page
    .getByRole('dialog')
    .filter({ has: page.locator('[data-saved-views]') })
    .getByRole('button', { name: 'Close dialog' })
    .click()
  await expect(dialog).toBeHidden()
  return query
}

function viewRow(dialog: Locator, name: string): Locator {
  return dialog.locator('[data-saved-view]').filter({ hasText: name })
}

test.describe('saved views', () => {
  test('saves the filtered list with its query string and display', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, LIST_PATH)

    const query = await saveFilteredView(page, 'Blocked high work')

    // The stored record is the server's, with the exact URL query and display.
    const views = await readSavedViews(page, server)
    expect(views).toHaveLength(1)
    expect(views[0]).toMatchObject({
      name: 'Blocked high work',
      scope: 'project',
      project_id: PROJECT_ID,
      query,
      display: 'list',
      version: 1,
    })

    // And the control lists it back from the server.
    const dialog = await openSavedViews(page)
    await expect(viewRow(dialog, 'Blocked high work')).toContainText(query)
    await expect(viewRow(dialog, 'Blocked high work')).toContainText('v1')
  })

  test('restores filters, sort and the list display after a reload', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, LIST_PATH)
    const query = await saveFilteredView(page, 'Blocked high work')

    // Leave the list, come back through the board, and reload the app: the
    // saved view must not depend on this page instance.
    // A project tab carries its own badge count in the accessible name.
    await page.getByRole('link', { name: /^Board/ }).click()
    await expect(page).toHaveURL(new RegExp(`${escapeRegExp(BOARD_PATH)}$`))
    await page.reload()
    await expect(page.getByRole('link', { name: /^Board/ })).toHaveAttribute('aria-current', 'page')

    const dialog = await openSavedViews(page)
    await viewRow(dialog, 'Blocked high work').locator('[data-saved-view-apply]').click()
    await expect(dialog).toBeHidden()

    // Display: the stored `list` preference moves off the board to the list view.
    await expect(page).toHaveURL(new RegExp(`${escapeRegExp(LIST_PATH)}\\?${escapeRegExp(query)}$`))

    // Filters and sort are the stored ones, not the view's defaults.
    await expect(page.getByLabel('Filter by status')).toContainText('Blocked')
    await expect(page.getByLabel('Filter by priority')).toContainText('High')
    await expect(page.getByLabel('Sort tasks')).toContainText('Title')

    // The priority filter really narrows the two blocked tasks to the high one.
    await expect(page.getByRole('button', { name: FILTERED_TASK, exact: true })).toBeVisible()
    await expect(page.getByText(HIDDEN_TASK)).toHaveCount(0)
    await expect(page.getByText('1 task · 1 open')).toBeVisible()

    // A reload of the restored address keeps every part of the preference.
    await page.reload()
    await expect(page).toHaveURL(new RegExp(`\\?${escapeRegExp(query)}$`))
    await expect(page.getByLabel('Filter by status')).toContainText('Blocked')
    await expect(page.getByText('1 task · 1 open')).toBeVisible()
  })

  test('renames and deletes the view on the server', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, LIST_PATH)
    await saveFilteredView(page, 'Before rename')

    const dialog = await openSavedViews(page)

    await viewRow(dialog, 'Before rename').locator('[data-saved-view-rename]').click()
    const input = dialog.locator('[data-saved-view-name-input]')
    await expect(input).toHaveValue('Before rename')
    await input.fill('After rename')
    await dialog.locator('[data-saved-view-submit]').click()

    await expect(page.getByText('Saved view renamed', { exact: true })).toBeVisible()
    await expect(dialog).toHaveAttribute('data-saved-view-mode', 'list')
    await expect(viewRow(dialog, 'After rename')).toContainText('v2')

    let views: SavedViewRecord[] = await readSavedViews(page, server)
    expect(views.map((view) => view.name)).toEqual(['After rename'])
    expect(views[0].version).toBe(2)
    expect(views[0].query, 'a rename keeps the stored filters').toBe(
      'status=blocked&priority=high&sort=title',
    )

    await viewRow(dialog, 'After rename').locator('[data-saved-view-delete]').click()
    await expect(dialog).toHaveAttribute('data-saved-view-mode', 'delete')
    await dialog.locator('[data-saved-view-submit]').click()

    await expect(page.getByText('Saved view deleted', { exact: true })).toBeVisible()
    await expect(dialog.locator('[data-saved-views-state]')).toHaveAttribute(
      'data-saved-views-state',
      'empty',
    )
    views = await readSavedViews(page, server)
    expect(views, 'the deleted view is gone from the server').toEqual([])
  })

  test('survives a browser restart: a new context signs in and still sees it', async ({
    context,
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, LIST_PATH)
    const query = await saveFilteredView(page, 'Fresh browser view')

    // A second browser context: its own cookie jar, storage and in-memory cache.
    const second = await openSecondBrowser(context, server)
    try {
      await second.page.goto(`${server.base}${LIST_PATH}`)
      await expect(second.page.getByLabel('Filter by status')).toBeVisible()

      const dialog = await openSavedViews(second.page)
      await expect(dialog.locator('[data-saved-view]')).toHaveCount(1)
      await expect(viewRow(dialog, 'Fresh browser view')).toContainText(query)

      await viewRow(dialog, 'Fresh browser view').locator('[data-saved-view-apply]').click()
      await expect(second.page).toHaveURL(
        new RegExp(`${escapeRegExp(LIST_PATH)}\\?${escapeRegExp(query)}$`),
      )
      await expect(second.page.getByLabel('Filter by status')).toContainText('Blocked')
      await expect(
        second.page.getByRole('button', { name: FILTERED_TASK, exact: true }),
      ).toBeVisible()
    } finally {
      await second.context.close()
    }
  })

  test('saving a view changes neither the workflow binding nor the ledger', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, LIST_PATH)

    const bindingBefore = await blueprintBinding(page, server, PROJECT_ID)
    const revisionBefore = await authorityRevision(page, server)
    expect(bindingBefore, 'the project must have a pinned workflow').not.toBeNull()

    await saveFilteredView(page, 'No side effects')

    expect(await blueprintBinding(page, server, PROJECT_ID)).toEqual(bindingBefore)
    expect(await authorityRevision(page, server), 'a view never advances the ledger').toBe(
      revisionBefore,
    )
    expect((await readSavedViews(page, server)).map((view) => view.name)).toEqual(['No side effects'])
  })

  test('refuses a stale version with the server text and refreshes the list', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, LIST_PATH)
    await saveFilteredView(page, 'Stale candidate')

    const [stored] = await readSavedViews(page, server)
    // Another device renames first, with the version the open list still holds.
    const racing = await renameViewViaApi(page, server, {
      commandId: 'e2e-view-race',
      viewId: stored.id,
      expectedVersion: stored.version,
      name: 'Renamed elsewhere',
    })
    expect(racing.status, 'the racing rename is accepted').toBe(200)

    const dialog = await openSavedViews(page)
    // The list in this browser is still the pre-race version.
    await expect(viewRow(dialog, 'Stale candidate')).toContainText('v1')

    await viewRow(dialog, 'Stale candidate').locator('[data-saved-view-rename]').click()
    await dialog.locator('[data-saved-view-name-input]').fill('From the stale form')
    await dialog.locator('[data-saved-view-submit]').click()

    const error = dialog.locator('[data-saved-view-error]')
    await expect(error).toBeVisible()
    await expect(error).toContainText('saved view changed on the server')

    // Backing out shows the version that actually won: the list refreshed.
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click()
    await expect(dialog).toHaveAttribute('data-saved-view-mode', 'list')
    await expect(viewRow(dialog, 'Renamed elsewhere')).toContainText('v2')
    await expect(viewRow(dialog, 'Stale candidate')).toHaveCount(0)

    const views = await readSavedViews(page, server)
    expect(views.map((view) => view.name)).toEqual(['Renamed elsewhere'])
  })
})
