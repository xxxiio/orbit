#!/usr/bin/env node
/**
 * M31 I6 — measured browser performance on the deterministic fixture.
 *
 *   cd web && node scripts/measure.mjs [--repeat 10] [--label before|after]
 *
 * What this does
 * --------------
 * 1. Builds the fixture seeder (`tests/fixture/seed.ts`) with esbuild — the
 *    fixture is TypeScript and this is a plain Node 18 script, so it is bundled
 *    into a temp module instead of duplicating the 122-task fixture here.
 * 2. Starts the real `orbit-server` binary against a throwaway database and the
 *    built SPA in `web/dist` (same deployment shape as the Playwright harness:
 *    one origin, no CORS proxy).
 * 3. Seeds the deterministic 122-task workspace through the public API.
 * 4. Drives Chromium and records, per repeat:
 *      cold_load      navigation → first meaningful Home content
 *      project_switch in-workspace project switcher → new project content
 *      task_list      Tasks view navigation → first rendered rows
 *      board_move     `m` + arrows + Enter → card in the target column
 *      detail_open    card click → task drawer with its description field
 *      mutation       priority change → server answer + footer settled
 * 5. Writes the raw samples, medians and p95s to a JSON file and prints a table.
 *
 * Method (stated so the numbers are not hand-waving)
 * --------------------------------------------------
 * - Every scenario is repeated N times (default 10; the contract requires ≥5).
 * - `cold_load` uses a fresh browser context per repeat, so the HTTP cache,
 *   cookies and JS heap start cold; the server stays warm (its SQLite pages are
 *   hot after seeding). Each other scenario starts from a warm page and measures
 *   one interaction, with a fresh route navigation before the measured action so
 *   React Query's cache is not the thing being timed twice in a row.
 * - Times come from `performance.now()` inside the page, relative to the
 *   navigation's time origin (`performance.timeOrigin`), not from Node wall
 *   clock around protocol round trips. `first meaningful content` is the first
 *   animation frame after `main h1` exists *and* no `[aria-busy="true"]` surface
 *   remains in `main` — i.e. the route identity and its data have rendered, not
 *   just the shell. Cold load also records FCP and DCL from the Navigation and
 *   Paint timing entries.
 * - `board_move` and `mutation` additionally record the matching API request's
 *   response time (server round trip) so UI latency and server latency are not
 *   conflated.
 * - Median is the middle sample of the sorted set. p95 is the nearest-rank
 *   percentile: `sorted[ceil(0.95 * n) - 1]` (with n = 10 the largest sample).
 *   Small-sample percentiles are descriptive for this host and fixture; do not
 *   compare them across machines without re-running.
 *
 * Output: `.agent/evidence/m31-web-perf-<label>.json` and a printed table.
 * The Markdown report (`m31-web-perf.md`) quotes the generated JSON.
 */
import { execFileSync, spawn } from 'node:child_process'
import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { build } from 'esbuild'
import { chromium } from '@playwright/test'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const WEB = path.resolve(HERE, '..')
const REPO = path.resolve(WEB, '..')
const DIST = path.join(WEB, 'dist')
const EVIDENCE = path.join(REPO, '.agent', 'evidence')
const SERVER_BINARY = process.env.ORBIT_SERVER_BIN ?? path.join(REPO, 'target', 'debug', 'orbit-server')
const RUN_ROOT = path.join('/tmp/opencode', 'orbit-i6-perf')

const argv = process.argv.slice(2)
const arg = (name, fallback) => {
  const index = argv.indexOf(name)
  return index >= 0 && argv[index + 1] ? argv[index + 1] : fallback
}
const REPEAT = Number(arg('--repeat', '10'))
const LABEL = arg('--label', 'run')
if (!Number.isInteger(REPEAT) || REPEAT < 5) {
  throw new Error('--repeat must be an integer ≥ 5 (the evidence contract requires at least 5)')
}

// Chromium on this machine needs the local bundle of system libraries.
for (const candidate of [
  process.env.ORBIT_E2E_BROWSER_LIBS,
  '/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu',
].filter(Boolean)) {
  try {
    await readFile(path.join(candidate, 'libnspr4.so'))
    process.env.LD_LIBRARY_PATH = process.env.LD_LIBRARY_PATH
      ? `${candidate}:${process.env.LD_LIBRARY_PATH}`
      : candidate
    break
  } catch {
    // try the next candidate
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
const log = (message) => process.stdout.write(`[measure] ${message}\n`)

function sha256(value) {
  return createHash('sha256').update(value).digest('hex')
}

async function hashDirectory(directory) {
  const { readdir } = await import('node:fs/promises')
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
  return {
    sha256: sha256(files.map((file) => `${file.path}\u0000${file.sha256}`).join('\n')),
    fileCount: files.length,
    totalBytes: files.reduce((total, file) => total + file.bytes, 0),
  }
}

function git(args) {
  return execFileSync('git', args, { cwd: REPO }).toString().trim()
}

function requireCleanTree() {
  const commit = git(['rev-parse', 'HEAD'])
  const status = git(['status', '--porcelain'])
  if (status.length > 0 && process.env.ORBIT_MEASURE_ALLOW_DIRTY !== '1') {
    throw new Error(
      `refusing to record measurements from a dirty tree at ${commit}. Commit first, or set ORBIT_MEASURE_ALLOW_DIRTY=1 for a labelled exploratory run:\n${status}`,
    )
  }
  return { commit, dirty: status.length > 0 }
}

function findFreePort() {
  return new Promise((resolve, reject) => {
    const probe = createServer()
    probe.unref()
    probe.once('error', reject)
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address()
      const port = typeof address === 'object' && address !== null ? address.port : 0
      probe.close(() => (port > 0 ? resolve(port) : reject(new Error('no free port'))))
    })
  })
}

async function startServer({ base, dbPath, adminKey }) {
  const env = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (!key.startsWith('ORBIT_')) env[key] = value
  }
  Object.assign(env, {
    ORBIT_SERVER_DB: dbPath,
    ORBIT_SERVER_BIND: `127.0.0.1:${new URL(base).port}`,
    ORBIT_SERVER_ADMIN_KEY: adminKey,
    ORBIT_WEB_DIST: DIST,
  })
  const child = spawn(SERVER_BINARY, [], {
    cwd: REPO,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  const chunks = []
  child.stdout.on('data', (chunk) => chunks.push(String(chunk)))
  child.stderr.on('data', (chunk) => chunks.push(String(chunk)))
  const started = Date.now()
  while (Date.now() - started < 30_000) {
    if (child.exitCode !== null) {
      throw new Error(`orbit-server exited (${child.exitCode}) before health:\n${chunks.join('')}`)
    }
    try {
      const response = await fetch(`${base}/health`, { signal: AbortSignal.timeout(1_500) })
      if (response.ok) return child
    } catch {
      // not up yet
    }
    await sleep(100)
  }
  throw new Error(`orbit-server did not become healthy:\n${chunks.join('')}`)
}

async function stopServer(child) {
  if (!child || child.exitCode !== null) return
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

async function seedFixture(base, deviceId, token, nowMs) {
  const bundle = path.join(RUN_ROOT, 'seed.bundle.mjs')
  await build({
    entryPoints: [path.join(WEB, 'tests', 'fixture', 'seed.ts')],
    outfile: bundle,
    bundle: true,
    format: 'esm',
    platform: 'node',
    target: 'node18',
    logLevel: 'silent',
  })
  const { seedWorkspace, bearerTransport } = await import(pathToFileURL(bundle).href)
  const fixture = await seedWorkspace(bearerTransport({ base, deviceId, token }), {
    machineRef: deviceId,
    nowMs,
  })
  return fixture
}

function stats(samples) {
  if (samples.length === 0) return { n: 0, median: null, p95: null, min: null, max: null }
  const sorted = [...samples].sort((left, right) => left - right)
  const middle = Math.floor(sorted.length / 2)
  const median =
    sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle]
  const rank = Math.ceil(0.95 * sorted.length)
  return {
    n: sorted.length,
    median: round(median),
    p95: round(sorted[Math.min(rank, sorted.length) - 1]),
    min: round(sorted[0]),
    max: round(sorted[sorted.length - 1]),
  }
}

const round = (value) => (value === null || value === undefined ? null : Math.round(value * 10) / 10)

async function sessionCookie(context, base, deviceId, token) {
  const response = await context.request.post(`${base}/v1/web/session`, {
    data: { device_id: deviceId, token },
    headers: { 'content-type': 'application/json' },
  })
  if (response.status() !== 201) {
    throw new Error(`web session failed: ${response.status()} ${await response.text()}`)
  }
}

async function newPage(context, nowMs) {
  // Deliberately no `page.clock.setFixedTime`: Playwright's fake clock replaces
  // the Performance Timeline (timeOrigin becomes -1 and navigation/paint entries
  // disappear), which would silently turn every timing into a guess. The fixture
  // is seeded against the same real clock instead, so date-relative UI stays
  // consistent without breaking the thing being measured.
  return context.newPage()
}

async function resetRoute(page, pathname, selector = 'main h1') {
  await page.goto(pathname, { waitUntil: 'domcontentloaded' })
  await page.waitForSelector(selector, { timeout: 20_000 })
  await page.waitForFunction(
    () => !document.querySelector('main [aria-busy="true"]'),
    undefined,
    { timeout: 20_000 },
  )
}

async function measureColdLoad(browser, base, nowMs, deviceId, token) {
  const samples = []
  for (let index = 0; index < REPEAT; index += 1) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, baseURL: base })
    await sessionCookie(context, base, deviceId, token)
    const page = await newPage(context, nowMs)

    const started = Date.now()
    await page.goto('/', { waitUntil: 'domcontentloaded' })
    await page.waitForSelector('main h1', { timeout: 20_000 })
    await page
      .waitForFunction(() => !document.querySelector('main [aria-busy="true"]'), undefined, {
        timeout: 20_000,
        polling: 'raf',
      })
    const contentMs = Date.now() - started
    const paint = await page.evaluate(() => {
      const entries = performance.getEntriesByType('paint')
      return Object.fromEntries(entries.map((entry) => [entry.name, entry.startTime]))
    })
    const navigation = await page.evaluate(() => {
      const entry = performance.getEntriesByType('navigation')[0]
      if (!entry) return null
      return {
        domContentLoaded: entry.domContentLoadedEventEnd,
        load: entry.loadEventEnd,
        transferSize: entry.transferSize,
      }
    })
    samples.push({
      contentMs: round(contentMs),
      fcp: round(paint['first-contentful-paint'] ?? null),
      domContentLoaded: round(navigation?.domContentLoaded ?? null),
      load: round(navigation?.load ?? null),
      transferSize: navigation?.transferSize ?? null,
    })
    await context.close()
  }
  return samples
}

async function measureProjectSwitch(page, base, fixture) {
  const samples = []
  const current = 'Orbit Development'
  const target = 'Learning'
  for (let index = 0; index < REPEAT; index += 1) {
    await resetRoute(page, '/projects/p-orbit-development')
    await page.waitForFunction(
      (name) => document.querySelector('main h1')?.textContent?.trim() === name,
      current,
      { timeout: 20_000 },
    )
    const started = Date.now()
    await page.getByRole('button', { name: current, exact: true }).first().click()
    await page.getByRole('menuitem', { name: target, exact: true }).click()
    await page.waitForFunction(
      (name) => document.querySelector('main h1')?.textContent?.trim() === name,
      target,
      { timeout: 20_000 },
    )
    await page.waitForFunction(
      () => !document.querySelector('main [aria-busy="true"]'),
      undefined,
      { timeout: 20_000 },
    )
    samples.push({ switchMs: round(Date.now() - started) })
  }
  return samples
}

async function measureTaskList(page, base, fixture) {
  const samples = []
  for (let index = 0; index < REPEAT; index += 1) {
    const started = Date.now()
    await page.goto('/projects/p-learning/tasks', { waitUntil: 'domcontentloaded' })
    // Wait until the list has rows and its skeleton is gone.
    await page.waitForFunction(
      () => {
        const table = document.querySelector('main table')
        if (!table) return false
        const rows = table.querySelectorAll('tbody tr')
        return rows.length > 0 && !document.querySelector('main [aria-busy="true"]')
      },
      undefined,
      { timeout: 20_000, polling: 'raf' },
    )
    const elapsed = Date.now() - started
    const rows = await page.locator('main table tbody tr').count()
    samples.push({ renderMs: round(elapsed), rows })
  }
  return samples
}

async function createBoardTask(page, base, title) {
  const snapshot = await page.request.get(`${base}/v1/authoritative/tasks`)
  if (snapshot.status() !== 200) {
    throw new Error(`could not read the revision: ${snapshot.status()}`)
  }
  const { revision } = await snapshot.json()
  const response = await page.request.post(`${base}/v1/authoritative/task-commands`, {
    headers: { 'x-orbit-csrf': '1' },
    data: {
      command_id: `measure-board-${randomUUID()}`,
      expected_revision: revision,
      action: {
        type: 'create',
        task_id: `t-measure-${randomUUID().slice(0, 8)}`,
        title,
        project_id: 'p-basic',
      },
    },
  })
  if (response.status() !== 200) {
    throw new Error(`could not create the board measurement task: ${response.status()} ${await response.text()}`)
  }
}

async function measureBoardMove(page, base) {
  const samples = []
  for (let index = 0; index < REPEAT; index += 1) {
    // One fresh inbox task per repeat, so every sample is a real single move
    // (the domain has no planned → inbox edge to reset a used card with).
    const title = `Measure board move ${index + 1}`
    await createBoardTask(page, base, title)
    await resetRoute(page, '/projects/p-basic/board')
    const card = page.getByRole('button', { name: title, exact: true })
    await card.waitFor({ timeout: 20_000 })

    const started = Date.now()
    let apiMs = null
    const requestDone = page
      .waitForResponse(
        (response) =>
          response.url().includes('/v1/authoritative/task-commands') &&
          response.request().method() === 'POST',
        { timeout: 20_000 },
      )
      .then((response) => {
        apiMs = round(Date.now() - started)
        return response
      })
    await card.focus()
    await page.keyboard.press('m')
    const menu = page.getByRole('menu')
    await menu.waitFor({ timeout: 20_000 })
    await page.getByRole('menuitem', { name: 'Planned', exact: true }).click()
    await requestDone
    const planned = page.getByRole('region', { name: /^Planned column/ })
    await planned.getByRole('button', { name: title, exact: true }).waitFor({ timeout: 20_000 })
    samples.push({ moveMs: round(Date.now() - started), apiMs })
  }
  return samples
}

async function measureDetailOpen(page, base, fixture) {
  const samples = []
  const task = fixture.tasks.find((candidate) => candidate.projectId === 'p-basic')
  if (!task) throw new Error('the fixture has no p-basic task for the detail measurement')
  for (let index = 0; index < REPEAT; index += 1) {
    await resetRoute(page, '/projects/p-basic/board')
    const card = page.getByRole('button', { name: task.title, exact: true })
    await card.waitFor({ timeout: 20_000 })
    const started = Date.now()
    await card.click()
    const drawer = page.getByRole('dialog')
    await drawer.waitFor({ timeout: 20_000 })
    await drawer.getByLabel('Task description').waitFor({ timeout: 20_000 })
    samples.push({ openMs: round(Date.now() - started) })
    await drawer.getByRole('button', { name: 'Close task details' }).click()
    await drawer.waitFor({ state: 'hidden', timeout: 10_000 })
  }
  return samples
}

async function measureMutation(page, base, fixture) {
  const samples = []
  const task = fixture.tasks.find((candidate) => candidate.projectId === 'p-basic')
  if (!task) throw new Error('the fixture has no p-basic task for the mutation measurement')
  for (let index = 0; index < REPEAT; index += 1) {
    await resetRoute(page, '/projects/p-basic/board')
    const card = page.getByRole('button', { name: task.title, exact: true })
    await card.waitFor({ timeout: 20_000 })
    await card.click()
    const drawer = page.getByRole('dialog')
    await drawer.getByLabel('Task description').waitFor({ timeout: 20_000 })
    const current = (await drawer.getByLabel('Task priority').innerText()).trim().toLowerCase()
    const next = ['urgent', 'high', 'medium', 'low'].find((value) => value !== current) ?? 'medium'

    const started = Date.now()
    let apiMs = null
    const responsePromise = page.waitForResponse(
      (response) =>
        response.url().includes('/v1/authoritative/task-commands') &&
        response.request().method() === 'POST',
      { timeout: 20_000 },
    )
    await drawer.getByLabel('Task priority').click()
    await page.getByRole('option', { name: new RegExp(`^${next}$`, 'i') }).click()
    const response = await responsePromise
    apiMs = round(Date.now() - started)
    if (response.status() !== 200) {
      throw new Error(`priority command failed: ${response.status()} ${await response.text()}`)
    }
    await drawer
      .locator('footer')
      .filter({ hasText: /saved|All changes saved|No pending/i })
      .waitFor({ timeout: 20_000 })
    samples.push({ mutationMs: round(Date.now() - started), apiMs })
    await drawer.getByRole('button', { name: 'Close task details' }).click()
    await drawer.waitFor({ state: 'hidden', timeout: 10_000 })
  }
  return samples
}

async function run() {
  const tree = requireCleanTree()
  const repeat = REPEAT
  log(`commit ${tree.commit}${tree.dirty ? ' (DIRTY)' : ''}, ${repeat} repeats per scenario`)

  await rm(RUN_ROOT, { recursive: true, force: true })
  const runDir = path.join(RUN_ROOT, randomUUID())
  const dbPath = path.join(runDir, 'vault', 'db.sqlite3')
  await mkdir(path.dirname(dbPath), { recursive: true })
  await mkdir(EVIDENCE, { recursive: true })

  const port = await findFreePort()
  const base = `http://127.0.0.1:${port}`
  const adminKey = randomBytes(32).toString('hex')
  const deviceId = `measure-${randomUUID().slice(0, 8)}`
  const token = randomBytes(32).toString('hex')

  const dist = await hashDirectory(DIST)
  const binary = await readFile(SERVER_BINARY)

  let server
  let browser
  try {
    server = await startServer({ base, dbPath, adminKey })
    log(`server healthy on ${base}`)
    const enrolled = await fetch(`${base}/v1/devices`, {
      method: 'POST',
      headers: { authorization: `Bearer ${adminKey}`, 'content-type': 'application/json' },
      body: JSON.stringify({ device_id: deviceId, token }),
    })
    if (enrolled.status !== 201) {
      throw new Error(`device enrolment failed: ${enrolled.status} ${await enrolled.text()}`)
    }

    // Seed against the real browser clock (see `newPage`): the measurement must
    // not fake time, or the Performance Timeline stops reporting.
    const nowMs = Date.now()
    const fixture = await seedFixture(base, deviceId, token, nowMs)
    log(`seeded ${fixture.counts.total} tasks across ${fixture.projects.length} projects`)

    browser = await chromium.launch({ headless: true })

    const scenarioContext = await browser.newContext({ viewport: { width: 1440, height: 900 }, baseURL: base })
    await sessionCookie(scenarioContext, base, deviceId, token)
    const page = await newPage(scenarioContext, nowMs)

    const cold = await measureColdLoad(browser, base, nowMs, deviceId, token)
    log('cold load done')
    const projectSwitch = await measureProjectSwitch(page, base, fixture)
    log('project switch done')
    const taskList = await measureTaskList(page, base, fixture)
    log('task list done')
    const boardMove = await measureBoardMove(page, base)
    log('board move done')
    const detailOpen = await measureDetailOpen(page, base, fixture)
    log('detail open done')
    const mutation = await measureMutation(page, base, fixture)
    log('mutation done')

    const scenarios = {
      cold_load: {
        description: 'navigation → first meaningful Home content (heading rendered, no aria-busy surface)',
        samples: cold,
        stats: {
          contentMs: stats(cold.map((sample) => sample.contentMs)),
          fcp: stats(cold.map((sample) => sample.fcp).filter((value) => value !== null)),
          domContentLoaded: stats(cold.map((sample) => sample.domContentLoaded).filter((value) => value !== null)),
          load: stats(cold.map((sample) => sample.load).filter((value) => value !== null)),
        },
      },
      project_switch: {
        description: 'in-workspace project switcher: menu selection → new project heading and data',
        samples: projectSwitch,
        stats: { switchMs: stats(projectSwitch.map((sample) => sample.switchMs)) },
      },
      task_list: {
        description: `Tasks view navigation → rows rendered (${taskList[0]?.rows ?? 0} rows from the 122-task fixture)`,
        samples: taskList,
        stats: { renderMs: stats(taskList.map((sample) => sample.renderMs)) },
      },
      board_move: {
        description: 'card focused → m → Planned → card in the Planned column, with the server response time',
        samples: boardMove,
        stats: {
          moveMs: stats(boardMove.map((sample) => sample.moveMs)),
          apiMs: stats(boardMove.map((sample) => sample.apiMs).filter((value) => value !== null)),
        },
      },
      detail_open: {
        description: 'board card click → drawer with its description field',
        samples: detailOpen,
        stats: { openMs: stats(detailOpen.map((sample) => sample.openMs)) },
      },
      mutation: {
        description: 'priority change → task-commands response → footer saved announcement',
        samples: mutation,
        stats: {
          mutationMs: stats(mutation.map((sample) => sample.mutationMs)),
          apiMs: stats(mutation.map((sample) => sample.apiMs).filter((value) => value !== null)),
        },
      },
    }

    const result = {
      label: LABEL,
      capturedAt: new Date().toISOString(),
      commit: tree.commit,
      dirty: tree.dirty,
      host: {
        platform: process.platform,
        arch: process.arch,
        node: process.version,
        cpus: (await import('node:os')).cpus().length,
        totalMemoryBytes: (await import('node:os')).totalmem(),
      },
      fixture: {
        totalTasks: fixture.counts.total,
        projects: fixture.projects.length,
        taskListRows: taskList[0]?.rows ?? null,
        nowMs,
      },
      build: { dist, serverBinaryBytes: binary.length, serverBinarySha256: sha256(binary) },
      method: {
        repeats: repeat,
        median: 'middle sample of the sorted set',
        p95: 'nearest-rank: sorted[ceil(0.95*n)-1]',
        clock: 'Node wall clock around the Playwright action, resolved by a page-side condition (includes a few ms of CDP overhead); FCP/DCL/load come from the Paint and Navigation timing entries',
        coldLoad: 'fresh browser context per repeat; server warm',
        warmInteractions: 'same page, route reset before each measured action',
      },
      scenarios,
    }

    const jsonPath = path.join(EVIDENCE, `m31-web-perf-${LABEL}.json`)
    await writeFile(jsonPath, JSON.stringify(result, null, 2) + '\n')
    log(`wrote ${path.relative(REPO, jsonPath)}`)

    const rows = []
    for (const [name, scenario] of Object.entries(scenarios)) {
      for (const [metric, value] of Object.entries(scenario.stats)) {
        rows.push([
          name,
          metric,
          value.median === null ? '—' : String(value.median),
          value.p95 === null ? '—' : String(value.p95),
          String(value.n),
        ])
      }
    }
    const widths = rows.reduce(
      (acc, row) => row.map((cell, index) => Math.max(acc[index] ?? 0, cell.length)),
      [0, 0, 0, 0, 0],
    )
    const print = (row) =>
      row.map((cell, index) => cell.padEnd(widths[index])).join('  ').trimEnd()
    process.stdout.write('\n' + print(['scenario', 'metric', 'median', 'p95', 'n']) + '\n')
    process.stdout.write(widths.map((width) => '-'.repeat(width)).join('  ') + '\n')
    for (const row of rows) process.stdout.write(print(row) + '\n')
    process.stdout.write(`\nunits: milliseconds. JSON: ${path.relative(REPO, jsonPath)}\n`)
  } finally {
    if (browser) await browser.close().catch(() => undefined)
    await stopServer(server)
    if (process.env.ORBIT_KEEP_MEASURE_STATE !== '1') {
      await rm(runDir, { recursive: true, force: true }).catch(() => undefined)
    }
  }
}

run().catch((error) => {
  process.stderr.write(`[measure] FAILED: ${error?.stack ?? error}\n`)
  process.exit(1)
})
