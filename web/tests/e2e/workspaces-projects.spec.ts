/**
 * I2 acceptance — dependencies, readiness and project settings.
 *
 * The drawer's prerequisite commands and the settings upgrade path both read
 * back from the same server; a refusal is quoted, never paraphrased.
 */
import { expect, test } from './harness/fixtures'
import { goto, signInViaApi } from './harness/session'
import { dependencies, tasksFor } from './harness/workspace-reads'

test.describe('dependencies and readiness', () => {
  test('add and remove a prerequisite, with the server readiness reason named in product copy', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await signInViaApi(page, server)
    const edges = await dependencies(page, server)
    const touched = new Set(edges.flatMap((edge) => [edge.task_id, edge.prerequisite_id]))
    const tasks = await tasksFor(page, server, 'p-basic')

    const target = tasks.find(
      (task) =>
        !touched.has(task.id) &&
        (task.status === 'ready' || task.status === 'planned' || task.status === 'in_progress'),
    )
    const candidate = tasks.find(
      (task) =>
        task.id !== target?.id &&
        !touched.has(task.id) &&
        task.status !== 'done' &&
        task.status !== 'cancelled',
    )
    expect(target, 'a dependency-free task in p-basic').toBeTruthy()
    expect(candidate, 'an eligible prerequisite in p-basic').toBeTruthy()

    await goto(page, server, `/projects/p-basic/tasks?task=${target!.id}`)
    const drawer = page.getByRole('dialog')
    await expect(drawer).toBeVisible()
    const section = drawer.getByRole('region', { name: 'Dependencies' })
    await expect(section.getByText('Unblocks', { exact: false })).toBeVisible()
    await expect(section.getByText(/Prerequisite depth/)).toBeVisible()

    await section.getByLabel('Add a prerequisite').click()
    await page.getByRole('option', { name: candidate!.title, exact: true }).click()
    await section.getByRole('button', { name: 'Add', exact: true }).click()

    const remove = section.getByRole('button', {
      name: `Remove prerequisite ${candidate!.title}`,
      exact: true,
    })
    await expect(remove).toBeVisible()
    // The server's readiness reason is named in the product's vocabulary, never
    // as its raw enum value.
    await expect(section.getByText('Waiting on unfinished prerequisites.', { exact: true })).toBeVisible()
    await expect(section.getByText('unfinished_prerequisites', { exact: true })).toHaveCount(0)

    await page.reload()
    const reopened = page.getByRole('dialog')
    await expect(reopened).toBeVisible()
    const reopenedSection = reopened.getByRole('region', { name: 'Dependencies' })
    await expect(
      reopenedSection.getByRole('button', {
        name: `Remove prerequisite ${candidate!.title}`,
        exact: true,
      }),
    ).toBeVisible()

    await reopenedSection
      .getByRole('button', { name: `Remove prerequisite ${candidate!.title}`, exact: true })
      .click()
    await expect(
      reopenedSection.getByRole('button', {
        name: `Remove prerequisite ${candidate!.title}`,
        exact: true,
      }),
    ).toHaveCount(0)

    const remaining = await dependencies(page, server)
    expect(
      remaining.some((edge) => edge.task_id === target!.id && edge.prerequisite_id === candidate!.id),
    ).toBe(false)
  })
})

test.describe('project settings', () => {
  test('shows the pinned version and the editable identity, and changing the workflow updates the tabs', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    await goto(page, server, '/projects/p-development/settings')
    await expect(page.getByRole('heading', { name: 'Development', level: 1 })).toBeVisible()
    // Identity is an authoritative form now; the id itself stays immutable.
    await expect(page.locator('[data-project-identity]')).toBeVisible()
    await expect(page.locator('[data-project-name-input]')).toHaveValue('Development')
    await expect(page.getByText(/This id never changes/)).toBeVisible()
    await expect(page.getByText('Development v3', { exact: true }).first()).toBeVisible()
    await expect(page.getByRole('link', { name: /^Milestones/ })).toBeVisible()

    // Pick the Basic workflow; the preview is a read, so the tabs are unchanged.
    await page.getByRole('radio', { name: /^Basic/ }).check()
    await expect(page.getByText(/^fingerprint /)).toBeVisible()
    await expect(page.getByText(/Views added:/)).toBeVisible()
    await expect(page.getByRole('link', { name: /^Milestones/ })).toBeVisible()

    await page.getByRole('button', { name: 'Change workflow…' }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: 'Change workflow', exact: true }).click()
    await expect(dialog).toBeHidden()

    // The project now follows Basic: its extra Agent jobs view goes, while the
    // canonical List/Board/Backlog/Roadmap/Milestones remain available.
    await expect(page.getByRole('link', { name: /^List/ })).toBeVisible()
    await expect(page.getByRole('link', { name: /^Agent jobs/ })).toHaveCount(0)
    await expect(page.getByText('Basic v1', { exact: true }).first()).toBeVisible()

    await page.reload()
    await expect(page.getByRole('link', { name: /^List/ })).toBeVisible()
    await expect(page.getByRole('link', { name: /^Agent jobs/ })).toHaveCount(0)
  })
})
