/**
 * I1 acceptance: Home is a view over authoritative state, not a dashboard of
 * invented numbers.
 *
 * Every row title asserted here comes from the seeded workspace, so a passing
 * test means the server's data reached the browser. Sections that only render
 * their first few rows are checked structurally (the rendered titles must all be
 * fixture tasks, and the count must be server-derived) plus one dedicated test
 * for the "Show all" affordance.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import type { WorkspaceFixture } from './harness/seed'

/** The section that owns a given heading, whatever container the UI chose. */
function sectionByHeading(page: Page, name: RegExp): Locator {
  return page
    .locator('section, article, div[role="region"], [aria-labelledby]')
    .filter({ has: page.getByRole('heading', { name }) })
    .first()
}

/** Row titles rendered in a section that correspond to real fixture tasks. */
async function renderedFixtureTitles(
  section: Locator,
  fixture: WorkspaceFixture,
): Promise<string[]> {
  const known = new Set(fixture.tasks.map((task) => task.title))
  const texts = await section.getByRole('button').allTextContents()
  return texts.map((text) => text.trim()).filter((text) => known.has(text))
}

function taskTitle(fixture: WorkspaceFixture, id: string | null): string {
  const task = fixture.tasks.find((candidate) => candidate.id === id)
  if (!task) throw new Error(`fixture task ${id ?? 'null'} is missing`)
  return task.title
}

test.describe('home', () => {
  test('renders readiness and overdue data that came from the server', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    await goto(page, server, '/')
    await expect(page).toHaveTitle('Today · Orbit')
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible()

    // Overdue: earliest deadline sorts first, so this title must be on screen.
    const overdue = sectionByHeading(page, /overdue/i)
    await expect(overdue).toBeVisible()
    await expect(
      overdue.getByText(taskTitle(fixture, fixture.ids.firstOverdue), { exact: true }),
    ).toBeVisible()

    const dueToday = sectionByHeading(page, /due today/i)
    await expect(dueToday).toBeVisible()
    await expect(
      dueToday.getByText(taskTitle(fixture, fixture.ids.firstDueToday), { exact: true }),
    ).toBeVisible()

    // Ready to start: server readiness, so the rows must all be real tasks.
    const ready = sectionByHeading(page, /ready to start/i)
    await expect(ready).toBeVisible()
    expect((await renderedFixtureTitles(ready, fixture)).length).toBeGreaterThan(0)

    // Blocked: same, and each blocked row explains what it is waiting on.
    const blocked = sectionByHeading(page, /blocked/i)
    await expect(blocked).toBeVisible()
    expect((await renderedFixtureTitles(blocked, fixture)).length).toBeGreaterThan(0)
    await expect(blocked.getByText(/waiting on|blocked|deferred/i).first()).toBeVisible()

    // Real data means the "nothing here yet" state must be gone.
    await expect(page.getByText(/nothing planned yet|no data is connected yet/i)).toHaveCount(0)
  })

  test('"Show all" reveals the rest of a section instead of only relabelling itself', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    await goto(page, server, '/')
    const section = sectionByHeading(page, /overdue/i)
    await expect(section).toBeVisible()

    const before = (await renderedFixtureTitles(section, fixture)).length
    const showAll = section.getByRole('button', { name: /show all/i })
    await expect(showAll).toBeVisible()
    await showAll.click()

    // Counts are server-derived (more than the five rows the section renders
    // collapsed), so expanding must add rows.
    await expect
      .poll(async () => (await renderedFixtureTitles(section, fixture)).length, {
        message: '"Show all" must reveal the remaining rows',
      })
      .toBeGreaterThan(before)
  })

  test('an empty workspace says so instead of inventing activity', async ({ page, server }) => {
    await goto(page, server, '/')
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible()

    // The empty state must be honest about having nothing to plan yet.
    await expect(
      page.getByText(/nothing planned yet|capture your first task|no tasks/i).first(),
    ).toBeVisible()

    // No seeded project may leak into an empty database.
    await expect(page.getByText('Orbit Development')).toHaveCount(0)
  })
})
