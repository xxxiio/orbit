/**
 * M31 I6 — responsive matrix for every primary route.
 *
 * The contract is not "looks nice"; it is the measurable one: the document must
 * not scroll horizontally, navigation must remain reachable, and a task form and
 * a task drawer must still open at every width. Attachments live inside the task
 * drawer (there is no standalone attachments route), so they are checked there.
 *
 * Widths: 1440 / 1280 / 1024 (sidebar breakpoint) / 900 (sidebar collapses) /
 * 390 (phone).
 */
import type { Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import { signInViaApi } from './harness/session'

const VIEWPORTS = [
  { name: '1440', width: 1440, height: 900 },
  { name: '1280', width: 1280, height: 800 },
  { name: '1024', width: 1024, height: 768 },
  { name: '900', width: 900, height: 700 },
  { name: '390', width: 390, height: 844 },
] as const

const ROUTES = [
  { name: 'home', path: '/' },
  { name: 'inbox', path: '/inbox' },
  { name: 'projects', path: '/projects' },
  { name: 'project-overview', path: '/projects/p-orbit-development' },
  { name: 'project-tasks-board', path: '/projects/p-orbit-development/tasks?view=board' },
  { name: 'project-board', path: '/projects/p-orbit-development/board' },
  { name: 'project-roadmap', path: '/projects/p-orbit-development/roadmap' },
  { name: 'project-milestones', path: '/projects/p-orbit-development/milestones' },
  { name: 'planning', path: '/projects/p-development/planning' },
  { name: 'knowledge', path: '/knowledge' },
  { name: 'knowledge-note', path: '/knowledge/notes/orbit-overview.md' },
  { name: 'agents', path: '/agents' },
] as const

async function settle(page: Page): Promise<void> {
  await page.locator('main h1').first().waitFor({ timeout: 20_000 })
  await page.waitForFunction(
    () => !document.querySelector('main [aria-busy="true"]'),
    undefined,
    { timeout: 20_000 },
  )
  await page.evaluate(async () => {
    await document.fonts.ready
    const finite = document
      .getAnimations()
      .filter((animation) => animation.effect?.getTiming()?.iterations !== Infinity)
    await Promise.all(finite.map((animation) => animation.finished.catch(() => undefined)))
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve(null))))
  })
}

interface OverflowReport {
  viewport: number
  documentWidth: number
  bodyWidth: number
  offenders: string[]
}

async function overflow(page: Page): Promise<OverflowReport> {
  return page.evaluate(() => {
    const viewport = document.documentElement.clientWidth
    const offenders: string[] = []
    for (const element of Array.from(document.querySelectorAll('body *'))) {
      const rect = element.getBoundingClientRect()
      if (rect.width === 0 && rect.height === 0) continue
      // A horizontally scrollable ancestor is allowed to hold wide content.
      let scrollable = false
      let parent = element.parentElement
      while (parent) {
        const style = getComputedStyle(parent)
        if (/(auto|scroll)/.test(style.overflowX) && parent.scrollWidth > parent.clientWidth) {
          scrollable = true
          break
        }
        parent = parent.parentElement
      }
      if (scrollable) continue
      if (rect.right > viewport + 1 || rect.left < -1) {
        const id = element.id ? `#${element.id}` : ''
        const cls = typeof element.className === 'string' ? element.className.split(' ').slice(0, 2).join('.') : ''
        offenders.push(
          `${element.tagName.toLowerCase()}${id}.${cls} left=${Math.round(rect.left)} right=${Math.round(rect.right)}`,
        )
        if (offenders.length === 8) break
      }
    }
    return {
      viewport,
      documentWidth: Math.max(document.documentElement.scrollWidth, document.body.scrollWidth),
      bodyWidth: document.body.scrollWidth,
      offenders,
    }
  })
}

function expectNoOverflow(report: OverflowReport, label: string): void {
  expect(
    report.documentWidth,
    `${label}: document scrollWidth ${report.documentWidth} exceeds viewport ${report.viewport}\n` +
      report.offenders.join('\n'),
  ).toBeLessThanOrEqual(report.viewport + 1)
}

for (const viewport of VIEWPORTS) {
  test.describe(`responsive ${viewport.name}`, () => {
    test.setTimeout(180_000)

    test(`routes do not overflow and stay reachable at ${viewport.name}px`, async ({ page, workspace }) => {
      const { server, fixture } = workspace
      await page.setViewportSize({ width: viewport.width, height: viewport.height })
      await signInViaApi(page, server)

      for (const route of ROUTES) {
        await page.goto(`${server.base}${route.path}`)
        await settle(page)
        expectNoOverflow(await overflow(page), `${viewport.name} ${route.name}`)

        // Navigation stays available: the desktop sidebar at the project's `lg`
        // breakpoint (900px) and above, the hamburger below it.
        if (viewport.width >= 900) {
          await expect(
            page.getByRole('navigation', { name: 'Primary' }),
            `${route.name}: desktop navigation`,
          ).toBeVisible()
          await expect(page.getByRole('button', { name: 'Open navigation' })).toBeHidden()
        } else {
          const trigger = page.getByRole('button', { name: 'Open navigation' })
          await expect(trigger, `${route.name}: mobile navigation trigger`).toBeVisible()
          await trigger.click()
          const drawer = page.getByRole('dialog', { name: 'Navigation' })
          await expect(drawer).toBeVisible()
          await expect(drawer.getByRole('navigation', { name: 'Primary' })).toBeVisible()
          expectNoOverflow(await overflow(page), `${route.name}: navigation drawer`)
          await page.keyboard.press('Escape')
          await expect(drawer).toBeHidden()
        }
      }

      // A task form must still open at this width.
      await page.goto(`${server.base}/projects/p-basic/board`)
      await settle(page)
      await page.getByRole('button', { name: 'New task', exact: true }).first().click()
      const create = page.getByRole('dialog')
      await expect(create.getByLabel('Title')).toBeVisible()
      expectNoOverflow(await overflow(page), `${viewport.name} new-task dialog`)
      await create.getByRole('button', { name: 'Cancel' }).click()
      await expect(create).toBeHidden()

      // A task drawer (with its attachments section) must open without overflow.
      await page.goto(`${server.base}/projects/p-orbit-development/board?task=${fixture.ids.orbitTask}`)
      const drawer = page.getByRole('dialog')
      await expect(drawer.getByLabel('Task description')).toBeVisible()
      // The drawer slides in; measuring its box mid-animation reports a
      // fractional overshoot that is not a layout overflow.
      await settle(page)
      await drawer.getByText('Attachments', { exact: true }).first().scrollIntoViewIfNeeded()
      await expect(drawer.getByText('Attachments', { exact: true }).first()).toBeVisible()
      expectNoOverflow(await overflow(page), `${viewport.name} task drawer`)
      const box = await drawer.boundingBox()
      expect(box, 'the drawer must have a box').not.toBeNull()
      expect(box!.x).toBeGreaterThanOrEqual(-1)
      expect(box!.x + box!.width).toBeLessThanOrEqual(viewport.width + 1)
    })
  })
}
