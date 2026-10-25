/**
 * I2 acceptance: the workflow-driven workspace.
 *
 * Every assertion is about the rendered result and, where a write is involved,
 * the authoritative state read back from the same server. Board validity,
 * milestone edges and readiness are the server's decisions; these specs check
 * that the UI sends the command, shows the answer and never moves anything on
 * its own.
 */
import type { Locator, Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto, signInViaApi } from './harness/session'

interface Edge {
  task_id: string
  prerequisite_id: string
}

interface AuthoritativeTask {
  id: string
  status: string
  title: string
}

async function tasksFor(page: Page, server: E2EServer, projectId: string): Promise<AuthoritativeTask[]> {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks`)
  expect(response.status()).toBe(200)
  const body = (await response.json()) as { tasks: Array<AuthoritativeTask & { project_id?: string }> }
  return body.tasks.filter((task) => task.project_id === projectId)
}

async function taskDetail(page: Page, server: E2EServer, id: string): Promise<AuthoritativeTask> {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks/${id}`)
  expect(response.status()).toBe(200)
  return (await response.json()) as AuthoritativeTask
}

async function dependencies(page: Page, server: E2EServer): Promise<Edge[]> {
  const response = await page.request.get(`${server.base}/v1/authoritative/task-dependencies`)
  expect(response.status()).toBe(200)
  return ((await response.json()) as { edges: Edge[] }).edges
}

function boardColumn(page: Page, label: RegExp): Locator {
  return page.getByRole('region', { name: label })
}

test.describe('board', () => {
  test('an accepted keyboard move follows the server and survives a reload', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) => candidate.projectId === 'p-basic' && candidate.status === 'inbox',
    )
    expect(task, 'the fixture needs an inbox task in p-basic').toBeTruthy()

    await goto(page, server, '/projects/p-basic/board')
    const inbox = boardColumn(page, /^Inbox column/)
    const planned = boardColumn(page, /^Planned column/)

    const card = inbox.getByRole('button', { name: task!.title, exact: true })
    await expect(card).toBeVisible()

    // Keyboard parity: the card is one tab stop and h/j/k/l move the cursor.
    await card.focus()
    await page.keyboard.press('l')
    await expect(page.locator('button[data-board-action]:focus')).not.toHaveCount(0)

    await goto(page, server, '/projects/p-basic/board')
    await card.focus()
    await page.keyboard.press('m')
    const menu = page.getByRole('menu')
    await expect(menu).toBeVisible()
    await expect(menu.getByRole('menuitem')).toHaveCount(9)
    await page.getByRole('menuitem', { name: 'Planned', exact: true }).click()

    // The card is in the new column because the server moved it, not because
    // the client did.
    await expect(planned.getByRole('button', { name: task!.title, exact: true })).toBeVisible()
    await expect(inbox.getByRole('button', { name: task!.title, exact: true })).toHaveCount(0)
    expect((await taskDetail(page, server, task!.id)).status).toBe('planned')

    await page.reload()
    await expect(page.getByRole('heading', { name: 'Basic', level: 1 })).toBeVisible()
    await expect(
      boardColumn(page, /^Planned column/).getByRole('button', { name: task!.title, exact: true }),
    ).toBeVisible()
  })

  test('a rejected move keeps the card where it was and quotes the server', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) => candidate.projectId === 'p-basic' && candidate.status === 'inbox',
    )
    expect(task, 'the fixture needs an inbox task in p-basic').toBeTruthy()

    await goto(page, server, '/projects/p-basic/board')
    const inbox = boardColumn(page, /^Inbox column/)
    await inbox
      .getByRole('button', { name: `Move ${task!.title} to another status` })
      .click()
    // `inbox → done` is not a legal transition; only the server may say so.
    await page.getByRole('menuitem', { name: 'Done', exact: true }).click()

    const alert = inbox.getByRole('alert')
    await expect(alert).toBeVisible()
    // The client frames the refusal; the quoted reason is exactly what the API
    // sent, with no generic "invalid" fallback.
    await expect(alert).toHaveText(
      'The server did not allow that status change from Inbox. invalid task status transition: inbox -> done',
    )

    // The card never moved and nothing was written.
    await expect(inbox.getByRole('button', { name: task!.title, exact: true })).toBeVisible()
    expect((await taskDetail(page, server, task!.id)).status).toBe('inbox')

    await page.reload()
    await expect(
      boardColumn(page, /^Inbox column/).getByRole('button', { name: task!.title, exact: true }),
    ).toBeVisible()
  })
})

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
  test('shows the pinned version, refuses to invent rename, and changing the workflow updates the tabs', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    await goto(page, server, '/projects/p-development/settings')
    await expect(page.getByRole('heading', { name: 'Development', level: 1 })).toBeVisible()
    await expect(page.getByText(/Not supported by the API yet/).first()).toBeVisible()
    await expect(page.getByText('Development v2', { exact: true }).first()).toBeVisible()
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

    // The project now follows Basic: Tasks appears, Milestones and Agent jobs go.
    await expect(page.getByRole('link', { name: /^Tasks/ })).toBeVisible()
    await expect(page.getByRole('link', { name: /^Milestones/ })).toHaveCount(0)
    await expect(page.getByText('Basic v1', { exact: true }).first()).toBeVisible()

    await page.reload()
    await expect(page.getByRole('link', { name: /^Tasks/ })).toBeVisible()
    await expect(page.getByRole('link', { name: /^Milestones/ })).toHaveCount(0)
  })
})

test.describe('task list URL state', () => {
  test('filter, sort, columns and selection survive reload and a deep link', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const basic = fixture.tasks.filter((task) => task.projectId === 'p-basic')
    const blocked = basic.filter((task) => task.status === 'blocked')
    expect(blocked.length).toBeGreaterThan(0)
    const selected = blocked[0]
    // A word from a real blocked title, so the search filter is meaningful.
    const needle = selected.title.split(' ').at(-1)!
    const expected = blocked.filter((task) =>
      task.title.toLowerCase().includes(needle.toLowerCase()),
    )
    expect(expected.length).toBeGreaterThan(0)

    const query = new URLSearchParams({
      q: needle,
      status: 'blocked',
      sort: 'title',
      dir: 'desc',
      cols: 'title,status,priority,due,readiness,dependencies',
      sel: selected.id,
    })
    await goto(page, server, `/projects/p-basic/tasks?${query.toString()}`)

    const search = page.getByLabel('Search tasks in this project')
    await expect(search).toHaveValue(needle)
    await expect(page.getByLabel('Filter by status')).toContainText('Blocked')
    const table = page.locator('table')
    await expect(table.getByRole('columnheader', { name: 'Readiness' })).toBeVisible()
    await expect(table.getByRole('columnheader', { name: 'Dependencies' })).toBeVisible()

    const expectedTitles = expected.map((task) => task.title).sort((a, b) => b.localeCompare(a))
    const renderedTitles = await table.locator('tbody tr td button[data-row-action]').allTextContents()
    expect(renderedTitles).toEqual(expectedTitles)

    // The URL selection is real: the bulk bar is already active.
    await expect(page.getByRole('region', { name: 'Bulk actions' })).toContainText('1 selected')

    // The server is authoritative for readiness cells; the reason is named in
    // the product's vocabulary.
    const anyReason = table.locator('tbody td [data-readiness-reason]').first()
    await expect(anyReason).toBeVisible()

    await page.reload()
    await expect(page.getByLabel('Search tasks in this project')).toHaveValue(needle)
    await expect(page.getByLabel('Filter by status')).toContainText('Blocked')
    await expect(page.getByRole('region', { name: 'Bulk actions' })).toContainText('1 selected')
    await expect(page.locator('table').getByRole('columnheader', { name: 'Readiness' })).toBeVisible()

    // Changing the column set is also URL state.
    await page.getByRole('button', { name: /^Columns/ }).click()
    await page.getByRole('checkbox', { name: 'Show Readiness column' }).click()
    await expect(page.locator('table').getByRole('columnheader', { name: 'Readiness' })).toHaveCount(0)
    await expect(page).toHaveURL(/cols=[^&]*dependencies/)
    await page.reload()
    await expect(page.locator('table').getByRole('columnheader', { name: 'Readiness' })).toHaveCount(0)
    await expect(page.locator('table').getByRole('columnheader', { name: 'Dependencies' })).toBeVisible()
  })
})

test.describe('declared counts and filters', () => {
  test('a declared count and filter read the real rows, not the project total', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const basic = fixture.tasks.filter((task) => task.projectId === 'p-basic')
    const expected = {
      inProgress: basic.filter((task) => task.status === 'in_progress').length,
      done: basic.filter((task) => task.status === 'done').length,
    }
    expect(expected.inProgress, 'the fixture needs an in-progress p-basic task').toBeGreaterThan(0)
    expect(expected.done, 'the fixture needs a done p-basic task').toBeGreaterThan(0)
    expect(expected.inProgress).toBeLessThan(basic.length)

    await goto(page, server, '/projects/p-basic/overview')

    // Basic declares `count` metrics filtered by status. A count of the whole
    // project would overstate both; the declared filter must narrow real rows.
    const inProgressPanel = page.locator('[data-view-component="in_progress"]')
    await expect(
      inProgressPanel.getByRole('heading', { name: 'In progress', exact: true }),
    ).toBeVisible()
    await expect(
      inProgressPanel.getByText(
        `${expected.inProgress} ${expected.inProgress === 1 ? 'task' : 'tasks'}`,
        { exact: true },
      ),
    ).toBeVisible()

    const completedPanel = page.locator('[data-view-component="completed"]')
    await expect(
      completedPanel.getByRole('heading', { name: 'Completed', exact: true }),
    ).toBeVisible()
    await expect(
      completedPanel.getByText(`${expected.done} ${expected.done === 1 ? 'task' : 'tasks'}`, {
        exact: true,
      }),
    ).toBeVisible()

    // "Next actions" declares `filter: { actionable: true }` with a limit of 6.
    // The rows it shows must be exactly the server-actionable slice of the
    // project, cut to the declared limit — an ignored filter would list six
    // arbitrary tasks instead.
    const nextActions = page.locator('[data-view-component="next_actions"]')
    await expect(
      nextActions.getByRole('heading', { name: 'Next actions', exact: true }),
    ).toBeVisible()

    const readinessResponse = await page.request.get(
      `${server.base}/v1/authoritative/task-readiness?now_ms=${fixture.nowMs}`,
    )
    expect(readinessResponse.status()).toBe(200)
    const readiness = (await readinessResponse.json()) as {
      tasks: Array<{ task_id: string; actionable: boolean }>
    }
    const actionable = new Set(
      readiness.tasks.filter((entry) => entry.actionable).map((entry) => entry.task_id),
    )
    const expectedActionable = basic.filter((task) => actionable.has(task.id))
    expect(
      expectedActionable.length,
      'the fixture must leave some p-basic tasks outside the actionable filter',
    ).toBeLessThan(basic.length)

    const titles = await nextActions.locator('ul > li button').allTextContents()
    const expectedRows = Math.min(6, expectedActionable.length)
    expect(titles, 'the declared filter and limit decide the visible rows').toHaveLength(expectedRows)
    for (const title of titles) {
      const task = basic.find((candidate) => candidate.title === title)
      expect(task, `${title} must be a seeded p-basic task`).toBeTruthy()
      expect(actionable.has(task!.id), `${title} must be server-actionable`).toBe(true)
    }
    if (expectedRows === 0) {
      await expect(
        nextActions.getByText('No task matches actionable is true right now.'),
      ).toBeVisible()
    }
  })
})

interface ManifestComponent {
  id: string
  type: string
  title: string | null
  source: string | null
}

interface ManifestView {
  id: string
  title: string
  components: ManifestComponent[]
}

/** The four declared types this build names as unavailable. */
const NAMED_REFUSAL_TYPES = ['habit_tracker', 'metric_history', 'knowledge_refs', 'agent_jobs']

function componentHeading(component: ManifestComponent): string {
  return component.title?.trim() || 'View panel'
}

test.describe('workflow view registry', () => {
  test('every bundled workflow renders all of its declared views through one path', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    await signInViaApi(page, server)

    const projects = [
      'p-basic',
      'p-development',
      'p-research',
      'p-learning',
      'p-habits',
    ]
    const refusalsSeen = new Set<string>()

    for (const projectId of projects) {
      const bindingResponse = await page.request.get(
        `${server.base}/v1/authoritative/blueprints/binding?project_id=${projectId}`,
      )
      expect(bindingResponse.status()).toBe(200)
      const binding = (await bindingResponse.json()) as {
        blueprint_id: string
        version: number
      } | null
      expect(binding, `${projectId} must be pinned to a workflow`).toBeTruthy()

      const manifestResponse = await page.request.get(
        `${server.base}/v1/authoritative/blueprints/show?id=${binding!.blueprint_id}&version=${binding!.version}`,
      )
      expect(manifestResponse.status()).toBe(200)
      const manifest = (await manifestResponse.json()) as { views: ManifestView[] }
      expect(manifest.views.length, `${projectId} declares views`).toBeGreaterThan(0)

      for (const view of manifest.views) {
        await goto(page, server, `/projects/${projectId}/${view.id}`)
        await expect(
          page.getByRole('link', { name: new RegExp(`^${view.title}`) }),
        ).toHaveAttribute('aria-current', 'page')

        // EC-004: every declared component is a real panel on the page, not a
        // blank surface behind the active tab. The wrapper carries the declared
        // id/type so the assertion is about this component, not some heading
        // elsewhere on the page.
        expect(view.components.length, `${projectId}/${view.id} declares components`).toBeGreaterThan(0)
        for (const component of view.components) {
          const surface = page.locator(`[data-view-component="${component.id}"]`)
          await expect(surface, `${projectId}/${view.id}/${component.id}`).toHaveCount(1)
          await expect(
            surface.getByRole('heading', { name: componentHeading(component), exact: true }).first(),
            `${projectId}/${view.id}/${component.id} renders its panel heading`,
          ).toBeVisible()

          if (NAMED_REFUSAL_TYPES.includes(component.type)) {
            refusalsSeen.add(component.type)
            await expect(
              surface.getByText('Not available in the browser yet', { exact: true }),
              `${component.type} is refused by name`,
            ).toBeVisible()
          }

          // A work-item panel is not just its heading: the seed has Research,
          // Learning and Habits items, so each such surface must show data.
          if (component.source === 'work_items') {
            if (projectId === 'p-development') {
              // The seed deliberately has no work items here, so the honest
              // data assertion is the empty state, not a row that cannot exist.
              await expect(
                surface.getByText('No work items here yet', { exact: true }),
                `${projectId}/${view.id}/${component.id} says the read was empty`,
              ).toBeVisible()
            } else if (component.type === 'metric') {
              await expect(
                surface.locator('[data-work-item-count]'),
                `${projectId}/${view.id}/${component.id} counts work items`,
              ).toHaveAttribute('data-work-item-count', /^[1-9]\d*$/)
            } else {
              await expect(
                surface.locator('[data-work-item-key]').first(),
                `${projectId}/${view.id}/${component.id} lists a work item`,
              ).toBeVisible()
            }
          }
        }

        // A type this build cannot render at all is different from a named,
        // declared refusal: the sweep must never see the former.
        await expect(page.getByText(/is not supported/)).toHaveCount(0)
      }
    }

    expect([...refusalsSeen].sort()).toEqual([...NAMED_REFUSAL_TYPES].sort())
  })
})
