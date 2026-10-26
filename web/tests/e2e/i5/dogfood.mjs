#!/usr/bin/env node
/**
 * M31 I5 — real dogfooding cycle for the Orbit web app.
 *
 *   cd web && node tests/e2e/i5/dogfood.mjs
 *
 * What this script is
 * -------------------
 * A headless Playwright run against a *persistent, isolated authoritative
 * database* on a dedicated port (8790), serving the built SPA from
 * `web/dist`. The only direct HTTP calls are setup (enrol one device, install
 * the bundled Development workflow, open a browser session) and one deliberate
 * "second writer" used to prove revision fencing. Projects, tasks, milestones,
 * work items, notes, attachments and every task change in the cycle are typed
 * through the real browser UI.
 *
 * State lives in `/tmp/orbit-i5/vault/db.sqlite3` so the run is reproducible
 * from a clean `/tmp` state; the script deletes that directory first. At the
 * end it stops the server and copies the SQLite database (and WAL, when one
 * exists) into `web/evidence/i5/`.
 *
 * The run is intentionally not part of the default Playwright suite: it is a
 * long, stateful dogfooding cycle, not a per-test fixture. It is still a normal
 * Node script, so `node tests/e2e/i5/dogfood.mjs` is the whole command.
 */
import { spawn, execFileSync } from 'node:child_process'
import { randomBytes, randomUUID } from 'node:crypto'
import { copyFile, mkdir, readFile, rm, writeFile, access } from 'node:fs/promises'
import { createConnection } from 'node:net'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

import { chromium } from '@playwright/test'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const REPO = path.resolve(HERE, '../../../..')
const WEB = path.join(REPO, 'web')
const DIST = path.join(WEB, 'dist')
const OUT = path.join(WEB, 'evidence', 'i5')
const RUN_DIR = '/tmp/orbit-i5'
const DB = path.join(RUN_DIR, 'vault', 'db.sqlite3')
const PORT = 8790
const BASE = `http://127.0.0.1:${PORT}`
const DEVICE_ID = 'orbit-development'
const DEVICE_TOKEN = randomBytes(32).toString('hex')
const ADMIN_KEY = randomBytes(32).toString('hex')

// Chromium on this machine needs the local bundle of system libraries.
for (const candidate of [
  process.env.ORBIT_E2E_BROWSER_LIBS,
  '/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu',
].filter(Boolean)) {
  try {
    await access(path.join(candidate, 'libnspr4.so'))
    process.env.LD_LIBRARY_PATH = process.env.LD_LIBRARY_PATH
      ? `${candidate}:${process.env.LD_LIBRARY_PATH}`
      : candidate
    break
  } catch {
    // try the next candidate
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

function log(message) {
  process.stdout.write(`[i5] ${message}\n`)
}

async function waitUntil(check, { label = 'condition', timeout = 20_000, interval = 100 } = {}) {
  const started = Date.now()
  let lastError = null
  while (Date.now() - started < timeout) {
    try {
      const result = await check()
      if (result) return result
    } catch (error) {
      lastError = error
    }
    await sleep(interval)
  }
  throw new Error(
    `Timed out after ${timeout}ms waiting for ${label}${lastError ? ` (${lastError.message})` : ''}`,
  )
}

async function portIsFree() {
  return new Promise((resolve) => {
    const socket = createConnection({ host: '127.0.0.1', port: PORT })
    socket.once('connect', () => {
      socket.destroy()
      resolve(false)
    })
    socket.once('error', () => resolve(true))
  })
}

async function api(pathname, { method = 'GET', body, admin = false } = {}) {
  const headers = { 'content-type': 'application/json' }
  if (admin) headers.authorization = `Bearer ${ADMIN_KEY}`
  else {
    headers.authorization = `Bearer ${DEVICE_TOKEN}`
    headers['x-orbit-device-id'] = DEVICE_ID
  }
  const response = await fetch(`${BASE}${pathname}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const text = await response.text()
  let parsed = null
  try {
    parsed = text.length > 0 ? JSON.parse(text) : null
  } catch {
    parsed = text
  }
  return { status: response.status, body: parsed, text }
}

/* ------------------------------------------------------------------ */
/* Server lifecycle                                                    */
/* ------------------------------------------------------------------ */

let server = null
let serverLog = ''

async function startServer() {
  const clean = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (!key.startsWith('ORBIT_')) clean[key] = value
  }
  const child = spawn(path.join(REPO, 'target', 'debug', 'orbit-server'), [], {
    cwd: REPO,
    env: {
      ...clean,
      ORBIT_SERVER_DB: DB,
      ORBIT_SERVER_BIND: `127.0.0.1:${PORT}`,
      ORBIT_SERVER_ADMIN_KEY: ADMIN_KEY,
      ORBIT_WEB_DIST: DIST,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  const chunks = []
  child.stdout.on('data', (chunk) => chunks.push(String(chunk)))
  child.stderr.on('data', (chunk) => chunks.push(String(chunk)))
  server = child
  const started = Date.now()
  while (Date.now() - started < 30_000) {
    if (child.exitCode !== null) {
      throw new Error(`orbit-server exited (${child.exitCode}) before health:\n${chunks.join('')}`)
    }
    try {
      const response = await fetch(`${BASE}/health`, { signal: AbortSignal.timeout(1_500) })
      if (response.ok) {
        log(`server healthy on ${BASE} (pid ${child.pid})`)
        return
      }
    } catch {
      // not up yet
    }
    await sleep(100)
  }
  throw new Error(`orbit-server did not become healthy:\n${chunks.join('')}`)
}

async function stopServer() {
  if (!server) return
  const child = server
  server = null
  if (child.exitCode === null) {
    child.kill('SIGTERM')
    const exited = await Promise.race([
      new Promise((resolve) => child.once('exit', () => resolve(true))),
      sleep(8_000).then(() => false),
    ])
    if (!exited) {
      child.kill('SIGKILL')
      await new Promise((resolve) => child.once('exit', resolve))
    }
  }
  serverLog = ''
}

/* ------------------------------------------------------------------ */
/* Browser helpers                                                     */
/* ------------------------------------------------------------------ */

let browser = null
let context = null
let page = null
const httpErrors = []
const steps = []

function shotPath(name) {
  return path.join(OUT, `${name}.png`)
}

async function shot(name) {
  await page.evaluate(() => document.fonts.ready).catch(() => undefined)
  await sleep(300)
  await page.screenshot({ path: shotPath(name) })
  log(`capture ${name}.png`)
}

async function dialog() {
  return page.getByRole('dialog')
}

async function checkNoAlert(where) {
  const alerts = where.locator('[role="alert"]')
  const count = await alerts.count()
  if (count === 0) return
  const texts = []
  for (let index = 0; index < count; index += 1) {
    const text = (await alerts.nth(index).innerText()).trim()
    if (text.length > 0) texts.push(text)
  }
  if (texts.length > 0) throw new Error(`unexpected alert: ${texts.join(' | ')}`)
}

async function openTaskFromBoard(title) {
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  const card = page.getByRole('button', { name: title, exact: true })
  await card.waitFor({ timeout: 15_000 })
  await card.click()
  const drawer = await dialog()
  await drawer.waitFor()
  await drawer.getByLabel('Task description').waitFor({ timeout: 15_000 })
  const taskId = new URL(page.url()).searchParams.get('task')
  if (!taskId) throw new Error('the task drawer did not put the task id in the URL')
  return { drawer, taskId }
}

async function closeTaskDrawer() {
  const drawer = await dialog()
  if (!(await drawer.isVisible().catch(() => false))) return
  const close = drawer.getByRole('button', { name: 'Close task details' })
  await close.click()
  try {
    await drawer.waitFor({ state: 'hidden', timeout: 4_000 })
  } catch {
    // A pending edit or the close guard asked for a second press.
    await close.click()
    await drawer.waitFor({ state: 'hidden', timeout: 6_000 })
  }
  await sleep(150)
}

/** Radix select: click the trigger, click the named option. */
async function chooseOption(locator, optionName, label) {
  await locator.click()
  const option = page.getByRole('option', { name: optionName, exact: true })
  await option.waitFor({ timeout: 8_000 })
  await option.click()
  await waitUntil(
    async () => (await locator.innerText()).toLowerCase().includes(optionName.toLowerCase()),
    { label: `${label} to read ${optionName}`, timeout: 10_000 },
  )
}

async function setPriority(drawer, priority) {
  await chooseOption(drawer.getByLabel('Task priority'), priority, 'priority')
  await checkNoAlert(drawer)
}

async function setStatus(drawer, statusLabel) {
  await chooseOption(drawer.getByLabel('Task status'), statusLabel, 'status')
  await checkNoAlert(drawer)
}

async function setLabels(drawer, labels) {
  await drawer.getByRole('button', { name: 'Edit task labels' }).click()
  const input = drawer.locator('#task-labels')
  await input.fill(labels.join(', '))
  await input.press('Enter')
  await waitUntil(
    async () => {
      const text = await drawer.getByRole('button', { name: 'Edit task labels' }).innerText()
      return labels.every((label) => text.toLowerCase().includes(label.toLowerCase()))
    },
    { label: `labels ${labels.join(', ')}` },
  )
  await checkNoAlert(drawer)
}

async function setDescription(drawer, description, expectSave = true) {
  const area = drawer.getByLabel('Task description')
  await area.fill(description)
  await area.press('Tab')
  if (!expectSave) return
  await waitUntil(
    async () => {
      const footer = await drawer.locator('footer').innerText()
      return /Description saved\.|Changes save when you leave a field\./.test(footer) &&
        !/Saving…/.test(footer)
    },
    { label: 'the description to be saved' },
  )
  await checkNoAlert(drawer)
}

async function addCriterion(drawer, text) {
  const input = drawer.getByLabel('New acceptance criterion')
  await input.fill(text)
  await input.press('Enter')
  await waitUntil(
    async () => (await drawer.getByRole('button', { name: `Edit criterion ${text}` }).count()) > 0,
    { label: `criterion “${text}”` },
  )
  await checkNoAlert(drawer)
}

async function addPrerequisite(drawer, title) {
  const dependencies = drawer.getByRole('region', { name: 'Dependencies' })
  await chooseOption(dependencies.getByLabel('Add a prerequisite'), title, 'prerequisite')
  await dependencies.getByRole('button', { name: 'Add', exact: true }).click()
  await waitUntil(
    async () => (await dependencies.getByRole('button', { name: `Remove prerequisite ${title}` }).count()) > 0,
    { label: `prerequisite “${title}”` },
  )
  await checkNoAlert(drawer)
}

async function setMilestone(drawer, name) {
  await chooseOption(drawer.getByLabel('Task milestone'), name, 'milestone')
  await checkNoAlert(drawer)
}


/** Move one board card with the surface's own keyboard grammar (m + arrows + Enter). */
async function boardKeyboardMove(title, targetLabel) {
  const card = page.getByRole('button', { name: title, exact: true })
  await card.waitFor()
  await card.focus()
  await page.keyboard.press('m')
  await page.getByRole('menu').waitFor({ timeout: 8_000 })
  // Radix focuses the first item after the open effect; reading the cursor
  // before that lands on the trigger and never sees the arrow keys move it.
  await sleep(350)
  let arrowPresses = 0
  let focused = ''
  for (let index = 0; index < 12; index += 1) {
    focused = await page.evaluate(() => document.activeElement?.textContent?.trim() ?? '')
    if (focused === targetLabel) break
    await page.keyboard.press('ArrowDown')
    arrowPresses += 1
    await sleep(60)
  }
  if (focused !== targetLabel) {
    throw new Error(`could not reach ${targetLabel} with the keyboard (focused: ${focused})`)
  }
  await page.keyboard.press('Enter')
  await waitUntil(
    async () =>
      (await page
        .getByRole('listitem')
        .filter({ has: page.getByRole('button', { name: title, exact: true }) })
        .getByText(targetLabel, { exact: true })
        .count()) > 0,
    { label: `${title} to land in ${targetLabel}`, timeout: 15_000 },
  )
  await sleep(250)
  return arrowPresses
}

async function createTask(title) {
  await page.getByRole('button', { name: 'New task', exact: true }).first().click()
  const modal = await dialog()
  await modal.getByLabel('Title').fill(title)
  await page.getByRole('button', { name: 'Create task', exact: true }).click()
  await modal.waitFor({ state: 'detached', timeout: 15_000 })
  await page.getByRole('button', { name: title, exact: true }).first().waitFor({ timeout: 15_000 })
}

/* ------------------------------------------------------------------ */
/* The project content                                                 */
/* ------------------------------------------------------------------ */

const MILESTONE_M31 = 'M31 completion'
const MILESTONE_REF = 'Reference-client ready'

/**
 * Real remaining work for M31, drawn from the gate logs and carry-forwards.
 * Estimates and deadlines are intentionally absent: the browser has no
 * control for `set_planning`, which is the headline finding of this run.
 */
const TASKS = [
  {
    title: 'I5: write the dogfooding findings summary',
    priority: 'Urgent',
    labels: ['i5', 'docs', 'gate'],
    milestone: MILESTONE_M31,
    status: 'Ready',
    description:
      '## Context\n\nM31 I5 runs one real planning-to-completion cycle on the **Orbit Development** project and records every friction point.\n\nThe summary must classify each finding by root-cause layer and give a reproduction, expected and actual behaviour, severity and a regression test.\n\n## Output\n\n`.slim/deepwork/m31-web-app/i5-issues.md` and `web/evidence/i5/`.',
    criteria: [
      'Every significant finding names its root-cause layer',
      'The log records reproduction, severity and resolution for each entry',
    ],
  },
  {
    title: 'I5: wire estimate and deadline editing into the task drawer',
    priority: 'Urgent',
    labels: ['i5', 'planning', 'frontend'],
    milestone: MILESTONE_M31,
    status: 'Ready',
    description:
      'The API client already exposes `set_planning` (`estimated_minutes`, `earliest_start_at_ms`, `deadline_at_ms`), but no browser surface calls it.\n\nThe task drawer’s **Details** section has status, priority, project and milestone only, so a person cannot size work or give it a deadline. Without an estimate the server marks every day-plan row `missing_estimate`, the schedule plan has no slots, and the review/apply gate cannot be reached.\n\nAdd the three declared fields with the same read-back and revision fences as the other property edits, plus a regression test that a person can size a task and get a schedule slot.',
    criteria: [
      'Estimate, earliest start and deadline can be authored from the task drawer',
      'Values survive reload and appear in readiness and the planning surfaces',
    ],
  },
  {
    title: 'I5: prove browser planning with a sized task',
    priority: 'High',
    labels: ['i5', 'planning', 'e2e'],
    milestone: MILESTONE_M31,
    description:
      'Once estimate authoring exists, run the full capacity → day plan → window → review → approve → apply path in a browser and assert that the accepted blocks survive reload.\n\nThis task carries the current run’s blocked outcome so the gate does not mistake the planning surface for verified.',
    dependsOn: 'I5: wire estimate and deadline editing into the task drawer',
  },
  {
    title: 'I6: refine the surfaces flagged by dogfooding',
    priority: 'High',
    labels: ['i6', 'ux'],
    milestone: MILESTONE_M31,
    status: 'Ready',
    description:
      'Work the I5 issue log top-down. Keep each fix in its root-cause layer: a missing API call belongs in the client, a missing control belongs in the surface, a misleading promise belongs in the copy.',
    dependsOn: 'I5: write the dogfooding findings summary',
  },
  {
    title: 'I6: update docs/web-app.md for I3 to I5',
    priority: 'Medium',
    labels: ['i6', 'docs'],
    milestone: MILESTONE_M31,
    status: 'Planned',
    description:
      'The guide still describes the I2 surface. Add work items, knowledge, attachments, agents, the planning gate and the honest gaps (no project rename, no workflow editing, no estimate authoring until fixed).',
    dependsOn: 'I6: refine the surfaces flagged by dogfooding',
    criteria: [
      'Every route reachable in the product is described',
      'Known gaps are named instead of omitted',
    ],
  },
  {
    title: 'I6: write the TUI redesign spec',
    priority: 'High',
    labels: ['i6', 'tui', 'docs'],
    milestone: MILESTONE_REF,
    description:
      'Turn the dogfooding findings into a concrete TUI redesign: navigation, task detail parity, planning, and the command grammar. Keep it a specification, not an implementation, and do not redesign the TUI during M31.',
    dependsOn: 'I5: write the dogfooding findings summary',
    criteria: [
      'The spec names the surfaces that must reach parity with the web app',
      'Every proposed change is scoped outside M31',
    ],
  },
  {
    title: 'I6: close carry-forward 3 with one shared command queue',
    priority: 'Medium',
    labels: ['i6', 'debt', 'runtime'],
    milestone: MILESTONE_M31,
    description:
      'Cross-surface concurrent commands still fence on the same revision: the second surface answers 409 and self-heals only after a reload.\n\nOne shared command queue (or an explicit refresh-on-conflict path) would keep two open surfaces from stepping on each other.',
  },
  {
    title: 'I6: consume the attachment associate API',
    priority: 'Low',
    labels: ['i6', 'attachments'],
    milestone: MILESTONE_REF,
    description:
      '`/v1/authoritative/attachments/associate` has no browser control. Decide whether moving an attachment between owners is a real workflow or should be refused by name.',
  },
  {
    title: 'I6: expose note deletion in the knowledge workspace',
    priority: 'Medium',
    labels: ['i6', 'knowledge'],
    milestone: MILESTONE_REF,
    description:
      'Folder creation and note deletion remain console-only. A vault that can create but never remove a note is only half a filesystem; the note-delete command does not exist yet, so the API comes first.',
  },
  {
    title: 'I6: name the byte limit in the 413 upload copy',
    priority: 'Low',
    labels: ['i6', 'attachments', 'copy'],
    milestone: MILESTONE_REF,
    description:
      'The upload error says the file is too large but cannot say by how much, because the server response omits the configured limit. Return the limit (or make it readable) and quote it in the copy.',
  },
  {
    title: 'I6: split the remaining api mutation modules',
    priority: 'Low',
    labels: ['i6', 'debt', 'frontend'],
    milestone: MILESTONE_M31,
    description:
      '`api/mutations.ts` still carries the largest command surface. Split it along the same family boundaries the endpoints already use, without changing behaviour.',
  },
  {
    title: 'I6: remove the project settings view diff duplication',
    priority: 'Low',
    labels: ['i6', 'debt', 'frontend'],
    milestone: MILESTONE_M31,
    description:
      'The upgrade preview and the settings panel each build the view-title map. One shared resolver avoids the two drifting apart.',
  },
  {
    title: 'I6: prove an agent job record without launching a runtime',
    priority: 'Medium',
    labels: ['i6', 'agents'],
    milestone: MILESTONE_REF,
    description:
      'The browser can read, review and resume agent jobs but cannot create one without an external runtime. Record whether a job record can be created through a supported local path, or keep the gap explicit.',
    criteria: [
      'Either a job record is created through a supported path, or the gap is documented',
    ],
  },
  {
    title: 'I6: restore getdone-validate-project in this environment',
    priority: 'Medium',
    labels: ['i6', 'tooling', 'gate'],
    milestone: MILESTONE_M31,
    description:
      '`getdone-validate-project` fails before it reads any record because the skill pack has no `manifests.json` in this checkout. Either restore the pack or record a written waiver so completion claims can be validated.',
  },
  {
    title: 'I6: review the gate 4 findings and record dispositions',
    priority: 'High',
    labels: ['i6', 'gate'],
    milestone: MILESTONE_M31,
    status: ['Planned', 'In progress'],
    description:
      'Gate 4 is running. Read the review, answer each finding, and record fixed / waived / deferred with a reason. Do not let the gate close on an unread report.',
    criteria: [
      'Every gate 4 finding has a written disposition',
      'Deferred findings are scheduled into a named iteration',
    ],
  },
  {
    title: 'I6: add a first-class note to task relation',
    priority: 'Medium',
    labels: ['i6', 'knowledge', 'tasks'],
    milestone: MILESTONE_REF,
    description:
      'A task can only point at a note with a hand-written Markdown link. There is no association the server can read, count or show as a relation. Decide the storage shape before building the chip.',
    dependsOn: 'I5: write the dogfooding findings summary',
  },
  {
    title: 'I5: document the Development feature kind title gap',
    priority: 'Low',
    labels: ['i5', 'docs', 'workflow'],
    milestone: MILESTONE_M31,
    description:
      'The bundled Development workflow declares a Feature kind with only a `priority` field. Every work item therefore renders as “Feature” and two features cannot be told apart. Either the manifest needs a title field or the product needs a declared title convention; record which one before I6.',
  },
  {
    title: 'I5: record the board keyboard move evidence',
    priority: 'Medium',
    labels: ['i5', 'evidence', 'board'],
    milestone: MILESTONE_M31,
    status: 'Ready',
    description:
      'Capture the before and after frames of a status move made with `m` plus arrow keys on the board, and record the number of keystrokes and any focus surprises next to the accepted card.',
    criteria: [
      'Before and after frames are committed under web/evidence/i5',
      'The frame is reachable from the evidence README',
    ],
  },
]

const INBOX_TRIAGE_TITLE = 'Triage: capture I5 friction before the gate'

/* ------------------------------------------------------------------ */
/* Run                                                                 */
/* ------------------------------------------------------------------ */

const state = {
  startedAt: new Date().toISOString(),
  projectId: null,
  taskIds: {},
  milestoneIds: {},
  workItemId: null,
  notePath: null,
  planning: {},
  conflict: {},
  boardMove: {},
  restart: {},
  captureCommit: null,
}

async function run() {
  if (!(await portIsFree())) {
    throw new Error(`port ${PORT} is already in use; stop the other server first`)
  }
  await rm(RUN_DIR, { recursive: true, force: true })
  await mkdir(path.dirname(DB), { recursive: true })
  await mkdir(OUT, { recursive: true })
  for (const stale of ['failure.png', 'failure-page.html', 'failure-state.json']) {
    await rm(path.join(OUT, stale), { force: true })
  }

  try {
    state.captureCommit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: REPO })
      .toString()
      .trim()
  } catch {
    state.captureCommit = 'unknown'
  }

  await startServer()

  // Setup: one device for the whole cycle.
  const enrolled = await api('/v1/devices', {
    method: 'POST',
    admin: true,
    body: { device_id: DEVICE_ID, token: DEVICE_TOKEN },
  })
  if (enrolled.status !== 201) throw new Error(`device enrolment failed: ${enrolled.status} ${enrolled.text}`)
  log(`enrolled ${DEVICE_ID}`)

  browser = await chromium.launch({ headless: true })
  context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    colorScheme: 'light',
    baseURL: BASE,
  })
  page = await context.newPage()
  await page.addInitScript(() => window.localStorage.setItem('orbit.theme', 'light'))
  page.on('response', (response) => {
    if (response.status() >= 400 && response.url().includes('/v1/')) {
      httpErrors.push({ status: response.status(), method: response.request().method(), url: response.url() })
    }
  })

  // ---- Step 1: Home and sign-in through the real form -------------------
  await page.goto(`${BASE}/login`)
  await page.getByRole('heading', { name: 'Sign in to Orbit' }).waitFor()
  await page.getByLabel('Device ID').fill(DEVICE_ID)
  await page.getByLabel('Device token').fill(DEVICE_TOKEN)
  await page.getByRole('button', { name: 'Sign in', exact: true }).click()
  await page.waitForURL((url) => !url.pathname.startsWith('/login'), { timeout: 15_000 })
  await page.locator('main h1').first().waitFor()
  await shot('01-home-empty')
  steps.push('Home rendered signed in, before any project existed')

  // ---- Step 2: Orbit Development through the create-project UI ----------
  await page.goto(`${BASE}/projects`)
  await page.getByRole('heading', { level: 1, name: 'Projects' }).waitFor()
  await page.getByRole('button', { name: 'New project' }).first().click()
  let createProject = await dialog()
  await createProject.waitFor()
  await createProject.getByText('No other workflows are installed on this server yet.').waitFor()
  await shot('01b-new-project-no-workflows')
  await createProject.getByRole('button', { name: 'Cancel' }).click()
  await createProject.waitFor({ state: 'detached' })

  // Setup: the bundled Development workflow must be published before a project
  // can pin it. There is no browser control for this (issue I5-02), so this is
  // the one setup write outside the UI; the choice itself is made in the form.
  const installed = await api('/v1/authoritative/blueprints/install', {
    method: 'POST',
    body: { id: 'development' },
  })
  if (installed.status !== 200) throw new Error(`workflow install failed: ${installed.status} ${installed.text}`)
  log(`published Development v${installed.body?.version}`)
  steps.push('A fresh server had no Development workflow; the New project dialog listed only the default. Published it with the setup API, reloaded so the cached list refreshed, then chose it in the form.')

  // The workflow list is cached for 60s and the dialog has no refresh control;
  // a full reload is the only way a freshly published workflow becomes visible.
  await page.reload()
  await page.getByRole('heading', { level: 1, name: 'Projects' }).waitFor()
  await page.getByRole('button', { name: 'New project' }).first().click()
  createProject = await dialog()
  await createProject.waitFor()
  await page.getByRole('radio', { name: /Development/ }).check()
  await createProject.getByLabel('Project name').fill('Orbit Development')
  await shot('02-new-project-development')
  await page.getByRole('button', { name: 'Create project' }).click()
  await page.waitForURL(/\/projects\/[^/]+$/, { timeout: 20_000 })
  state.projectId = new URL(page.url()).pathname.split('/').filter(Boolean)[1]
  await page.getByRole('heading', { level: 1, name: 'Orbit Development' }).waitFor()
  await page.getByText('Development v2', { exact: true }).first().waitFor()
  await shot('03-project-overview')
  log(`project ${state.projectId}`)
  steps.push(`Created project Orbit Development (${state.projectId}) with the Development workflow through the dialog`)

  // ---- Step 3: 18 real tasks through the new-task dialog -----------------
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.getByRole('button', { name: 'New task', exact: true }).first().click()
  const copyDialog = await dialog()
  await copyDialog.getByLabel('Title').waitFor()
  await shot('04-new-task-dialog-copy')
  await copyDialog.getByRole('button', { name: 'Cancel' }).click()
  await copyDialog.waitFor({ state: 'detached' })

  for (const task of TASKS) {
    await createTask(task.title)
  }
  log(`created ${TASKS.length} tasks through the dialog`)
  steps.push(`Created ${TASKS.length} tasks through the New task dialog`)

  // ---- Step 4: two milestones through the milestones view ---------------
  await page.goto(`${BASE}/projects/${state.projectId}/milestones`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  for (const name of [MILESTONE_M31, MILESTONE_REF]) {
    await page.getByRole('button', { name: 'New milestone', exact: true }).first().click()
    const modal = await dialog()
    await modal.getByLabel('Milestone name').fill(name)
    await page.getByRole('button', { name: 'Create milestone', exact: true }).click()
    await modal.waitFor({ state: 'detached' })
    await page.getByText(name, { exact: true }).first().waitFor()
  }
  const milestones = await api('/v1/authoritative/milestones')
  for (const milestone of milestones.body?.milestones ?? []) {
    if (milestone.project_id === state.projectId) state.milestoneIds[milestone.name] = milestone.id
  }
  await shot('05-milestones-created')
  log(`milestones ${JSON.stringify(state.milestoneIds)}`)
  steps.push('Created milestones M31 completion and Reference-client ready through the milestones view')

  // ---- Step 5: enrich through the drawer --------------------------------
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  for (const task of TASKS) {
    const { drawer, taskId } = await openTaskFromBoard(task.title)
    state.taskIds[task.title] = taskId
    await setPriority(drawer, task.priority)
    await setLabels(drawer, task.labels)
    if (task.status) {
      const path = Array.isArray(task.status) ? task.status : [task.status]
      for (const status of path) await setStatus(drawer, status)
    }
    if (task.description) await setDescription(drawer, task.description)
    if (task.criteria) for (const text of task.criteria) await addCriterion(drawer, text)
    if (task.dependsOn) await addPrerequisite(drawer, task.dependsOn)
    if (task.milestone) await setMilestone(drawer, task.milestone)
    await closeTaskDrawer()
  }
  log('enriched priorities, labels, statuses, descriptions, criteria, dependencies')
  steps.push('Set priorities, labels, initial statuses, Markdown descriptions, acceptance criteria and dependencies in the drawer')

  // Milestone attachment happened in the enrichment pass; verify one drawer
  // reads it back.
  {
    const { drawer } = await openTaskFromBoard(TASKS[0].title)
    await waitUntil(
      async () => (await drawer.getByLabel('Task milestone').innerText()).includes(MILESTONE_M31),
      { label: 'the milestone to read back' },
    )
    await closeTaskDrawer()
  }

  // ---- Step 6: a Development feature work item ---------------------------
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.getByRole('button', { name: 'New work item', exact: true }).first().click()
  const itemDialog = await dialog()
  await itemDialog.waitFor()
  await shot('06-new-work-item-fields')
  const featureFields = await itemDialog.locator('label, [role="combobox"], input, button').allInnerTexts()
  await page.getByRole('button', { name: 'Create work item', exact: true }).click()
  await itemDialog.waitFor({ state: 'detached', timeout: 15_000 })
  const items = await api(`/v1/authoritative/blueprints/items?project_id=${state.projectId}`)
  state.workItemId = items.body?.[0]?.id ?? null
  if (!state.workItemId) throw new Error('the created work item could not be read back')
  await page.locator(`[data-work-item-key="${state.workItemId}"]`).waitFor({ timeout: 15_000 })
  await shot('07-work-item-created')
  steps.push(`Created one Feature work item (${state.workItemId}); declared fields: ${featureFields.join(' / ').slice(0, 180)}`)

  // ---- Step 7: knowledge note, then link it from Task A ------------------
  await page.goto(`${BASE}/knowledge`)
  await page.getByRole('heading', { level: 1, name: 'Knowledge' }).waitFor()
  await page.getByRole('button', { name: 'New note' }).first().click()
  const noteDialog = await dialog()
  await noteDialog.getByLabel('Path').fill('i5-dogfooding.md')
  await noteDialog.getByLabel('Title').fill('I5 dogfooding findings')
  const taskAId = state.taskIds[TASKS[0].title]
  await noteDialog
    .getByLabel('Markdown')
    .fill(
      `This note is filed from the real browser cycle for task \`${taskAId}\` (**${TASKS[0].title}**).\n\n## Headline\n\n- The browser cannot author an estimate or a deadline, so the planning review/apply gate is unreachable.\n- A bundled workflow has no install control; the project dialog only lists what is already published.\n- The Development Feature kind declares no title field.\n- Two writers fence on one revision, but the drawer offers no reload action.\n\nEvidence lives in \`web/evidence/i5/\`.\n`,
    )
  await page.getByRole('button', { name: 'Create note', exact: true }).click()
  await noteDialog.waitFor({ state: 'detached', timeout: 15_000 })
  await page.getByText('I5 dogfooding findings', { exact: true }).first().waitFor()
  state.notePath = new URL(page.url()).pathname.replace(/^\/knowledge\//, '')
  await shot('08-knowledge-note')
  steps.push(`Created knowledge note ${state.notePath} through the vault dialog`)

  {
    const { drawer, taskId } = await openTaskFromBoard(TASKS[0].title)
    const withLink = `${TASKS[0].description}\n\nDecision record: [I5 dogfooding findings](${BASE}/knowledge/${state.notePath}).`
    await setDescription(drawer, withLink)
    const descriptionSection = drawer.getByRole('region', { name: 'Description' })
    await descriptionSection.getByRole('tab', { name: 'Preview' }).click()
    await descriptionSection.getByRole('link', { name: 'I5 dogfooding findings' }).waitFor()
    state.taskIds[TASKS[0].title] = taskId
    await shot('09-task-with-note-link')
    await closeTaskDrawer()
  }
  steps.push('Linked the note from the task description and verified the rendered link')

  // ---- Step 8: Home → Inbox triage --------------------------------------
  await page.goto(`${BASE}/`)
  await page.locator('main h1').first().waitFor()
  await shot('10-home-with-project')
  await page.goto(`${BASE}/inbox`)
  await page.getByRole('heading', { level: 1, name: 'Inbox' }).waitFor()
  await page.getByLabel('Capture a task for the Inbox').fill(INBOX_TRIAGE_TITLE)
  await page.getByRole('button', { name: 'Capture', exact: true }).click()
  await page.getByRole('button', { name: INBOX_TRIAGE_TITLE, exact: true }).click()
  const triageDrawer = await dialog()
  await triageDrawer.getByLabel('Task description').waitFor()
  state.taskIds[INBOX_TRIAGE_TITLE] = new URL(page.url()).searchParams.get('task')
  await closeTaskDrawer()
  await page.getByRole('heading', { level: 1, name: 'Inbox' }).waitFor()
  await shot('11-inbox-before-triage')

  // The drawer offers every status, including edges the domain refuses. Record
  // the refusal rather than hiding the friction: this is Inbox -> In progress.
  {
    await page.getByRole('button', { name: INBOX_TRIAGE_TITLE, exact: true }).click()
    const refusedDrawer = await dialog()
    await refusedDrawer.getByLabel('Task description').waitFor()
    await refusedDrawer.getByLabel('Task status').click()
    await page.getByRole('option', { name: 'In progress', exact: true }).click()
    await waitUntil(async () => (await refusedDrawer.getByRole('alert').count()) > 0, {
      label: 'the invalid status transition refusal',
    })
    state.invalidTransition = {
      attempted: 'inbox -> in_progress',
      offeredByControl: true,
      refusal: (await refusedDrawer.getByRole('alert').first().innerText()).trim(),
    }
    await shot('11b-invalid-status-refused')
    await closeTaskDrawer()
  }

  const priorityControl = page.locator(`select[aria-label="Priority for ${INBOX_TRIAGE_TITLE}"]:visible`).first()
  await priorityControl.selectOption({ label: 'High' })
  await waitUntil(async () => (await priorityControl.inputValue()) === 'high', {
    label: 'the inbox priority to save',
  })
  const projectControl = page.locator(`select[aria-label="Project for ${INBOX_TRIAGE_TITLE}"]:visible`).first()
  await projectControl.selectOption({ label: 'Orbit Development' })
  await waitUntil(async () => (await projectControl.count()) === 0, {
    label: 'the triaged task to leave the Inbox',
  })
  await page.getByRole('heading', { level: 1, name: 'Inbox' }).waitFor()
  await shot('12-inbox-after-triage')
  steps.push(`Triaged “${INBOX_TRIAGE_TITLE}” from Home → Inbox into the project, priority High`)

  // ---- Step 9: Planning (capacity, day plan, window, review gate) --------
  await page.goto(`${BASE}/projects/${state.projectId}/planning`)
  await page.getByRole('heading', { name: 'Plan Orbit Development' }).waitFor()
  const capacity = page.getByLabel('Available minutes today')
  await capacity.fill('240')
  await page.getByRole('button', { name: 'Build day plan' }).click()
  await page.getByText('Capacity', { exact: true }).first().waitFor()
  await page.getByText('Missing an estimate').first().waitFor({ timeout: 15_000 })
  await shot('13-planning-day-plan')
  const dayPlanRows = await page.getByText('Missing an estimate').count()

  const date = page.getByLabel('Schedule window date')
  const from = page.getByLabel('Schedule window start time')
  const to = page.getByLabel('Schedule window end time')
  const tomorrow = new Date()
  tomorrow.setDate(tomorrow.getDate() + 1)
  const dateValue = `${tomorrow.getFullYear()}-${String(tomorrow.getMonth() + 1).padStart(2, '0')}-${String(tomorrow.getDate()).padStart(2, '0')}`
  await date.fill(dateValue)
  await from.fill('13:00')
  await to.fill('16:00')
  await waitUntil(
    async () =>
      (await page.getByText('The server placed nothing in this window').count()) > 0 ||
      (await page.locator('[data-schedule-slot]').count()) > 0,
    { label: 'the schedule plan for the chosen window' },
  )
  const review = page.getByRole('button', { name: 'Review this plan' })
  state.planning = {
    capacityMinutes: 240,
    dayPlanRowsMissingEstimate: dayPlanRows,
    window: { date: dateValue, from: '13:00', to: '16:00' },
    proposedSlots: await page.locator('[data-schedule-slot]').count(),
    reviewDisabled: await review.isDisabled(),
    reviewDisabledReason: await page
      .getByText('There is nothing to review until the server places at least one task.')
      .innerText()
      .catch(() => null),
    applied: false,
    blockedBecause: 'No task can be given an estimate from the browser, so the server proposes no slots and the review/apply gate cannot be reached.',
  }
  await shot('14-planning-window')
  steps.push(
    `Authored 240 minutes; the server returned ${dayPlanRows} “missing an estimate” rows and 0 slots; review disabled=${state.planning.reviewDisabled}`,
  )
  log(`planning: ${JSON.stringify(state.planning)}`)

  // ---- Step 10: Task A end to end, opened from the day plan -------------
  await page.getByRole('button', { name: TASKS[0].title, exact: true }).first().click()
  const taskA = await dialog()
  await taskA.getByLabel('Task description').waitFor()
  await shot('15-task-a-opened')

  // Description + criteria (the description already carries the note link).
  const editedDescription = `${TASKS[0].description}\n\nDecision record: [I5 dogfooding findings](${BASE}/knowledge/${state.notePath}).\n\nCompleted during the I5 cycle; evidence is committed under \`web/evidence/i5/\`.`
  await setDescription(taskA, editedDescription)
  for (const criterion of TASKS[0].criteria) {
    await taskA.getByRole('checkbox', { name: `Check ${criterion}` }).click()
    await waitUntil(
      async () => (await taskA.getByRole('checkbox', { name: `Uncheck ${criterion}` }).count()) > 0,
      { label: `criterion “${criterion}” to be checked` },
    )
  }
  await shot('16-task-a-description-criteria')

  // ---- Deliberate second writer: prove the revision fence ---------------
  const before = await api(`/v1/authoritative/tasks/${state.taskIds[TASKS[0].title]}`)
  const secondWriterText = `${before.body?.description ?? ''}\n\nSecond writer: this sentence was written by another session at ${new Date().toISOString()}.`
  const secondWriter = await api('/v1/authoritative/task-commands', {
    method: 'POST',
    body: {
      command_id: `second-writer-${randomUUID()}`,
      expected_revision: (await api('/v1/authoritative/tasks')).body.revision,
      action: {
        type: 'set_details',
        task_id: state.taskIds[TASKS[0].title],
        description: secondWriterText,
        priority: (before.body?.priority ?? null),
        labels: before.body?.labels ?? [],
      },
    },
  })
  if (secondWriter.status !== 200) {
    throw new Error(`second writer failed: ${secondWriter.status} ${secondWriter.text}`)
  }
  // The drawer still holds the pre-second-writer revision. Its next edit must
  // be refused and say so.
  const conflictText = `${editedDescription}\n\nA third sentence typed after the second writer.`
  const textarea = taskA.getByLabel('Task description')
  await textarea.fill(conflictText)
  await textarea.press('Tab')
  await waitUntil(async () => (await taskA.getByRole('alert').count()) > 0, {
    label: 'the stale-revision refusal',
  })
  const conflictAlert = (await taskA.getByRole('alert').first().innerText()).trim()
  const conflictReloadControls = await taskA.getByRole('button', { name: /^(Reload|Refresh|Retry)$/ }).count()
  state.conflict = {
    secondWriterStatus: secondWriter.status,
    refusal: conflictAlert,
    reloadControlsInDrawer: conflictReloadControls,
    recoveredBy: 'full page reload, then reopen the drawer',
  }
  await shot('17-task-a-stale-revision')
  log(`conflict: ${conflictAlert}`)

  // Recover the way a person can: reload the workspace, reopen, continue. The
  // drawer keeps its `?task=` in the address, so reload in place would reopen
  // the stale panel; a fresh board load is the documented recovery.
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  {
    const reopened = await openTaskFromBoard(TASKS[0].title)
    const value = await reopened.drawer.getByLabel('Task description').inputValue()
    if (!value.includes('Second writer:')) {
      throw new Error('the second writer’s accepted text did not survive the reload')
    }
    await closeTaskDrawer()
  }

  // Reopen from the plan, then attach evidence and complete.
  await page.goto(`${BASE}/projects/${state.projectId}/planning?capacity=240`)
  await page.getByText('Missing an estimate').first().waitFor({ timeout: 15_000 })
  await page.getByRole('button', { name: TASKS[0].title, exact: true }).first().click()
  const taskAFinal = await dialog()
  await taskAFinal.getByLabel('Task description').waitFor()
  const attachmentPath = path.join(OUT, 'task-a-evidence.md')
  await writeFile(
    attachmentPath,
    `# I5 task evidence\n\nTask: ${state.taskIds[TASKS[0].title]}\nTitle: ${TASKS[0].title}\nCaptured: ${new Date().toISOString()}\n\nThis file was uploaded through the task drawer's attachment control during the real browser cycle.\n`,
  )
  await taskAFinal.locator('[data-attachment-file-input]').setInputFiles(attachmentPath)
  await waitUntil(
    async () => (await taskAFinal.getByRole('region', { name: 'Attachments' }).getByText('task-a-evidence.md').count()) > 0,
    { label: 'the attachment to be listed', timeout: 25_000 },
  )
  await taskAFinal.getByRole('region', { name: 'Attachments' }).scrollIntoViewIfNeeded()
  await shot('18-task-a-attachment')

  await setStatus(taskAFinal, 'Ready')
  await setStatus(taskAFinal, 'In progress')
  await setStatus(taskAFinal, 'Done')
  await taskAFinal.getByRole('region', { name: 'Activity' }).scrollIntoViewIfNeeded()
  await shot('19-task-a-complete-activity')
  const activity = await taskAFinal.getByRole('region', { name: 'Activity' }).innerText()
  state.taskActivity = activity.split('\n').filter((line) => line.trim().length > 0).slice(0, 20)
  await closeTaskDrawer()
  steps.push(`Completed “${TASKS[0].title}” (${state.taskIds[TASKS[0].title]}) through description, criteria, attachment, Ready → In progress → Done; activity visible`)

  // ---- Step 11: persistent state across a real server restart -----------
  log('stopping the server for the persistence proof')
  await stopServer()
  await startServer()
  await page.reload()
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  const completedCard = page
    .getByRole('listitem')
    .filter({ has: page.getByRole('button', { name: TASKS[0].title, exact: true }) })
  await completedCard.getByText('Done', { exact: true }).waitFor({ timeout: 15_000 })
  const doneColumn = page.getByRole('region', { name: /^Done column, \d+ task/ })
  await doneColumn.scrollIntoViewIfNeeded()
  await sleep(300)
  state.restart = {
    serverRestarted: true,
    sessionSurvived: true,
    completedCardInDoneColumn: true,
    boardTaskCount: await page.locator('[data-board-action]').count(),
  }
  await shot('20-after-restart-board-done')
  steps.push('Restarted the server between tasks; the session, the completed task and its board column survived a reload')

  // ---- Step 12: Task B completed from the board with the keyboard --------
  const taskBTitle = TASKS[TASKS.length - 1].title
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  const cardB = page.getByRole('button', { name: taskBTitle, exact: true })
  await cardB.waitFor()
  await cardB.focus()
  await shot('21-board-task-b-before')
  // Ready -> In progress is the domain's allowed edge; the board offers every
  // status and lets the server refuse the ones the lifecycle does not allow.
  const firstArrows = await boardKeyboardMove(taskBTitle, 'In progress')
  await page.getByRole('status').first().waitFor({ timeout: 5_000 }).catch(() => undefined)
  const secondArrows = await boardKeyboardMove(taskBTitle, 'Done')
  state.boardMove = {
    taskId: state.taskIds[taskBTitle],
    title: taskBTitle,
    moves: ['In progress', 'Done'],
    arrowPresses: firstArrows + secondArrows,
    usedMouse: false,
  }
  await page.getByRole('region', { name: /^Done column, \d+ task/ }).scrollIntoViewIfNeeded()
  await sleep(300)
  await shot('22-board-task-b-done')
  steps.push(
    `Completed “${taskBTitle}” from the board with two keyboard moves (Ready → In progress → Done, m → arrow → Enter, ${firstArrows + secondArrows} ArrowDown presses, no pointer used for the move)`,
  )

  // ---- Step 13: milestone progress + honest upgrade check ----------------
  await page.goto(`${BASE}/projects/${state.projectId}/milestones`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.getByText(MILESTONE_M31, { exact: true }).first().waitFor()
  await page.getByText(MILESTONE_REF, { exact: true }).first().waitFor()
  await shot('23-milestone-progress')
  const milestonePage = await page.locator('main').innerText()
  state.milestones = {
    names: [MILESTONE_M31, MILESTONE_REF],
    progressCopy: milestonePage.split('\n').filter((line) => /task|done|progress|%|\d+\/\d+/i.test(line)).slice(0, 12),
  }

  await page.goto(`${BASE}/projects/${state.projectId}/settings`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.getByText('This project is on the newest published version of Development. No upgrade is pending.').waitFor()
  const upgradeOffered = await page.locator('[data-upgrade-available]').count()
  state.upgrade = { offered: upgradeOffered > 0, copy: 'This project is on the newest published version of Development. No upgrade is pending.' }
  await shot('24-settings-no-upgrade')
  steps.push('Reviewed milestone progress; settings honestly reported no newer published workflow version')

  // ---- Step 14: agent surface and final home -----------------------------
  await page.goto(`${BASE}/agents`)
  await page.getByRole('heading', { level: 1, name: 'Agents' }).waitFor()
  await shot('25-agents-no-jobs')
  const agentsText = await page.locator('main').innerText()

  await page.goto(`${BASE}/`)
  await page.locator('main h1').first().waitFor()
  await shot('26-home-final')

  state.agentJobs = {
    created: false,
    reason: 'The web app exposes no launch or create control for agent jobs; the overview was empty and says so.',
    overviewCopy: agentsText.split('\n').filter((line) => line.trim().length > 0).slice(0, 10),
  }

  // ---- Close and back up the database -----------------------------------
  await context.close()
  await browser.close()
  browser = null
  await stopServer()

  const backup = path.join(OUT, 'orbit-i5-db.sqlite3')
  await copyFile(DB, backup)
  const backups = [backup]
  // SQLite rebuilds the transient -shm index; only the WAL carries committed data.
  for (const suffix of ['-wal']) {
    try {
      await access(`${DB}${suffix}`)
      await copyFile(`${DB}${suffix}`, path.join(OUT, `orbit-i5-db.sqlite3${suffix}`))
      backups.push(path.join(OUT, `orbit-i5-db.sqlite3${suffix}`))
    } catch {
      // no WAL/SHM after a clean stop
    }
  }

  // Final state read from the stopped database's sibling vault is not possible
  // through HTTP; the run recorded the ids as the UI minted them.
  const summary = {
    ...state,
    finishedAt: new Date().toISOString(),
    dbPath: DB,
    backups,
    evidenceDir: OUT,
    httpErrors,
    steps,
    environment: {
      port: PORT,
      device: DEVICE_ID,
      dist: DIST,
      vault: path.join(RUN_DIR, 'vault', 'db.knowledge'),
      serverBinary: path.join(REPO, 'target', 'debug', 'orbit-server'),
    },
  }
  await writeFile(path.join(OUT, 'run.json'), `${JSON.stringify(summary, null, 2)}\n`)
  log(`wrote ${path.join(OUT, 'run.json')}`)
  log(`database backup ${backup}`)
  log('I5 cycle finished')
  return summary
}

try {
  const summary = await run()
  process.stdout.write(`\n${JSON.stringify({ ok: true, project: summary.projectId, tasks: Object.keys(summary.taskIds).length, conflict: summary.conflict, planning: summary.planning }, null, 2)}\n`)
} catch (error) {
  process.stderr.write(`\n[i5] FAILED: ${error?.stack ?? error}\n`)
  try {
    if (page) {
      await page.screenshot({ path: path.join(OUT, 'failure.png'), fullPage: true }).catch(() => undefined)
      await writeFile(path.join(OUT, 'failure-page.html'), await page.content()).catch(() => undefined)
      await writeFile(
        path.join(OUT, 'failure-state.json'),
        JSON.stringify({ state, httpErrors, steps, url: page.url() }, null, 2),
      )
    }
  } catch {
    // best effort
  }
  if (browser) await browser.close().catch(() => undefined)
  await stopServer().catch(() => undefined)
  process.exitCode = 1
}
