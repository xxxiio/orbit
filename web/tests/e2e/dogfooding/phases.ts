/**
 * Phase functions for the automated interactive dogfooding session.
 *
 * Each phase is written the way a person would use the screen: named controls,
 * visible text, real reloads. Where a phase records a finding, the observation
 * says what was on screen and what was expected instead.
 *
 * Nothing here is a product assertion suite: a failed step is recorded and the
 * session moves on, exactly like a person who hits a wall and tries the next
 * thing they need to do.
 */
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'

import { expect, type Page } from '@playwright/test'

import { runExecutor } from '../harness/capability'
import {
  claim,
  jobClock,
  observe,
  readJob,
  reportLaunch,
  submitHandoff,
} from '../harness/agents'
import type { DogfoodContext, StepRecord } from './session'
import { runStep } from './session'

type Phase = {
  id: string
  title: string
  run: (page: Page, ctx: DogfoodContext, record: (step: StepRecord) => void) => Promise<void>
}

/** A step inside a phase: wrapped so one failure does not abort the phase. */
async function step(
  page: Page,
  ctx: DogfoodContext,
  record: (step: StepRecord) => void,
  spec: Omit<StepRecord, 'status' | 'durationMs' | 'screenshot' | 'observed' | 'friction'> & {
    friction?: string[]
    run: () => Promise<
      { observed?: string; friction?: string[]; data?: Record<string, string> } | void
    >
  },
): Promise<StepRecord> {
  const result = await runStep(page, ctx.scratch.capturesDir as string, spec)
  if (result.data) Object.assign(ctx.ids, result.data)
  record(result)
  return result
}

function drawer(page: Page) {
  return page.locator('[data-task-drawer]')
}

function section(page: Page, name: string) {
  return page.getByRole('region', { name, exact: true })
}
export function phases(): Phase[] {
  return [
    { id: '01-sign-in', title: 'Sign in', run: signIn },
    { id: '02-home', title: 'Read today', run: home },
    { id: '03-capture', title: 'Capture work', run: capture },
    { id: '04-search', title: 'Find existing work', run: search },
    { id: '05-prioritize', title: 'Prioritise and save a view', run: prioritize },
    { id: '06-plan', title: 'Plan a day', run: planDay },
    { id: '07-progress', title: 'Record progress', run: progress },
    { id: '08-blocker', title: 'Open and resolve a blocker', run: blocker },
    { id: '09-knowledge', title: 'Write and connect a note', run: knowledge },
    { id: '10-agents', title: 'Review an agent job', run: agents },
    { id: '11-capability', title: 'Run a workflow action', run: capability },
    { id: '12-complete', title: 'Complete work and review a milestone', run: complete },
    { id: '13-switch', title: 'Switch projects, return, and resume', run: switchProjects },
  ]
}

/* ------------------------------------------------------------------ */
/* 01 sign in                                                          */
/* ------------------------------------------------------------------ */

async function signIn(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '01a-login-form',
    phase: '01-sign-in',
    action: 'Open the app at / and sign in with the enrolled device through the login form.',
    expected: 'The app redirects to /login, accepts the device and lands on Today.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/`)
      await expect(page).toHaveURL(/\/login/, { timeout: 15_000 })
      await expect(page.getByRole('heading', { name: 'Sign in to Orbit' })).toBeVisible()
      await page.getByLabel('Device ID').fill(ctx.workspace.state.deviceId)
      await page.getByLabel('Device token').fill(ctx.workspace.state.token)
      const started = Date.now()
      await page.getByRole('button', { name: 'Sign in', exact: true }).click()
      await expect(page).not.toHaveURL(/\/login/, { timeout: 20_000 })
      await expect(page).toHaveTitle('Today · Orbit')
      const landed = Date.now() - started
      return {
        observed: `Signed in and landed on Today in ${landed} ms (${page.url().replace(ctx.workspace.base, '')}).`,
        friction: landed > 4_000 ? [`Sign-in took ${landed} ms before Today rendered.`] : [],
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 02 home                                                             */
/* ------------------------------------------------------------------ */

async function home(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '02a-today',
    phase: '02-home',
    action: 'Read the Today screen: the summary and the sections that need attention.',
    expected: 'Sections name a project, a status and a deadline; counts agree with the list.',
    run: async () => {
      await expect(page).toHaveTitle('Today · Orbit')
      const summary = (await page.locator('#main').innerText()).replace(/\s+/g, ' ')
      const friction: string[] = []
      if (!/Overdue|Due today|Ready to start/.test(summary)) {
        friction.push('Today shows none of the expected working sections.')
      }
      const stats = await page.locator('#main').getByText(/\d+ (open|unassigned)/).allInnerTexts()
      return {
        observed: `${stats.join(' · ')} — ${summary.slice(0, 220)}`,
        friction,
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 03 capture                                                          */
/* ------------------------------------------------------------------ */

async function capture(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  const quickTitle = `${ctx.marker} triage the dogfooding findings`
  await step(page, ctx, record, {
    id: '03a-quick-capture',
    phase: '03-capture',
    action: `Type "${quickTitle}" into the Inbox quick capture and press Enter.`,
    expected: 'The task is accepted, appears in the list and the field clears.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/inbox`)
      await expect(page.getByRole('heading', { level: 1, name: 'Inbox' })).toBeVisible()
      const input = page.getByLabel('Capture a task for the Inbox')
      const started = Date.now()
      await input.fill(quickTitle)
      await input.press('Enter')
      await expect(page.getByText(quickTitle, { exact: true }).first()).toBeVisible()
      const acceptedIn = Date.now() - started
      await expect(input).toHaveValue('')

      // Opening the task: click the title cell. Clicking the middle of the row
      // lands on the inline project/priority controls, which is recorded below.
      const row = page.locator('tr[data-row]').filter({ hasText: quickTitle }).first()
      await row.getByText(quickTitle, { exact: true }).click()
      await expect(drawer(page)).toBeVisible()
      const id = new URL(page.url()).searchParams.get('task') ?? ''
      await page.keyboard.press('Escape')
      await expect(drawer(page)).toBeHidden()
      return {
        observed: `Task accepted in ${acceptedIn} ms, field cleared, drawer opened by clicking the title.`,
        data: { quickTaskId: id, quickTaskTitle: quickTitle },
        friction: [
          'Clicking the middle of the row opens the inline project selector, not the task; only the title text is a reliable way in.',
          ...(acceptedIn > 2_000 ? [`Quick capture took ${acceptedIn} ms to show the new row.`] : []),
        ],
      }
    },
  })

  await step(page, ctx, record, {
    id: '03b-new-task-dialog',
    phase: '03-capture',
    action: 'Create a project task with the full New task dialog, choosing Orbit Development.',
    expected: 'The dialog closes, the toast names the task and the task belongs to the project.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/`)
      await expect(page).toHaveTitle('Today · Orbit')
      await page.getByRole('button', { name: 'New task', exact: true }).first().click()
      const dialog = page.getByRole('dialog').filter({ hasText: 'New task' })
      await expect(dialog).toBeVisible()
      const title = `${ctx.marker} harden the capability receipt read`
      await dialog.getByLabel('Title').fill(title)
      await dialog.getByLabel('Project for the new task').click()
      await page.getByRole('option', { name: 'Orbit Development', exact: true }).click()
      await dialog.getByRole('button', { name: 'Create task', exact: true }).click()
      await expect(dialog).toBeHidden()
      await expect(page.getByText('Task created', { exact: true })).toBeVisible()

      await page.keyboard.press('Control+k')
      await page.locator('[data-search-query]').fill(title)
      const result = page.locator('[data-search-result^="task:"]').first()
      await expect(result).toBeVisible()
      const id = (await result.getAttribute('data-search-result'))?.split(':')[1] ?? ''
      await page.keyboard.press('Escape')
      return {
        observed: 'Created from the dialog and found again through search.',
        data: { dialogTaskId: id, dialogTaskTitle: title },
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 04 search                                                           */
/* ------------------------------------------------------------------ */

async function search(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '04a-unified-search',
    phase: '04-search',
    action: `Search for "${ctx.marker}" from Today and open the task result.`,
    expected:
      'Groups name Projects/Tasks/Knowledge; the captured task opens in the drawer and the palette closes.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/`)
      await expect(page.getByRole('button', { name: /Search or jump to/ })).toBeVisible()
      const started = Date.now()
      await page.keyboard.press('Control+k')
      const palette = page.locator('[data-command-palette]')
      await expect(palette).toBeVisible()
      await page.locator('[data-search-query]').fill(ctx.marker)
      await expect(palette.locator('[data-search-state]')).toHaveAttribute(
        'data-search-state',
        'ready',
      )
      const readyIn = Date.now() - started
      const groups = await palette
        .locator('[data-search-group]')
        .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-search-group') ?? ''))
      const countLabel = (await palette.locator('[data-search-count]').innerText()).trim()
      const taskRow = palette.locator('[data-search-result^="task:"]').first()
      await expect(taskRow).toBeVisible()
      await taskRow.click()
      await expect(drawer(page)).toBeVisible()
      await expect(palette).toBeHidden()
      return {
        observed: `Groups returned: ${groups.join(', ') || '(none)'} after ${readyIn} ms (${countLabel}); the task opened in the drawer and the palette closed.`,
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 05 prioritise + saved view                                          */
/* ------------------------------------------------------------------ */

async function prioritize(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '05a-prioritise',
    phase: '05-prioritize',
    action: 'In the drawer, set the captured task to high priority and move it to Planned.',
    expected: 'Priority and status change and survive a reload.',
    run: async () => {
      const taskId = ctx.ids.quickTaskId
      if (!taskId) throw new Error('no quick-captured task from 03a')
      await page.goto(`${ctx.workspace.base}/inbox?task=${taskId}`)
      await expect(drawer(page)).toBeVisible()
      await drawer(page).getByLabel('Task priority').click()
      await page.getByRole('option', { name: 'High', exact: true }).click()
      await expect(drawer(page).getByLabel('Task priority')).toContainText(/high/i)
      await drawer(page).getByLabel('Task status').click()
      await page.getByRole('option', { name: 'Planned', exact: true }).click()
      await expect(drawer(page).getByLabel('Task status')).toContainText(/planned/i)
      await page.reload()
      await expect(drawer(page).getByLabel('Task priority')).toContainText(/high/i)
      await expect(drawer(page).getByLabel('Task status')).toContainText(/planned/i)
      return { observed: 'Priority and status survived a reload, with the drawer still open.' }
    },
  })

  await step(page, ctx, record, {
    id: '05b-save-view',
    phase: '05-prioritize',
    action: 'Filter a task list to high priority, save it as a view, reload and restore it.',
    expected: 'The view round-trips through the server and restores filters, sort and display.',
    run: async () => {
      // The Development workflow declares no task-list view; the Basic project
      // does. A person with a workflow project still needs a list with filters,
      // so the list view is exercised where the product offers it.
      await page.goto(`${ctx.workspace.base}/projects/p-basic/tasks`)
      await expect(page.getByLabel('Filter by priority')).toBeVisible()
      await page.getByLabel('Filter by priority').click()
      await page.getByRole('option', { name: 'High', exact: true }).click()
      await page.getByLabel('Sort tasks').click()
      await page.getByRole('option', { name: 'Sort: Title' }).click()
      const query = new URL(page.url()).search.replace(/^\?/, '')

      await page.locator('[data-saved-views-trigger]').click()
      const dialog = page.locator('[data-saved-views]')
      await expect(dialog).toBeVisible()
      await dialog.locator('[data-saved-view-new]').click()
      await dialog.locator('[data-saved-view-name-input]').fill(`${ctx.marker} high priority`)
      await dialog.locator('[data-saved-view-submit]').click()
      await expect(page.getByText('Saved view created', { exact: true })).toBeVisible()
      await page
        .getByRole('dialog')
        .filter({ has: page.locator('[data-saved-views]') })
        .getByRole('button', { name: 'Close dialog' })
        .click()

      await page.reload()
      await expect(page.getByLabel('Filter by priority')).toBeVisible()
      await page.locator('[data-saved-views-trigger]').click()
      const reopened = page.locator('[data-saved-views]')
      const row = reopened
        .locator('[data-saved-view]')
        .filter({ hasText: `${ctx.marker} high priority` })
      await expect(row).toBeVisible()
      await row.locator('[data-saved-view-apply]').click()
      await expect(page).toHaveURL(
        new RegExp(`\\?${query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`),
      )
      await expect(page.getByLabel('Filter by priority')).toContainText('High')
      return { observed: `Restored the stored query after a reload: ${query}` }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 06 plan a day                                                       */
/* ------------------------------------------------------------------ */

/** A future day (never one an earlier session filled) as an `<input type=date>` value. */
function futureDate(daysAhead: number): string {
  const date = new Date()
  date.setDate(date.getDate() + daysAhead)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/**
 * Author a free window. Earlier sessions booked 09:00–12:00 on the following
 * days, and the planner correctly refuses to double-book, so a session that
 * wants to plan has to choose a day that is still empty — exactly like a person.
 */
async function authorWindow(
  panel: ReturnType<Page['locator']>,
  daysAhead: number,
): Promise<void> {
  await panel.getByLabel('Schedule window date').fill(futureDate(daysAhead))
  await panel.getByLabel('Schedule window start time').fill('09:00')
  await panel.getByLabel('Schedule window end time').fill('12:00')
  await panel.getByRole('button', { name: 'Refresh', exact: true }).click()
  await expect
    .poll(async () => {
      const slots = await panel.locator('[data-schedule-slot]').count()
      const empty = await panel.getByText(/placed nothing in this window/).count()
      return slots > 0 || empty > 0
    })
    .toBe(true)
}

async function planDay(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '06a-day-plan',
    phase: '06-plan',
    action: 'Open Orbit Development planning, author 3h of capacity and build the day plan.',
    expected: 'Capacity is accepted, the server dispositions appear and the URL keeps the input.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development/planning`)
      const panel = page
        .locator('section')
        .filter({ has: page.getByRole('heading', { name: 'Day plan', exact: true }) })
        .last()
      await expect(panel).toBeVisible()
      const capacity = panel.getByLabel('Available minutes today')
      await capacity.fill('180')
      const friction: string[] = []
      // Authoring the number alone leaves the panel as a form: the plan is only
      // requested by an explicit button.
      await capacity.press('Enter')
      await panel.getByRole('button', { name: 'Build day plan' }).click()
      await expect(page).toHaveURL(/capacity=180/)
      await expect(
        page.getByText(/actionable and \d+ blocked tasks were evaluated/),
      ).toBeVisible({ timeout: 15_000 })
      const body = (await page.locator('#main').innerText()).replace(/\s+/g, ' ')
      return { observed: body.slice(0, 300), friction }
    },
  })

  await step(page, ctx, record, {
    id: '06b-review-orbit-plan',
    phase: '06-plan',
    action: 'Review the proposed window for Orbit Development and approve it.',
    expected: 'Review is read-only, approval applies the exact command, and the blocks survive a reload.',
    run: async () => {
      const panel = page
        .locator('section')
        .filter({ has: page.getByRole('heading', { name: 'Schedule a window', exact: true }) })
        .last()
      await expect(panel).toBeVisible()
      await authorWindow(panel, 2 + ctx.runIndex)
      const slots = panel.locator('[data-schedule-slot]')
      if ((await slots.count()) === 0) {
        throw new Error(
          'The server placed nothing in a free future window, so there is nothing to review.',
        )
      }
      const slotCount = await slots.count()
      const firstTitle = (await slots.first().locator('p').first().textContent())?.trim() ?? ''

      await panel.getByRole('button', { name: 'Review this plan' }).click()
      const review = page.locator('[data-plan-review]')
      const refusal = panel.getByText(/Review failed/i)
      // Either the review appears, or the server refused its own proposal. Both
      // are recorded: the second is a defect, not a script problem.
      await expect(review.or(refusal).first()).toBeVisible({ timeout: 15_000 })
      if (!(await review.isVisible())) {
        const text = (await panel.innerText()).replace(/\s+/g, ' ')
        const reason = text.slice(text.indexOf('Review failed'), text.indexOf('Review failed') + 160)
        throw new Error(
          `${slotCount} slots proposed; the review was refused: "${reason}". The proposal cannot be approved.`,
        )
      }
      await expect(review.getByText('Reviewed · not applied')).toBeVisible()
      await review.getByRole('checkbox', { name: 'Approve the reviewed schedule' }).check()
      const started = Date.now()
      await panel.getByRole('button', { name: 'Apply approved plan' }).click()
      const receipt = page.locator('[data-plan-receipt]')
      await expect(receipt).toBeVisible()
      const appliedIn = Date.now() - started
      await expect(receipt.getByText(/Plan applied at revision \d+/)).toBeVisible()

      // Read it back from a different surface after a navigation.
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development/milestones`)
      const schedule = page
        .locator('section')
        .filter({ has: page.getByRole('heading', { name: 'Schedule', exact: true }) })
        .last()
      await expect(schedule.getByText(firstTitle, { exact: true }).first()).toBeVisible()
      return {
        observed: `${slotCount} blocks reviewed and applied in ${appliedIn} ms; "${firstTitle}" is on the milestones timeline.`,
        friction: appliedIn > 3_000 ? [`Applying the plan took ${appliedIn} ms.`] : [],
      }
    },
  })

  await step(page, ctx, record, {
    id: '06c-plan-development-project',
    phase: '06-plan',
    action: 'Try the same planning flow for the Development project, which has no overdue work.',
    expected:
      'If the refused proposal is specific to overdue tasks, the same steps complete here and the blocks persist.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/projects/p-development/planning`)
      const panel = page
        .locator('section')
        .filter({ has: page.getByRole('heading', { name: 'Schedule a window', exact: true }) })
        .last()
      await authorWindow(panel, 3 + ctx.runIndex)
      const slots = panel.locator('[data-schedule-slot]')
      if ((await slots.count()) === 0) {
        throw new Error('The server placed nothing in the free window for Development.')
      }
      const count = await slots.count()
      const firstTitle = (await slots.first().locator('p').first().textContent())?.trim() ?? ''
      await panel.getByRole('button', { name: 'Review this plan' }).click()
      const review = page.locator('[data-plan-review]')
      await expect(review).toBeVisible({ timeout: 15_000 })
      await review.getByRole('checkbox', { name: 'Approve the reviewed schedule' }).check()
      await panel.getByRole('button', { name: 'Apply approved plan' }).click()
      await expect(page.locator('[data-plan-receipt]')).toBeVisible()
      return {
        observed: `${count} blocks applied for Development ("${firstTitle}"); the same flow works where no task is overdue.`,
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 07 progress                                                         */
/* ------------------------------------------------------------------ */

async function progress(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '07a-record-progress',
    phase: '07-progress',
    action:
      'Open the created task, write a description, add two acceptance criteria, check one, and move it to In progress.',
    expected: 'Each field is stored, the checklist updates the count, and the status change lands.',
    run: async () => {
      const taskId = ctx.ids.dialogTaskId
      if (!taskId) throw new Error('no task created by the New task dialog (03b)')
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development?task=${taskId}`)
      await expect(drawer(page)).toBeVisible()

      const description = drawer(page).getByLabel('Task description')
      await description.fill('Exercise the receipt read against a real executor and record what moved.')
      await description.blur()

      const criteria = section(page, 'Acceptance criteria')
      const newCriterion = criteria.getByLabel('New acceptance criterion')
      const addButton = criteria.getByRole('button', { name: 'Add', exact: true })
      const friction: string[] = []
      const addCriterion = async (text: string) => {
        await newCriterion.fill(text)
        await newCriterion.press('Enter')
        // `Add` is disabled while a previous command is pending, so a fast
        // second Enter is swallowed. Wait for the save to settle: either the
        // field emptied (this add landed) or Add is enabled again.
        await expect
          .poll(async () => {
            const value = await newCriterion.inputValue()
            return value.trim().length === 0 || (await addButton.isEnabled())
          })
          .toBe(true)
        if ((await newCriterion.inputValue()).trim().length > 0) {
          friction.push(
            `Enter was ignored while the previous criterion was saving; "${text}" stayed in the field and needed an explicit Add.`,
          )
          await addButton.click()
          await expect(newCriterion).toHaveValue('')
        }
      }
      await addCriterion('The receipt is read for the claiming device only')
      await addCriterion('A second executor cannot claim the same action')
      await expect(criteria.getByText(/0\/2 done/).first()).toBeVisible({ timeout: 15_000 })

      await criteria
        .getByRole('checkbox', { name: 'Check The receipt is read for the claiming device only' })
        .click()
      await expect(criteria.getByText(/1\/2 done/).first()).toBeVisible()

      await drawer(page).getByLabel('Task status').click()
      await page.getByRole('option', { name: 'Ready', exact: true }).click()
      await expect(drawer(page).getByLabel('Task status')).toContainText(/ready/i)
      await drawer(page).getByLabel('Task status').click()
      await page.getByRole('option', { name: 'In progress', exact: true }).click()
      await expect(drawer(page).getByLabel('Task status')).toContainText(/in progress/i)

      await page.reload()
      await expect(drawer(page).getByLabel('Task description')).toHaveValue(
        'Exercise the receipt read against a real executor and record what moved.',
      )
      await expect(section(page, 'Acceptance criteria').getByText(/1\/2 done/).first()).toBeVisible()
      return {
        observed: 'Description, two criteria (one checked) and the In progress status survived a reload.',
        friction,
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 08 blocker                                                          */
/* ------------------------------------------------------------------ */

async function blocker(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '08a-blocker',
    phase: '08-blocker',
    action:
      'Add a prerequisite to the created task, try to mark it Blocked, read the reason, then remove the prerequisite and unblock it.',
    expected: 'The prerequisite is stored, Blocked names what it waits on, and both steps are reversible.',
    run: async () => {
      const taskId = ctx.ids.dialogTaskId
      if (!taskId) throw new Error('no task created by the New task dialog (03b)')
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development?task=${taskId}`)
      const dependencies = section(page, 'Dependencies')
      await expect(dependencies).toBeVisible()
      await dependencies.getByLabel('Add a prerequisite').click()
      const options = page.getByRole('option')
      await expect(options.first()).toBeVisible()
      const prerequisite = (await options.first().innerText()).trim()
      await options.first().click()
      await dependencies.getByRole('button', { name: 'Add', exact: true }).click()
      const remove = dependencies.getByRole('button', {
        name: `Remove prerequisite ${prerequisite}`,
        exact: true,
      })
      await expect(remove).toBeVisible()
      const friction: string[] = []

      // From In progress, Blocked may not be an offered edge; walk the legal
      // route the product shows instead of forcing an illegal command.
      const status = drawer(page).getByLabel('Task status')
      await status.click()
      const directBlocked = await page.getByRole('option', { name: 'Blocked', exact: true }).count()
      if (directBlocked === 0) {
        friction.push('Blocked is not offered from In progress; a person must know to move via Ready first.')
        await page.getByRole('option', { name: 'Ready', exact: true }).click()
        await expect(status).toContainText(/ready/i)
        await status.click()
      }
      await page.getByRole('option', { name: 'Blocked', exact: true }).click()
      await expect(status).toContainText(/blocked/i)
      const reads = (await dependencies.innerText()).replace(/\s+/g, ' ')
      if (!/unfinished prerequisites/i.test(reads)) {
        friction.push('Blocked status did not name the prerequisite that causes it.')
      }

      // Resolve it: remove the prerequisite and move back into progress.
      await remove.click()
      await expect(remove).toHaveCount(0)
      await status.click()
      const inProgress = page.getByRole('option', { name: 'In progress', exact: true })
      if ((await inProgress.count()) === 0) {
        await page.getByRole('option', { name: 'Ready', exact: true }).click()
        await status.click()
      }
      await page.getByRole('option', { name: 'In progress', exact: true }).click()
      await expect(status).toContainText(/in progress/i)
      return {
        observed: `Prerequisite "${prerequisite}" added and removed; Blocked explained it, then In progress was accepted.`,
        friction,
        data: { prerequisiteTitle: prerequisite },
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 09 knowledge                                                        */
/* ------------------------------------------------------------------ */

async function knowledge(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  const fileName = `df-run-${ctx.runIndex}-decisions.md`
  await step(page, ctx, record, {
    id: '09a-create-note',
    phase: '09-knowledge',
    action: 'Create a note in the notes folder, then link it to an existing note by id.',
    expected: 'The note is stored in the folder and the link lands in both directions.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/knowledge?folder=notes`)
      await expect(page.locator('[data-folder-action="new-note"]')).toBeVisible()
      await page.locator('[data-folder-action="new-note"]').click()
      const dialog = page.locator('[data-dialog="note-create"]')
      await expect(dialog).toBeVisible()
      await dialog.locator('[data-field="note-name"]').fill(fileName)
      await dialog.getByLabel('Title').fill(`Dogfooding decisions, run ${ctx.runIndex}`)
      await dialog
        .getByLabel('Markdown')
        .fill(
          `# Dogfooding decisions, run ${ctx.runIndex}\n\nThe receipt read stays owner-scoped; an executor never reads another device's claim.\n`,
        )
      await dialog.locator('[data-action="create-note"]').click()
      await expect(dialog).toBeHidden()
      const path = `notes/${fileName}`
      await expect(page).toHaveURL(new RegExp(`/knowledge/${path}$`))
      await expect(page.locator('[data-note-panel]')).toBeVisible()
      return {
        observed: `Note created at ${path}.`,
        data: { notePath: path },
      }
    },
  })

  await step(page, ctx, record, {
    id: '09b-link-and-reference',
    phase: '09-knowledge',
    action:
      'Link the note to an existing note, then reference the Orbit Development project and the task from 03b.',
    expected: 'Both links are visible with a backlink, and both references persist across a reload.',
    run: async () => {
      const notePath = ctx.ids.notePath
      if (!notePath) throw new Error('no note created in 09a')
      await page.goto(`${ctx.workspace.base}/knowledge/${notePath}`)
      await page.locator('[data-note-action="link"]').click()
      const linkDialog = page.locator('[data-dialog="note-link"]')
      await expect(linkDialog).toBeVisible()
      await linkDialog.locator('[data-field="note-search"]').fill('orbit')
      const option = linkDialog.locator('[data-note-option]').first()
      await expect(option).toBeVisible()
      await option.click()
      await linkDialog.locator('[data-action="link-note"]').click()
      await expect(linkDialog).toBeHidden()
      await expect(page.locator('[data-reference-group="links"] [data-note-link]').first()).toBeVisible()

      await page.goto(`${ctx.workspace.base}/knowledge/${notePath}?project=p-orbit-development`)
      const panel = page.locator('[data-reference-panel]')
      await panel.locator('[data-action="reference-this-project"]').click()
      await expect(panel.locator('[data-reference-target="project:p-orbit-development"]')).toBeVisible()

      const taskId = ctx.ids.dialogTaskId
      if (!taskId) throw new Error('no task to reference (03b)')
      await page.goto(
        `${ctx.workspace.base}/knowledge/${notePath}?project=p-orbit-development&task=${taskId}`,
      )
      const taskPanel = page.locator('[data-reference-panel]')
      await taskPanel.locator('[data-action="reference-this-task"]').click()
      await expect(taskPanel.locator(`[data-reference-target="task:${taskId}"]`)).toBeVisible()

      await page.reload()
      await expect(
        page.locator(`[data-reference-target="task:${taskId}"]`),
      ).toBeVisible()
      await expect(
        page.locator('[data-reference-target="project:p-orbit-development"]'),
      ).toBeVisible()
      await expect(page.locator('[data-reference-group="links"] [data-note-link]').first()).toBeVisible()
      return { observed: 'Link, project reference and task reference all survived a reload.' }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 10 agents                                                           */
/* ------------------------------------------------------------------ */

async function agents(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '10a-agents-overview',
    phase: '10-agents',
    action: 'Open Agents and read the queue: what needs attention and what is waiting for an executor.',
    expected: 'Jobs are grouped by state with the next action and the project named.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/agents`)
      await expect(page.getByRole('heading', { level: 1, name: 'Agents' })).toBeVisible()
      const attention = page.locator('[aria-label="Needs attention"] [data-agent-job]')
      const waiting = page.locator('[data-agent-job]')
      const body = (await page.locator('#main').innerText()).replace(/\s+/g, ' ')
      return {
        observed: `${await attention.count()} need attention, ${await waiting.count()} jobs listed. ${body.slice(0, 200)}`,
      }
    },
  })

  await step(page, ctx, record, {
    id: '10b-launch-job',
    phase: '10-agents',
    action: 'Queue an agent job for the in-progress task from its drawer, with an objective.',
    expected: 'The confirmation names the pinned job and the chosen executor, and the job lands queued.',
    run: async () => {
      const taskId = ctx.ids.dialogTaskId
      if (!taskId) throw new Error('no task created by the New task dialog (03b)')
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development?task=${taskId}`)
      const trigger = drawer(page).locator('[data-launch-agent-job]')
      await expect(trigger).toBeEnabled()
      await trigger.click()
      const dialog = page.locator('[data-launch-dialog]')
      await expect(dialog).toBeVisible()
      const objective = `${ctx.marker} verify the receipt read and report what changed.`
      await dialog.locator('#agent-launch-objective').fill(objective)
      await dialog.locator('#agent-launch-repository').fill('orbit')
      await dialog.getByRole('button', { name: 'Review and queue' }).click()
      await expect(dialog).toHaveAttribute('data-launch-step', 'confirm')
      const jobId = (await dialog.locator('[data-launch-job-id]').textContent())?.trim() ?? ''
      await dialog.getByRole('button', { name: 'Queue the job' }).click()
      await page.waitForURL((url) => url.pathname === `/agents/${jobId}`, { timeout: 20_000 })
      await expect(page.locator('[data-claim-state]')).toHaveAttribute('data-claim-state', 'none')
      await expect(page.getByText('Preparing', { exact: true }).first()).toBeVisible()
      return {
        observed: `Queued ${jobId}; it is Preparing with no claim, and the page says it waits for an executor.`,
        data: { agentJobId: jobId, agentObjective: objective },
      }
    },
  })

  await step(page, ctx, record, {
    id: '10c-executor-and-review',
    phase: '10-agents',
    action:
      'No agent runtime is installed here: act as the enrolled executor through the machine routes (claim, launch, observe, handoff), then approve the handoff in the browser.',
    expected: 'The browser shows the claim, the running state and the handoff; approval does not complete the task.',
    run: async () => {
      const jobId = ctx.ids.agentJobId
      if (!jobId) throw new Error('no job was queued in 10b')
      const server = ctx.workspace as unknown as Parameters<typeof readJob>[1]
      const job = await readJob(page, server as never, jobId)
      const clock = jobClock(job.created_at_ms)
      const claimToken = `claim-${jobId}`
      await claim(page, server as never, {
        jobId,
        expectedRevision: 1,
        claimToken,
        atMs: clock.next(),
      })
      const sessionRef = `session-${jobId}`
      const launched = await reportLaunch(page, server as never, {
        jobId,
        claimToken,
        sessionRef,
        atMs: clock.next(),
      })
      await observe(page, server as never, {
        jobId,
        expectedRevision: launched.revision,
        claimToken,
        sessionRef,
        activity: 'working',
        atMs: clock.next(),
      })
      const handoffId = `h-${jobId}`
      await submitHandoff(page, server as never, {
        jobId,
        handoffId,
        claimToken,
        sessionRef,
        payload: {
          summary: `${ctx.marker} read the capability receipt and left the item untouched.`,
          validation: 'the receipt was read from the server, not from the request',
          next_steps: ['review the receipt', 'confirm the task stayed in progress'],
        },
        atMs: clock.next(),
      })

      await page.reload()
      await expect(page.locator('[data-claim-state]')).toHaveAttribute(
        'data-claim-state',
        'confirmed',
      )
      await expect(page.getByText('Running', { exact: true }).first()).toBeVisible()
      const review = page.getByRole('region', { name: 'Review' })
      await review.getByRole('button', { name: 'Approve', exact: true }).click()
      await review.getByLabel('Reason').fill('Read the handoff and its validation; the task stays in progress.')
      await review.getByRole('button', { name: 'Record review' }).click()
      await expect(
        page.getByText('Review recorded as approved. The task was not completed.', { exact: true }),
      ).toBeVisible()
      await page.reload()
      await expect(page.getByRole('region', { name: 'Review' })).toContainText('Approved')
      return {
        observed: `Claim, launch, running state, handoff and the approval are all on screen; the task was not completed by the review.`,
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 11 capability                                                       */
/* ------------------------------------------------------------------ */

async function capability(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '11a-capability-panel',
    phase: '11-capability',
    action:
      'Open the connected-capability project, create a work item through the UI and read its action panel.',
    expected: 'The new item opens and the panel names the capability installed, granted and announced.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/projects/p-dogfood-capability`)
      await expect(page.getByRole('heading', { level: 1, name: 'Connected capability demo' })).toBeVisible()
      const before = await page
        .locator('[data-work-item-key]')
        .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-work-item-key') ?? ''))
      await page.getByRole('button', { name: 'New work item', exact: true }).click()
      const dialog = page.getByRole('dialog').filter({ hasText: 'New work item' })
      await expect(dialog).toBeVisible()
      await dialog.getByRole('button', { name: 'Create work item', exact: true }).click()
      await expect(dialog).toBeHidden()
      await expect
        .poll(async () => {
          const now = await page
            .locator('[data-work-item-key]')
            .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-work-item-key') ?? ''))
          return now.length
        })
        .toBe(before.length + 1)
      const after = await page
        .locator('[data-work-item-key]')
        .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-work-item-key') ?? ''))
      const itemId = after.find((id) => !before.includes(id)) ?? ''
      await page.locator(`[data-work-item-key="${itemId}"] button`).first().click()
      await expect(page).toHaveURL(new RegExp(`item=${itemId}`))
      const panel = page.locator('[data-capability-panel]')
      await expect(panel).toBeVisible()
      const status = page.locator('[data-capability-status="orbit.reference.work-item-note"]')
      const installed = await status.getAttribute('data-capability-installed')
      const granted = await status.getAttribute('data-capability-granted')
      const executors = await status.getAttribute('data-capability-executors')
      const offer = page.locator('[data-capability-offer="append_note"]')
      const available = await offer.getAttribute('data-capability-available')
      const code = await offer.getAttribute('data-capability-code')
      return {
        observed: `Created item ${itemId}; installed=${installed} granted=${granted} executors=${executors} offer available=${available} code=${code}`,
        data: { capabilityItemId: itemId },
      }
    },
  })

  await step(page, ctx, record, {
    id: '11b-request-action',
    phase: '11-capability',
    action: 'Request the append-note action with a note, then run the real executor and read the receipt.',
    expected: 'The request reaches succeeded, exactly one durable note exists, and the receipt is visible.',
    run: async () => {
      const panel = page.locator('[data-capability-panel]')
      await panel.locator('[data-capability-request-open="append_note"]').click()
      const form = page.locator('[data-capability-request-form="append_note"]')
      await expect(form).toBeVisible()
      const note = `${ctx.marker} dogfooding: the receipt read stayed owner-scoped.`
      await form.locator('[data-capability-input="note"] textarea').fill(note)
      await form.getByRole('button', { name: /Review the request|Request / }).click()
      const confirm = page.locator('[data-capability-confirm="append_note"]')
      if (await confirm.isVisible().catch(() => false)) {
        await page.locator('[data-capability-confirm-submit]').click()
      }
      await expect(page.locator('[data-capability-request]').first()).toBeVisible()

      const run = await runExecutor(
        // The runner only needs the origin and a private directory for its replica.
        ctx.workspace as unknown as Parameters<typeof runExecutor>[0],
        {
          deviceId: ctx.workspace.state.deviceId,
          token: ctx.workspace.state.token,
        },
      )
      const outcome = String(run.result.outcome ?? run.result.status ?? '')
      await page.reload()
      await expect(page.locator('[data-capability-receipt]').first()).toBeVisible({ timeout: 15_000 })
      await expect(
        page.locator('[data-capability-panel]').getByText(note, { exact: false }).first(),
      ).toBeVisible()
      return {
        observed: `Executor said outcome=${outcome} (${run.result.status}); the receipt and the note are on screen after a reload.`,
        friction: outcome === 'succeeded' ? [] : [`The executor reported ${outcome}.`],
      }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 12 complete + milestone                                             */
/* ------------------------------------------------------------------ */

async function complete(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '12a-complete-task',
    phase: '12-complete',
    action:
      'Assign the task to the I1 acceptance milestone, check the remaining criterion and move the task to Done.',
    expected: 'The milestone is stored, the checklist reads 2/2 and the server accepts Done; a reload keeps it.',
    run: async () => {
      const taskId = ctx.ids.dialogTaskId
      if (!taskId) throw new Error('no task created by the New task dialog (03b)')

      // Read the milestone's progress before this task joins it, so the change
      // can be attributed to this action rather than assumed.
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development/milestones`)
      const before = (await page.locator('[data-milestone-card="m-orbit-i1"]').innerText())
        .replace(/\s+/g, ' ')
      ctx.scratch.milestoneBefore = before

      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development?task=${taskId}`)
      await expect(drawer(page)).toBeVisible()
      const milestone = drawer(page).getByLabel('Milestone')
      await milestone.click()
      await page.getByRole('option', { name: 'I1 acceptance', exact: true }).click()
      await expect(milestone).toContainText('I1 acceptance')

      const criteria = section(page, 'Acceptance criteria')
      await criteria
        .getByRole('checkbox', { name: 'Check A second executor cannot claim the same action' })
        .click()
      await expect(criteria.getByText(/2\/2 done/).first()).toBeVisible()

      await drawer(page).getByLabel('Task status').click()
      await page.getByRole('option', { name: 'Done', exact: true }).click()
      await expect(drawer(page).getByLabel('Task status')).toContainText(/done/i)
      await page.reload()
      await expect(drawer(page).getByLabel('Task status')).toContainText(/done/i)
      return {
        observed: `Both criteria checked, the milestone assigned and Done accepted; still Done after a reload. Milestone before: ${before.slice(0, 80)}`,
      }
    },
  })

  await step(page, ctx, record, {
    id: '12b-milestone',
    phase: '12-complete',
    action: 'Review the milestone progress after the completion and check the count moved.',
    expected: 'The milestone row names its status and its task progress from the server.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development/milestones`)
      const row = page.locator('[data-milestone-card="m-orbit-i1"]')
      await expect(row).toBeVisible()
      const text = (await row.innerText()).replace(/\s+/g, ' ')
      const after = /(\d+)\/(\d+) done/.exec(text)
      const before = /(\d+)\/(\d+) done/.exec(String(ctx.scratch.milestoneBefore ?? ''))
      const friction: string[] = []
      if (before && after) {
        const doneMoved = Number(after[1]) === Number(before[1]) + 1
        const totalMoved = Number(after[2]) === Number(before[2]) + 1
        if (!doneMoved || !totalMoved) {
          friction.push(
            `The completed task joined the milestone but the count read ${after[0]} (was ${before[0]}).`,
          )
        }
      }
      return { observed: text.slice(0, 220), friction }
    },
  })
}

/* ------------------------------------------------------------------ */
/* 13 switch projects, reload, resume                                  */
/* ------------------------------------------------------------------ */

async function switchProjects(page: Page, ctx: DogfoodContext, record: (s: StepRecord) => void) {
  await step(page, ctx, record, {
    id: '13a-switch-projects',
    phase: '13-switch',
    action: 'Switch to Development, then back to Orbit Development, and reload once.',
    expected: 'Both project pages render their own workflow views and the return keeps the task state.',
    run: async () => {
      await page.goto(`${ctx.workspace.base}/projects/p-development`)
      await expect(page.getByRole('heading', { level: 1, name: 'Development' })).toBeVisible()
      await page.goto(`${ctx.workspace.base}/projects/p-orbit-development`)
      await expect(page.getByRole('heading', { level: 1, name: 'Orbit Development' })).toBeVisible()
      await page.reload()
      await expect(page.getByRole('heading', { level: 1, name: 'Orbit Development' })).toBeVisible()
      const tabs = (await page.getByRole('link').allInnerTexts())
        .map((text) => text.replace(/\s+/g, ' ').trim())
        .filter((text) => /^(Overview|Board|Milestones|Agent jobs|Planning)\b/.test(text))
      return { observed: `Orbit Development tabs after reload: ${tabs.join(' | ')}` }
    },
  })

  await step(page, ctx, record, {
    id: '13b-resume-previous-run',
    phase: '13-switch',
    action: 'Search for the previous run’s marker and open what it left behind.',
    expected:
      'A previous run’s work is still stored; this run resumes it instead of starting from an empty workspace.',
    run: async () => {
      if (ctx.runIndex === 1) {
        return { observed: 'First session on this database: nothing to resume yet.' }
      }
      const previous = `DF${ctx.runIndex - 1}`
      const previousReport = JSON.parse(
        await readFile(
          join(ctx.scratch.capturesDir as string, `steps-run-${ctx.runIndex - 1}.json`),
          'utf8',
        ),
      ) as { steps: StepRecord[] }
      const completed = previousReport.steps.find((step) => step.id === '12a-complete-task')
      const created = previousReport.steps.find((step) => step.id === '03b-new-task-dialog')
      const completedTaskId = created?.data?.dialogTaskId ?? ''

      await page.goto(`${ctx.workspace.base}/`)
      await expect(page.getByRole('button', { name: /Search or jump to/ })).toBeVisible()
      await page.keyboard.press('Control+k')
      const palette = page.locator('[data-command-palette]')
      await expect(palette).toBeVisible()
      await page.locator('[data-search-query]').fill(previous)
      const results = page.locator('[data-search-result]')
      await expect(results.first()).toBeVisible()
      const count = await results.count()
      await page.keyboard.press('Escape')

      const friction: string[] = []
      if (completedTaskId) {
        await page.goto(`${ctx.workspace.base}/projects/p-orbit-development?task=${completedTaskId}`)
        await expect(drawer(page)).toBeVisible()
        const status = await drawer(page).getByLabel('Task status').innerText()
        if (completed?.status === 'ok' && !/done/i.test(status)) {
          friction.push('The previous run marked this task Done but it now reads back as not Done.')
        }
        const completedInPreviousRun = completed?.status === 'ok' ? /done/i.test(status) : true
        return {
          observed: `${count} search results for the previous run's marker; its task still opens with status "${status.trim()}".`,
          friction,
          data: { resumedTaskStatus: status.trim(), completedInPreviousRun: String(completedInPreviousRun) },
        }
      }
      return { observed: `${count} search results for the previous run's marker.`, friction }
    },
  })
}
