#!/usr/bin/env node
/**
 * Captures the evidence set from a running preview server.
 *
 *   npm run build && npm run preview -- --port 4173
 *   node scripts/capture-screenshots.mjs
 *
 * The shell requires a session to render; only GET/DELETE /v1/web/session is
 * stubbed here so the empty shell can be photographed. No project or task data
 * is invented.
 */
import { chromium } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const BASE_URL = process.env.PREVIEW_URL ?? 'http://127.0.0.1:4173'
const OUT_DIR = path.join(ROOT, 'evidence')

const VIEWPORTS = [
  { name: '1440x900', width: 1440, height: 900 },
  { name: '390x844', width: 390, height: 844 },
]

const THEMES = ['light', 'dark']

async function settle(page) {
  await page.evaluate(() => document.fonts.ready)
  await page.waitForTimeout(650)
}

async function newContext(browser, viewport, theme) {
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    deviceScaleFactor: 1,
    colorScheme: theme,
  })
  await context.addInitScript(
    ([key, value]) => {
      window.localStorage.setItem(key, value)
    },
    ['orbit.theme', theme],
  )
  return context
}

async function stubSession(page) {
  await page.route('**/v1/web/session', async (route) => {
    const method = route.request().method()
    if (method === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          device_id: 'evidence-workstation',
          expires_at_ms: Date.now() + 12 * 60 * 60 * 1000,
        }),
      })
      return
    }
    if (method === 'DELETE') {
      await route.fulfill({ status: 204 })
      return
    }
    await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' })
  })
}

async function captureLogin(browser, viewport, theme) {
  const context = await newContext(browser, viewport, theme)
  const page = await context.newPage()
  await page.goto(`${BASE_URL}/login`, { waitUntil: 'networkidle' })
  await settle(page)
  const file = path.join(OUT_DIR, `login-${theme}-${viewport.name}.png`)
  await page.screenshot({ path: file })
  console.log(`captured ${path.relative(ROOT, file)}`)
  await context.close()
}

async function captureShell(browser, viewport, theme) {
  const context = await newContext(browser, viewport, theme)
  const page = await context.newPage()
  await stubSession(page)
  await page.goto(`${BASE_URL}/`, { waitUntil: 'networkidle' })
  await page.getByRole('heading', { name: 'Home', level: 1 }).waitFor()
  await settle(page)

  if (viewport.width < 900) {
    await page.getByRole('button', { name: 'Open navigation' }).click()
    await page.getByRole('dialog').waitFor()
    await page.waitForTimeout(400)
  }

  const file = path.join(OUT_DIR, `shell-${theme}-${viewport.name}.png`)
  await page.screenshot({ path: file })
  console.log(`captured ${path.relative(ROOT, file)}`)
  await context.close()
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true })
  const browser = await chromium.launch()
  try {
    for (const theme of THEMES) {
      for (const viewport of VIEWPORTS) {
        await captureLogin(browser, viewport, theme)
        await captureShell(browser, viewport, theme)
      }
    }
  } finally {
    await browser.close()
  }
}

main().catch((error) => {
  console.error(error)
  process.exit(1)
})
