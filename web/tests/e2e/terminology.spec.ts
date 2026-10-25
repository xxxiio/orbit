/**
 * I1 acceptance: user-facing terminology and per-route titles.
 *
 * "Blueprint" is an internal implementation word. It may appear in API paths and
 * Rust type names, never in rendered UI text. These assertions read the rendered
 * page (and `document.title`), not the source.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { Page } from '@playwright/test'

/**
 * Primary routes and the titles the app deliberately sets. Home is the "Today"
 * surface (its h1 is the date), so it is checked by title rather than by name.
 */
const ROUTES: readonly { path: string; title: string; heading?: string }[] = [
  { path: '/', title: 'Today · Orbit' },
  { path: '/inbox', title: 'Inbox · Orbit', heading: 'Inbox' },
  { path: '/projects', title: 'Projects · Orbit', heading: 'Projects' },
  { path: '/agents', title: 'Agents · Orbit', heading: 'Agents' },
]

async function expectNoBlueprint(page: Page, where: string): Promise<void> {
  await expect(page.locator('body'), `${where} must not render the internal term`).not.toContainText(
    /Blueprint/i,
  )
  expect(await page.title(), `${where} document title must not use the internal term`).not.toMatch(
    /Blueprint/i,
  )
}

test.describe('terminology', () => {
  test('primary routes never render "Blueprint" and set their own title', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    for (const route of ROUTES) {
      await goto(page, server, route.path)
      if (route.heading) {
        await expect(page.getByRole('heading', { name: route.heading, level: 1 })).toBeVisible()
      } else {
        await expect(page.getByRole('heading', { level: 1 })).toBeVisible()
      }
      await expect(page).toHaveTitle(route.title)
      await expectNoBlueprint(page, route.path)
    }
  })

  test('the project workspace keeps the same vocabulary', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const project = fixture.projects[0]

    await goto(page, server, `/projects/${project.id}`)
    await expect(page.getByRole('heading', { name: project.name, level: 1 })).toBeVisible()
    await expect(page).toHaveTitle(new RegExp(`${project.name} · Orbit`))
    await expectNoBlueprint(page, `/projects/${project.id}`)

    // The workflow is presented as "Workflow", including its pinned version.
    await expect(page.getByText(/workflow/i).first()).toBeVisible()
  })
})
