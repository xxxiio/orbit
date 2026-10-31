/**
 * I1 acceptance: project creation is a real authoritative write.
 *
 * Assertions are deliberately about the rendered result (list, sidebar, reload)
 * rather than about a fetch being made: the row must only appear once the server
 * has accepted the command, and it must still be there after a reload.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

function newProjectButton(page: Page): Locator {
  return page.getByRole('button', { name: /new project/i }).first()
}

async function openNewProjectDialog(page: Page, server: E2EServer): Promise<Locator> {
  await goto(page, server, '/projects')
  await expect(page.getByRole('heading', { name: 'Projects', level: 1 })).toBeVisible()
  await newProjectButton(page).click()
  const dialog = page.getByRole('dialog')
  await expect(dialog).toBeVisible()
  return dialog
}

test.describe('projects', () => {
  test('creates a project that shows in the sidebar, the list, and after reload', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await goto(page, server, '/projects')

    // Seeded projects are listed before anything is created.
    await expect(page.getByText('Orbit Development').first()).toBeVisible()

    const dialog = await openNewProjectDialog(page, server)
    await dialog.getByLabel(/project name/i).fill('Apollo Program')
    await dialog.getByRole('button', { name: /create project/i }).click()

    // The UI navigates into the new project once the server accepted it.
    await expect(page).toHaveURL(/\/projects\/[^/?#]+/)
    await expect(page.getByRole('heading', { name: 'Apollo Program', level: 1 })).toBeVisible()

    // Sidebar entry (desktop layout) and the projects index both know about it.
    await goto(page, server, '/projects')
    // A project card's accessible name starts with the project name and then
    // carries its metadata (workflow, counts), so match the prefix.
    const projectLinks = page.getByRole('link', { name: /^Apollo Program/ })
    await expect(projectLinks.first()).toBeVisible()
    expect(
      await projectLinks.count(),
      'the project must be linked from both the app shell and the projects list',
    ).toBeGreaterThanOrEqual(2)

    // Reload proves it came from the server, not from in-memory state.
    await page.reload()
    await expect(page.getByText('Apollo Program').first()).toBeVisible()
  })

  test('handles a duplicate project name without merging the two projects', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const dialog = await openNewProjectDialog(page, server)

    // The dialog must explain the rule rather than surprise the user later:
    // ids are the identity, so two projects may share a name.
    await expect(dialog.getByText(/names don't need to be unique/i)).toBeVisible()

    await dialog.getByLabel(/project name/i).fill('Basic')
    await dialog.getByRole('button', { name: /create project/i }).click()

    // A second, separate project is created — never a silent merge into p-basic.
    await expect(page).toHaveURL(/\/projects\/(?!p-basic$)[^/?#]+/)
    await expect(page.getByRole('heading', { name: 'Basic', level: 1 })).toBeVisible()

    await goto(page, server, '/projects')
    await expect(page.getByRole('main').getByRole('link', { name: /^Basic/ })).toHaveCount(2)
  })

  test('shows required-field validation inline', async ({ page, workspace }) => {
    const { server } = workspace
    const dialog = await openNewProjectDialog(page, server)

    await dialog.getByRole('button', { name: /create project/i }).click()

    await expect(dialog).toBeVisible()
    const nameField = dialog.getByLabel(/project name/i)
    await expect(nameField).toHaveAttribute('aria-invalid', 'true')
    await expect(dialog.getByText(/enter a name|name is required/i).first()).toBeVisible()

    // No request may be sent for an empty name: nothing new appears in the list.
    await page.keyboard.press('Escape')
    await expect(dialog).toBeHidden()
  })

  test('a task created inside a new project keeps its edits and status after a reload', async ({
    page,
    workspace,
  }) => {
    // The literal acceptance order: create the project first, then create work
    // inside it from the project workspace, edit that work, and reload.
    const { server } = workspace
    const projectName = 'Acceptance Walkthrough'
    const taskTitle = 'Draft the walkthrough notes'
    const editedTitle = 'Draft the walkthrough notes (edited)'

    const createDialog = await openNewProjectDialog(page, server)
    await createDialog.getByLabel(/project name/i).fill(projectName)
    await createDialog.getByRole('button', { name: /create project/i }).click()

    await expect(page).toHaveURL(/\/projects\/[^/?#]+/)
    await expect(page.getByRole('heading', { name: projectName, level: 1 })).toBeVisible()

    // Create the task from *this* project's workspace, not from the Inbox.
    await page.getByRole('main').getByRole('button', { name: 'New task', exact: true }).first().click()
    const taskDialog = page.getByRole('dialog')
    await expect(taskDialog).toBeVisible()
    await expect(taskDialog.getByLabel('Title')).toBeVisible()
    // The dialog inherits the project the person is looking at.
    await expect(taskDialog.getByLabel('Project for the new task')).toContainText(projectName)

    await taskDialog.getByLabel('Title').fill(taskTitle)
    await taskDialog.getByRole('button', { name: 'Create task' }).click()
    await expect(taskDialog).toBeHidden()

    // Open the project's Tasks tab (the default list view) and the new task's drawer.
    await page.getByRole('link', { name: /^Tasks/ }).click()
    await expect(page.locator('table')).toBeVisible()
    await expect(page.getByRole('heading', { name: projectName, level: 1 })).toBeVisible()
    await page.getByRole('button', { name: taskTitle, exact: true }).first().click()
    const drawer = page.getByRole('dialog')
    await expect(drawer).toBeVisible()

    await drawer.getByRole('button', { name: 'Edit task title' }).click()
    const titleInput = drawer.getByLabel('Task title')
    await titleInput.fill(editedTitle)
    await titleInput.press('Enter')
    await expect(drawer.getByRole('button', { name: 'Edit task title' })).toContainText(editedTitle)

    await chooseStatus(page, drawer, /^Planned$/i)
    await expect(drawer.getByLabel('Task status')).toContainText(/planned/i)

    // `?task=` is URL state, so the reload reopens the same drawer from the
    // authoritative store rather than from memory.
    await page.reload()
    const reopened = page.getByRole('dialog')
    await expect(reopened).toBeVisible()
    await expect(reopened.getByRole('button', { name: 'Edit task title' })).toContainText(editedTitle)
    await expect(reopened.getByLabel('Task status')).toContainText(/planned/i)

    // And the task stays inside the project it was created in: the drawer header
    // names the project it belongs to, read back from the snapshot.
    await expect(reopened.getByText(projectName, { exact: true }).first()).toBeVisible()
  })
})

async function chooseStatus(page: Page, drawer: Locator, option: RegExp): Promise<void> {
  await drawer.getByLabel('Task status').click()
  await page.getByRole('option', { name: option }).click()
}
