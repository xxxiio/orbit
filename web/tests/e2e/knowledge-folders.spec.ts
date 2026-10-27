/**
 * I2 acceptance: a folder is a real directory and a note is a real file.
 *
 * Every step is asserted twice: once through the interface a person uses, and
 * once through the server read the interface claims to be a view of
 * (`knowledge-tree`, `markdown-inventory`, `markdown-document`). A note that
 * only appears on screen has not been written; a note that moved only on screen
 * is still at its old path on disk.
 *
 * Each test owns its server, so a refusal in one cannot influence another. A
 * refused folder command is a `400` validation refusal and releases its durable
 * intent, so the workspace stays writable afterwards; the invalid-operation
 * tests assert that by performing a valid write after the refusal where it
 * matters.
 */
import { expect, test } from './harness/fixtures'
import {
  chooseOption,
  documentAt,
  documentStatus,
  folderCreateDialog,
  folderList,
  folderMoveDialog,
  folderNotes,
  folderPaths,
  folderUrl,
  folderUrlPattern,
  inventoryAt,
  noteAt,
  noteCreateDialog,
  noteMoveDialog,
  notePaths,
  treeAt,
} from './harness/knowledge'
import { goto } from './harness/session'

test.describe('knowledge folders', () => {
  test('a new folder and a note inside it survive a reload, on screen and on disk', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const folder = 'projects'
    const notePath = 'projects/browser-note.md'

    await goto(page, server, '/knowledge')
    await expect(page.getByRole('heading', { level: 1, name: 'Knowledge' })).toBeVisible()
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute('data-selected-folder', '')

    // Folder create: one name, at the root the person is looking at. The dialog
    // previews the exact path the server will be asked to create.
    await page.locator('[data-folder-action="new-folder"]').click()
    const newFolder = folderCreateDialog(page)
    await expect(newFolder).toBeVisible()
    await expect(newFolder.locator('[data-field="folder"]')).toHaveAttribute(
      'data-folder-value',
      '',
    )
    await newFolder.locator('[data-field="folder-name"]').fill(folder)
    await expect(newFolder.locator('[data-preview-path]')).toHaveText(folder)
    await newFolder.locator('[data-action="create-folder"]').click()
    await expect(newFolder).toBeHidden()
    await expect(page).toHaveURL(folderUrlPattern(folder))
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      folder,
    )

    // Note create: the folder comes from the picker, pre-set to the folder being
    // viewed, not typed into a path field.
    await page.locator('[data-folder-action="new-note"]').click()
    const createNote = noteCreateDialog(page)
    await expect(createNote).toBeVisible()
    await expect(createNote.locator('[data-field="folder"]')).toHaveAttribute(
      'data-folder-value',
      folder,
    )
    await expect(createNote.locator('[data-preview-path]')).toHaveText(`${folder}/`)
    await createNote.locator('[data-field="note-name"]').fill('browser-note.md')
    await expect(createNote.locator('[data-preview-path]')).toHaveText(notePath)
    await createNote.getByLabel('Title').fill('Browser note')
    await createNote.getByLabel('Markdown').fill('Written from the folder view.')
    await createNote.locator('[data-action="create-note"]').click()
    await expect(createNote).toBeHidden()

    // The note is addressed by its real path and says where it lives.
    await expect(page).toHaveURL(new RegExp(`/knowledge/${notePath}$`))
    await expect(page.locator('[data-note-location]')).toHaveText('browser-note.md')
    await expect(page.locator('[data-note-location-path]')).toHaveAttribute(
      'data-note-location-path',
      folder,
    )
    await expect(page.locator('[data-note-location-path]')).toHaveText(notePath)
    await expect(page.getByText('Written from the folder view.')).toBeVisible()

    // Server: the folder exists first, and the note is a file inside it.
    const inventory = await inventoryAt(page, server)
    const created = noteAt(inventory, notePath)
    const document = await documentAt(page, server, notePath)
    expect(document.body).toContain(`orbit_id: ${created.id}`)
    expect(document.body).toContain('# Browser note')
    const tree = await treeAt(page, server)
    expect(folderPaths(tree)).toContain(folder)
    expect(notePaths(tree)).toContain(notePath)

    // Reload: the tree, the location line and the inventory all agree again.
    await page.reload()
    await expect(page.locator('[data-note-location]')).toHaveText('browser-note.md')
    await expect(page.locator('[data-note-location-path]')).toHaveText(notePath)
    await expect(
      page.locator(`[data-knowledge-tree] [data-folder-path="${folder}"]`),
    ).toBeVisible()
    await expect(
      page.locator(`[data-knowledge-tree] [data-note-path="${notePath}"]`),
    ).toBeVisible()
    await expect(page.getByText('Written from the folder view.')).toBeVisible()
    const reloaded = await inventoryAt(page, server)
    expect(noteAt(reloaded, notePath).id).toBe(created.id)
  })

  test('renaming a note from the note view moves the file and retires the old path', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const oldPath = 'notes/i1-acceptance.md'
    const newPath = 'notes/i1-acceptance-renamed.md'

    await goto(page, server, `/knowledge/${oldPath}`)
    await expect(page.locator('[data-note-location]')).toHaveText('i1-acceptance.md')
    const before = await documentAt(page, server, oldPath)
    expect(before.body).toContain('orbit_id: n-i1-acceptance')

    // The explicit rename affordance on the note is the same command as a move:
    // one file name, the folder unchanged.
    await page.locator('[data-note-action="move"]').click()
    const rename = noteMoveDialog(page)
    await expect(rename).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Rename or move note' })).toBeVisible()
    await rename.locator('[data-field="note-name"]').fill('i1-acceptance-renamed.md')
    await expect(rename.locator('[data-preview-path]')).toHaveText(newPath)
    await rename.locator('[data-action="move-note"]').click()
    await expect(rename).toBeHidden()

    await expect(page).toHaveURL(new RegExp(`/knowledge/${newPath}$`))
    await expect(page.locator('[data-note-location]')).toHaveText('i1-acceptance-renamed.md')
    await expect(page.getByText(/I1 acceptance script/)).toBeVisible()

    // Server: the bytes and the stable id moved; the old path is gone.
    const after = await documentAt(page, server, newPath)
    expect(after.body).toBe(before.body)
    expect(await documentStatus(page, server, oldPath)).toBe(404)
    const inventory = await inventoryAt(page, server)
    expect(noteAt(inventory, newPath).id).toBe('n-i1-acceptance')
    expect(inventory.some((note) => note.path === oldPath)).toBe(false)

    await page.reload()
    await expect(page.locator('[data-note-location]')).toHaveText('i1-acceptance-renamed.md')
    await expect(
      page.locator(`[data-knowledge-tree] [data-note-path="${newPath}"]`),
    ).toBeVisible()
    await expect(
      page.locator(`[data-knowledge-tree] [data-note-path="${oldPath}"]`),
    ).toHaveCount(0)
  })

  test('moving a note to another folder changes its location, tree row and inventory', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const oldPath = 'notes/i1-acceptance.md'
    const newPath = 'notes/research/i1-acceptance.md'

    await goto(page, server, `/knowledge/${oldPath}`)
    const before = await documentAt(page, server, oldPath)

    await page.locator('[data-note-action="move"]').click()
    const move = noteMoveDialog(page)
    await expect(move).toBeVisible()
    await chooseOption(page, move.locator('[data-field="folder"]'), /research/)
    await expect(move.locator('[data-preview-path]')).toHaveText(newPath)
    await move.locator('[data-action="move-note"]').click()
    await expect(move).toBeHidden()

    await expect(page).toHaveURL(new RegExp(`/knowledge/${newPath}$`))
    await expect(page.locator('[data-note-location-path]')).toHaveAttribute(
      'data-note-location-path',
      'notes/research',
    )
    await expect(page.locator('[data-note-location]')).toHaveText('i1-acceptance.md')

    const after = await documentAt(page, server, newPath)
    expect(after.body).toBe(before.body)
    expect(await documentStatus(page, server, oldPath)).toBe(404)
    const tree = await treeAt(page, server)
    expect(notePaths(tree)).toContain(newPath)
    expect(notePaths(tree)).not.toContain(oldPath)

    await page.reload()
    await expect(page.locator('[data-note-location-path]')).toHaveText(newPath)
    await expect(
      page.locator(`[data-knowledge-tree] [data-note-path="${newPath}"]`),
    ).toBeVisible()
    const inventory = await inventoryAt(page, server)
    expect(noteAt(inventory, newPath).id).toBe('n-i1-acceptance')
    expect(inventory.some((note) => note.path === oldPath)).toBe(false)
  })

  test('renaming a folder carries its notes, and the old deep link admits the move', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const oldFolder = 'notes/decisions'
    const newFolder = 'notes/decisions-archive'
    const oldPath = `${oldFolder}/cookie-sessions.md`
    const newPath = `${newFolder}/cookie-sessions.md`

    // Rename the folder from its own view; the folder is a real destination in
    // the URL, so the app can follow the rename to the new address.
    await goto(page, server, folderUrl(oldFolder))
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      oldFolder,
    )
    await page.locator('[data-folder-action="rename"]').click()
    const rename = folderMoveDialog(page)
    await expect(rename).toBeVisible()
    await expect(rename.locator('[data-current-path]')).toHaveText(oldFolder)
    await expect(rename.locator('[data-field="folder-name"]')).toHaveValue('decisions')
    await rename.locator('[data-field="folder-name"]').fill('decisions-archive')
    await expect(rename.locator('[data-preview-path]')).toHaveText(newFolder)
    await rename.locator('[data-action="move-folder"]').click()
    await expect(rename).toBeHidden()

    await expect(page).toHaveURL(folderUrlPattern(newFolder))
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      newFolder,
    )
    await expect(
      folderNotes(page).locator(`[data-note-path="${newPath}"]`),
    ).toBeVisible()

    // The note inside kept its id and bytes; only its path moved.
    const moved = await documentAt(page, server, newPath)
    expect(moved.body).toContain('orbit_id: n-decision-cookie-sessions')
    expect(await documentStatus(page, server, oldPath)).toBe(404)
    const tree = await treeAt(page, server)
    expect(folderPaths(tree)).toContain(newFolder)
    expect(folderPaths(tree)).not.toContain(oldFolder)
    expect(notePaths(tree)).toContain(newPath)

    // The note stays reachable at its new path.
    await page.locator(`[data-note-path="${newPath}"]`).first().click()
    await expect(page).toHaveURL(new RegExp(`/knowledge/${newPath}$`))
    await expect(page.locator('[data-note-location-path]')).toHaveText(newPath)
    await expect(page.getByText(/Device credentials are exchanged once/)).toBeVisible()
    await expect(page.getByText('n-decision-cookie-sessions').first()).toBeVisible()

    // A deep link to the old path says it is gone and does not pretend it moved
    // to a path it guessed; the folder behind it is honest too.
    await goto(page, server, `/knowledge/${oldPath}`)
    await expect(page.getByText('This note could not be found')).toBeVisible()
    await expect(page.getByText(/It may have been moved or removed from another device/)).toBeVisible()
    await page.getByRole('link', { name: 'Back to the folder' }).click()
    await expect(page).toHaveURL(folderUrlPattern(oldFolder))
    await expect(page.getByText('This folder is not in the vault')).toBeVisible()
    await page.getByRole('link', { name: 'Open the vault root' }).click()
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute('data-selected-folder', '')
  })

  test('a folder create whose parent does not exist is refused and creates nothing', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const missingParent = 'ghost'
    const child = 'ghost/child'

    // A stale address is the one way the picker can offer a folder the vault
    // does not have: the address names it, so the control keeps it visible.
    await goto(page, server, folderUrl(missingParent))
    await expect(page.getByText('This folder is not in the vault')).toBeVisible()

    await page.locator('[data-action="new-folder"]').click()
    const dialog = folderCreateDialog(page)
    await expect(dialog).toBeVisible()
    await expect(dialog.locator('[data-field="folder"]')).toHaveAttribute(
      'data-folder-value',
      missingParent,
    )
    await dialog.locator('[data-field="folder-name"]').fill('child')
    await expect(dialog.locator('[data-preview-path]')).toHaveText(child)
    await dialog.locator('[data-action="create-folder"]').click()

    // The server's refusal is quoted verbatim; nothing is reported as created.
    // A missing parent is the caller's mistake, so the server answers 400 with
    // its own fixed wording instead of a conflict, and it does not keep the
    // refused create as a pending command: later writes on this server still
    // work.
    await expect(dialog.getByText('The server refused this folder.')).toBeVisible()
    await expect(dialog.getByRole('alert')).toContainText('folder parent does not exist')
    await expect(dialog).toBeVisible()

    // Nothing was created: neither the parent (it never existed) nor the child.
    const tree = await treeAt(page, server)
    expect(folderPaths(tree)).not.toContain(missingParent)
    expect(folderPaths(tree)).not.toContain(child)
    await expect(page.locator(`[data-folder-path="${child}"]`)).toHaveCount(0)
    await expect(page.getByText('This folder is not in the vault')).toBeVisible()

    // The refused intent did not wedge the workspace: a valid folder created
    // immediately after the refusal is accepted and persisted. This is the
    // browser-level regression for a refused folder command leaving a durable
    // pending row that blocked every later knowledge write.
    await goto(page, server, folderUrl(''))
    await page.locator('[data-action="new-folder"]').click()
    const recovered = folderCreateDialog(page)
    await expect(recovered).toBeVisible()
    await recovered.locator('[data-field="folder-name"]').fill('recovered')
    await recovered.locator('[data-action="create-folder"]').click()
    await expect(recovered).toBeHidden()
    expect(folderPaths(await treeAt(page, server))).toContain('recovered')
  })

  test('a folder cannot be moved into its own subtree, and the server refuses it by name', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const parent = 'notes'
    const nested = 'notes/research'

    await goto(page, server, folderUrl(parent))
    await page.locator('[data-folder-action="rename"]').click()
    const dialog = folderMoveDialog(page)
    await expect(dialog).toBeVisible()

    // The picker does not offer the folder itself or anything inside it, so the
    // nested destination cannot be composed in this dialog at all.
    await dialog.locator('[data-field="folder"]').click()
    await expect(page.getByRole('option', { name: /research/ })).toHaveCount(0)
    await expect(page.getByRole('option', { name: /^notes$/ })).toHaveCount(0)
    await expect(page.locator('[data-folder-option="notes/research"]')).toHaveCount(0)
    // Close the listbox by re-choosing the value it already holds, so the dialog
    // itself stays open (Escape would close both).
    await page.locator('[data-folder-option=""]').click()
    await expect(page.getByRole('option')).toHaveCount(0)
    await dialog.getByRole('button', { name: 'Cancel' }).click()
    await expect(dialog).toBeHidden()

    // The refusal is the server's, not a client invention: the same move over
    // the authoritative route answers 400 with its own words.
    const response = await page.request.post(`${server.base}/v1/authoritative/folder-moves`, {
      data: {
        command_id: 'spec-nested-folder-move',
        command: { from: parent, to: `${nested}/notes` },
      },
      headers: { 'content-type': 'application/json', 'x-orbit-csrf': '1' },
    })
    expect(response.status(), 'the server must refuse a nested folder move').toBe(400)
    expect((await response.json()) as { error: string }).toEqual({ error: 'invalid folder move' })

    // Nothing moved.
    const tree = await treeAt(page, server)
    expect(folderPaths(tree)).toContain(nested)
    expect(notePaths(tree)).toContain('notes/research/readiness.md')
    await page.reload()
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      parent,
    )
    await expect(folderList(page).locator(`[data-folder-path="${nested}"]`)).toBeVisible()
  })

  test('a folder move onto an existing folder name is refused and neither folder moves', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const source = 'notes/research'
    const destination = 'notes/decisions'
    const sourceNote = `${source}/readiness.md`
    const destinationNote = `${destination}/cookie-sessions.md`

    await goto(page, server, folderUrl(source))
    await page.locator('[data-folder-action="rename"]').click()
    const dialog = folderMoveDialog(page)
    await expect(dialog).toBeVisible()

    // The picker does not hide an existing destination: only the server knows
    // what is on disk right now, so the refusal is the server's to give.
    await dialog.locator('[data-field="folder-name"]').fill('decisions')
    await expect(dialog.locator('[data-preview-path]')).toHaveText(destination)
    await dialog.locator('[data-action="move-folder"]').click()

    // Again the server's wording is quoted as-is: a taken destination is a
    // name the caller can change, so it answers 400 and leaves the queue free.
    await expect(dialog.getByText('The server refused this move.')).toBeVisible()
    await expect(dialog.getByRole('alert')).toContainText('folder destination already exists')
    await expect(dialog).toBeVisible()

    // Both folders and both notes are still where they were.
    expect(await documentStatus(page, server, sourceNote)).toBe(200)
    expect(await documentStatus(page, server, destinationNote)).toBe(200)
    const tree = await treeAt(page, server)
    expect(folderPaths(tree)).toEqual(expect.arrayContaining([source, destination]))
    expect(notePaths(tree)).toEqual(expect.arrayContaining([sourceNote, destinationNote]))
    await dialog.getByRole('button', { name: 'Cancel' }).click()
    await expect(dialog).toBeHidden()
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      source,
    )
    await expect(folderNotes(page).locator(`[data-note-path="${sourceNote}"]`)).toBeVisible()
  })

  test('the breadcrumb reaches parent and child folders and each folder is a real address', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace

    await goto(page, server, folderUrl('notes/decisions'))
    const breadcrumb = page.locator('[data-breadcrumb]')
    await expect(breadcrumb.locator('[data-breadcrumb-path=""]')).toHaveText('Vault')
    await expect(breadcrumb.locator('[data-breadcrumb-path="notes"]')).toHaveText('notes')
    await expect(breadcrumb.locator('[data-breadcrumb-path="notes/decisions"]')).toHaveAttribute(
      'aria-current',
      'page',
    )

    // Up one level: a real folder destination, not a highlight.
    await breadcrumb.locator('[data-breadcrumb-path="notes"]').click()
    await expect(page).toHaveURL(folderUrlPattern('notes'))
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      'notes',
    )
    await expect(folderList(page).locator('[data-folder-path="notes/decisions"]')).toBeVisible()
    await expect(folderList(page).locator('[data-folder-path="notes/research"]')).toBeVisible()

    // Back to the root, then down through the folder list.
    await breadcrumb.locator('[data-breadcrumb-path=""]').click()
    await expect(page).toHaveURL(/\/knowledge$/)
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute('data-selected-folder', '')
    await folderList(page).locator('[data-folder-path="notes"]').click()
    await expect(page).toHaveURL(folderUrlPattern('notes'))
    await folderList(page).locator('[data-folder-path="notes/research"]').click()
    await expect(page).toHaveURL(folderUrlPattern('notes/research'))
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      'notes/research',
    )
    await expect(
      folderNotes(page).locator('[data-note-path="notes/research/readiness.md"]'),
    ).toBeVisible()

    // The folder is a destination: a reload lands on the same place.
    await page.reload()
    await expect(page).toHaveURL(folderUrlPattern('notes/research'))
    await expect(page.locator('[data-selected-folder]')).toHaveAttribute(
      'data-selected-folder',
      'notes/research',
    )
    await expect(
      page.locator('[data-breadcrumb-path="notes/research"]'),
    ).toHaveAttribute('aria-current', 'page')
    await expect(
      folderNotes(page).locator('[data-note-path="notes/research/readiness.md"]'),
    ).toBeVisible()
    await expect(folderList(page)).toHaveCount(0)
  })
})
