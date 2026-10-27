/**
 * Browser session helpers.
 *
 * The session lives in an HttpOnly `orbit_session` cookie, so every helper here
 * goes through the real transport: either the login form (when the test is about
 * login UX) or `POST /v1/web/session` from the page context (when it is not).
 * `page.request` and `context.request` share the browser cookie jar, which is
 * what makes the API shortcut set a cookie the app can actually use.
 */
import { expect, type BrowserContext, type Page } from '@playwright/test'
import type { E2EServer } from './server'

export const SESSION_COOKIE = 'orbit_session'

/** Fill the login form with the given credentials (default: the harness device). */
export async function signIn(
  page: Page,
  server: E2EServer,
  credentials: { deviceId: string; token: string } = server,
): Promise<void> {
  const current = page.url()
  const onLoginPage = current.startsWith(server.base) && new URL(current).pathname.startsWith('/login')
  if (!onLoginPage) await page.goto(`${server.base}/login`)
  await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toBeVisible()
  await page.getByLabel('Device ID').fill(credentials.deviceId)
  await page.getByLabel('Device token').fill(credentials.token)
  await page.getByRole('button', { name: 'Sign in', exact: true }).click()
  await expect(page).not.toHaveURL(/\/login/, { timeout: 15_000 })
}

/**
 * Exchange device credentials for a cookie without touching the form. Use this
 * for every spec that is not about login UX; it keeps the cookie HttpOnly and
 * same-origin exactly like the real flow.
 */
export interface WebSession {
  device_id: string
  expires_at_ms: number
}

/**
 * Exchange device credentials for a cookie. Defaults to the harness device; a
 * spec that needs a second (or deliberately different) enrolled device passes
 * its own credentials.
 */
export async function signInViaApi(
  page: Page,
  server: E2EServer,
  credentials: { deviceId: string; token: string } = server,
): Promise<WebSession> {
  const response = await page.request.post(`${server.base}/v1/web/session`, {
    data: { device_id: credentials.deviceId, token: credentials.token },
    headers: { 'content-type': 'application/json' },
  })
  expect(response.status(), 'POST /v1/web/session should create a session').toBe(201)
  const cookies = await page.context().cookies(server.base)
  expect(cookies.map((cookie) => cookie.name)).toContain(SESSION_COOKIE)
  const responseCookies = response
    .headersArray()
    .filter((header) => header.name.toLowerCase() === 'set-cookie')
    .map((header) => header.value)
  expect(responseCookies.join('\n')).toContain('HttpOnly')
  return (await response.json()) as WebSession
}

/** Signed-in navigation: establish the cookie, then open a route. */
export async function goto(
  page: Page,
  server: E2EServer,
  path = '/',
  credentials?: { deviceId: string; token: string },
): Promise<void> {
  await signInViaApi(page, server, credentials)
  await page.goto(`${server.base}${path}`)
}

/** Asserts the browser has no session and the app sent it back to /login. */
export async function expectSignedOut(page: Page, server: E2EServer): Promise<void> {
  await expect(page).toHaveURL(/\/login/, { timeout: 15_000 })
  const cookies = await page.context().cookies(server.base)
  expect(
    cookies.filter((cookie) => cookie.name === SESSION_COOKIE && cookie.value !== ''),
    'the session cookie must be cleared',
  ).toHaveLength(0)
  const session = await page.request.get(`${server.base}/v1/web/session`)
  expect(session.status(), 'GET /v1/web/session after sign-out').toBe(401)
}

/** Cookie jar probe that does not navigate. */
export async function hasSessionCookie(
  context: BrowserContext,
  server: E2EServer,
): Promise<boolean> {
  const cookies = await context.cookies(server.base)
  return cookies.some((cookie) => cookie.name === SESSION_COOKIE && cookie.value.length > 0)
}

/** Create a second, independent browser session (its own cookie jar). */
export async function openSecondBrowser(
  context: BrowserContext,
  server: E2EServer,
): Promise<{ context: BrowserContext; page: Page }> {
  const second = await context.browser()!.newContext({ baseURL: server.base })
  const page = await second.newPage()
  await signInViaApi(page, server)
  return { context: second, page }
}
