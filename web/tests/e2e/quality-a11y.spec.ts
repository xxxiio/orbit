/**
 * M31 I6 — automated accessibility audit for the primary routes.
 *
 * Runs axe-core (through `@axe-core/playwright`, which reuses the Playwright
 * Chromium already on this machine — no browser download and no second browser)
 * against every primary route in both themes. Contrast is therefore checked
 * against the real computed colours, not against a hand-written palette.
 *
 * Rules that need a human (focus visibility, focus order, the meaning of a
 * live-region announcement) are not guessed at here; they are covered by
 * `quality-keyboard.spec.ts`, and anything axe cannot decide is named in
 * `.agent/evidence/m31-web-quality.md`.
 */
import AxeBuilder from '@axe-core/playwright'
import type { Page } from '@playwright/test'
import type { Result } from 'axe-core'
import { expect, test } from './harness/fixtures'
import { goto, signInViaApi } from './harness/session'

const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice']

interface RouteCase {
  name: string
  path: (fixture: { ids: { orbitTask: string } }) => string
  /** Selector that proves the route's own content rendered. */
  ready: string
}

const ROUTES: RouteCase[] = [
  { name: 'home', path: () => '/', ready: 'main h1' },
  { name: 'inbox', path: () => '/inbox', ready: 'main h1' },
  { name: 'projects', path: () => '/projects', ready: 'main h1' },
  { name: 'project-overview', path: () => '/projects/p-orbit-development', ready: 'main h1' },
  { name: 'project-tasks-board', path: () => '/projects/p-orbit-development/tasks?view=board', ready: 'main [data-board-action]' },
  { name: 'project-board', path: () => '/projects/p-orbit-development/board', ready: 'main [data-board-action]' },
  { name: 'project-roadmap', path: () => '/projects/p-orbit-development/roadmap', ready: 'main h1' },
  { name: 'project-milestones', path: () => '/projects/p-orbit-development/milestones', ready: 'main h1' },
  { name: 'planning', path: () => '/projects/p-development/planning', ready: 'main h1' },
  { name: 'knowledge', path: () => '/knowledge', ready: 'main h1' },
  { name: 'knowledge-note', path: () => '/knowledge/notes/orbit-overview.md', ready: 'main h1' },
  { name: 'agents', path: () => '/agents', ready: 'main h1' },
  {
    name: 'task-drawer',
    path: ({ ids }) => `/projects/p-orbit-development/board?task=${ids.orbitTask}`,
    ready: '[role="dialog"]',
  },
]

async function settle(page: Page, ready: string): Promise<void> {
  await page.locator(ready).first().waitFor({ timeout: 20_000 })
  await page
    .waitForFunction(() => !document.querySelector('main [aria-busy="true"]'), undefined, {
      timeout: 20_000,
    })
  // The shell animates route content in. axe reads the *computed* colour at the
  // instant it runs, so measuring during a fade reports blended (and therefore
  // falsely low-contrast) text. Wait for fonts and every finite animation, then
  // let two frames paint the settled state.
  await page.evaluate(async () => {
    await document.fonts.ready
    const finite = document.getAnimations().filter((animation) => {
      const timing = animation.effect?.getTiming()
      return timing && timing.iterations !== Infinity
    })
    await Promise.all(finite.map((animation) => animation.finished.catch(() => undefined)))
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve(null))))
  })
}

function describeViolation(violation: Result): string {
  const nodes = violation.nodes
    .slice(0, 4)
    .map(
      (node) =>
        `${node.target.join(' ')} — ${node.failureSummary ?? ''}\n      HTML: ${node.html}`,
    )
    .join('\n      ')
  return `${violation.id} (${violation.impact ?? 'n/a'}): ${violation.help}\n      ${nodes}`
}

for (const theme of ['light', 'dark'] as const) {
  test.describe(`axe — ${theme}`, () => {
    test.setTimeout(180_000)
    test(`primary routes have no automated accessibility violations (${theme})`, async ({
      page,
      workspace,
    }) => {
      const { server, fixture } = workspace
      await page.addInitScript((value) => {
        window.localStorage.setItem('orbit.theme', value)
      }, theme)
      await signInViaApi(page, server)
      await goto(page, server, '/')
      await expect
        .poll(() => page.evaluate(() => document.documentElement.dataset.theme))
        .toBe(theme)

      const failures: string[] = []
      for (const route of ROUTES) {
        const target = `${server.base}${route.path(fixture)}`
        await page.goto(target)
        await settle(page, route.ready)
        const results = await new AxeBuilder({ page })
          .withTags(TAGS)
          .analyze()
        if (results.violations.length === 0) continue
        failures.push(
          `[${route.name}] ${route.path(fixture)}\n    ${results.violations.map(describeViolation).join('\n    ')}`,
        )
      }
      expect(failures.join('\n'), `axe violations (${theme})`).toBe('')
    })

    test(`the mobile navigation drawer has dialog semantics (${theme})`, async ({ page, workspace }) => {
      const { server } = workspace
      await page.addInitScript((value) => {
        window.localStorage.setItem('orbit.theme', value)
      }, theme)
      await signInViaApi(page, server)
      await page.setViewportSize({ width: 390, height: 844 })
      await page.goto(`${server.base}/`)
      await settle(page, 'main h1')
      await page.getByRole('button', { name: 'Open navigation' }).click()
      const drawer = page.getByRole('dialog', { name: 'Navigation' })
      await expect(drawer).toBeVisible()
      await settle(page, '[role="dialog"]')
      const results = await new AxeBuilder({ page }).withTags(TAGS).analyze()
      expect(results.violations.map(describeViolation).join('\n')).toBe('')
    })
  })
}


/**
 * axe does not judge document outline. This is the focused DOM check for
 * heading order: one h1, no skipped levels, DOM order.
 */
test('primary routes have a single, unskipped heading outline', async ({ page, workspace }) => {
  test.setTimeout(120_000)
  const { server, fixture } = workspace
  await signInViaApi(page, server)
  const failures: string[] = []
  for (const route of ROUTES) {
    await page.goto(`${server.base}${route.path(fixture)}`)
    await settle(page, route.ready)
    const outline = await page.evaluate(() =>
      Array.from(document.querySelectorAll('main h1, main h2, main h3, main h4, main h5, main h6')).map(
        (heading) => ({
          level: Number(heading.tagName.slice(1)),
          text: (heading.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 60),
        }),
      ),
    )
    const h1s = outline.filter((heading) => heading.level === 1)
    if (h1s.length !== 1) {
      failures.push(`[${route.name}] expected one h1, found ${h1s.length}`)
      continue
    }
    if (outline[0]?.level !== 1) {
      failures.push(`[${route.name}] first heading is h${outline[0]?.level ?? '?'}, not h1`)
    }
    for (let index = 1; index < outline.length; index += 1) {
      const previous = outline[index - 1].level
      const current = outline[index].level
      if (current > previous + 1) {
        failures.push(
          `[${route.name}] h${previous} → h${current} (“${outline[index].text}”) skips a level`,
        )
      }
    }
  }
  expect(failures.join('\n')).toBe('')
})

test('reduced motion removes the shell choreography', async ({ page, workspace }) => {
  const { server } = workspace
  await signInViaApi(page, server)
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await page.goto(`${server.base}/projects/p-orbit-development/board`)
  await settle(page, 'main [data-board-action]')
  const durations = await page.evaluate(() =>
    document
      .getAnimations()
      .map((animation) => animation.effect?.getTiming().duration)
      .filter((duration): duration is number => typeof duration === 'number'),
  )
  for (const duration of durations) {
    expect(duration, 'reduced motion must not run a visible animation').toBeLessThanOrEqual(0.02)
  }
})

/** Guard the audit itself: a route that silently fails to render must not pass. */
test('every audited route renders its own content', async ({ page, workspace }) => {
  test.setTimeout(120_000)
  const { server, fixture } = workspace
  await signInViaApi(page, server)
  for (const route of ROUTES) {
    await page.goto(`${server.base}${route.path(fixture)}`)
    await settle(page, route.ready)
    await expect(page.locator('main h1').first(), route.name).toBeVisible()
  }
})
