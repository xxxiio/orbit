/**
 * I2 acceptance — milestone chrome in the workspace.
 *
 * Create, rename, transition and delete go through real commands; the illegal
 * edge is the server's refusal, quoted with the stored status below it proving
 * nothing was applied.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'

test.describe('milestones', () => {
  test('create, rename, transition and an invalid edge the server refuses', async ({ page, workspace }) => {
    const { server } = workspace
    const name = 'Acceptance checkpoint'
    const renamed = 'Acceptance checkpoint II'

    await goto(page, server, '/projects/p-development/milestones')
    await expect(page.getByRole('heading', { name: 'Development', level: 1 })).toBeVisible()

    await page.getByRole('button', { name: 'New milestone' }).first().click()
    const createDialog = page.getByRole('dialog')
    await expect(createDialog).toBeVisible()
    await createDialog.getByLabel('Milestone name').fill(name)
    await createDialog.getByRole('button', { name: 'Create milestone' }).click()
    await expect(createDialog).toBeHidden()

    const progress = page
      .locator('section')
      .filter({ has: page.getByRole('heading', { name: 'Progress', exact: true }) })

    const milestonesResponse = await page.request.get(`${server.base}/v1/authoritative/milestones`)
    expect(milestonesResponse.status()).toBe(200)
    const created = (
      (await milestonesResponse.json()) as { milestones: Array<{ id: string; name: string }> }
    ).milestones.find((milestone) => milestone.name === name)
    expect(created, 'the server must have stored the milestone').toBeTruthy()
    const row = progress.locator(`[data-milestone-row="${created!.id}"]`)
    await expect(row).toBeVisible()
    await expect(row.getByLabel(`Status for ${name}`)).toContainText('Planned')

    await row.getByRole('button', { name: 'Rename' }).click()
    const rename = row.getByLabel(`New name for ${name}`)
    await rename.fill(renamed)
    await row.getByRole('button', { name: 'Save' }).click()
    await expect(progress.getByText(renamed, { exact: true })).toBeVisible()

    const row2 = progress.locator(`[data-milestone-row="${created!.id}"]`)
    const status = row2.getByLabel(`Status for ${renamed}`)
    await status.click()
    await page.getByRole('option', { name: 'In progress', exact: true }).click()
    await expect(status).toContainText('In progress')

    // in_progress → planned is illegal; the stored status must survive.
    await status.click()
    await page.getByRole('option', { name: 'Planned', exact: true }).click()
    const refusal = row2.getByRole('alert')
    await expect(refusal).toBeVisible()
    // The API's exact refusal text is quoted, and the stored status below
    // proves nothing was applied.
    await expect(refusal).toHaveText(
      'The server did not allow that milestone status change. invalid milestone status transition',
    )
    await expect(status).toContainText('In progress')

    await page.reload()
    const reloadedRow = page
      .locator('section')
      .filter({ has: page.getByRole('heading', { name: 'Progress', exact: true }) })
      .locator(`[data-milestone-row="${created!.id}"]`)
    await expect(reloadedRow.getByLabel(`Status for ${renamed}`)).toContainText('In progress')

    // Delete is a real API command; with no tasks attached it is accepted.
    await reloadedRow.getByRole('button', { name: 'Delete', exact: true }).click()
    await reloadedRow.getByRole('button', { name: 'Delete', exact: true }).click()
    await expect(page.getByText(renamed, { exact: true })).toHaveCount(0)
  })
})
