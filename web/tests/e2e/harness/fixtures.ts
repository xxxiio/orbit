/**
 * Playwright fixtures shared by every acceptance spec.
 *
 * - `server`: a real `orbit-server` with an isolated temp database, stopped and
 *   deleted after the test. Tests that only exercise auth or routing get an
 *   empty server for free.
 * - `workspace`: the same server, seeded through the public API. Opt in with
 *   `test('…', async ({ workspace }) => …)`.
 *
 * Failure evidence is attached to the HTML report here (screenshot on any
 * failure), while the config writes files into `outputDir` and records traces on
 * the first retry.
 */
import { test as base, expect } from '@playwright/test'

import { seedWorkspace, type WorkspaceFixture } from './seed'
import { startServer, type E2EServer } from './server'

export interface Workspace {
  server: E2EServer
  fixture: WorkspaceFixture
}

export const test = base.extend<{ server: E2EServer; workspace: Workspace }>({
  server: async ({}, use) => {
    const server = await startServer()
    try {
      await use(server)
    } finally {
      await server.stop()
    }
  },

  workspace: async ({ server }, use) => {
    const fixture = await seedWorkspace(server)
    expect(fixture.counts.total, 'fixture must be big enough for list views').toBeGreaterThan(100)
    await use({ server, fixture })
  },
})

test.afterEach(async ({ page }, testInfo) => {
  if (testInfo.status === testInfo.expectedStatus) return
  // Attach a full-page screenshot so a failure is reviewable without replaying
  // the run. The file is also written to `outputDir` by the config.
  const screenshot = await page.screenshot({ fullPage: true }).catch(() => null)
  if (screenshot) {
    await testInfo.attach('failure-screenshot', { body: screenshot, contentType: 'image/png' })
  }
  await testInfo.attach('page-url', {
    body: Buffer.from(page.url() || '(no page)'),
    contentType: 'text/plain',
  })
})

export { expect }
