/**
 * I2 acceptance — every declared view renders through one registry path.
 *
 * EC-004: a declared component must be a real panel on the page, not a blank
 * surface behind an active tab. This sweep walks every bundled workflow's
 * declaration, so a renderer that silently drops a panel fails here.
 *
 * M33 corrective rule: a workflow `tasks`/`board` view becomes the one Tasks
 * tab's configuration only when *every* component of that view reads tasks (or
 * declares no source). The projection is then the canonical list/board, and the
 * declared id is a URL view, not a section. A mixed view — the Development
 * board also declares a Work Item board — keeps its own nav entry at its own
 * address and must render all of its components.
 */
import { expect, test } from './harness/fixtures'
import { goto, signInViaApi } from './harness/session'

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

/** The declared types this build still names as unavailable. */
const NAMED_REFUSAL_TYPES = ['habit_tracker', 'metric_history']

function componentHeading(component: ManifestComponent): string {
  return component.title?.trim() || 'View panel'
}

/** The product's consumption rule, restated so the sweep can predict it. */
function isPureTaskView(view: ManifestView): boolean {
  if (view.components.length === 0) return false
  return view.components.every(
    (component) => component.source === null || component.source === 'tasks',
  )
}

/** The section a declared view id resolves to, and the label its nav entry uses. */
function navigationFor(view: ManifestView): { section: string; label: string } {
  if (view.id === 'overview') return { section: 'overview', label: 'Overview' }
  if (view.id === 'tasks') {
    // A pure `tasks` view is the Tasks tab's list configuration; an impure one
    // keeps its own address because the canonical Tasks route owns `/tasks`.
    return isPureTaskView(view)
      ? { section: 'tasks', label: 'Tasks' }
      : { section: 'tasks-view', label: view.title }
  }
  if (view.id === 'board') {
    return isPureTaskView(view)
      ? { section: 'tasks', label: 'Tasks' }
      : { section: 'board', label: view.title }
  }
  if (view.id === 'timeline' || view.id === 'milestones') {
    return { section: 'milestones', label: 'Milestones' }
  }
  return { section: view.id, label: view.title }
}

test.describe('workflow view registry', () => {
  test('every bundled workflow renders all of its declared views through one path', async ({
    page,
    workspace,
  }) => {
    // The sweep walks every view of six projects (~30 routes) with a fresh
    // session per navigation; solo it needs ~29s, so the default 30s budget is
    // not a meaningful bound for it.
    test.setTimeout(180_000)
    const { server, fixture } = workspace
    await signInViaApi(page, server)

    const projects = ['p-basic', 'p-development', 'p-research', 'p-learning', 'p-habits', 'p-guarded']
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
        const { section, label } = navigationFor(view)
        const consumedAsTaskView =
          (view.id === 'tasks' || view.id === 'board') && isPureTaskView(view)

        await goto(page, server, `/projects/${projectId}/${section}`)
        await expect(
          page.getByRole('link', { name: new RegExp(`^${label}`) }),
          `${projectId}/${view.id} resolves to the ${section} section`,
        ).toHaveAttribute('aria-current', 'page')

        if (consumedAsTaskView && view.id === 'tasks') {
          // The canonical list replaces the declared table; its rows are the
          // assertion, not a view-component wrapper.
          await expect(page.locator('[data-row-list]').first()).toBeVisible()
          for (const component of view.components) {
            await expect(
              page.locator(`[data-view-component="${component.id}"]`),
              `${projectId}/${component.id} is a URL view, not a section panel`,
            ).toHaveCount(0)
          }
          continue
        }

        if (consumedAsTaskView && view.id === 'board') {
          // The consumed board is the compact Tasks board: its legacy `/board`
          // address redirects to `?view=board`.
          await goto(page, server, `/projects/${projectId}/board`)
          await expect(page).toHaveURL(
            new RegExp(`/projects/${projectId}/tasks\\?[^#]*view=board`),
          )
          const url = new URL(page.url())
          expect(url.pathname, `${projectId}/board is a Tasks view`).toBe(
            `/projects/${projectId}/tasks`,
          )
          expect(url.searchParams.get('view')).toBe('board')
          await expect(page.locator('[data-board-action]').first()).toBeVisible()
          for (const component of view.components) {
            await expect(
              page.locator(`[data-view-component="${component.id}"]`),
              `${projectId}/${component.id} is not rendered as a declared panel`,
            ).toHaveCount(0)
          }
          continue
        }

        // A mixed board is a declared section: it must stay at its own address
        // (no redirect into the Tasks tab).
        if (view.id === 'board') {
          const url = new URL(page.url())
          expect(
            url.pathname,
            `${projectId}/board mixes sources and must keep its own address`,
          ).toBe(`/projects/${projectId}/board`)
        }

        // The canonical Milestones section owns a conventional `milestones`
        // (or legacy `timeline`) declaration: one card list, not the declared
        // panels. The cards carry the server's progress and dates.
        if (view.id === 'milestones' || view.id === 'timeline') {
          await expect(
            page.getByRole('heading', { name: 'Checkpoints and their work' }),
            `${projectId}/${view.id} renders the canonical Milestones section`,
          ).toBeVisible()
          for (const component of view.components) {
            const panel = page.locator(`[data-view-component="${component.id}"]`)
            if (component.source === 'milestones') {
              await expect(
                panel,
                `${projectId}/${component.id} is drawn by the canonical milestone cards`,
              ).toHaveCount(0)
            } else {
              await expect(
                panel,
                `${projectId}/${component.id} is a workflow panel kept beside the cards`,
              ).toHaveCount(1)
            }
          }
          continue
        }

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

          // A knowledge panel is the project-scoped read (C9): a note appears
          // only when it references this project or one of its tasks. The
          // fixture seeds notes but no typed reference, so every knowledge panel
          // must render the read's empty state and still offer a way into the
          // workspace. Listing the whole inventory here would be the I4
          // behaviour, not this contract.
          if (component.source === 'knowledge') {
            await expect(
              surface.getByText('No note references this project yet'),
              `${projectId}/${view.id}/${component.id} says the project read is empty`,
            ).toBeVisible()
            await expect(
              surface.getByRole('link', { name: 'Open Knowledge' }),
              `${projectId}/${view.id}/${component.id} links into Knowledge`,
            ).toBeVisible()
          }
          if (component.source === 'agent_jobs') {
            if (component.type === 'metric') {
              await expect(
                surface.locator('[data-agent-job-count]'),
                `${projectId}/${view.id}/${component.id} counts real agent jobs`,
              ).toHaveAttribute('data-agent-job-count', /^[1-9]\d*$/)
            } else {
              await expect(
                surface.locator('[data-agent-job]').first(),
                `${projectId}/${view.id}/${component.id} lists a real agent job`,
              ).toBeVisible()
            }
          }

          // The fixture's guarded view declares a descending task sort. The
          // rendered order must be the server titles descending, not ascending.
          if (component.id === 'descending_tasks') {
            const titles = fixture.tasks
              .filter((task) => task.projectId === 'p-guarded')
              .map((task) => task.title)
              .sort((left, right) => right.localeCompare(left))
              .slice(0, 5)
            await expect(surface.getByRole('button')).toHaveText(titles)
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
