/**
 * I1 acceptance: one server search across projects, tasks and knowledge.
 *
 * The marker is seeded through the public API in this run, so a result can only
 * exist because the server matched it. Grouping, order and subtitles are
 * asserted against the server's own answer as well as the rendered palette:
 * the client is not allowed to invent a group, a count or a title.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { expectSignedOut, goto, signInViaApi } from './harness/session'
import {
  noteDocument,
  postAuthority,
  readProjects,
  search,
} from './harness/authority'

const MARKER = 'quasarquill'
const PROJECT_ID = 'p-search-marker'
const TASK_ID = 't-search-marker'
const NOTE_ID = 'n-search-marker'
const NOTE_PATH = 'notes/quasarquill.md'
const MOVED_PATH = 'notes/quasarquill-moved.md'

function palette(page: Page): Locator {
  return page.locator('[data-command-palette]')
}

function searchInput(page: Page): Locator {
  return page.locator('[data-search-query]')
}

/**
 * One project, one task and one note that share the marker. Written through the
 * same routes the app uses, before the first navigation, so the app's first
 * snapshot already contains the project.
 */
async function seedMarker(page: Page, server: E2EServer): Promise<void> {
  await signInViaApi(page, server)

  const projects = await readProjects(page, server)
  const project = await postAuthority(page, server, 'project-commands', {
    command_id: 'e2e-search-project',
    expected_revision: projects.revision,
    project_id: PROJECT_ID,
    name: 'Quasarquill Project',
  })
  expect(project.status, 'seeding the marker project').toBe(200)

  const task = await postAuthority(page, server, 'task-commands', {
    command_id: 'e2e-search-task',
    expected_revision: project.body?.revision,
    action: { type: 'create', task_id: TASK_ID, title: 'Quasarquill task', project_id: PROJECT_ID },
  })
  expect(task.status, 'seeding the marker task').toBe(200)

  // Notes carry their own revision and are published to the vault.
  const note = await postAuthority(page, server, 'markdown-commands', {
    command_id: 'e2e-search-note',
    command: {
      kind: 'Create',
      note_id: NOTE_ID,
      path: NOTE_PATH,
      body: '# Quasarquill note\n\nThe marker note for the unified search acceptance test.\n',
    },
  })
  expect(note.status, 'seeding the marker note').toBe(200)
}

/** The task drawer, addressed by its own close control (its title changes). */
function taskDrawer(page: Page): Locator {
  return page
    .getByRole('dialog')
    .filter({ has: page.getByRole('button', { name: 'Close task details' }) })
}

/** The shell is mounted once the sidebar's search control is on screen. */
async function waitForApp(page: Page): Promise<void> {
  await expect(page.getByRole('button', { name: /Search or jump to/ })).toBeVisible()
}

/** Open the palette with the keyboard and type the marker. */
async function openWithMarker(page: Page): Promise<Locator> {
  await waitForApp(page)
  await page.keyboard.press('Control+k')
  await expect(palette(page)).toBeVisible()
  await searchInput(page).fill(MARKER)
  await expect(palette(page).locator('[data-search-state]')).toHaveAttribute(
    'data-search-state',
    'ready',
  )
  return palette(page)
}

test.describe('unified search', () => {
  test('groups one result per kind with the server labels', async ({ page, workspace }) => {
    const { server } = workspace
    await seedMarker(page, server)
    await page.goto(`${server.base}/`)
    await expect(page).toHaveTitle('Today · Orbit')

    const panel = await openWithMarker(page)

    // The server's own answer: three groups, always in this order, one row each.
    const api = await search(page, server, MARKER)
    expect(api.groups.map((group) => group.kind)).toEqual(['project', 'task', 'note'])
    expect(api.groups.map((group) => group.results.length)).toEqual([1, 1, 1])

    await expect(panel.locator('[data-search-group]')).toHaveCount(3)
    await expect(panel.locator('[data-search-group="project"]')).toContainText('Projects')
    await expect(panel.locator('[data-search-group="task"]')).toContainText('Tasks')
    await expect(panel.locator('[data-search-group="note"]')).toContainText('Knowledge')

    await expect(panel.locator('[data-search-result]')).toHaveCount(3)
    await expect(panel.locator(`[data-search-result="project:${PROJECT_ID}"]`)).toContainText(
      'Quasarquill Project',
    )
    await expect(panel.locator(`[data-search-result="task:${TASK_ID}"]`)).toContainText(
      'Quasarquill task',
    )
    await expect(panel.locator(`[data-search-result="note:${NOTE_ID}"]`)).toContainText(NOTE_PATH)

    // Rows returned, not a fabricated match total.
    await expect(panel.locator('[data-search-count]')).toHaveText(
      `3 results for “${MARKER}”`,
    )
  })

  test('each result opens its own destination and the keyboard returns to search', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await seedMarker(page, server)
    await page.goto(`${server.base}/`)

    // Project → the project workspace.
    let panel = await openWithMarker(page)
    await panel.locator(`[data-search-result="project:${PROJECT_ID}"]`).click()
    await expect(page).toHaveURL(new RegExp(`/projects/${PROJECT_ID}$`))
    await expect(
      page.getByRole('heading', { level: 1, name: 'Quasarquill Project' }),
    ).toBeVisible()

    // Task → the drawer on the URL, opened from the palette with the keyboard.
    panel = await openWithMarker(page)
    await page.keyboard.press('ArrowDown')
    await expect(panel.locator(`[data-search-result="task:${TASK_ID}"]`)).toHaveAttribute(
      'aria-selected',
      'true',
    )
    await page.keyboard.press('Enter')
    await expect(page).toHaveURL(new RegExp(`/projects/${PROJECT_ID}\\?task=${TASK_ID}$`))
    await expect(taskDrawer(page)).toContainText('Quasarquill task')

    // Close the drawer, then return to search with the keyboard.
    await page.keyboard.press('Escape')
    await expect(taskDrawer(page)).toBeHidden()

    // Note → the knowledge note.
    panel = await openWithMarker(page)
    await panel.locator(`[data-search-result="note:${NOTE_ID}"]`).click()
    await expect(page).toHaveURL(new RegExp(`/knowledge/${NOTE_PATH}$`))
    await expect(
      page.getByText('The marker note for the unified search acceptance test.'),
    ).toBeVisible()
  })

  test('keeps the command list for an empty query and is honest about no matches', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await seedMarker(page, server)
    await page.goto(`${server.base}/`)

    const trigger = page.getByRole('button', { name: /Search or jump to/ })
    await trigger.focus()
    await page.keyboard.press('Control+k')

    // Empty query: the commands, not a server search.
    await expect(palette(page)).toBeVisible()
    await expect(palette(page).locator('[data-search-state]')).toHaveAttribute(
      'data-search-state',
      'commands',
    )
    await expect(palette(page).getByRole('option', { name: 'New task' })).toBeVisible()
    await expect(palette(page).getByRole('option', { name: 'Go to Inbox' })).toBeVisible()

    // Escape closes and puts focus back on the control that had it.
    await page.keyboard.press('Escape')
    await expect(palette(page)).toBeHidden()
    await expect(trigger).toBeFocused()

    // A query with no matches says so; it does not fall back to commands.
    await page.keyboard.press('Control+k')
    await expect(palette(page)).toBeVisible()
    await searchInput(page).fill('zzzz-no-such-result')
    await expect(palette(page).locator('[data-search-state]')).toHaveAttribute(
      'data-search-state',
      'ready',
    )
    await expect(palette(page)).toContainText('No matches for “zzzz-no-such-result”')
    await expect(palette(page).locator('[data-search-result]')).toHaveCount(0)

    // ↑/↓ move the active row and Enter runs it: the second row is the task.
    await searchInput(page).fill(MARKER)
    await expect(palette(page).locator('[data-search-result]')).toHaveCount(3)
    await expect(palette(page).locator(`[data-search-result="project:${PROJECT_ID}"]`)).toHaveAttribute(
      'aria-selected',
      'true',
    )
    await page.keyboard.press('ArrowDown')
    await expect(palette(page).locator(`[data-search-result="task:${TASK_ID}"]`)).toHaveAttribute(
      'aria-selected',
      'true',
    )
    await page.keyboard.press('ArrowUp')
    await expect(palette(page).locator(`[data-search-result="project:${PROJECT_ID}"]`)).toHaveAttribute(
      'aria-selected',
      'true',
    )
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    await expect(page).toHaveURL(new RegExp(`task=${TASK_ID}$`))
    await expect(taskDrawer(page)).toContainText('Quasarquill task')
  })

  test('refuses unauthenticated search and sends the app back to the login form', async ({
    page,
    server,
  }) => {
    // A real session first, so the cleared cookie is the only difference.
    await goto(page, server, '/')
    await expect(page).toHaveTitle('Today · Orbit')

    await page.context().clearCookies()
    const refused = await page.request.get(`${server.base}/v1/authoritative/search?q=x`)
    expect(refused.status(), 'search without a session').toBe(401)

    await page.goto(`${server.base}/`)
    await expectSignedOut(page, server)
    await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toBeVisible()
  })

  test('a result whose note moved away lands on an honest not-found state', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await seedMarker(page, server)
    await page.goto(`${server.base}/`)

    const pageErrors: string[] = []
    page.on('pageerror', (error) => pageErrors.push(error.message))

    // The palette holds the result; then the target stops existing at that path.
    const panel = await openWithMarker(page)
    await expect(panel.locator(`[data-search-result="note:${NOTE_ID}"]`)).toBeVisible()
    const document = await noteDocument(page, server, NOTE_PATH)
    const moved = await postAuthority(page, server, 'markdown-commands', {
      command_id: 'e2e-search-move-note',
      command: {
        kind: 'Move',
        note_id: NOTE_ID,
        from: NOTE_PATH,
        to: MOVED_PATH,
        expected: document.body,
      },
    })
    expect(moved.status, 'moving the note out of the searched path').toBe(200)
    const gone = await page.request.get(
      `${server.base}/v1/authoritative/markdown-document?path=${encodeURIComponent(NOTE_PATH)}`,
    )
    expect(gone.status(), 'the searched path must now be absent').toBe(404)

    // Activating the stale row is a normal navigation to a missing note.
    await panel.locator(`[data-search-result="note:${NOTE_ID}"]`).click()
    await expect(page).toHaveURL(new RegExp(`/knowledge/${NOTE_PATH}$`))
    await expect(page.getByText('This note could not be found')).toBeVisible()
    await expect(
      page.getByRole('heading', { level: 1, name: 'Knowledge' }),
    ).toBeVisible()
    expect(pageErrors, 'a stale result must not throw').toEqual([])
  })
})
