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
 * `web/dist`. The only direct HTTP calls are setup (enrol two devices, install
 * the bundled Development workflow, open a browser session), deliberate refusal
 * probes, and reads used to assert what the UI showed. Projects, tasks,
 * milestones, work items, notes, attachments and every task change in the cycle
 * are typed through the real browser UI. The second device has its own browser
 * context and writes the conflict through its own drawer, so “Another device”
 * attribution is exercised end-to-end.
 *
 * The script refuses to start on a dirty git tree, records the commit, the
 * `web/dist` content hash, the server binary hash and the project's workflow
 * version, and writes both `run.json` and a `README.md` generated from it.
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
import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { copyFile, mkdir, readFile, readdir, rm, writeFile, access } from 'node:fs/promises'
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
const SECOND_DEVICE_ID = 'orbit-development-2'
const SECOND_DEVICE_TOKEN = randomBytes(32).toString('hex')
const ADMIN_KEY = randomBytes(32).toString('hex')
const SERVER_BINARY = path.join(REPO, 'target', 'debug', 'orbit-server')
const STATUS_BY_LABEL = {
  Inbox: 'inbox',
  Planned: 'planned',
  Ready: 'ready',
  'In progress': 'in_progress',
  Waiting: 'waiting',
  Blocked: 'blocked',
  'In review': 'review',
  Deferred: 'deferred',
  Done: 'done',
  Cancelled: 'cancelled',
}

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

function sha256(value) {
  return createHash('sha256').update(value).digest('hex')
}

async function hashFile(file) {
  const bytes = await readFile(file)
  return { path: path.relative(REPO, file), sha256: sha256(bytes), bytes: bytes.length }
}

/** A content hash over the built SPA, independent of mtimes and inode order. */
async function hashDirectory(directory) {
  const files = []
  const walk = async (current, prefix = '') => {
    const entries = await readdir(current, { withFileTypes: true })
    entries.sort((left, right) => left.name.localeCompare(right.name))
    for (const entry of entries) {
      const relative = prefix ? `${prefix}/${entry.name}` : entry.name
      const absolute = path.join(current, entry.name)
      if (entry.isDirectory()) await walk(absolute, relative)
      else if (entry.isFile()) {
        const bytes = await readFile(absolute)
        files.push({ path: relative, sha256: sha256(bytes), bytes: bytes.length })
      }
    }
  }
  await walk(directory)
  const content = files.map((file) => `${file.path}\u0000${file.sha256}`).join('\n')
  return {
    path: path.relative(REPO, directory),
    sha256: sha256(content),
    files,
    fileCount: files.length,
    totalBytes: files.reduce((total, file) => total + file.bytes, 0),
  }
}

function gitOutput(args) {
  return execFileSync('git', args, { cwd: REPO }).toString()
}

/**
 * The evidence is only meaningful if it names the tree it was captured from.
 * A dirty tree means the product code may not be the commit in `HEAD`, so the
 * run refuses rather than producing another hybrid capture.
 */
function requireCleanTree() {
  const commit = gitOutput(['rev-parse', 'HEAD']).trim()
  const status = gitOutput(['status', '--porcelain'])
  if (status.trim().length > 0) {
    throw new Error(
      `refusing to capture evidence from a dirty tree at ${commit}. Commit or restore these paths first:\n${status.trim()}`,
    )
  }
  return commit
}

async function resetEvidence() {
  await mkdir(OUT, { recursive: true })
  for (const entry of await readdir(OUT, { withFileTypes: true })) {
    // A deliberately kept pre-fix set may live in `before/`; everything else is
    // this run's output and must not survive beside it.
    if (entry.name === 'before') continue
    await rm(path.join(OUT, entry.name), { recursive: true, force: true })
  }
}

/** The exact invocation, relative to the directory it was run from. */
function invokedCommand() {
  const script = path.relative(process.cwd(), fileURLToPath(import.meta.url))
  return {
    cwd: process.cwd(),
    invocation: `node ${script.split(path.sep).join('/')}`,
    argv: process.argv,
  }
}

async function api(pathname, { method = 'GET', body, admin = false, device } = {}) {
  const headers = { 'content-type': 'application/json' }
  if (admin) headers.authorization = `Bearer ${ADMIN_KEY}`
  else {
    const credentials = device ?? { id: DEVICE_ID, token: DEVICE_TOKEN }
    headers.authorization = `Bearer ${credentials.token}`
    headers['x-orbit-device-id'] = credentials.id
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

async function readTask(taskId, device) {
  const response = await api(`/v1/authoritative/tasks/${encodeURIComponent(taskId)}`, { device })
  if (response.status !== 200) {
    throw new Error(`could not read task ${taskId}: ${response.status} ${response.text}`)
  }
  return response.body
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
  const child = spawn(SERVER_BINARY, [], {
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
        return child.pid
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
let secondContext = null
let secondPage = null
const httpErrorRecords = []
const steps = []

/**
 * Every 4xx/5xx from the API is a recorded fact, not white noise. The page
 * listener keeps the body so the final assertion can name the refusal instead
 * of only its status code.
 */
function trackHttpErrors(target, source) {
  target.on('response', (response) => {
    if (response.status() < 400 || !response.url().includes('/v1/')) return
    const record = {
      status: response.status(),
      method: response.request().method(),
      url: response.url(),
      source,
      body: '',
      textPromise: response.text().catch(() => ''),
    }
    httpErrorRecords.push(record)
  })
}

function recordProbeFailure({ status, method, url, body, source }) {
  httpErrorRecords.push({
    status,
    method,
    url,
    source,
    body,
    textPromise: Promise.resolve(body),
  })
}

function shotPath(name) {
  return path.join(OUT, `${name}.png`)
}

async function shotOn(target, name) {
  await target.evaluate(() => document.fonts.ready).catch(() => undefined)
  await sleep(300)
  await target.screenshot({ path: shotPath(name) })
  state.captures.push(name)
  log(`capture ${name}.png`)
}

async function shot(name) {
  await shotOn(page, name)
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
  return openTaskOnPage(page, title)
}

async function openTaskOnPage(target, title) {
  const card = target.getByRole('button', { name: title, exact: true })
  await card.waitFor({ timeout: 15_000 })
  await card.click()
  const drawer = target.getByRole('dialog')
  await drawer.waitFor()
  await drawer.getByLabel('Task description').waitFor({ timeout: 15_000 })
  const taskId = new URL(target.url()).searchParams.get('task')
  if (!taskId) throw new Error('the task drawer did not put the task id in the URL')
  return { drawer, taskId }
}

async function closeTaskDrawerOn(target) {
  const drawer = target.getByRole('dialog')
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

async function closeTaskDrawer() {
  return closeTaskDrawerOn(page)
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

/**
 * Change status through the drawer, then assert the server's stored value.
 * The move is recorded only after the read confirms it, so `run.json` never
 * claims a transition the server did not accept.
 */
async function setStatus(drawer, statusLabel, taskId, surface) {
  const expected = STATUS_BY_LABEL[statusLabel]
  if (!expected) throw new Error(`unknown status label “${statusLabel}”`)
  const before = (await readTask(taskId)).status
  await chooseOption(drawer.getByLabel('Task status'), statusLabel, 'status')
  await waitUntil(async () => (await readTask(taskId)).status === expected, {
    label: `task ${taskId} to be stored as ${expected}`,
    timeout: 15_000,
  })
  await checkNoAlert(drawer)
  if (before !== expected) {
    state.moves.push({ taskId, from: before, to: expected, surface })
  }
  return { from: before, to: expected }
}

/** Author an estimate and deadline through the drawer's Planning group. */
async function setPlanning(drawer, { estimate, deadline }, taskId) {
  const input = drawer.getByLabel('Task estimate in minutes')
  await input.fill(String(estimate))
  await input.press('Tab')
  await waitUntil(async () => Number(await input.inputValue()) === estimate, {
    label: `the estimate draft to read back ${estimate}`,
    timeout: 10_000,
  })
  // The estimate and deadline are two commands on one mutation instance. Wait
  // for the server to store the first before sending the second, or the second
  // fences on the pre-estimate revision and is refused as stale by our own
  // script's speed rather than by the deliberate conflict later.
  await waitUntil(async () => (await readTask(taskId)).estimated_minutes === estimate, {
    label: `the server to store the estimate ${estimate}`,
    timeout: 15_000,
  })
  await waitUntil(async () => !/Saving…/.test(await drawer.locator('footer').innerText()), {
    label: 'the estimate command to settle',
  })
  await checkNoAlert(drawer)
  const deadlineInput = drawer.getByLabel('Task deadline')
  await deadlineInput.fill(deadline)
  // A `datetime-local` input has internal segments; Tab moves between them, so
  // blur explicitly to commit the whole value.
  await deadlineInput.blur()
  const expectedDeadlineAtMs = new Date(deadline).getTime()
  await waitUntil(async () => (await readTask(taskId)).deadline_at_ms === expectedDeadlineAtMs, {
    label: `the server to store the deadline ${deadline}`,
    timeout: 15_000,
  })
  await waitUntil(async () => (await deadlineInput.inputValue()) === deadline, {
    label: `the deadline to read back ${deadline}`,
    timeout: 10_000,
  })
  await checkNoAlert(drawer)
  return readTask(taskId)
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
      return /Description saved\.|Edits are sent when you leave a field\./.test(footer) &&
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
 * The first task is sized and given a deadline through the drawer's Planning
 * group during the enrichment pass, so the planning gate has real input.
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
      'Run the full capacity → day plan → window → review → approve → apply path in a browser and assert that the accepted blocks survive reload.\n\nThis run records the applied receipt ids and re-reads the schedule after a reload.',
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
      'The guide still describes the I2 surface. Add work items, knowledge, attachments, agents, the planning gate and the honest gaps (no project rename, no workflow editing).',
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
  command: null,
  identity: {},
  project: {},
  taskIds: {},
  moves: [],
  triage: null,
  milestoneIds: {},
  milestones: {},
  workItem: null,
  note: null,
  attachment: null,
  planning: {},
  conflict: {},
  invalidTransition: null,
  boardMove: {},
  restart: {},
  activity: {},
  captures: [],
}

async function run() {
  if (!(await portIsFree())) {
    throw new Error(`port ${PORT} is already in use; stop the other server first`)
  }

  state.command = invokedCommand()

  // Stale failure diagnostics are this script's own output, not source. Clear
  // them before the clean-tree check so a failed attempt can be re-run.
  await mkdir(OUT, { recursive: true })
  for (const stale of ['failure.png', 'failure-page.html', 'failure-state.json']) {
    await rm(path.join(OUT, stale), { force: true })
  }

  const commit = requireCleanTree()
  state.identity = {
    commit,
    tree: 'clean',
    dist: await hashDirectory(DIST),
    serverBinary: await hashFile(SERVER_BINARY),
    workflow: null,
  }
  log(`capturing ${commit} (clean tree); dist ${state.identity.dist.sha256.slice(0, 12)}`)

  await resetEvidence()
  await rm(RUN_DIR, { recursive: true, force: true })
  await mkdir(path.dirname(DB), { recursive: true })

  await startServer()

  // Setup: two enrolled devices. The second one writes the deliberate conflict
  // through its own browser session, so the activity origin is genuinely
  // another device rather than the same credential with a new revision.
  for (const device of [
    { device_id: DEVICE_ID, token: DEVICE_TOKEN },
    { device_id: SECOND_DEVICE_ID, token: SECOND_DEVICE_TOKEN },
  ]) {
    const enrolled = await api('/v1/devices', {
      method: 'POST',
      admin: true,
      body: device,
    })
    if (enrolled.status !== 201) {
      throw new Error(`device enrolment failed for ${device.device_id}: ${enrolled.status} ${enrolled.text}`)
    }
    log(`enrolled ${device.device_id}`)
  }

  browser = await chromium.launch({ headless: true })
  context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    colorScheme: 'light',
    baseURL: BASE,
  })
  page = await context.newPage()
  await page.addInitScript(() => {
    window.localStorage.setItem('orbit.theme', 'light')
    window.__orbitPointerDowns = 0
    window.addEventListener('pointerdown', () => {
      window.__orbitPointerDowns = (window.__orbitPointerDowns ?? 0) + 1
    }, true)
  })
  trackHttpErrors(page, 'primary browser')

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
  const createProject = await dialog()
  await createProject.waitFor()
  // A clean server publishes no non-default workflow. The dialog installs the
  // bundled one itself (I5-02) and refreshes its own list — no page reload and
  // no setup API call.
  await createProject.getByText('Bundled workflows').waitFor()
  await shot('01b-new-project-bundled')
  await createProject.getByRole('button', { name: 'Install Development' }).click()
  await page.getByRole('radio', { name: /Development/ }).waitFor({ timeout: 15_000 })
  steps.push('A fresh server published no Development workflow; the New project dialog installed the bundled Development workflow and offered it without a reload.')
  await page.getByRole('radio', { name: /Development/ }).check()
  await createProject.getByLabel('Project name').fill('Orbit Development')
  await shot('02-new-project-development')
  await page.getByRole('button', { name: 'Create project' }).click()
  await page.waitForURL(/\/projects\/[^/]+$/, { timeout: 20_000 })
  state.projectId = new URL(page.url()).pathname.split('/').filter(Boolean)[1]
  await page.getByRole('heading', { level: 1, name: 'Orbit Development' }).waitFor()
  await page.getByText('Development v3', { exact: true }).first().waitFor()
  const projects = await api('/v1/authoritative/projects')
  const projectRecord = (projects.body?.projects ?? []).find((entry) => entry.id === state.projectId)
  const binding = await api(`/v1/authoritative/blueprints/binding?project_id=${state.projectId}`)
  if (binding.status !== 200 || !binding.body) {
    throw new Error(`the project binding could not be read: ${binding.status} ${binding.text}`)
  }
  state.project = {
    id: state.projectId,
    name: projectRecord?.name ?? 'Orbit Development',
    status: projectRecord?.status ?? null,
    workflow: { id: binding.body.blueprint_id, version: binding.body.version },
  }
  state.identity.workflow = {
    label: 'Development v3',
    id: binding.body.blueprint_id,
    version: binding.body.version,
  }
  if (binding.body.version !== 3) {
    throw new Error(`expected the seeded project to bind Development v3, got v${binding.body.version}`)
  }
  await shot('03-project-overview')
  log(`project ${state.projectId} (${state.project.name}) on ${binding.body.blueprint_id} v${binding.body.version}`)
  steps.push(`Created project Orbit Development (${state.projectId}) with the Development v${binding.body.version} workflow through the dialog`)

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
      for (const status of path) await setStatus(drawer, status, taskId, 'task drawer (setup)')
    }
    if (task.description) await setDescription(drawer, task.description)
    if (task.criteria) for (const text of task.criteria) await addCriterion(drawer, text)
    if (task.dependsOn) await addPrerequisite(drawer, task.dependsOn)
    if (task.milestone) await setMilestone(drawer, task.milestone)
    await closeTaskDrawer()
  }
  log('enriched priorities, labels, statuses, descriptions, criteria, dependencies')
  steps.push('Set priorities, labels, initial statuses, Markdown descriptions, acceptance criteria and dependencies in the drawer')

  // Size the task the planning stage will schedule (I5-01). The deadline is
  // far enough out that the server does not judge it past.
  {
    const { drawer, taskId } = await openTaskFromBoard(TASKS[0].title)
    const stored = await setPlanning(drawer, { estimate: 60, deadline: '2030-01-15T12:30' }, taskId)
    if (stored.estimated_minutes !== 60) throw new Error('the drawer did not store the estimate')
    await closeTaskDrawer()
    steps.push(`Sized “${TASKS[0].title}” at 60 minutes with a deadline through the drawer's Planning group`)
  }

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
  // I5-03: a Feature is named by its title, not by its priority.
  await itemDialog.getByLabel('Title').fill('Real feature titles in the work-item list')
  await page.getByRole('button', { name: 'Create work item', exact: true }).click()
  await itemDialog.waitFor({ state: 'detached', timeout: 15_000 })
  const items = await api(`/v1/authoritative/blueprints/items?project_id=${state.projectId}`)
  const workItem =
    (items.body ?? []).find((entry) => entry.properties?.title === 'Real feature titles in the work-item list') ??
    (items.body ?? [])[0] ??
    null
  if (!workItem) throw new Error('the created work item could not be read back')
  state.workItem = {
    id: workItem.id,
    kind: workItem.kind,
    state: workItem.state,
    properties: workItem.properties,
    declaredFields: featureFields,
  }
  await page.locator(`[data-work-item-key="${workItem.id}"]`).waitFor({ timeout: 15_000 })
  await shot('07-work-item-created')
  steps.push(`Created one ${workItem.kind} work item (${workItem.id}) titled “${String(workItem.properties?.title ?? '')}”`)

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
      `This note is filed from the real browser cycle for task \`${taskAId}\` (**${TASKS[0].title}**).\n\n## Headline\n\n- The drawer authors an estimate and a deadline; the planning review/apply gate was reached and the applied block survived a reload.\n- The New project dialog installs the bundled Development workflow without a reload.\n- A Feature is named by its own title field.\n- A stale revision offers a reload in the drawer.\n\nEvidence lives in \`web/evidence/i5/\`.\n`,
    )
  await page.getByRole('button', { name: 'Create note', exact: true }).click()
  await noteDialog.waitFor({ state: 'detached', timeout: 15_000 })
  await page.getByText('I5 dogfooding findings', { exact: true }).first().waitFor()
  state.notePath = new URL(page.url()).pathname.replace(/^\/knowledge\//, '')
  const inventory = await api('/v1/authoritative/markdown-inventory')
  const note = (inventory.body ?? []).find((entry) => entry.path === state.notePath) ?? null
  state.note = {
    path: state.notePath,
    id: note?.id ?? null,
    title: note?.title ?? 'I5 dogfooding findings',
    linkedFromTaskId: taskAId,
  }
  if (!state.note.id) throw new Error(`the created note ${state.notePath} was not in the markdown inventory`)
  await shot('08-knowledge-note')
  steps.push(`Created knowledge note ${state.notePath} (${state.note.id}) through the vault dialog`)

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

  // The drawer offers only the domain's edges. Inbox cannot go straight to In
  // progress, so the control must not offer it. The 400 allowlist needs a real
  // invalid-transition refusal too, so the same task is asked directly for an
  // edge the domain rejects; that response is recorded as a deliberate probe.
  const triageTaskId = state.taskIds[INBOX_TRIAGE_TITLE]
  const triageBefore = await readTask(triageTaskId)
  {
    await page.getByRole('button', { name: INBOX_TRIAGE_TITLE, exact: true }).click()
    const statusDrawer = await dialog()
    await statusDrawer.getByLabel('Task description').waitFor()
    await statusDrawer.getByLabel('Task status').click()
    await waitUntil(async () => (await page.getByRole('option').count()) > 0, {
      label: 'the status edge options',
    })
    const offered = (await page.getByRole('option').allInnerTexts()).map((text) => text.trim())
    await page.keyboard.press('Escape')
    const offeredByControl = offered.includes('In progress')
    if (offeredByControl) {
      throw new Error('the status control offered In progress from Inbox even though the domain refuses it')
    }
    const revision = (await api('/v1/authoritative/tasks')).body?.revision
    const invalid = await api('/v1/authoritative/task-commands', {
      method: 'POST',
      body: {
        command_id: `invalid-edge-${randomUUID()}`,
        expected_revision: revision,
        action: { type: 'change_status', task_id: triageTaskId, status: 'done' },
      },
    })
    if (invalid.status !== 400 || !/invalid task status transition: inbox -> done/.test(invalid.text)) {
      throw new Error(`the deliberate invalid transition was not refused as expected: ${invalid.status} ${invalid.text}`)
    }
    recordProbeFailure({
      status: invalid.status,
      method: 'POST',
      url: `${BASE}/v1/authoritative/task-commands`,
      body: invalid.text,
      source: 'deliberate invalid-transition probe',
    })
    state.invalidTransition = {
      attempted: 'inbox -> done',
      offeredByControl,
      offered,
      status: invalid.status,
      refusal: invalid.body?.error ?? invalid.text,
    }
    await shot('11b-status-edges')
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
  const triageAfter = await readTask(triageTaskId)
  state.triage = {
    taskId: triageTaskId,
    before: {
      status: triageBefore.status,
      projectId: triageBefore.project_id ?? null,
      priority: triageBefore.priority ?? null,
    },
    after: {
      status: triageAfter.status,
      projectId: triageAfter.project_id ?? null,
      priority: triageAfter.priority ?? null,
    },
    surface: 'Inbox inline project and priority controls',
  }
  if (state.triage.after.projectId !== state.projectId || state.triage.after.priority !== 'high') {
    throw new Error(`the Inbox triage did not store the project and priority: ${JSON.stringify(state.triage)}`)
  }
  await shot('12-inbox-after-triage')
  steps.push(`Triaged “${INBOX_TRIAGE_TITLE}” from Home → Inbox into the project, priority High`)

  // ---- Step 9: Planning (capacity, day plan, window, review gate) --------
  await page.goto(`${BASE}/projects/${state.projectId}/planning`)
  await page.getByRole('heading', { name: 'Plan Orbit Development' }).waitFor()
  const capacity = page.getByLabel('Available minutes today')
  await capacity.fill('240')
  await page.getByRole('button', { name: 'Build day plan' }).click()
  await page.getByText('Capacity', { exact: true }).first().waitFor()
  const plannedRow = page.getByRole('button', { name: TASKS[0].title, exact: true }).first()
  await plannedRow.waitFor({ timeout: 15_000 })
  await shot('13-planning-day-plan')
  const dayPlanRows = await page.getByText('Selected', { exact: true }).count()

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
    async () => (await page.locator('[data-schedule-slot]').count()) > 0,
    { label: 'the schedule plan for the chosen window' },
  )
  const review = page.getByRole('button', { name: 'Review this plan' })
  // Read the proposal before review: the run may not claim slots it only saw
  // after the applied plan was rendered again on reload.
  const proposedSlots = await page.locator('[data-schedule-slot]').count()
  const reviewWasDisabled = await review.isDisabled()
  if (proposedSlots < 1) throw new Error('the schedule plan proposed no slots before review')
  if (reviewWasDisabled) throw new Error('Review was disabled even though the plan proposed slots')
  await review.click()
  await page.locator('[data-plan-review]').waitFor({ timeout: 15_000 })
  const reviewFingerprint = (await page.locator('[data-review-fingerprint]').innerText()).trim()
  const reviewCommand = (await page.locator('[data-review-command]').innerText()).trim()
  await page.getByRole('checkbox', { name: 'Approve the reviewed schedule' }).check()
  await page.getByRole('button', { name: 'Apply approved plan' }).click()
  const receipt = page.locator('[data-plan-receipt]')
  await receipt.waitFor({ timeout: 15_000 })
  const receiptVisible = await receipt.isVisible()
  const receiptBlocks = await receipt
    .locator('[data-receipt-block]')
    .evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-receipt-block') ?? ''))
  const beforeReload = await api('/v1/authoritative/schedule')
  const blocksBeforeReload = beforeReload.body?.blocks ?? []
  const applied =
    receiptVisible &&
    receiptBlocks.length > 0 &&
    blocksBeforeReload.some(
      (block) => block.title === TASKS[0].title && receiptBlocks.includes(block.id),
    )
  if (!applied) {
    throw new Error(`the apply receipt did not match a stored schedule block: ${JSON.stringify({ receiptBlocks, blocksBeforeReload })}`)
  }
  await shot('14-planning-applied')

  // A reload proves the block is stored, not held in React state.
  await page.reload()
  await page.getByRole('heading', { name: 'Plan Orbit Development' }).waitFor()
  const afterReload = await api('/v1/authoritative/schedule')
  const blocksAfterReload = afterReload.body?.blocks ?? []
  const survivedReload = blocksAfterReload.some(
    (block) => block.title === TASKS[0].title && receiptBlocks.includes(block.id),
  )
  state.planning = {
    capacityMinutes: 240,
    dayPlanRowsSelected: dayPlanRows,
    window: { date: dateValue, from: '13:00', to: '16:00' },
    proposedSlotsBeforeReview: proposedSlots,
    reviewDisabledBeforeReview: reviewWasDisabled,
    reviewFingerprint,
    reviewCommandId: reviewCommand,
    receiptBlocks,
    blocksBeforeReload: blocksBeforeReload.length,
    blocksAfterReload: blocksAfterReload.length,
    applied,
    survivedReload,
    blockedBecause: applied ? null : 'the apply receipt did not match a stored block',
  }
  if (!survivedReload) {
    throw new Error('the applied schedule block did not survive the reload')
  }
  await shot('14b-planning-after-reload')
  steps.push(
    `Sized task selected in the day plan (${proposedSlots} proposed slot before review); reviewed, approved and applied ${blocksAfterReload.length} block(s); the applied block survived a reload`,
  )
  log(`planning: ${JSON.stringify(state.planning)}`)

  // ---- Step 10: Task A end to end, opened from the board ----------------
  const reopenedA = await openTaskFromBoard(TASKS[0].title)
  const taskA = reopenedA.drawer
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
  // The second writer is a second enrolled device with its own browser
  // context, session and drawer. Its accepted description edit advances the
  // revision while the primary drawer still holds the old one; the primary's
  // next edit must be refused, offer recovery, and must not overwrite it.
  const taskAIdForConflict = state.taskIds[TASKS[0].title]
  const beforeSecondWrite = await readTask(taskAIdForConflict)
  const secondWriterMarker = `Second device note: this sentence was typed in another browser session at ${new Date().toISOString()}.`
  const secondWriterText = `${beforeSecondWrite.description}\n\n${secondWriterMarker}`

  secondContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    colorScheme: 'light',
    baseURL: BASE,
  })
  secondPage = await secondContext.newPage()
  await secondPage.addInitScript(() => window.localStorage.setItem('orbit.theme', 'light'))
  trackHttpErrors(secondPage, 'second-device browser')
  await secondPage.goto(`${BASE}/login`)
  await secondPage.getByRole('heading', { name: 'Sign in to Orbit' }).waitFor()
  await secondPage.getByLabel('Device ID').fill(SECOND_DEVICE_ID)
  await secondPage.getByLabel('Device token').fill(SECOND_DEVICE_TOKEN)
  await secondPage.getByRole('button', { name: 'Sign in', exact: true }).click()
  await secondPage.waitForURL((url) => !url.pathname.startsWith('/login'), { timeout: 15_000 })
  await secondPage.goto(`${BASE}/projects/${state.projectId}/board`)
  await secondPage.getByRole('navigation', { name: 'Project sections' }).waitFor()
  const secondOpened = await openTaskOnPage(secondPage, TASKS[0].title)
  const secondArea = secondOpened.drawer.getByLabel('Task description')
  await secondArea.fill(secondWriterText)
  await secondArea.press('Tab')
  await waitUntil(
    async () => (await readTask(taskAIdForConflict, { id: SECOND_DEVICE_ID, token: SECOND_DEVICE_TOKEN })).description.includes(secondWriterMarker),
    { label: 'the second device description to be stored', timeout: 20_000 },
  )
  const afterSecondWrite = await readTask(taskAIdForConflict)
  if (!afterSecondWrite.description.includes(secondWriterMarker)) {
    throw new Error('the second device write was accepted in its own drawer but is not stored')
  }
  await shotOn(secondPage, '17a-second-device-write')
  log(`second device ${SECOND_DEVICE_ID} stored “${secondWriterMarker}”`)

  // The primary drawer still holds the pre-second-writer revision. Its next
  // edit must be refused and say so exactly once, beside the field.
  const conflictText = `${editedDescription}\n\nA third sentence typed after the second writer.`
  const textarea = taskA.getByLabel('Task description')
  await textarea.fill(conflictText)
  await textarea.press('Tab')
  await waitUntil(async () => (await taskA.getByRole('alert').count()) > 0, {
    label: 'the stale-revision refusal',
  })
  const alertsInDrawer = await taskA.getByRole('alert').count()
  if (alertsInDrawer !== 1) {
    throw new Error(`the stale-revision refusal must render exactly one in-drawer alert, got ${alertsInDrawer}`)
  }
  const conflictAlert = (await taskA.getByRole('alert').innerText()).trim()
  if (!/conflicted with a newer version/i.test(conflictAlert)) {
    throw new Error(`the refusal did not name the stale revision: ${conflictAlert}`)
  }
  const reloadControl = taskA.getByRole('button', { name: 'Reload this task and try again' })
  const conflictReloadControls = await reloadControl.count()
  if (conflictReloadControls !== 1) {
    throw new Error(
      `the stale-revision refusal must offer exactly one drawer reload action, got ${conflictReloadControls}`,
    )
  }
  state.conflict = {
    secondDeviceId: SECOND_DEVICE_ID,
    secondWrite: {
      taskId: taskAIdForConflict,
      surface: 'second browser context → board → task drawer',
      marker: secondWriterMarker,
      storedDescriptionIncludesMarker: afterSecondWrite.description.includes(secondWriterMarker),
    },
    secondWriterAccepted: afterSecondWrite.description.includes(secondWriterMarker),
    refusal: conflictAlert,
    alertsInDrawer,
    reloadControlsInDrawer: conflictReloadControls,
    recoveredBy: 'drawer reload action',
    recovered: false,
    noSilentOverwrite: null,
  }
  await shot('17-task-a-stale-revision')
  log(`conflict: ${conflictAlert}`)

  // Recover in place: the drawer reloads the task and snapshot, then the stored
  // text is the second device's, not the refused draft.
  await reloadControl.click()
  await waitUntil(
    async () => (await taskA.getByLabel('Task description').inputValue()).includes(secondWriterMarker),
    { label: 'the second writer text after the drawer reload', timeout: 15_000 },
  )
  const afterRecovery = await readTask(taskAIdForConflict)
  state.conflict.recovered = true
  state.conflict.noSilentOverwrite =
    afterRecovery.description.includes(secondWriterMarker) && !afterRecovery.description.includes('A third sentence typed after the second writer.')
  if (!state.conflict.noSilentOverwrite) {
    throw new Error('the refused primary draft overwrote the second device’s accepted description')
  }
  await closeTaskDrawerOn(secondPage)
  await secondContext.close()
  secondContext = null
  secondPage = null

  // Recover the way a person can: reload the workspace, reopen, continue. The
  // drawer keeps its `?task=` in the address, so reload in place would reopen
  // the stale panel; a fresh board load is the documented recovery.
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  {
    const reopened = await openTaskFromBoard(TASKS[0].title)
    const value = await reopened.drawer.getByLabel('Task description').inputValue()
    if (!value.includes(secondWriterMarker)) {
      throw new Error('the second writer’s accepted text did not survive the reload')
    }
    await closeTaskDrawer()
  }

  // Reopen from the board, then attach evidence and complete.
  const reopenedFinal = await openTaskFromBoard(TASKS[0].title)
  const taskAFinal = reopenedFinal.drawer
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
  const attachmentList = await api(
    `/v1/authoritative/attachments?owner_kind=task&owner_id=${encodeURIComponent(state.taskIds[TASKS[0].title])}`,
  )
  const storedAttachment =
    (attachmentList.body?.attachments ?? []).find((entry) => entry.filename === 'task-a-evidence.md') ?? null
  if (!storedAttachment) throw new Error('the uploaded attachment was not readable from the API')
  state.attachment = {
    id: storedAttachment.id,
    filename: storedAttachment.filename,
    contentType: storedAttachment.content_type,
    sizeBytes: storedAttachment.size_bytes,
    checksum: storedAttachment.checksum,
    ownerTaskId: storedAttachment.owner_id,
  }
  await taskAFinal.getByRole('region', { name: 'Attachments' }).scrollIntoViewIfNeeded()
  await shot('18-task-a-attachment')

  await setStatus(taskAFinal, 'Ready', taskAId, 'task drawer (completion)')
  await setStatus(taskAFinal, 'In progress', taskAId, 'task drawer (completion)')
  await setStatus(taskAFinal, 'Done', taskAId, 'task drawer (completion)')
  await taskAFinal.getByRole('region', { name: 'Activity' }).scrollIntoViewIfNeeded()
  await shot('19-task-a-complete-activity')
  const activity = await taskAFinal.getByRole('region', { name: 'Activity' }).innerText()
  state.taskActivity = activity.split('\n').filter((line) => line.trim().length > 0).slice(0, 20)
  state.activityOrigins = {
    labelsThisDevice: activity.includes('This device'),
    labelsAnotherDevice: activity.includes('Another device'),
  }
  if (!state.activityOrigins.labelsThisDevice) {
    throw new Error('this browser’s own writes were not labelled “This device”')
  }
  if (!state.activityOrigins.labelsAnotherDevice) {
    throw new Error('the second device’s write was not labelled “Another device”')
  }
  await closeTaskDrawer()
  steps.push(`Completed “${TASKS[0].title}” (${state.taskIds[TASKS[0].title]}) through description, criteria, attachment, Ready → In progress → Done; activity visible`)

  // ---- Step 11: persistent state across a real server restart -----------
  log('stopping the server for the persistence proof')
  const firstPid = server?.pid ?? null
  await stopServer()
  const secondPid = await startServer()
  await page.reload()
  const afterRestartPath = new URL(page.url()).pathname
  const sessionSurvived = !afterRestartPath.startsWith('/login')
  if (!sessionSurvived) throw new Error('the browser session did not survive the server restart')
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  const completedCard = page
    .getByRole('listitem')
    .filter({ has: page.getByRole('button', { name: TASKS[0].title, exact: true }) })
  await completedCard.getByText('Done', { exact: true }).waitFor({ timeout: 15_000 })
  const completedCardInDoneColumn = await completedCard.getByText('Done', { exact: true }).isVisible()
  const doneColumn = page.getByRole('region', { name: /^Done column, \d+ task/ })
  await doneColumn.scrollIntoViewIfNeeded()
  await sleep(300)
  state.restart = {
    firstPid,
    secondPid,
    serverRestarted: firstPid !== null && secondPid !== null && firstPid !== secondPid,
    sessionSurvived,
    completedCardInDoneColumn,
    boardTaskCount: await page.locator('[data-board-action]').count(),
  }
  if (!state.restart.serverRestarted) throw new Error('the server pid did not change across the restart proof')
  if (!completedCardInDoneColumn) throw new Error('the completed card was not visible in the Done column after restart')
  await shot('20-after-restart-board-done')
  steps.push('Restarted the server between tasks; the session, the completed task and its board column survived a reload')

  // ---- Step 12: Task B completed from the board with the keyboard --------
  const taskBTitle = TASKS[TASKS.length - 1].title
  const taskBId = state.taskIds[taskBTitle]
  await page.goto(`${BASE}/projects/${state.projectId}/board`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  const cardB = page.getByRole('button', { name: taskBTitle, exact: true })
  await cardB.waitFor()
  await page.evaluate(() => {
    window.__orbitPointerDowns = 0
  })
  await cardB.focus()
  await shot('21-board-task-b-before')
  const beforeMoveB = await readTask(taskBId)
  const firstArrows = await boardKeyboardMove(taskBTitle, 'In progress')
  await page.getByRole('status').first().waitFor({ timeout: 5_000 }).catch(() => undefined)
  const afterFirstMoveB = await readTask(taskBId)
  const secondArrows = await boardKeyboardMove(taskBTitle, 'Done')
  const afterSecondMoveB = await readTask(taskBId)
  const pointerDowns = await page.evaluate(() => window.__orbitPointerDowns ?? 0)
  const usedMouse = pointerDowns > 0
  if (usedMouse) throw new Error(`the board move observed ${pointerDowns} pointer press(es)`)
  if (afterSecondMoveB.status !== 'done') {
    throw new Error(`the board keyboard move did not store Done (stored: ${afterSecondMoveB.status})`)
  }
  state.moves.push(
    { taskId: taskBId, from: beforeMoveB.status, to: afterFirstMoveB.status, surface: 'board keyboard (m, arrows, Enter)' },
    { taskId: taskBId, from: afterFirstMoveB.status, to: afterSecondMoveB.status, surface: 'board keyboard (m, arrows, Enter)' },
  )
  state.boardMove = {
    taskId: taskBId,
    title: taskBTitle,
    moves: ['In progress', 'Done'],
    arrowPresses: firstArrows + secondArrows,
    pointerDownsDuringMove: pointerDowns,
    usedMouse,
  }
  await page.getByRole('region', { name: /^Done column, \d+ task/ }).scrollIntoViewIfNeeded()
  await sleep(300)
  await shot('22-board-task-b-done')
  steps.push(
    `Completed “${taskBTitle}” from the board with two keyboard moves (Ready → In progress → Done, m → arrow → Enter, ${firstArrows + secondArrows} ArrowDown presses, ${pointerDowns} pointer presses)`,
  )

  // ---- Step 13: milestone progress + honest upgrade check ----------------
  await page.goto(`${BASE}/projects/${state.projectId}/milestones`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.getByText(MILESTONE_M31, { exact: true }).first().waitFor()
  await page.getByText(MILESTONE_REF, { exact: true }).first().waitFor()
  const finalTasksResponse = await api('/v1/authoritative/tasks')
  const finalTasks = finalTasksResponse.body?.tasks ?? []
  const expectedMilestones = [
    { name: MILESTONE_M31, assigned: 12, done: 2 },
    { name: MILESTONE_REF, assigned: 6, done: 0 },
  ]
  const milestoneRows = page.locator('[data-milestone-row]')
  if ((await milestoneRows.count()) !== expectedMilestones.length) {
    throw new Error(`expected ${expectedMilestones.length} milestone rows, found ${await milestoneRows.count()}`)
  }
  const milestoneState = {}
  for (const expected of expectedMilestones) {
    const milestoneId = state.milestoneIds[expected.name]
    if (!milestoneId) throw new Error(`milestone ${expected.name} has no id`)
    const assigned = finalTasks.filter((task) => task.milestone_id === milestoneId)
    const done = assigned.filter((task) => task.status === 'done').length
    const copy = `${done} of ${assigned.length} tasks done`
    if (assigned.length !== expected.assigned || done !== expected.done) {
      throw new Error(
        `milestone ${expected.name} is ${copy}, expected ${expected.done} of ${expected.assigned}`,
      )
    }
    const row = page.locator(`[data-milestone-row="${milestoneId}"]`)
    await waitUntil(async () => (await row.innerText()).includes(copy), {
      label: `milestone ${expected.name} to read “${copy}”`,
    })
    milestoneState[expected.name] = {
      id: milestoneId,
      assigned: assigned.length,
      done,
      copy,
      taskIds: assigned.map((task) => task.id),
    }
  }
  state.milestones = milestoneState
  await shot('23-milestone-progress')
  steps.push('Milestone progress read exactly 2 of 12 and 0 of 6 tasks done against the seeded plan')

  await page.goto(`${BASE}/projects/${state.projectId}/settings`)
  await page.getByRole('navigation', { name: 'Project sections' }).waitFor()
  await page.getByText('This project is on the newest published version of Development. No upgrade is pending.').waitFor()
  const upgradeOffered = await page.locator('[data-upgrade-available]').count()
  const versions = await api('/v1/authoritative/blueprints')
  const currentBinding = await api(`/v1/authoritative/blueprints/binding?project_id=${state.projectId}`)
  const publishedForWorkflow = (versions.body ?? [])
    .filter((entry) => entry[0] === currentBinding.body?.blueprint_id)
    .map((entry) => entry[1])
  const newestPublished = publishedForWorkflow.length > 0 ? Math.max(...publishedForWorkflow) : null
  if (upgradeOffered > 0) throw new Error('settings offered an upgrade that does not exist')
  if (newestPublished !== null && currentBinding.body.version !== newestPublished) {
    throw new Error(`an upgrade to v${newestPublished} exists but was not offered`)
  }
  state.upgrade = {
    offered: upgradeOffered > 0,
    currentVersion: currentBinding.body?.version ?? null,
    newestPublished,
    copy: 'This project is on the newest published version of Development. No upgrade is pending.',
  }
  await shot('24-settings-no-upgrade')
  steps.push('Reviewed milestone progress; settings honestly reported no newer published workflow version')

  // ---- Step 14: agent surface and final home -----------------------------
  await page.goto(`${BASE}/agents`)
  await page.getByRole('heading', { level: 1, name: 'Agents' }).waitFor()
  await shot('25-agents-no-jobs')
  const agentsText = await page.locator('main').innerText()
  const agentOverview = await api('/v1/authoritative/agent-jobs/overview')

  await page.goto(`${BASE}/`)
  await page.locator('main h1').first().waitFor()
  await shot('26-home-final')

  state.agentJobs = {
    created: false,
    visibleJobs: Array.isArray(agentOverview.body) ? agentOverview.body.length : null,
    reason: 'The web app exposes no launch or create control for agent jobs; the overview was empty and says so.',
    overviewCopy: agentsText.split('\n').filter((line) => line.trim().length > 0).slice(0, 10),
  }
  if (state.agentJobs.visibleJobs !== 0) {
    throw new Error(`expected no agent jobs for this device, found ${state.agentJobs.visibleJobs}`)
  }

  // ---- Final reads before the database is stopped -----------------------
  const finalTasksNowResponse = await api('/v1/authoritative/tasks')
  const finalTasksNow = finalTasksNowResponse.body?.tasks ?? []
  const finalMilestonesResponse = await api('/v1/authoritative/milestones')
  const finalMilestones = finalMilestonesResponse.body?.milestones ?? []
  const finalSchedule = await api('/v1/authoritative/schedule')
  const finalInventory = await api('/v1/authoritative/markdown-inventory')
  const finalItems = await api(`/v1/authoritative/blueprints/items?project_id=${state.projectId}`)
  const tasksByStatus = {}
  for (const task of finalTasksNow) tasksByStatus[task.status] = (tasksByStatus[task.status] ?? 0) + 1
  state.counts = {
    tasks: finalTasksNow.length,
    tasksByStatus,
    tasksWithMilestone: finalTasksNow.filter((task) => task.milestone_id).length,
    milestones: finalMilestones.filter((milestone) => milestone.project_id === state.projectId).length,
    scheduleBlocks: (finalSchedule.body?.blocks ?? []).length,
    notes: (finalInventory.body ?? []).length,
    workItems: (finalItems.body ?? []).length,
    attachments: state.attachment ? 1 : 0,
  }
  if (state.counts.tasks !== TASKS.length + 1) {
    throw new Error(`expected ${TASKS.length + 1} tasks (seeded plus the Inbox triage capture), found ${state.counts.tasks}`)
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

  // Every 4xx/5xx must belong to the two deliberate refusals this run is
  // allowed to provoke. A new error anywhere else fails the run rather than
  // hiding in a list nobody reads.
  const httpErrors = await Promise.all(
    httpErrorRecords.map(async (record) => {
      const { textPromise, ...rest } = record
      return { ...rest, body: await textPromise }
    }),
  )
  const isAllowed = (error) =>
    (error.status === 400 && /invalid task status transition: inbox -> done/.test(error.body)) ||
    (error.status === 409 && /conflicted with a newer version|stale server revision/i.test(error.body))
  const unexpectedErrors = httpErrors.filter((error) => !isAllowed(error))
  if (unexpectedErrors.length > 0) {
    throw new Error(`unexpected HTTP errors outside the allowlist:\n${JSON.stringify(unexpectedErrors, null, 2)}`)
  }
  const hasInvalidTransition = httpErrors.some(
    (error) => error.status === 400 && /invalid task status transition/.test(error.body),
  )
  const hasStaleRevision = httpErrors.some(
    (error) => error.status === 409 && /conflicted with a newer version|stale server revision/i.test(error.body),
  )
  if (!hasInvalidTransition || !hasStaleRevision || httpErrors.length !== 2) {
    throw new Error(
      `expected exactly one 400 invalid-transition and one 409 stale-revision refusal, got ${JSON.stringify(
        httpErrors.map((error) => ({ status: error.status, source: error.source, body: error.body })),
        null,
        2,
      )}`,
    )
  }

  const finishedAt = new Date().toISOString()
  const summary = {
    ...state,
    finishedAt,
    dbPath: DB,
    backups,
    evidenceDir: OUT,
    httpErrors,
    httpErrorAllowlist: [
      '400 invalid task status transition: inbox -> done (deliberate probe)',
      '409 stale server revision (deliberate second-device conflict)',
    ],
    steps,
    environment: {
      port: PORT,
      base: BASE,
      devices: [DEVICE_ID, SECOND_DEVICE_ID],
      dist: DIST,
      vault: path.join(RUN_DIR, 'vault', 'db.knowledge'),
      attachmentStore: path.join(RUN_DIR, 'vault', 'attachments', 'objects'),
      serverBinary: SERVER_BINARY,
      browser: 'Chromium 1440x900, light theme, headless',
    },
  }
  await writeFile(path.join(OUT, 'run.json'), `${JSON.stringify(summary, null, 2)}\n`)
  await writeFile(path.join(OUT, 'README.md'), renderReadme(summary))
  log(`wrote ${path.join(OUT, 'run.json')}`)
  log(`wrote ${path.join(OUT, 'README.md')}`)
  log(`database backup ${backup}`)
  log('I5 cycle finished')
  return summary
}

/* ------------------------------------------------------------------ */
/* Generated evidence README                                           */
/* ------------------------------------------------------------------ */

function mdCell(value) {
  const text = value === null || value === undefined ? '—' : String(value)
  return text.replace(/\|/g, '\\|').replace(/\r?\n/g, ' ')
}

function mdTable(headers, rows) {
  const lines = [
    `| ${headers.map(mdCell).join(' | ')} |`,
    `| ${headers.map(() => '---').join(' | ')} |`,
    ...rows.map((row) => `| ${row.map(mdCell).join(' | ')} |`),
  ]
  return lines.join('\n')
}

/**
 * Everything below is read from `run.json`/the recorded state. There are no
 * separately maintained claims to drift: change the run, and this file changes.
 */
function renderReadme(summary) {
  const identity = summary.identity
  const titles = new Map(Object.entries(summary.taskIds).map(([title, id]) => [id, title]))
  const moveRows = summary.moves.map((move) => [
    titles.get(move.taskId) ?? move.taskId,
    move.taskId,
    `${move.from} → ${move.to}`,
    move.surface,
  ])
  if (summary.triage) {
    moveRows.push([
      titles.get(summary.triage.taskId) ?? 'Inbox triage capture',
      summary.triage.taskId,
      `${summary.triage.before.status} (status unchanged)`,
      `${summary.triage.surface}: project ${summary.triage.before.projectId ?? 'none'} → ${summary.triage.after.projectId}, priority ${summary.triage.before.priority ?? 'none'} → ${summary.triage.after.priority}`,
    ])
  }
  const milestoneRows = Object.entries(summary.milestones).map(([name, milestone]) => [
    name,
    milestone.id,
    milestone.assigned,
    milestone.done,
    milestone.copy,
  ])
  const planning = summary.planning
  const conflict = summary.conflict
  const board = summary.boardMove
  const restart = summary.restart
  const upgrade = summary.upgrade
  const counts = summary.counts
  const errorRows = summary.httpErrors.map((error) => [
    error.status,
    error.method,
    error.source,
    error.body,
  ])

  return `# I5 — real dogfooding cycle, evidence

This directory is generated by \`web/tests/e2e/i5/dogfood.mjs\`. The numbers and
identities below come from \`run.json\`; do not edit them by hand.

## Capture identity

${mdTable(
  ['Item', 'Value'],
  [
    ['Application commit', `\`${identity.commit}\``],
    ['Tree at capture', identity.tree],
    ['Built SPA', `\`${identity.dist.sha256}\` (${identity.dist.fileCount} files, ${identity.dist.totalBytes} bytes)`],
    ['Server binary', `\`${identity.serverBinary.sha256}\` (${identity.serverBinary.bytes} bytes)`],
    ['Workflow manifest', `${identity.workflow.label} (\`${identity.workflow.id}\` v${identity.workflow.version})`],
    ['Origin', summary.environment.base],
    ['Database', summary.dbPath],
    ['Knowledge vault', summary.environment.vault],
    ['Attachment store', summary.environment.attachmentStore],
    ['Enrolled devices', summary.environment.devices.join(', ')],
    ['Browser', summary.environment.browser],
    ['Capture window', `${summary.startedAt} → ${summary.finishedAt}`],
    ['Exact command', `\`${summary.command.invocation}\` from \`${summary.command.cwd}\``],
    ['Machine-readable run', '[`run.json`](run.json)'],
  ],
)}

## Project and final content

${mdTable(
  ['Item', 'Value'],
  [
    ['Project', `${summary.project.name} (\`${summary.project.id}\`), status ${summary.project.status}`],
    ['Workflow binding', `\`${summary.project.workflow.id}\` v${summary.project.workflow.version}`],
    ['Tasks (including the Inbox capture)', counts.tasks],
    ['Tasks by status', Object.entries(counts.tasksByStatus).map(([status, count]) => `${status}: ${count}`).join(', ')],
    ['Tasks with a milestone', counts.tasksWithMilestone],
    ['Milestones', counts.milestones],
    ['Schedule blocks', counts.scheduleBlocks],
    ['Knowledge notes', counts.notes],
    ['Work items', counts.workItems],
    ['Attachments', counts.attachments],
    ['Feature work item', `${summary.workItem.id} — title “${summary.workItem.properties?.title ?? ''}”, kind ${summary.workItem.kind}, state ${summary.workItem.state}`],
    ['Knowledge note', `${summary.note.id} — \`${summary.note.path}\` (“${summary.note.title}”)`],
    ['Attachment', `${summary.attachment.id} — \`${summary.attachment.filename}\`, ${summary.attachment.sizeBytes} bytes, checksum \`${summary.attachment.checksum}\``],
  ],
)}

## Task ids that actually moved

${mdTable(['Task', 'Id', 'Stored change', 'Surface'], moveRows)}

## Milestone progress against the seeded plan

${mdTable(['Milestone', 'Id', 'Assigned', 'Done', 'Observed copy'], milestoneRows)}

The run refused to finish unless the milestone rows read exactly these counts.

## Planning outcome

${mdTable(
  ['Fact', 'Value'],
  [
    ['Capacity authored', `${planning.capacityMinutes} minutes`],
    ['Day-plan rows selected', planning.dayPlanRowsSelected],
    ['Proposed slots read before review', planning.proposedSlotsBeforeReview],
    ['Review disabled before review', planning.reviewDisabledBeforeReview],
    ['Window', `${planning.window.date} ${planning.window.from}–${planning.window.to}`],
    ['Review fingerprint', `\`${planning.reviewFingerprint}\``],
    ['Review command id', `\`${planning.reviewCommandId}\``],
    ['Receipt block ids', planning.receiptBlocks.map((id) => `\`${id}\``).join(', ')],
    ['Blocks before reload', planning.blocksBeforeReload],
    ['Blocks after reload', planning.blocksAfterReload],
    ['Applied', planning.applied],
    ['Survived reload', planning.survivedReload],
    ['Blocked because', planning.blockedBecause],
  ],
)}

## Conflict recovery

${mdTable(
  ['Fact', 'Value'],
  [
    ['Second enrolled device', conflict.secondDeviceId],
    ['Second write', `${conflict.secondWrite.surface} on task \`${conflict.secondWrite.taskId}\``],
    ['Second write stored', conflict.secondWrite.storedDescriptionIncludesMarker],
    ['Primary refusal', conflict.refusal],
    ['In-drawer alerts', conflict.alertsInDrawer],
    ['Reload controls in drawer', conflict.reloadControlsInDrawer],
    ['Recovered by', conflict.recoveredBy],
    ['Recovered', conflict.recovered],
    ['Second device text kept, refused draft absent', conflict.noSilentOverwrite],
  ],
)}

## Activity attribution

${mdTable(
  ['Fact', 'Value'],
  [
    ['Labels this browser’s writes “This device”', summary.activityOrigins.labelsThisDevice],
    ['Labels the second device’s write “Another device”', summary.activityOrigins.labelsAnotherDevice],
  ],
)}

## Board keyboard move

${mdTable(
  ['Fact', 'Value'],
  [
    ['Task', `${board.title} (\`${board.taskId}\`)`],
    ['Moves', board.moves.join(' → ')],
    ['ArrowDown presses', board.arrowPresses],
    ['Pointer presses during the move', board.pointerDownsDuringMove],
    ['Used mouse', board.usedMouse],
  ],
)}

## Server restart

${mdTable(
  ['Fact', 'Value'],
  [
    ['Server pid before', restart.firstPid],
    ['Server pid after', restart.secondPid],
    ['Server restarted', restart.serverRestarted],
    ['Session survived', restart.sessionSurvived],
    ['Completed card in Done column', restart.completedCardInDoneColumn],
    ['Board task buttons after restart', restart.boardTaskCount],
  ],
)}

## HTTP errors

Only these refusals are allowed; any other 4xx/5xx fails the run.

${mdTable(['Status', 'Method', 'Source', 'Body'], errorRows)}

## Honest non-claims recorded by the run

${mdTable(
  ['Surface', 'Fact', 'Value'],
  [
    ['Upgrade', 'Current / newest published version', `${upgrade.currentVersion} / ${upgrade.newestPublished}`],
    ['Upgrade', 'Upgrade offered', upgrade.offered],
    ['Agent jobs', 'Visible jobs for this device', summary.agentJobs.visibleJobs],
    ['Agent jobs', 'Created through the browser', summary.agentJobs.created],
    ['Agent jobs', 'Reason', summary.agentJobs.reason],
    ['Invalid transition', 'Attempted', `${summary.invalidTransition.attempted} (offered by control: ${summary.invalidTransition.offeredByControl})`],
    ['Invalid transition', 'Refusal', summary.invalidTransition.refusal],
  ],
)}

## Captures

${summary.captures.map((name, index) => `${index + 1}. [\`${name}.png\`](${name}.png)`).join('\n')}

## Database backup

${summary.backups
  .map((file) => `- [\`${path.basename(file)}\`](${path.basename(file)})`)
  .join('\n')}

The attachment source file is committed as
[\`task-a-evidence.md\`](task-a-evidence.md). Attachment bytes live in the server's
object store, not in SQLite.

## How to reproduce

\`\`\`bash
cd ${summary.command.cwd}
node ${path.relative(summary.command.cwd, fileURLToPath(import.meta.url)).split(path.sep).join('/')}
\`\`\`

The script refuses to start on a dirty tree, wipes \`/tmp/orbit-i5\`, enrols both
devices, and performs every content change through the browser. It rewrites this
directory and the database backup on success.
`
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
        JSON.stringify({ state, httpErrors: httpErrorRecords, steps, url: page.url() }, null, 2),
      )
    }
  } catch {
    // best effort
  }
  if (secondContext) await secondContext.close().catch(() => undefined)
  if (browser) await browser.close().catch(() => undefined)
  await stopServer().catch(() => undefined)
  process.exitCode = 1
}
