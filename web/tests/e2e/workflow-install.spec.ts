/**
 * I5-02 acceptance: a clean server can install a bundled workflow from the
 * browser and create a project on it without reloading the page.
 *
 * The `server` fixture is deliberately unseeded: this is the state a person
 * meets after `orbit-server` starts for the first time.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'

test.describe('bundled workflow install', () => {
  test('New project installs Development and creates a project on it', async ({
    page,
    server,
  }) => {
    await goto(page, server, '/projects')
    await page.getByRole('button', { name: 'New project' }).first().click()
    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('heading', { name: 'New project' })).toBeVisible()

    // The install control is part of the dialog; the bundled list comes from
    // `GET /blueprints/bundled`, not from what is already published.
    await expect(dialog.getByText('Bundled workflows')).toBeVisible()
    const install = dialog.getByRole('button', { name: 'Install Development' })
    await expect(install).toBeVisible()
    await install.click()

    // Same dialog, same page: the installed workflow becomes selectable and
    // drops out of the “not installed yet” list.
    await expect(dialog.getByRole('button', { name: 'Install Development' })).toHaveCount(0)
    const choice = dialog.getByRole('radio', { name: /Development/ })
    await expect(choice).toBeVisible()
    await choice.check()

    await dialog.getByLabel('Project name').fill('Installed from the dialog')
    await dialog.getByRole('button', { name: 'Create project' }).click()

    await expect(page).toHaveURL(/\/projects\/[^/]+$/, { timeout: 20_000 })
    const projectId = new URL(page.url()).pathname.split('/').filter(Boolean)[1]
    await expect(page.getByRole('heading', { level: 1, name: 'Installed from the dialog' })).toBeVisible()
    await expect(page.getByText('Development v3', { exact: true }).first()).toBeVisible()

    // The server, not the badge alone, holds the binding.
    const binding = await page.request.get(
      `${server.base}/v1/authoritative/blueprints/binding?project_id=${projectId}`,
    )
    expect(binding.status()).toBe(200)
    expect(await binding.json()).toMatchObject({ blueprint_id: 'development', version: 3 })

    // The same explicit install control is in project settings, where the rest
    // of the pinned workflow is managed.
    await page.goto(`${server.base}/projects/${projectId}/settings`)
    await expect(page.getByText('Bundled workflows')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Install Learning' })).toBeVisible()
  })
})
