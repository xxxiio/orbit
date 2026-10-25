/**
 * I1 acceptance: the session is a real server-side object.
 *
 * These specs use the login *form* on purpose — they are the only place where
 * login UX is under test. Everything else uses `signInViaApi`.
 */
import { expect, test } from './harness/fixtures'
import {
  SESSION_COOKIE,
  expectSignedOut,
  hasSessionCookie,
  signIn,
  signInViaApi,
} from './harness/session'
import { startServer } from './harness/server'

const WRONG_TOKEN = 'not-the-token-issued-for-this-device-0001'

test.describe('authentication', () => {
  test('a wrong device token is rejected inline without creating a session', async ({
    page,
    server,
  }) => {
    await page.goto(`${server.base}/login`)
    await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toBeVisible()

    await page.getByLabel('Device ID').fill(server.deviceId)
    await page.getByLabel('Device token').fill(WRONG_TOKEN)
    await page.getByRole('button', { name: 'Sign in', exact: true }).click()

    const alert = page.getByRole('alert').filter({ hasText: /credentials/i })
    await expect(alert).toBeVisible()
    await expect(alert).toContainText(/did not match/i)

    // The failure is honest: still on /login, no cookie, no usable session.
    await expect(page).toHaveURL(/\/login/)
    expect(await hasSessionCookie(page.context(), server)).toBe(false)
    const probe = await page.request.get(`${server.base}/v1/web/session`)
    expect(probe.status()).toBe(401)
  })

  test('valid credentials leave /login and survive a reload', async ({ page, server }) => {
    await signIn(page, server)
    await expect(page).toHaveURL(new RegExp(`^${server.base}/?$`))
    await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toHaveCount(0)
    expect(await hasSessionCookie(page.context(), server)).toBe(true)
    // Rendering Home itself is covered by home.spec.ts, which owns the
    // empty-workspace and data cases; this spec owns the session lifecycle.
    const session = await page.request.get(`${server.base}/v1/web/session`)
    expect(session.status()).toBe(200)
    expect((await session.json()).device_id).toBe(server.deviceId)

    await page.reload()
    await expect(page).not.toHaveURL(/\/login/)
    expect(await hasSessionCookie(page.context(), server)).toBe(true)
    expect((await page.request.get(`${server.base}/v1/web/session`)).status()).toBe(200)

    // The cookie is HttpOnly: script cannot read it, the server still can.
    const cookie = (await page.context().cookies(server.base)).find(
      (entry) => entry.name === SESSION_COOKIE,
    )
    expect(cookie?.httpOnly, 'session cookie must be HttpOnly').toBe(true)
    expect(await page.evaluate(() => document.cookie)).not.toContain(SESSION_COOKIE)
  })

  test('signing out returns to /login and clears the server session', async ({
    page,
    workspace,
  }) => {
    // Sign-out lives in the app shell, which needs a workspace that renders.
    const { server } = workspace
    await signIn(page, server)
    await expect(page).not.toHaveURL(/\/login/)

    // The rail is only mounted at >= lg; below that it lives in the drawer.
    const narrow = (page.viewportSize()?.width ?? 1440) < 1024
    if (narrow) {
      await page.getByRole('button', { name: 'Open navigation' }).click()
      await page.getByRole('dialog').getByRole('button', { name: 'Sign out' }).click()
    } else {
      await page.getByRole('button', { name: 'Sign out' }).click()
    }

    await expectSignedOut(page, server)
    await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toBeVisible()
  })

  test('an expired session bounces to /login and says why', async ({ page }) => {
    // A dedicated server with a 1s sliding TTL: expiry is a server fact, not a
    // timer in the test. The polling below waits for wall-clock time without
    // touching `/v1/web/session`, because every authenticated read slides the TTL.
    const shortLived = await startServer({ env: { ORBIT_WEB_SESSION_TTL_SECONDS: '1' } })
    try {
      const session = await signInViaApi(page, shortLived)
      await expect
        .poll(() => Date.now(), { timeout: 15_000, intervals: [200] })
        .toBeGreaterThan(session.expires_at_ms + 500)

      await page.goto(`${shortLived.base}/`)
      await expectSignedOut(page, shortLived)
      await expect(
        page.getByText(/session (has )?(expired|ended)|your session expired|signed out/i).first(),
        'an expired session must be explained, not silently bounced',
      ).toBeVisible()
    } finally {
      await shortLived.stop()
    }
  })
})
