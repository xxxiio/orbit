import { existsSync } from 'node:fs'
import { cpus, homedir } from 'node:os'
import { join } from 'node:path'
import { defineConfig, devices } from '@playwright/test'

/**
 * Config for the real-browser acceptance suite (`web/tests/e2e`).
 *
 * The suite does NOT talk to `vite preview` and has no `baseURL`: `orbit-server`
 * has no CORS layer, so the app and the API must share an origin. Each test
 * starts its own `orbit-server` with a throwaway database and navigates to the
 * app build that server hosts (`ORBIT_WEB_DIST=web/dist`), which is exactly how
 * the product is deployed. Build first (`npm run build`) — that is what
 * `npm run test:e2e` does. The harness in `tests/e2e/harness/server.ts` is the
 * only server launcher; there is no second, port-bound preview process.
 */

/**
 * Playwright's Chromium needs a handful of system libraries that are not
 * installed on this machine (and `sudo` is unavailable). If a local bundle of
 * those `.so` files exists, put it on the loader path for the browser process —
 * playwright.config is evaluated inside every worker before the browser starts.
 */
const BROWSER_LIBS = [
  process.env.ORBIT_E2E_BROWSER_LIBS,
  join(homedir(), '.cache', 'orbit-e2e', 'pwlibs'),
  '/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu',
].filter((candidate): candidate is string => Boolean(candidate))

for (const candidate of BROWSER_LIBS) {
  if (!existsSync(join(candidate, 'libnspr4.so'))) continue
  const current = process.env.LD_LIBRARY_PATH
  if (!current?.split(':').includes(candidate)) {
    process.env.LD_LIBRARY_PATH = current ? `${candidate}:${current}` : candidate
  }
  break
}

const CI = Boolean(process.env.CI)
const EVIDENCE = process.env.ORBIT_E2E_EVIDENCE === '1'
const RUN_ID =
  process.env.ORBIT_E2E_RUN_ID ?? new Date().toISOString().replace(/[:.]/g, '-').replace('T', '_')

/**
 * One server per parallel test keeps isolation cheap enough at this suite size,
 * but the machine still has to breathe: leave headroom for the servers.
 */
const WORKERS = Math.max(1, Math.min(4, Math.floor(cpus().length / 2)))

const outputDir = EVIDENCE ? join('evidence', 'e2e', RUN_ID) : join('test-results')

export default defineConfig({
  testDir: './tests/e2e',
  outputDir,
  fullyParallel: true,
  forbidOnly: CI,
  // Locally a retry hides flakiness; in CI one retry separates noise from breaks.
  retries: CI ? 1 : 0,
  workers: CI ? 2 : WORKERS,
  reporter: [['list'], ['html', { open: 'never' }]],
  expect: { timeout: 10_000 },
  use: {
    // URLs are absolute: every spec navigates to its own harness server.
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'off',
    actionTimeout: 10_000,
    navigationTimeout: 20_000,
  },
  projects: [
    {
      name: 'chromium',
      testMatch: /.*\.spec\.ts/,
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } },
    },
    {
      // The one flow that must work on a phone: signing in.
      name: 'mobile-chromium',
      testMatch: /auth\.spec\.ts/,
      use: { ...devices['Pixel 7'] },
    },
  ],
})
