/**
 * I2 acceptance: project knowledge is a read over the notes that reference a
 * project or one of its tasks.
 *
 * A reference is stored in the note body as `[project:id](orbit://project:id)` /
 * `[task:id](orbit://task:id)`; there is no second table. Each test therefore
 * asserts the note's bytes, the project-scoped read (`project-knowledge`) and
 * the surface a person sees: the note's reference panel, the project's
 * `knowledge_refs` panel, and the task drawer's Notes section.
 *
 * References are written through the server's reference command, which re-reads
 * and republishes the note. Nothing here fabricates a reference client-side.
 */
import { expect, test } from './harness/fixtures'
import {
  chooseOption,
  documentAt,
  projectKnowledgeAt,
  referencesAt,
} from './harness/knowledge'
import { goto } from './harness/session'

const RESEARCH = 'p-research'
const NOTE_PATH = 'notes/orbit-overview.md'
const NOTE_ID = 'n-orbit-overview'

test.describe('project knowledge', () => {
  test('a project reference from a note appears in the project panel and survives a reload', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    // Opened with the project in context, the panel offers that project by name.
    // (`?project=` is a note context only; it does not open another surface.)
    await goto(page, server, `/knowledge/${NOTE_PATH}?project=${RESEARCH}`)
    const panel = page.locator('[data-reference-panel]')
    await expect(panel.getByText('This note does not reference a project or a task yet.')).toBeVisible()

    await panel.locator('[data-action="reference-this-project"]').click()

    const chip = panel.locator('[data-reference-target="project:p-research"]')
    await expect(chip).toBeVisible()
    await expect(chip).toHaveAttribute('data-reference-state', 'ok')
    await expect(chip).toContainText('Research')
    await expect(page.getByText('Reference to Research added.')).toBeVisible()

    // Server: the token is in the body, and the project-scoped read names this
    // note with the reference that caused its inclusion.
    const stored = await documentAt(page, server, NOTE_PATH)
    expect(stored.body).toContain('[project:p-research](orbit://project:p-research)')
    const knowledge = await projectKnowledgeAt(page, server, RESEARCH)
    const included = knowledge.notes.find((note) => note.id === NOTE_ID)
    expect(included, 'the project read must include the referencing note').toBeTruthy()
    expect(included!.references).toEqual([{ kind: 'project', id: RESEARCH }])
    const referencing = await referencesAt(page, server, `project:${RESEARCH}`)
    expect(referencing.notes.map((note) => note.id)).toContain(NOTE_ID)

    // The project's declared `knowledge_refs` panel lists the note with a chip.
    await goto(page, server, `/projects/${RESEARCH}/overview`)
    const projectPanel = page.locator('[data-view-component="knowledge"]')
    await expect(projectPanel.getByRole('heading', { name: 'Knowledge' })).toBeVisible()
    const row = projectPanel.locator(`[data-note-id="${NOTE_ID}"]`)
    await expect(row).toBeVisible()
    const rowChip = row.locator('[data-reference-target="project:p-research"]')
    await expect(rowChip).toBeVisible()
    await expect(rowChip).toContainText('This project')
    await expect(row).toContainText(NOTE_PATH)

    // Reload: the panel is a server read, not a client cache.
    await page.reload()
    await expect(
      projectPanel.locator(`[data-note-id="${NOTE_ID}"] [data-reference-target="project:p-research"]`),
    ).toBeVisible()
    expect((await documentAt(page, server, NOTE_PATH)).body).toBe(stored.body)
  })

  test('a task reference appears through the task in the project panel and in the drawer', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find((candidate) => candidate.projectId === RESEARCH)
    expect(task, 'the fixture must have a research task').toBeTruthy()
    const taskTitle = task!.title
    const notePath = 'notes/i1-acceptance.md'
    const noteId = 'n-i1-acceptance'

    // Open the note with the project in context: the task picker is then limited
    // to this project's tasks, and each option is a real task title.
    await goto(page, server, `/knowledge/${notePath}?project=${RESEARCH}`)
    const panel = page.locator('[data-reference-panel]')
    await chooseOption(
      page,
      panel.locator('[data-field="reference-target"]'),
      new RegExp(`^${taskTitle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`),
    )
    await panel.locator('[data-action="add-reference"]').click()

    const chip = panel.locator(`[data-reference-target="task:${task!.id}"]`)
    await expect(chip).toBeVisible()
    await expect(chip).toHaveAttribute('data-reference-state', 'ok')
    await expect(chip).toContainText(taskTitle)

    const stored = await documentAt(page, server, notePath)
    expect(stored.body).toContain(`[task:${task!.id}](orbit://task:${task!.id})`)

    // Project read: included through the task, and it says so.
    const knowledge = await projectKnowledgeAt(page, server, RESEARCH)
    const included = knowledge.notes.find((note) => note.id === noteId)
    expect(included, 'the project read must include a note referencing its task').toBeTruthy()
    expect(included!.references).toEqual([{ kind: 'task', id: task!.id }])

    // The project's panel names the task through its title.
    await goto(page, server, `/projects/${RESEARCH}/overview`)
    const row = page.locator(`[data-view-component="knowledge"] [data-note-id="${noteId}"]`)
    await expect(row).toBeVisible()
    await expect(row.locator(`[data-reference-target="task:${task!.id}"]`)).toContainText(taskTitle)

    // The task drawer's Notes section lists the same note.
    await goto(page, server, `/projects/${RESEARCH}?task=${encodeURIComponent(task!.id)}`)
    const drawerNotes = page.locator('[data-task-notes]')
    await expect(drawerNotes).toBeVisible()
    await expect(drawerNotes.locator(`[data-note-id="${noteId}"]`)).toBeVisible()
    await expect(drawerNotes).toContainText(notePath)

    // Reload the drawer: the association is the file on disk.
    await page.reload()
    await expect(page.locator(`[data-task-notes] [data-note-id="${noteId}"]`)).toBeVisible()
    expect((await documentAt(page, server, notePath)).body).toBe(stored.body)
  })

  test('adding and removing a reference from the panel and the drawer moves the server read', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const notePath = 'notes/learning-plan.md'
    const noteId = 'n-learning-plan'
    const task = fixture.tasks.find((candidate) => candidate.projectId === RESEARCH)
    expect(task, 'the fixture must have a research task').toBeTruthy()

    const included = async (): Promise<boolean> => {
      const knowledge = await projectKnowledgeAt(page, server, RESEARCH)
      return knowledge.notes.some((note) => note.id === noteId)
    }

    // Add from the note's own panel.
    await goto(page, server, `/knowledge/${notePath}`)
    const panel = page.locator('[data-reference-panel]')
    await chooseOption(page, panel.locator('[data-field="reference-target"]'), /^Research$/)
    await panel.locator('[data-action="add-reference"]').click()
    await expect(panel.locator('[data-reference-target="project:p-research"]')).toBeVisible()
    await expect.poll(included, { message: 'the project read follows the panel add' }).toBe(true)

    // Remove from the panel: the chip, the bytes and the project read follow.
    await panel.locator('[data-reference-remove="project:p-research"]').click()
    await expect(panel.locator('[data-reference-target="project:p-research"]')).toHaveCount(0)
    await expect.poll(included, { message: 'the project read follows the panel remove' }).toBe(false)
    expect((await documentAt(page, server, notePath)).body).not.toContain('orbit://project:p-research')
    expect((await referencesAt(page, server, `project:${RESEARCH}`)).notes.map((note) => note.id)).not.toContain(noteId)

    // Link the same note from the task drawer's Notes section.
    await goto(page, server, `/projects/${RESEARCH}?task=${encodeURIComponent(task!.id)}`)
    const drawerNotes = page.locator('[data-task-notes]')
    await expect(drawerNotes.getByText('No note references this task yet.', { exact: false })).toBeVisible()
    await page.locator('[data-action="link-note-open"]').click()
    const dialog = page.locator('[data-dialog="reference-note"]')
    await expect(dialog).toBeVisible()
    await dialog.locator('[data-field="note-search"]').fill('learning-plan')
    await dialog.locator(`[data-note-option="${noteId}"]`).click()
    await dialog.locator('[data-action="link-note"]').click()
    await expect(dialog).toBeHidden()

    await expect(drawerNotes.locator(`[data-note-id="${noteId}"]`)).toBeVisible()
    await expect.poll(included, { message: 'the project read follows the drawer link' }).toBe(true)
    expect((await documentAt(page, server, notePath)).body).toContain(
      `[task:${task!.id}](orbit://task:${task!.id})`,
    )

    // Unlink from the drawer: the list, the bytes and the project read follow.
    await drawerNotes.locator(`[data-note-unlink="${noteId}"]`).click()
    await expect(drawerNotes.locator(`[data-note-id="${noteId}"]`)).toHaveCount(0)
    await expect.poll(included, { message: 'the project read follows the drawer unlink' }).toBe(false)
    expect((await documentAt(page, server, notePath)).body).not.toContain(`orbit://task:${task!.id}`)
    const taskReferences = await referencesAt(page, server, `task:${task!.id}`)
    expect(taskReferences.notes.map((note) => note.id)).not.toContain(noteId)
  })

  test('a reference to a task this workspace does not have is broken, and is not project knowledge', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const notePath = 'notes/habits-baseline.md'
    const noteId = 'n-habits-baseline'
    const missingTask = 't-research-99'

    // A hand-written reference is the way a note can outlive the task it names:
    // the vault keeps the token, the workspace no longer has the task.
    await goto(page, server, `/knowledge/${notePath}`)
    await page.getByRole('button', { name: 'Edit' }).click()
    await page.getByLabel('Note body').fill(
      [
        '# Habit baseline',
        '',
        'Start with three habits and a two week streak target. Anything more is noise.',
        '',
        `[task:${missingTask}](orbit://task:${missingTask})`,
        '',
      ].join('\n'),
    )
    await page.getByRole('button', { name: 'Save note' }).click()
    await expect(page.getByText('Note saved.')).toBeVisible()

    // The note says it is broken, in place, and keeps the reference visible.
    const panel = page.locator('[data-reference-panel]')
    const chip = panel.locator(`[data-reference-target="task:${missingTask}"]`)
    await expect(chip).toBeVisible()
    await expect(chip).toHaveAttribute('data-reference-state', 'broken')
    const brokenNote = panel.locator('[data-reference-broken]')
    await expect(brokenNote).toBeVisible()
    await expect(brokenNote).toContainText(missingTask)
    await expect(brokenNote).toContainText('is not in this workspace')
    await expect(panel.locator(`a[href*="${missingTask}"]`)).toHaveCount(0)

    // The vault read sees the reference; the project read does not include a
    // note whose only reference names no live entity.
    const stored = await documentAt(page, server, notePath)
    expect(stored.body).toContain(`[task:${missingTask}](orbit://task:${missingTask})`)
    const references = await referencesAt(page, server, `task:${missingTask}`)
    expect(references.notes.map((note) => note.id)).toContain(noteId)
    const knowledge = await projectKnowledgeAt(page, server, RESEARCH)
    expect(knowledge.notes.map((note) => note.id)).not.toContain(noteId)
    expect(knowledge.tasks).not.toContain(missingTask)

    // Reload: still broken, still absent from the live project list.
    await page.reload()
    await expect(
      panel.locator(`[data-reference-target="task:${missingTask}"][data-reference-state="broken"]`),
    ).toBeVisible()
    await expect(page.locator('[data-reference-broken]')).toBeVisible()
  })

  test('the project panel never claims unavailability on a server that serves the route', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    // The route exists on this server, so the only honest states are "no notes"
    // or a real list. The two unavailable reasons are code paths this server
    // cannot reach: the panel always receives a project from the project page,
    // and a 404 needs a server that predates the route (see the lane report).
    await goto(page, server, `/projects/${RESEARCH}/overview`)
    const response = await page.request.get(
      `${server.base}/v1/authoritative/project-knowledge?project_id=${RESEARCH}`,
    )
    expect(response.status(), 'this server serves the project knowledge read').toBe(200)
    const projectPanel = page.locator('[data-view-component="knowledge"]')
    await expect(projectPanel.getByRole('heading', { name: 'Knowledge' })).toBeVisible()
    await expect(projectPanel.getByText('No note references this project yet')).toBeVisible()
    await expect(
      projectPanel.getByText(/does not provide project knowledge yet/),
    ).toHaveCount(0)
    await expect(
      projectPanel.getByText(/so it needs a project to read/),
    ).toHaveCount(0)
  })

  test('a task on a note is a context, not a drawer, and its reference can be added and removed', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find((candidate) => candidate.projectId === RESEARCH)
    expect(task, 'the fixture must have a research task').toBeTruthy()
    const taskId = task!.id
    const taskTitle = task!.title
    const notePath = 'notes/orbit-overview.md'
    const noteId = 'n-orbit-overview'

    // A note deep link carries `?task=` as the task this note is being linked to.
    await goto(page, server, `/knowledge/${notePath}?task=${encodeURIComponent(taskId)}`)

    // No drawer is mounted over the note, so the note's own controls are what a
    // person can reach.
    await expect(page.locator('[data-task-drawer]')).toHaveCount(0)
    await expect(page.locator('[role="dialog"]')).toHaveCount(0)
    await expect(page).toHaveURL(new RegExp(`task=${taskId}`))

    // The parameter is not invisible state: the note names the task and its id.
    const contextBar = page.locator('[data-note-context]')
    await expect(contextBar).toBeVisible()
    await expect(contextBar).toHaveAttribute('data-context-task', taskId)
    await expect(contextBar.locator('[data-context-task-title]')).toHaveText(taskTitle)
    await expect(contextBar.locator('[data-context-task-id]')).toContainText(taskId)
    // The explicit way out lands on the task's own surface, not back here.
    await expect(contextBar.locator('[data-action="open-context-task"]')).toHaveAttribute(
      'href',
      `/projects/${RESEARCH}?task=${encodeURIComponent(taskId)}`,
    )

    // Escape has no overlay to dismiss: the note stays open, focus stays in the
    // page, and the context is not silently dropped from the address.
    await page.getByRole('button', { name: 'Edit' }).focus()
    await page.keyboard.press('Escape')
    await expect(page.locator('[data-note-body]')).toBeVisible()
    await expect(page).toHaveURL(new RegExp(`task=${taskId}`))

    // The control the drawer used to cover is clickable, and it stores the link
    // through the server.
    const panel = page.locator('[data-reference-panel]')
    await panel.locator('[data-action="reference-this-task"]').click()
    const chip = panel.locator(`[data-reference-target="task:${taskId}"]`)
    await expect(chip).toBeVisible()
    await expect(chip).toHaveAttribute('data-reference-state', 'ok')
    await expect(chip).toContainText(taskTitle)
    await expect(page.getByText(`Reference to ${taskTitle} added.`)).toBeVisible()

    const stored = await documentAt(page, server, notePath)
    expect(stored.body).toContain(`[task:${taskId}](orbit://task:${taskId})`)
    const knowledge = await projectKnowledgeAt(page, server, RESEARCH)
    expect(knowledge.notes.find((note) => note.id === noteId)?.references).toEqual([
      { kind: 'task', id: taskId },
    ])

    // The chip leaves the note: it opens the task inside its project, where the
    // drawer is the documented way to read a task.
    await expect(chip.locator('a')).toHaveAttribute(
      'href',
      `/projects/${RESEARCH}?task=${encodeURIComponent(taskId)}`,
    )
    await chip.locator('a').click()
    await expect(page).toHaveURL(
      new RegExp(`/projects/${RESEARCH}\\?task=${encodeURIComponent(taskId)}$`),
    )
    const drawer = page.locator(`[data-task-drawer="${taskId}"]`)
    await expect(drawer).toBeVisible()
    await expect(drawer.locator('[data-task-notes]')).toContainText(notePath)

    // Back on the note, the same reference is removable without a drawer in the
    // way, and the removal reaches the bytes and the project read.
    await goto(page, server, `/knowledge/${notePath}?task=${encodeURIComponent(taskId)}`)
    const removePanel = page.locator('[data-reference-panel]')
    await removePanel.locator(`[data-reference-remove="task:${taskId}"]`).click()
    await expect(removePanel.locator(`[data-reference-target="task:${taskId}"]`)).toHaveCount(0)
    expect((await documentAt(page, server, notePath)).body).not.toContain(
      `orbit://task:${taskId}`,
    )
    expect(
      (await referencesAt(page, server, `task:${taskId}`)).notes.map((note) => note.id),
    ).not.toContain(noteId)
    // The note is still the destination after the removal.
    await expect(page.locator('[data-note-context]')).toBeVisible()
    await expect(page.locator('[data-note-body]')).toBeVisible()
  })

  test('stopping a task context keeps the note and the project context, including on reload', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find((candidate) => candidate.projectId === RESEARCH)
    expect(task, 'the fixture must have a research task').toBeTruthy()
    const notePath = 'notes/learning-plan.md'

    await goto(
      page,
      server,
      `/knowledge/${notePath}?project=${RESEARCH}&task=${encodeURIComponent(task!.id)}`,
    )
    const contextBar = page.locator('[data-note-context]')
    await expect(contextBar).toBeVisible()
    await expect(page.locator('[data-task-drawer]')).toHaveCount(0)

    await contextBar.locator('[data-action="clear-task-context"]').click()

    // The note stays open; only the task context is gone. Focus lands on the
    // note, not on the document body.
    await expect(page.locator('[data-note-context]')).toHaveCount(0)
    await expect(page).toHaveURL(new RegExp(`/knowledge/${notePath}\\?project=${RESEARCH}$`))
    await expect(page.locator('[data-note-body]')).toBeVisible()
    await expect
      .poll(() =>
        page.evaluate(() => document.activeElement?.hasAttribute('data-note-panel') ?? false),
      )
      .toBe(true)

    // The project context is URL state: a reload keeps it, and the panel still
    // offers that project by name.
    await page.reload()
    await expect(page.locator('[data-note-context]')).toHaveCount(0)
    const panel = page.locator('[data-reference-panel]')
    await expect(panel.locator('[data-action="reference-this-project"]')).toBeVisible()
    await expect(panel.getByText('This note does not reference a project or a task yet.')).toBeVisible()
  })
})

