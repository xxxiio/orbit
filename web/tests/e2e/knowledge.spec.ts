/**
 * I4 acceptance: the Knowledge workspace is a view over Markdown files.
 *
 * Every write is read back from the same server. The uncertain test drops the
 * response *after* the server applied the write, so the UI must say the file may
 * have changed rather than claiming success or failure.
 */
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

async function documentAt(
  page: import('@playwright/test').Page,
  server: E2EServer,
  path: string,
): Promise<{ path: string; body: string }> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/markdown-document?path=${encodeURIComponent(path)}`,
  )
  expect(response.status(), `GET markdown-document ${path}`).toBe(200)
  return (await response.json()) as { path: string; body: string }
}

test.describe('knowledge notes', () => {
  test('creates, edits and moves a note, and reloads the file from disk', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const createdPath = 'notes/i4-browser-note.md'
    const movedPath = 'notes/research/i4-browser-note.md'

    await goto(page, server, '/knowledge')
    await expect(page.getByRole('heading', { level: 1, name: 'Knowledge' })).toBeVisible()

    // Create: the dialog names the file path and writes frontmatter with a
    // stable id; nothing is shown as a note until the server accepts it.
    await page.getByRole('button', { name: 'New note' }).click()
    const create = page.getByRole('dialog')
    await create.getByLabel('Path').fill(createdPath)
    await create.getByLabel('Title').fill('I4 browser note')
    await create.getByLabel('Markdown').fill('First body line.')
    await create.getByRole('button', { name: 'Create note' }).click()
    await expect(create).toBeHidden()
    await expect(page).toHaveURL(new RegExp(`/knowledge/${createdPath}$`))
    await expect(page.getByText('First body line.')).toBeVisible()

    const created = await documentAt(page, server, createdPath)
    expect(created.body).toContain('orbit_id:')
    expect(created.body).toContain('# I4 browser note')
    expect(created.body).toContain('First body line.')

    // Edit: the save re-reads the file and sends the exact bytes it replaces.
    await page.getByRole('button', { name: 'Edit' }).click()
    const editor = page.getByLabel('Note body')
    await editor.fill('Second body with [[n-orbit-overview]].\n')
    const saveRequest = page.waitForRequest(
      (request) =>
        request.method() === 'POST' &&
        request.url().includes('/v1/authoritative/markdown-commands'),
    )
    await page.getByRole('button', { name: 'Save note' }).click()
    // `expected` is the fresh read, not the editor's cached copy.
    const saved = JSON.parse((await saveRequest).postData() ?? '{}') as {
      command: { expected: string }
    }
    expect(saved.command.expected).toBe(created.body)
    await expect(page.getByText('Note saved.')).toBeVisible()
    await expect(page.getByText('Second body with')).toBeVisible()

    const edited = await documentAt(page, server, createdPath)
    expect(edited.body).toContain('Second body with [[n-orbit-overview]].')
    expect(edited.body).toContain('orbit_id:')

    // Relations: a resolvable id becomes a link with the target's real path.
    const links = page.getByRole('region', { name: 'Relations' })
    const chip = links.getByRole('link', { name: 'n-orbit-overview' })
    await expect(chip).toHaveAttribute('href', '/knowledge/notes/orbit-overview.md')
    await chip.click()
    await expect(page).toHaveURL(/\/knowledge\/notes\/orbit-overview\.md$/)
    await expect(page.getByText(/Orbit is a self-hosted workspace/)).toBeVisible()

    // Move: the file changes path, the stable id and body survive.
    await page.goBack()
    await expect(page.getByText('Second body with')).toBeVisible()
    await page.getByRole('button', { name: 'Move' }).click()
    const move = page.getByRole('dialog')
    await move.getByLabel('New path').fill(movedPath)
    await move.getByRole('button', { name: 'Move note' }).click()
    await expect(move).toBeHidden()
    await expect(page).toHaveURL(new RegExp(`/knowledge/${movedPath}$`))

    const moved = await documentAt(page, server, movedPath)
    expect(moved.body).toBe(edited.body)

    // Reload: the body is read from disk, not from a client store.
    await page.reload()
    await expect(page.getByText('Second body with')).toBeVisible()
    await expect(page.getByText(movedPath).first()).toBeVisible()
  })

  test('a file changed by another writer is refused before this page sends an Update', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const notePath = 'notes/decisions/cookie-sessions.md'

    await goto(page, server, `/knowledge/${notePath}`)
    await expect(page.getByRole('heading', { level: 2, name: /cookie-sessions/i })).toBeVisible()
    await page.getByRole('button', { name: 'Edit' }).click()

    const original = await documentAt(page, server, notePath)
    const otherBody = `${original.body}\nWritten by the other reader.\n`
    const other = await page.request.post(
      `${server.base}/v1/authoritative/markdown-commands`,
      {
        data: {
          command_id: 'spec-other-writer-update',
          command: {
            kind: 'Update',
            note_id: 'n-decision-cookie-sessions',
            expected: original.body,
            replacement: otherBody,
          },
        },
        headers: { 'content-type': 'application/json', 'x-orbit-csrf': '1' },
      },
    )
    expect(other.status(), 'the other writer must be accepted').toBe(200)

    // Count only page-originated commands: the external write above is already
    // recorded, and this listener starts after it.
    const pageCommands: string[] = []
    page.on('request', (request) => {
      if (
        request.method() === 'POST' &&
        request.url().includes('/v1/authoritative/markdown-commands')
      ) {
        pageCommands.push(request.url())
      }
    })

    await page
      .getByLabel('Note body')
      .fill('# Decision: cookie sessions\n\nA page edit that must not land.\n')
    await page.getByRole('button', { name: 'Save note' }).click()

    await expect(
      page.getByText(
        'The file changed on disk while this editor was open. Reload the note before saving; nothing has been overwritten.',
      ),
    ).toBeVisible()
    expect(pageCommands, 'the stale editor must not send its Update').toHaveLength(0)

    const onDisk = await documentAt(page, server, notePath)
    expect(onDisk.body).toBe(otherBody)
    expect(onDisk.body).not.toContain('A page edit that must not land')
  })

  test('an unconfirmed write is recoverable by reload and retries the same command', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const notePath = 'notes/decisions/cookie-sessions.md'
    const marker = 'Recovery marker written after the response was dropped.'

    await goto(page, server, `/knowledge/${notePath}`)
    await expect(page.getByRole('heading', { level: 2, name: /cookie-sessions/i })).toBeVisible()

    const requestBodies: string[] = []
    let dropped = false
    await page.route('**/v1/authoritative/markdown-commands', async (route) => {
      requestBodies.push(route.request().postData() ?? '')
      if (!dropped) {
        dropped = true
        // Apply the write on the server, then drop the answer: the browser sees
        // a transport failure while the file really changed.
        await route.fetch()
        await route.abort('failed')
        return
      }
      await route.continue()
    })

    await page.getByRole('button', { name: 'Edit' }).click()
    await page.getByLabel('Note body').fill(`# Decision: cookie sessions\n\n${marker}\n`)
    await page.getByRole('button', { name: 'Save note' }).click()

    await expect(page.getByText(/not confirmed; the file may still have changed/i)).toBeVisible()
    await expect(page.getByRole('button', { name: 'Reload note' })).toBeVisible()
    // The UI must not claim success before the response.
    await expect(page.getByText('Note saved.')).toHaveCount(0)

    const onDisk = await documentAt(page, server, notePath)
    expect(onDisk.body).toContain(marker)

    // Identical retry: same command id and same expected/replacement payload.
    await page.getByRole('button', { name: 'Retry the same change' }).first().click()
    await expect(page.getByText('Note saved.')).toBeVisible()
    await page.unroute('**/v1/authoritative/markdown-commands')

    expect(requestBodies).toHaveLength(2)
    const first = JSON.parse(requestBodies[0]) as { command_id: string; command: unknown }
    const second = JSON.parse(requestBodies[1]) as { command_id: string; command: unknown }
    expect(second.command_id).toBe(first.command_id)
    expect(second.command).toEqual(first.command)

    await page.reload()
    await expect(page.getByText(marker)).toBeVisible()
  })

  test('a missing note and an inventory search both lead somewhere useful', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    await goto(page, server, '/knowledge/notes/missing-note.md')
    await expect(page.getByText('This note could not be found')).toBeVisible()
    await page.getByRole('link', { name: 'Back to Knowledge' }).click()
    await expect(page).toHaveURL(/\/knowledge$/)

    const search = page.getByLabel('Search notes')
    await search.fill('readiness')
    const results = page.getByRole('list', { name: 'Search results' })
    await expect(results.getByRole('link').first()).toBeVisible()
    await results.getByRole('link', { name: /readiness/i }).first().click()
    await expect(page).toHaveURL(/\/knowledge\/notes\/research\/readiness\.md$/)
    await expect(page.getByText(/A task is actionable only when every prerequisite is finished/)).toBeVisible()
  })
})
