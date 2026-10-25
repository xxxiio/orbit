/**
 * I4 acceptance: attachments use raw bytes, a real file picker and the server's
 * own list. Nothing is shown as accepted before the response; removal refetches
 * instead of deleting locally.
 */
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

const PIXEL_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl2ZQAAAABJRU5ErkJggg==',
  'base64',
)

async function attachmentsFor(
  page: import('@playwright/test').Page,
  server: E2EServer,
  ownerKind: string,
  ownerId: string,
): Promise<Array<{ id: string; filename: string }>> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/attachments?owner_kind=${ownerKind}&owner_id=${encodeURIComponent(ownerId)}`,
  )
  expect(response.status()).toBe(200)
  return ((await response.json()) as { attachments: Array<{ id: string; filename: string }> })
    .attachments
}

async function upload(
  region: import('@playwright/test').Locator,
  file: { name: string; mimeType: string; buffer: Buffer },
  description: string,
): Promise<void> {
  await region.getByLabel('Description').fill(description)
  await region.locator('input[type="file"]').setInputFiles(file)
}

async function removeRow(
  region: import('@playwright/test').Locator,
  filename: string,
): Promise<void> {
  await region.getByRole('button', { name: `Remove ${filename}` }).click()
  await region.getByRole('button', { name: 'Remove', exact: true }).click()
  await expect(
    region.locator('[data-attachment-id]').filter({ hasText: filename }),
  ).toHaveCount(0)
}

test.describe('attachments', () => {
  test('task drawer: list, upload, preview, download and remove a real file', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const taskId = fixture.tasks[0].id

    await goto(page, server, `/inbox?task=${encodeURIComponent(taskId)}`)
    const drawer = page.getByRole('dialog')
    await expect(drawer).toBeVisible()
    const region = drawer.getByRole('region', { name: 'Attachments' })
    await expect(region).toBeVisible()

    // Seeded list: a text file and a real PNG, both from the fixture.
    await expect(region.getByText('acceptance-notes.txt')).toBeVisible()
    const imageRow = region.locator('[data-attachment-id="a-06"]')
    await expect(imageRow).toBeVisible()

    // Preview uses the bytes fetched from the content route, not a path guess.
    await imageRow.getByRole('button', { name: /Preview/i }).click()
    const preview = imageRow.getByRole('img', { name: /Preview of status-card\.svg/ })
    await expect(preview).toBeVisible()
    expect(
      await preview.evaluate((image) => (image as HTMLImageElement).naturalWidth),
      'the preview must decode real bytes, not a broken image',
    ).toBeGreaterThan(0)

    // Download asks for the server's filename.
    const downloadPromise = page.waitForEvent('download')
    await imageRow.getByRole('link', { name: 'Download status-card.svg' }).click()
    const download = await downloadPromise
    expect(download.suggestedFilename()).toBe('status-card.svg')

    // Upload: hold the response briefly so "pending" is observable, and prove
    // the filename is not in the server list until the answer arrives.
    await page.route('**/v1/authoritative/attachment-import*', async (route) => {
      await new Promise((resolve) => setTimeout(resolve, 500))
      await route.continue()
    })
    await upload(
      region,
      { name: 'i4-upload.txt', mimeType: 'text/plain', buffer: Buffer.from('I4 upload body\n') },
      'I4 upload',
    )
    await expect(region.getByText(/Not stored yet/)).toBeVisible()
    await expect(
      region.locator('[data-attachment-id]').filter({ hasText: 'i4-upload.txt' }),
    ).toHaveCount(0)

    const uploadedRow = region
      .locator('[data-attachment-id]')
      .filter({ hasText: 'i4-upload.txt' })
    await expect(uploadedRow).toBeVisible()
    await page.unroute('**/v1/authoritative/attachment-import*')
    await expect(uploadedRow.getByText('I4 upload')).toBeVisible()
    const afterUpload = await attachmentsFor(page, server, 'task', taskId)
    const uploaded = afterUpload.find((attachment) => attachment.filename === 'i4-upload.txt')
    expect(uploaded, 'the server must list the uploaded file').toBeTruthy()

    // Remove refetches: the server list no longer holds the row.
    await removeRow(region, 'i4-upload.txt')
    await expect
      .poll(async () => (await attachmentsFor(page, server, 'task', taskId)).map((item) => item.filename))
      .not.toContain('i4-upload.txt')
  })

  test('work-item sheet: list, upload, preview, download and remove a real file', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const itemId = 'wi-orbit-feature-ready'

    await goto(page, server, '/projects/p-orbit-development/board')
    await page.locator(`[data-work-item-key="${itemId}"]`).getByRole('button').first().click()
    const sheet = page.getByRole('dialog')
    await expect(sheet).toBeVisible()
    const region = sheet.getByRole('region', { name: 'Attachments' })
    await expect(region).toBeVisible()
    await expect(region.getByText('review-notes.txt')).toBeVisible()

    await upload(
      region,
      { name: 'i4-work-item.png', mimeType: 'image/png', buffer: PIXEL_PNG },
      'Work-item preview',
    )
    const row = region
      .locator('[data-attachment-id]')
      .filter({ hasText: 'i4-work-item.png' })
    await expect(row).toBeVisible()
    await expect(row.getByText('Work-item preview')).toBeVisible()

    await row.getByRole('button', { name: /Preview/i }).click()
    const preview = row.getByRole('img', { name: /Preview of i4-work-item\.png/ })
    await expect(preview).toBeVisible()
    expect(
      await preview.evaluate((image) => (image as HTMLImageElement).naturalWidth),
      'the preview must decode real bytes, not a broken image',
    ).toBeGreaterThan(0)

    const downloadPromise = page.waitForEvent('download')
    await row.getByRole('link', { name: 'Download i4-work-item.png' }).click()
    const download = await downloadPromise
    expect(download.suggestedFilename()).toBe('i4-work-item.png')

    const listed = await attachmentsFor(page, server, 'work_item', itemId)
    expect(listed.map((attachment) => attachment.filename)).toContain('i4-work-item.png')

    await removeRow(region, 'i4-work-item.png')
    await expect
      .poll(async () =>
        (await attachmentsFor(page, server, 'work_item', itemId)).map((item) => item.filename),
      )
      .not.toContain('i4-work-item.png')
  })

  test('a dropped upload response retries the same ids and stores one row', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const taskId = fixture.tasks[2].id
    const filename = 'i4-dropped-upload.txt'

    await goto(page, server, `/inbox?task=${encodeURIComponent(taskId)}`)
    const region = page.getByRole('dialog').getByRole('region', { name: 'Attachments' })
    await expect(region).toBeVisible()

    const attempts: URLSearchParams[] = []
    let dropped = false
    await page.route('**/v1/authoritative/attachment-import*', async (route) => {
      attempts.push(new URL(route.request().url()).searchParams)
      if (!dropped) {
        dropped = true
        // The server stores the bytes, then the answer is lost: the browser must
        // not mint a second attachment id for the retry.
        await route.fetch()
        await route.abort('failed')
        return
      }
      await route.continue()
    })

    await upload(
      region,
      { name: filename, mimeType: 'text/plain', buffer: Buffer.from('Dropped response body\n') },
      'Dropped response',
    )
    await expect(region.locator('[data-upload-state="uncertain"]')).toBeVisible()
    // Choosing another file while the outcome is unknown would create a second
    // row, so the picker is closed until Retry, Refresh or Dismiss.
    await expect(region.locator('input[type="file"]')).toBeDisabled()
    await expect(region.getByRole('button', { name: 'Upload file' })).toBeDisabled()

    await region.getByRole('button', { name: 'Retry the same upload' }).click()
    await expect(region.getByText(`${filename} uploaded.`)).toBeVisible()
    await page.unroute('**/v1/authoritative/attachment-import*')

    expect(attempts, 'the retry is a replay, not a second upload').toHaveLength(2)
    expect(attempts[1].get('command_id')).toBe(attempts[0].get('command_id'))
    expect(attempts[1].get('attachment_id')).toBe(attempts[0].get('attachment_id'))

    const stored = await attachmentsFor(page, server, 'task', taskId)
    expect(stored.filter((attachment) => attachment.filename === filename)).toHaveLength(1)
  })

  test('a dropped removal response is recovered by refreshing the real list', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const taskId = fixture.tasks[0].id
    const filename = 'acceptance-notes.txt'

    await goto(page, server, `/inbox?task=${encodeURIComponent(taskId)}`)
    const region = page.getByRole('dialog').getByRole('region', { name: 'Attachments' })
    const row = region.locator('[data-attachment-id]').filter({ hasText: filename })
    await expect(row).toBeVisible()

    await page.route('**/v1/authoritative/attachment-remove', async (route) => {
      // The server removes the row, then the response is lost.
      await route.fetch()
      await route.abort('failed')
    })

    await region.getByRole('button', { name: `Remove ${filename}` }).click()
    await region.getByRole('button', { name: 'Remove', exact: true }).click()
    await expect(region.getByText(/The file may still be listed/)).toBeVisible()

    // The server really removed it; the UI must not claim otherwise until it
    // reads the list again.
    expect(
      (await attachmentsFor(page, server, 'task', taskId)).map((item) => item.filename),
    ).not.toContain(filename)

    await region.getByRole('button', { name: 'Refresh list' }).click()
    await expect(row).toHaveCount(0)
    await page.unroute('**/v1/authoritative/attachment-remove')
  })

  test('shows a 413 honestly and never marks the file stored', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const taskId = fixture.tasks[1].id

    await goto(page, server, `/inbox?task=${encodeURIComponent(taskId)}`)
    const drawer = page.getByRole('dialog')
    await expect(drawer).toBeVisible()
    const region = drawer.getByRole('region', { name: 'Attachments' })

    await page.route('**/v1/authoritative/attachment-import*', async (route) => {
      await route.fulfill({
        status: 413,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'attachment body exceeds the server byte limit' }),
      })
    })
    await upload(
      region,
      { name: 'too-large.bin', mimeType: 'application/octet-stream', buffer: Buffer.alloc(16) },
      'Too large',
    )

    await expect(region.getByText(/larger than the server's upload limit/)).toBeVisible()
    await expect(region.getByText('The file was not stored.')).toBeVisible()
    await expect(
      region.locator('[data-attachment-id]').filter({ hasText: 'too-large.bin' }),
    ).toHaveCount(0)
    await page.unroute('**/v1/authoritative/attachment-import*')
  })
})
