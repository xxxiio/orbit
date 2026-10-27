/**
 * I1 acceptance: project rename and status editing are authoritative writes.
 *
 * Every claim about stored state is a server read through the browser's own
 * session; every claim about what the person sees is a rendered assertion. The
 * rename test additionally proves the three read surfaces (header, sidebar,
 * page title) update from the accepted record before any reload, and that the
 * project id — the identity — never moves.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'
import {
  projectById,
  projectStatuses,
  readProjects,
  updateProject,
  type ProjectStatusEdge,
} from './harness/authority'

/** The presentation labels the settings control uses for each server token. */
const STATUS_LABELS: Record<string, string> = {
  proposed: 'Proposed',
  active: 'Active',
  on_hold: 'On hold',
  completed: 'Completed',
  cancelled: 'Cancelled',
  archived: 'Archived',
}

function label(status: string): string {
  return STATUS_LABELS[status] ?? status
}

function identityForm(page: Page): Locator {
  return page.locator('[data-project-identity]')
}

function nameInput(page: Page): Locator {
  return page.locator('[data-project-name-input]')
}

function statusSelect(page: Page): Locator {
  return page.locator('[data-project-status-select]')
}

function pageHeader(page: Page): Locator {
  return page.locator('header').filter({ has: page.getByRole('heading', { level: 1 }) })
}

/** Open a project's settings and wait until the form holds its stored values. */
async function openSettings(
  page: Page,
  server: E2EServer,
  projectId: string,
  name: string,
): Promise<void> {
  await goto(page, server, `/projects/${projectId}/settings`)
  await expect(page.getByRole('heading', { level: 1, name })).toBeVisible()
  await expect(identityForm(page)).toBeVisible()
  await expect(nameInput(page)).toHaveValue(name)
}

function edgeFor(statuses: ProjectStatusEdge[], status: string): ProjectStatusEdge {
  const edge = statuses.find((candidate) => candidate.status === status)
  expect(edge, `the server must report a transition table for ${status}`).toBeTruthy()
  return edge as ProjectStatusEdge
}

test.describe('project settings', () => {
  test('renames a project on every surface without a reload, and keeps its id', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-basic'
    const renamed = 'Basic Renamed'

    await openSettings(page, server, projectId, 'Basic')
    const before = await readProjects(page, server)
    const idsBefore = before.projects.map((project) => project.id).sort()
    expect((await projectById(page, server, projectId)).name).toBe('Basic')

    await nameInput(page).fill(renamed)
    await identityForm(page).locator('[data-project-save]').click()

    // The accepted record is reported, not a local echo.
    await expect(page.getByText('Project updated', { exact: true })).toBeVisible()

    // Header, page title and sidebar row all read the same accepted project.
    await expect(page.getByRole('heading', { level: 1, name: renamed })).toBeVisible()
    await expect(page).toHaveTitle(`${renamed} · Orbit`)
    await expect(
      page.getByRole('complementary', { name: 'Workspace' }).getByRole('link', { name: renamed }),
    ).toBeVisible()

    // The projects list is a client-side navigation: no reload, still renamed.
    await page.getByRole('navigation', { name: 'Primary' }).getByRole('link', { name: 'Projects' }).click()
    await expect(page).toHaveURL(/\/projects$/)
    await expect(page.getByRole('main').getByRole('link', { name: new RegExp(`^${renamed}`) })).toBeVisible()

    // Reload: the name came from the server, and it is the same project.
    await page.reload()
    await expect(page.getByRole('main').getByRole('link', { name: new RegExp(`^${renamed}`) })).toBeVisible()

    const after = await readProjects(page, server)
    expect(after.projects.map((project) => project.id).sort()).toEqual(idsBefore)
    const stored = after.projects.find((project) => project.id === projectId)
    expect(stored?.name, 'the stored name is the accepted one').toBe(renamed)
    expect(
      after.projects.filter((project) => project.name === renamed),
      'a rename must not create a second project',
    ).toHaveLength(1)
    expect(after.revision, 'one rename is exactly one ledger write').toBe(before.revision + 1)
  })

  test('moves the project status to a server-reported target and keeps it', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-basic'

    await openSettings(page, server, projectId, 'Basic')
    const stored = await projectById(page, server, projectId)
    const statuses = await projectStatuses(page, server)
    const edges = edgeFor(statuses, stored.status)
    // The target comes from the server's own table, not from a client list.
    expect(edges.next).toContain('active')

    await statusSelect(page).click()
    await page.getByRole('option', { name: label('active'), exact: true }).click()
    await identityForm(page).locator('[data-project-save]').click()
    await expect(page.getByText('Project updated', { exact: true })).toBeVisible()

    // The status badge in the project header is the stored status.
    await expect(pageHeader(page).getByText('Active', { exact: true })).toBeVisible()
    await expect(statusSelect(page)).toContainText('Active')

    await page.reload()
    await expect(page.locator('[data-project-status-select]')).toContainText('Active')
    await expect(pageHeader(page).getByText('Active', { exact: true })).toBeVisible()
    expect((await projectById(page, server, projectId)).status).toBe('active')
  })

  test('offers only the server transition edges and refuses an illegal move', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-basic'

    await openSettings(page, server, projectId, 'Basic')
    const before = await readProjects(page, server)
    const stored = await projectById(page, server, projectId)
    const edges = edgeFor(await projectStatuses(page, server), stored.status)

    // The control offers the stored status plus exactly the reported targets.
    await statusSelect(page).click()
    const offered = await page.getByRole('option').allTextContents()
    await page.keyboard.press('Escape')
    expect(offered).toEqual([stored.status, ...edges.next].map(label))

    // A target the server does not report for this status is not offered.
    const illegal = (await projectStatuses(page, server))
      .flatMap((edge) => edge.next)
      .find((candidate) => !edges.next.includes(candidate) && candidate !== stored.status)
    expect(illegal, 'the transition table must have an unreachable status').toBeTruthy()
    await statusSelect(page).click()
    await expect(page.getByRole('option', { name: label(illegal as string), exact: true })).toHaveCount(0)
    await page.keyboard.press('Escape')

    // The same route the form posts to refuses the illegal move, in words, and
    // writes nothing: the status and the ledger revision stay as they were.
    const refused = await updateProject(page, server, {
      commandId: 'e2e-illegal-status',
      projectId,
      expectedRevision: before.revision,
      status: illegal as string,
    })
    expect(refused.status, 'an illegal transition is a refusal').toBe(400)
    expect(String(refused.body?.error)).toMatch(
      new RegExp(`invalid project status transition: ${stored.status} -> ${illegal}`),
    )

    const after = await readProjects(page, server)
    expect(after.revision, 'a refusal writes nothing').toBe(before.revision)
    expect(after.projects.find((project) => project.id === projectId)?.status).toBe(stored.status)
    await page.reload()
    await expect(statusSelect(page)).toContainText(label(stored.status))

    // A status with no successors keeps the control read-only instead of
    // inventing a move: archived is terminal in the server's table.
    const research = await projectById(page, server, 'p-research')
    const researchEdges = edgeFor(await projectStatuses(page, server), research.status)
    expect(researchEdges.next).toContain('archived')
    const archived = await updateProject(page, server, {
      commandId: 'e2e-archive-research',
      projectId: 'p-research',
      expectedRevision: (await readProjects(page, server)).revision,
      status: 'archived',
    })
    expect(archived.status).toBe(200)
    expect(edgeFor(await projectStatuses(page, server), 'archived').next).toHaveLength(0)

    await goto(page, server, '/projects/p-research/settings')
    await expect(page.getByRole('heading', { level: 1, name: 'Research' })).toBeVisible()
    await expect(statusSelect(page)).toBeDisabled()
    await expect(
      page.getByText('The server reports no further status changes from here.'),
    ).toBeVisible()
  })

  test('reports a stale revision, refreshes, and never claims success', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-basic'
    const serverName = 'Basic From Server'
    const draftName = 'Basic From Browser'

    await openSettings(page, server, projectId, 'Basic')
    // The form (and the cached snapshot behind its fence) is fully loaded now.
    await expect(statusSelect(page)).toBeVisible()
    const before = await readProjects(page, server)

    // Another device wins the race with the *current* revision.
    const racing = await updateProject(page, server, {
      commandId: 'e2e-racing-update',
      projectId,
      expectedRevision: before.revision,
      name: serverName,
    })
    expect(racing.status, 'the racing update is accepted').toBe(200)
    expect(racing.body?.revision).toBe(before.revision + 1)

    // The open form still fences on the revision it read, so the save is refused.
    await nameInput(page).fill(draftName)
    await identityForm(page).locator('[data-project-save]').click()

    const error = page.locator('[data-project-identity-error]')
    await expect(error).toBeVisible()
    await expect(error).toContainText(/this project changed on another device/i)
    await expect(page.getByText('Project updated', { exact: true })).toHaveCount(0)

    // The refresh affordance reads the record again: the header shows the
    // server's name, and the stored state is the server's, not the draft.
    await error.getByRole('button', { name: 'Refresh project' }).click()
    await expect(page.getByRole('heading', { level: 1, name: serverName })).toBeVisible()
    await expect(page).toHaveTitle(`${serverName} · Orbit`)

    const after = await readProjects(page, server)
    expect(after.revision, 'the refused save wrote nothing').toBe(before.revision + 1)
    expect(after.projects.find((project) => project.id === projectId)?.name).toBe(serverName)
    await page.reload()
    await expect(page.getByRole('heading', { level: 1, name: serverName })).toBeVisible()
  })

  test('cancel discards the draft and writes nothing', async ({ page, workspace }) => {
    const { server } = workspace
    const projectId = 'p-basic'

    await openSettings(page, server, projectId, 'Basic')
    const before = await readProjects(page, server)

    await nameInput(page).fill('Discarded Rename')
    await statusSelect(page).click()
    await page.getByRole('option', { name: 'Active', exact: true }).click()
    await identityForm(page).locator('[data-project-cancel]').click()

    // The form returns to the stored record.
    await expect(nameInput(page)).toHaveValue('Basic')
    await expect(statusSelect(page)).toContainText('Proposed')

    const after = await readProjects(page, server)
    expect(after.revision, 'Cancel sends no command').toBe(before.revision)
    const stored = after.projects.find((project) => project.id === projectId)
    expect(stored?.name).toBe('Basic')
    expect(stored?.status).toBe('proposed')
  })

  test('refuses an empty or blank name and keeps the stored name', async ({ page, workspace }) => {
    const { server } = workspace
    const projectId = 'p-basic'

    await openSettings(page, server, projectId, 'Basic')
    const before = await readProjects(page, server)
    const save = identityForm(page).locator('[data-project-save]')

    await nameInput(page).fill('')
    await expect(page.getByText('A project needs a name.')).toBeVisible()
    await expect(nameInput(page)).toHaveAttribute('aria-invalid', 'true')
    // Refused before sending: the control cannot submit, so no command exists.
    await expect(save).toBeDisabled()

    await nameInput(page).fill('   ')
    await expect(page.getByText('A project needs a name.')).toBeVisible()
    await expect(save).toBeDisabled()

    const after = await readProjects(page, server)
    expect(after.revision, 'a blank name never reaches the server').toBe(before.revision)
    expect(after.projects.find((project) => project.id === projectId)?.name).toBe('Basic')
    await page.reload()
    await expect(page.getByRole('heading', { level: 1, name: 'Basic' })).toBeVisible()
  })
})
