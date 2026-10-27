/**
 * I2 acceptance: note-to-note links are authored, followed and removed through
 * the server, and an id the vault cannot resolve is named rather than dropped.
 *
 * A link is `[[stable-id]]` inside a Markdown file. Authoring appends it with a
 * fenced `Update` (the file is re-read first), so the specs assert both the
 * rendered relation and the exact bytes on disk. The last test changes the file
 * from outside the page between the fresh read and the send: the server's
 * byte fence must refuse it, with the server's words and no silent retry.
 */
import { expect, test } from './harness/fixtures'
import {
  documentAt,
  folderUrl,
  folderUrlPattern,
  inventoryAt,
  noteAt,
  noteCreateDialog,
  noteLinkDialog,
  relationsAt,
} from './harness/knowledge'
import { goto } from './harness/session'
import type { E2EServer } from './harness/server'

/** Create one note through the folder view's dialog and return its inventory row. */
async function createNoteInFolder(
  page: import('@playwright/test').Page,
  server: E2EServer,
  folder: string,
  fileName: string,
  title: string,
  markdown: string,
) {
  await goto(page, server, folderUrl(folder))
  await page.locator('[data-folder-action="new-note"]').click()
  const dialog = noteCreateDialog(page)
  await expect(dialog).toBeVisible()
  await expect(dialog.locator('[data-field="folder"]')).toHaveAttribute(
    'data-folder-value',
    folder,
  )
  await dialog.locator('[data-field="note-name"]').fill(fileName)
  await dialog.getByLabel('Title').fill(title)
  await dialog.getByLabel('Markdown').fill(markdown)
  await dialog.locator('[data-action="create-note"]').click()
  await expect(dialog).toBeHidden()
  const path = `${folder}/${fileName}`
  await expect(page).toHaveURL(new RegExp(`/knowledge/${path}$`))
  return noteAt(await inventoryAt(page, server), path)
}

test.describe('knowledge links', () => {
  test('links two notes by id, follows the link, and both sides persist', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const source = await createNoteInFolder(page, server, 'notes', 'link-source.md', 'Link source', 'Nothing links from here yet.')
    const target = await createNoteInFolder(page, server, 'notes', 'link-target.md', 'Link target', 'The other end of the link.')

    await goto(page, server, `/knowledge/${source.path}`)
    await page.locator('[data-note-action="link"]').click()
    const dialog = noteLinkDialog(page)
    await expect(dialog).toBeVisible()

    // Only real note ids are offered: the picker names the target and its id.
    await dialog.locator('[data-field="note-search"]').fill('link-target')
    await dialog.locator(`[data-note-option="${target.id}"]`).click()
    await dialog.locator('[data-action="link-note"]').click()
    await expect(dialog).toBeHidden()

    // Outgoing relation with the target's real path, not a guessed one.
    const outgoing = page.locator('[data-reference-group="links"]')
    const chip = outgoing.locator(`[data-note-link="${target.id}"]`)
    await expect(chip).toBeVisible()
    await expect(chip).toHaveAttribute('href', `/knowledge/${target.path}`)

    // The bytes on disk carry the token, in the form the vault grammar expects.
    const sourceBody = await documentAt(page, server, source.path)
    expect(sourceBody.body).toContain(`[[${target.id}]]`)
    expect((await relationsAt(page, server, source.id)).outgoing).toContain(target.id)
    expect((await relationsAt(page, server, target.id)).backlinks).toContain(source.id)

    // Follow it: the target shows the back-reference.
    await chip.click()
    await expect(page).toHaveURL(new RegExp(`/knowledge/${target.path}$`))
    await expect(page.getByText('The other end of the link.')).toBeVisible()
    const backlinks = page.locator('[data-reference-group="backlinks"]')
    await expect(backlinks.locator(`[data-note-link="${source.id}"]`)).toBeVisible()

    // Reload both ends: the relation is read from disk, not from a store.
    await page.reload()
    await expect(backlinks.locator(`[data-note-link="${source.id}"]`)).toBeVisible()
    await goto(page, server, `/knowledge/${source.path}`)
    await expect(outgoing.locator(`[data-note-link="${target.id}"]`)).toBeVisible()
    expect((await documentAt(page, server, source.path)).body).toBe(sourceBody.body)
  })

  test('unlinking removes the outgoing link, the back-reference and the token on disk', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const sourcePath = 'notes/orbit-overview.md'
    const sourceId = 'n-orbit-overview'
    const targetId = 'n-decision-cookie-sessions'
    const targetPath = 'notes/decisions/cookie-sessions.md'

    await goto(page, server, `/knowledge/${sourcePath}`)
    const outgoing = page.locator('[data-reference-group="links"]')
    await expect(outgoing.locator(`[data-note-link="${targetId}"]`)).toBeVisible()
    const before = await documentAt(page, server, sourcePath)
    expect(before.body).toContain(`[[${targetId}]]`)
    expect((await relationsAt(page, server, targetId)).backlinks).toContain(sourceId)

    await page.locator(`[data-note-unlink="${targetId}"]`).click()
    await expect(page.getByText(`Link to ${targetId} removed.`)).toBeVisible()
    await expect(outgoing.locator(`[data-note-link="${targetId}"]`)).toHaveCount(0)

    // Server: the token is gone from the bytes; the surrounding prose is not.
    const after = await documentAt(page, server, sourcePath)
    expect(after.body).not.toContain(`[[${targetId}]]`)
    expect(after.body).toContain('before changing auth.')
    expect(after.body).toContain(`orbit_id: ${sourceId}`)
    expect((await relationsAt(page, server, targetId)).backlinks).not.toContain(sourceId)
    expect((await relationsAt(page, server, sourceId)).outgoing).not.toContain(targetId)

    // The other end agrees after a reload, and so does this note.
    await page.reload()
    await expect(outgoing.locator(`[data-note-link="${targetId}"]`)).toHaveCount(0)
    await goto(page, server, `/knowledge/${targetPath}`)
    await expect(
      page.locator('[data-reference-group="backlinks"]').locator(`[data-note-link="${sourceId}"]`),
    ).toHaveCount(0)
    expect((await documentAt(page, server, sourcePath)).body).toBe(after.body)
  })

  test('a hand-written unknown id is named as unresolved and is not a dead end', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const unknown = 'n-nowhere-note'
    const note = await createNoteInFolder(
      page,
      server,
      'notes',
      'unresolved-note.md',
      'Unresolved note',
      `This note mentions [[${unknown}]] on purpose.`,
    )

    // The token is shown as written in the body, and the relations panel lists it
    // with the reason instead of resolving it to a path the vault never published.
    const body = page.locator('[data-note-body]')
    await expect(body).toContainText(`[[${unknown}]]`)
    const unresolved = page.locator('[data-reference-group="unresolved"]')
    await expect(unresolved).toContainText(`[[${unknown}]]`)
    await expect(unresolved.getByTitle('No note in this vault has this id')).toHaveCount(1)

    // Nothing was invented: there is no anchor that claims to open the id.
    await expect(page.locator(`a[href*="${unknown}"]`)).toHaveCount(0)
    const relations = await relationsAt(page, server, note.id)
    expect(relations.unresolved).toContain(unknown)
    // The wire keeps every mentioned id in `outgoing`; the panel must not turn
    // an unresolved one into an outgoing link.
    await expect(
      page.locator('[data-reference-group="links"]').locator(`[data-note-link="${unknown}"]`),
    ).toHaveCount(0)
    expect((await documentAt(page, server, note.path)).body).toContain(`[[${unknown}]]`)

    // A person who tries the path anyway is told it is missing and given a way
    // back into a folder that does exist — not a dead end.
    const attempted = `${note.path.replace(/[^/]+$/, '')}${unknown}.md`
    await goto(page, server, `/knowledge/${attempted}`)
    await expect(page.getByText('This note could not be found')).toBeVisible()
    await page.getByRole('link', { name: 'Back to the folder' }).click()
    await expect(page).toHaveURL(folderUrlPattern('notes'))
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      'notes',
    )
    await expect(
      page.locator(`[data-note-path="${note.path}"]`).first(),
    ).toBeVisible()
  })

  test('an external writer between the read and the link is refused, with no retry', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const notePath = 'notes/decisions/markdown-vault.md'
    const noteId = 'n-decision-markdown-vault'
    const otherWriterMarker = 'A paragraph another writer added first.'

    await goto(page, server, `/knowledge/${notePath}`)
    const original = await documentAt(page, server, notePath)

    const pageCommands: string[] = []
    await page.route('**/v1/authoritative/markdown-commands', async (route) => {
      pageCommands.push(route.request().postData() ?? '')
      if (pageCommands.length === 1) {
        // Another device publishes a new body after this page re-read the file
        // and before its own `Update` reaches the server.
        const otherBody = `${original.body}\n${otherWriterMarker}\n`
        const other = await page.request.post(`${server.base}/v1/authoritative/markdown-commands`, {
          data: {
            command_id: 'spec-other-writer-link',
            command: { kind: 'Update', note_id: noteId, expected: original.body, replacement: otherBody },
          },
          headers: { 'content-type': 'application/json', 'x-orbit-csrf': '1' },
        })
        expect(other.status(), 'the other writer must be accepted').toBe(200)
      }
      await route.continue()
    })

    await page.locator('[data-note-action="link"]').click()
    const dialog = noteLinkDialog(page)
    await expect(dialog).toBeVisible()
    const targetId = 'n-orbit-overview'
    await dialog.locator('[data-field="note-search"]').fill('orbit-overview')
    await dialog.locator(`[data-note-option="${targetId}"]`).click()
    await dialog.locator('[data-action="link-note"]').click()

    // The server's fence answers; the dialog says so in the server's words.
    await expect(dialog.getByText('The server refused this change to the note.')).toBeVisible()
    await expect(dialog.getByRole('alert')).toContainText('stale server revision')
    await expect(dialog).toBeVisible()

    // Exactly one Update left this page: no silent retry, no second command id.
    expect(pageCommands).toHaveLength(1)
    const sent = JSON.parse(pageCommands[0]) as {
      command_id: string
      command: { kind: string; expected: string }
    }
    expect(sent.command.kind).toBe('Update')
    expect(sent.command.expected).toBe(original.body)

    // Nothing of this page's write landed; the other writer's bytes survived.
    const onDisk = await documentAt(page, server, notePath)
    expect(onDisk.body).toContain(otherWriterMarker)
    expect(onDisk.body).not.toContain(`[[${targetId}]]`)

    await page.unroute('**/v1/authoritative/markdown-commands')
    await dialog.getByRole('button', { name: 'Cancel' }).click()
    await page.reload()
    await expect(page.getByText(otherWriterMarker)).toBeVisible()
    await expect(page.getByText('No resolvable outgoing links.')).toBeVisible()
    await expect(page.locator(`[data-note-link="${targetId}"]`)).toHaveCount(0)
  })
})
