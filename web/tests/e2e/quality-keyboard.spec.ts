/**
 * M31 I6 — keyboard-only pass over the shell and the primary interactions.
 *
 * Every action in this file is a key press or a focus move. The point is not to
 * repeat the click-driven acceptance specs but to prove the surfaces are
 * operable without a pointer and that focus is never dropped on the floor:
 *
 *  - tab order through the shell, with a visible focus indicator on every stop;
 *  - the skip link;
 *  - the mobile navigation drawer: focus trap, Escape, focus return to trigger;
 *  - the command palette: open, focus, Escape, focus return;
 *  - board moves: `m` + arrows + Enter, Escape cancels, focus follows the card;
 *  - planning review and approval without a mouse;
 *  - the drawer's aria-live save announcement.
 */
import type { Page } from '@playwright/test'
import { expect, test } from './harness/fixtures'
import { goto, signInViaApi } from './harness/session'

interface FocusFact {
  tag: string
  label: string
  text: string
  focusVisible: boolean
  outlineWidth: number
  boxShadow: string
}

async function focusFact(page: Page): Promise<FocusFact | null> {
  return page.evaluate(() => {
    const element = document.activeElement as HTMLElement | null
    if (!element || element === document.body) return null
    const style = getComputedStyle(element)
    return {
      tag: element.tagName.toLowerCase(),
      label: element.getAttribute('aria-label') ?? '',
      text: (element.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 80),
      focusVisible: element.matches(':focus-visible'),
      outlineWidth: Number.parseFloat(style.outlineWidth) || 0,
      boxShadow: style.boxShadow,
    }
  })
}

function hasVisibleFocus(fact: FocusFact): boolean {
  return fact.focusVisible && (fact.outlineWidth >= 2 || fact.boxShadow !== 'none')
}

/** Press Tab until the focused control's label/text matches, or fail loudly. */
async function tabTo(page: Page, pattern: RegExp, max = 40): Promise<FocusFact> {
  const seen: string[] = []
  for (let index = 0; index < max; index += 1) {
    await page.keyboard.press('Tab')
    const fact = await focusFact(page)
    if (!fact) {
      seen.push('(body)')
      continue
    }
    seen.push(fact.label || fact.text)
    if (pattern.test(fact.label) || pattern.test(fact.text)) return fact
  }
  throw new Error(`could not Tab to ${pattern} within ${max} stops. Seen: ${seen.join(' → ')}`)
}

async function settle(page: Page): Promise<void> {
  await page
    .waitForFunction(() => !document.querySelector('main [aria-busy="true"]'), undefined, {
      timeout: 20_000,
    })
  await page.evaluate(async () => {
    await document.fonts.ready
    const finite = document
      .getAnimations()
      .filter((animation) => animation.effect?.getTiming()?.iterations !== Infinity)
    await Promise.all(finite.map((animation) => animation.finished.catch(() => undefined)))
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve(null))))
  })
}

test.describe('keyboard-only', () => {
  test('tab order runs through the shell with a visible focus indicator', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, '/')
    await settle(page)
    // A fresh navigation leaves the document body focused; start Tab there.
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur())

    const stops: FocusFact[] = []
    for (let index = 0; index < 9; index += 1) {
      await page.keyboard.press('Tab')
      const fact = await focusFact(page)
      expect(fact, `tab stop ${index + 1} must not fall to <body>`).not.toBeNull()
      expect(hasVisibleFocus(fact!), `tab stop ${index + 1} (${fact!.label || fact!.text})`).toBe(true)
      stops.push(fact!)
    }

    expect(stops[0].text).toBe('Skip to content')
    const labels = stops.slice(1).map((stop) => stop.label || stop.text)
    expect(labels[0], 'the brand link follows the skip link').toContain('Orbit')
    expect(labels[1]).toContain('Search or jump to')
    expect(labels.slice(2, 7)).toEqual(['Home', 'Inbox', 'Projects', 'Knowledge', 'Agents'])
  })

  test('the skip link moves focus into main', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, '/')
    await settle(page)
    await page.keyboard.press('Tab')
    await expect(page.locator(':focus')).toHaveText('Skip to content')
    await page.keyboard.press('Enter')
    await expect
      .poll(() => page.evaluate(() => document.activeElement?.id ?? ''))
      .toBe('main')
  })

  test('mobile drawer traps focus, closes on Escape and returns focus', async ({ page, workspace }) => {
    const { server } = workspace
    await signInViaApi(page, server)
    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(`${server.base}/`)
    await settle(page)

    const trigger = page.getByRole('button', { name: 'Open navigation' })
    await trigger.focus()
    expect((await focusFact(page))?.label).toBe('Open navigation')
    await page.keyboard.press('Enter')

    const drawer = page.getByRole('dialog', { name: 'Navigation' })
    await expect(drawer).toBeVisible()
    await expect
      .poll(() => page.evaluate(() => Boolean(document.activeElement?.closest('[role="dialog"]'))))
      .toBe(true)

    // Shift+Tab from the first focusable must stay inside the modal.
    await page.keyboard.press('Shift+Tab')
    expect(
      await page.evaluate(() => Boolean(document.activeElement?.closest('[role="dialog"]'))),
      'focus must not escape the drawer on Shift+Tab',
    ).toBe(true)

    await page.keyboard.press('Escape')
    await expect(drawer).toBeHidden()
    await expect
      .poll(async () => (await focusFact(page))?.label ?? '')
      .toBe('Open navigation')
  })

  test('the command palette opens, takes focus, and returns it on Escape', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, '/')
    await settle(page)

    const search = page.getByRole('button', { name: /Search or jump to/ })
    await search.focus()
    await page.keyboard.press('Control+k')

    const palette = page.getByRole('dialog', { name: 'Command palette' })
    await expect(palette).toBeVisible()
    const input = palette.getByRole('combobox')
    await expect(input).toBeFocused()
    await expect(input).toHaveAttribute('placeholder', 'Search commands…')
    await expect(palette.getByRole('option', { name: /New task/ })).toBeVisible()
    await expect(palette.getByRole('option', { name: /Go to Inbox/ })).toBeVisible()

    // A query that matches nothing says so, and Escape returns focus to the
    // control that had it.
    await page.keyboard.type('zzz')
    await expect(palette).toContainText('No matching commands')
    await page.keyboard.press('Escape')
    await expect(palette).toBeHidden()
    await expect(search).toBeFocused()
  })

  test('a board card moves with the keyboard and keeps focus', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) => candidate.projectId === 'p-basic' && candidate.status === 'inbox',
    )
    expect(task, 'the fixture needs an inbox task in p-basic').toBeTruthy()

    await goto(page, server, '/projects/p-basic/board')
    await settle(page)
    const card = page.getByRole('button', { name: task!.title, exact: true })
    await card.waitFor()
    await card.focus()

    // Escape cancels the move menu and leaves the card focused.
    await page.keyboard.press('m')
    await expect(page.getByRole('menu')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('menu')).toBeHidden()
    // Radix returns focus to the Move trigger beside the card; what matters is
    // that it is inside the card's row, not that it is the title button.
    await expect
      .poll(() =>
        page.evaluate(() =>
          Boolean(document.activeElement?.closest('li')?.querySelector('[data-board-action]')),
        ),
      )
      .toBe(true)

    // The Escape above left focus on the row's Move trigger. Shift+Tab returns
    // to the card title, which is the element that owns the `m` shortcut.
    await page.keyboard.press('Shift+Tab')
    await expect(card).toBeFocused()

    // Keyboard move: m → Planned → Enter.
    await page.keyboard.press('m')
    const menu = page.getByRole('menu')
    await expect(menu).toBeVisible()
    let reached = false
    for (let index = 0; index < 8; index += 1) {
      if ((await focusFact(page))?.text === 'Planned') {
        reached = true
        break
      }
      await page.keyboard.press('ArrowDown')
    }
    expect(reached, 'the Planned edge must be reachable with ArrowDown').toBe(true)
    await page.keyboard.press('Enter')

    const planned = page.getByRole('region', { name: /^Planned column/ })
    await expect(planned.getByRole('button', { name: task!.title, exact: true })).toBeVisible()
    await expect
      .poll(async () => (await focusFact(page))?.text ?? '')
      .toBe(task!.title)
  })

  test('planning review and approval are reachable and operable by keyboard', async ({ page, workspace }) => {
    const { server } = workspace
    await goto(page, server, '/projects/p-development/planning')
    await settle(page)

    const capacity = page.getByLabel('Available minutes today')
    await capacity.focus()
    await page.keyboard.press('Control+a')
    await page.keyboard.type('240')
    await page.keyboard.press('Enter')
    await expect(page.locator('[data-schedule-slot]').first()).toBeVisible({ timeout: 20_000 })

    const review = page.getByRole('button', { name: 'Review this plan' })
    await tabTo(page, /Review this plan/)
    await expect(review).toBeFocused()
    await page.keyboard.press('Enter')
    await expect(page.locator('[data-plan-review]')).toBeVisible()
    // Focus must land on the reviewed plan, not on <body>.
    await expect(page.locator('[data-plan-review]')).toBeFocused()

    const approve = page.getByRole('checkbox', { name: 'Approve the reviewed schedule' })
    await approve.focus()
    await page.keyboard.press('Space')
    await expect(approve).toBeChecked()

    const apply = page.getByRole('button', { name: 'Apply approved plan' })
    await apply.focus()
    await page.keyboard.press('Enter')
    await expect(page.locator('[data-plan-receipt]')).toBeVisible({ timeout: 20_000 })
    expect(await focusFact(page), 'applying must not drop focus to <body>').not.toBeNull()
  })

  test('the task drawer announces saves through a live region and returns focus', async ({
    page,
    workspace,
  }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find((candidate) => candidate.projectId === 'p-orbit-development')
    expect(task).toBeTruthy()

    await goto(page, server, '/projects/p-orbit-development/board')
    await settle(page)
    const card = page.getByRole('button', { name: task!.title, exact: true })
    await card.waitFor()
    await card.focus()
    await page.keyboard.press('Enter')

    const drawer = page.getByRole('dialog')
    await expect(drawer.getByLabel('Task description')).toBeVisible()

    // Close with Escape: focus must return to the card that opened the drawer.
    await page.keyboard.press('Escape')
    await expect(drawer).toBeHidden()
    await expect
      .poll(async () => (await focusFact(page))?.text ?? '')
      .toBe(task!.title)

    // Reopen and change priority with the keyboard only.
    await page.keyboard.press('Enter')
    const reopened = page.getByRole('dialog')
    await expect(reopened.getByLabel('Task description')).toBeVisible()

    // A description edit commits on blur and reports through the live region.
    const description = reopened.getByLabel('Task description')
    const before = await description.inputValue()
    await description.focus()
    await description.press('Control+End')
    await page.keyboard.type(' (keyboard check)')
    await page.keyboard.press('Tab')
    expect(await description.inputValue(), 'the edit must differ from what was stored').not.toBe(
      before,
    )

    const live = reopened.locator('footer [aria-live="polite"]')
    await expect(live).toContainText(/saved\./i, { timeout: 20_000 })
  })
})
