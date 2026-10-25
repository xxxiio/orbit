/**
 * I4 acceptance: the upload limit is enforced by the real server, not only by a
 * mocked 413. This spec starts its own server with a one-kilobyte limit and
 * uploads a real buffer above it through the browser.
 */
import { expect, test } from './harness/fixtures'
import { fixtureNowMs, seedWorkspace } from './harness/seed'
import { startServer } from './harness/server'
import { goto } from './harness/session'

test('a real upload above ORBIT_ATTACHMENT_MAX_BYTES is refused with 413', async ({ page }) => {
  const server = await startServer({ env: { ORBIT_ATTACHMENT_MAX_BYTES: '1024' } })
  try {
    const fixture = await seedWorkspace(server, { nowMs: fixtureNowMs(), extras: false })
    const taskId = fixture.tasks[0].id

    await goto(page, server, `/inbox?task=${encodeURIComponent(taskId)}`)
    const region = page.getByRole('dialog').getByRole('region', { name: 'Attachments' })
    await expect(region).toBeVisible()

    await region.getByLabel('Description').fill('Real over-limit upload')
    await region.locator('input[type="file"]').setInputFiles({
      name: 'real-over-limit.bin',
      mimeType: 'application/octet-stream',
      buffer: Buffer.alloc(2048, 7),
    })

    await expect(region.getByText(/larger than the server's upload limit/)).toBeVisible()
    await expect(region.getByText('The file was not stored.')).toBeVisible()
    await expect(
      region.locator('[data-attachment-id]').filter({ hasText: 'real-over-limit.bin' }),
    ).toHaveCount(0)

    const listed = await page.request.get(
      `${server.base}/v1/authoritative/attachments?owner_kind=task&owner_id=${encodeURIComponent(taskId)}`,
    )
    expect(listed.status()).toBe(200)
    expect(
      ((await listed.json()) as { attachments: Array<{ filename: string }> }).attachments.map(
        (attachment) => attachment.filename,
      ),
    ).not.toContain('real-over-limit.bin')
  } finally {
    await server.stop()
  }
})
