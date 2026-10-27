/**
 * A **persistent** dogfooding workspace: one real `orbit-server`, one real
 * SQLite database, reused between runs.
 *
 * This is deliberately not the acceptance harness. `tests/e2e/harness/server.ts`
 * creates a throwaway database per test; dogfooding needs the opposite: the
 * state a person built yesterday must still be there today, and a second run is
 * supposed to *resume* rather than reset. So:
 *
 * - the database lives outside the test temp dirs (`/tmp/opencode/orbit-dogfood`)
 *   and is never deleted by a test run;
 * - the enrolled device credential and the admin key are stored next to it, so a
 *   later run signs in as the same device without re-enrolling;
 * - a server that is already listening on the fixed port is reused if it proves
 *   it owns this workspace (its admin key answers `GET /v1/devices`).
 *
 * `ORBIT_DOGFOOD_DIR` and `ORBIT_DOGFOOD_PORT` override the location.
 */
import { execFileSync, spawn, type ChildProcess } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { createWriteStream, type WriteStream } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import { DEFAULT_SERVER_BIN, REPO_ROOT, WEB_DIST } from '../harness/server'

export const DOGFOOD_DIR = process.env.ORBIT_DOGFOOD_DIR ?? '/tmp/opencode/orbit-dogfood'
export const DOGFOOD_PORT = Number(process.env.ORBIT_DOGFOOD_PORT ?? 8781)
export const DOGFOOD_BASE = `http://127.0.0.1:${DOGFOOD_PORT}`
export const DOGFOOD_DB = join(DOGFOOD_DIR, 'data', 'server.db')
export const STATE_PATH = join(DOGFOOD_DIR, 'workspace.json')

/** Everything a later run needs to sign in again, plus the run ledger. */
export interface WorkspaceState {
  createdAtMs: number
  deviceId: string
  token: string
  adminKey: string
  seededAtMs: number | null
  /** Fixture facts worth remembering (counts, ids of seeded extras). */
  seed: Record<string, unknown> | null
  /** One entry per completed dogfooding session. */
  runs: Array<{
    index: number
    startedAtMs: number
    finishedAtMs: number
    commit: string
    steps: number
    failures: number
  }>
}

export interface DogfoodWorkspace {
  base: string
  dataDir: string
  dbPath: string
  state: WorkspaceState
  /** True when this call started the server process (and must stop it). */
  owned: boolean
  /** True when the database already existed before this call. */
  resumed: boolean
  logTail(lines?: number): Promise<string>
  /** Persist the state file after a change. */
  save(): Promise<void>
  stop(): Promise<void>
}

let log: WriteStream | null = null
let child: ChildProcess | null = null
const tail: string[] = []

async function readState(): Promise<WorkspaceState | null> {
  try {
    return JSON.parse(await readFile(STATE_PATH, 'utf8')) as WorkspaceState
  } catch {
    return null
  }
}

async function writeState(state: WorkspaceState): Promise<void> {
  await writeFile(STATE_PATH, `${JSON.stringify(state, null, 2)}\n`, 'utf8')
}

async function healthy(base: string): Promise<boolean> {
  try {
    const response = await fetch(`${base}/health`, { signal: AbortSignal.timeout(1_500) })
    return response.ok
  } catch {
    return false
  }
}

/** The enrolled device's token is the only proof that a listening server owns this workspace. */
async function workspaceAnswers(base: string, deviceId: string, token: string): Promise<boolean> {
  try {
    const response = await fetch(`${base}/v1/authoritative/projects`, {
      headers: { authorization: `Bearer ${token}`, 'x-orbit-device-id': deviceId },
      signal: AbortSignal.timeout(2_000),
    })
    return response.status === 200
  } catch {
    return false
  }
}

async function waitForHealth(base: string, deadlineMs = 30_000): Promise<void> {
  const started = Date.now()
  let last = 'no response'
  while (Date.now() - started < deadlineMs) {
    if (child && child.exitCode !== null) {
      throw new Error(`orbit-server exited ${child.exitCode} before becoming healthy.\n${await logTail(60)}`)
    }
    if (await healthy(base)) return
    last = 'not healthy'
    await delay(100)
  }
  throw new Error(`orbit-server did not become healthy at ${base}: ${last}\n${await logTail(60)}`)
}

async function logTail(lines = 80): Promise<string> {
  const buffered = tail.join('')
  if (buffered.trim().length > 0) return buffered.split('\n').slice(-lines).join('\n')
  return ''
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** Read the admin key out of a workspace state file (used by the second run). */
export async function readWorkspaceState(): Promise<WorkspaceState | null> {
  return readState()
}

/**
 * Open (or start) the persistent workspace.
 *
 * - Fresh directory: start a server, enroll one browser device, write the state.
 * - Existing directory, server not running: start a server against the same
 *   database with the stored admin key and device.
 * - Server already running for this workspace: reuse it; do not stop it on
 *   teardown (it outlives the run).
 */
export async function openWorkspace(): Promise<DogfoodWorkspace> {
  await mkdir(join(DOGFOOD_DIR, 'data'), { recursive: true })
  const previous = await readState()
  const resumed = previous !== null
  const base = DOGFOOD_BASE

  if (await healthy(base)) {
    if (
      !previous ||
      !(await workspaceAnswers(base, previous.deviceId, previous.token))
    ) {
      throw new Error(
        `a server is listening on ${base} but it does not answer as this workspace's enrolled device; ` +
          `refusing to dogfood against a workspace I do not own (set ORBIT_DOGFOOD_PORT to another port)`,
      )
    }
    return {
      base,
      dataDir: DOGFOOD_DIR,
      dbPath: DOGFOOD_DB,
      state: previous,
      owned: false,
      resumed,
      logTail,
      save: () => writeState(previous),
      stop: async () => undefined,
    }
  }

  const adminKey = previous?.adminKey ?? randomBytes(32).toString('hex')
  const logPath = join(DOGFOOD_DIR, 'server.log')
  log = createWriteStream(logPath, { flags: 'a' })
  const env: NodeJS.ProcessEnv = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (key.startsWith('ORBIT_')) continue
    env[key] = value
  }
  Object.assign(env, {
    ORBIT_SERVER_DB: DOGFOOD_DB,
    ORBIT_SERVER_BIND: `127.0.0.1:${DOGFOOD_PORT}`,
    ORBIT_SERVER_ADMIN_KEY: adminKey,
    ORBIT_WEB_DIST: WEB_DIST,
  })

  child = spawn(DEFAULT_SERVER_BIN, [], {
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
    cwd: REPO_ROOT,
  })
  const record = (chunk: Buffer) => {
    const text = chunk.toString()
    tail.push(text)
    if (tail.length > 400) tail.shift()
    log?.write(text)
  }
  child.stdout?.on('data', record)
  child.stderr?.on('data', record)
  await waitForHealth(base)

  let state = previous
  if (!state) {
    const deviceId = `orbit-dogfood-${randomBytes(4).toString('hex')}`
    const token = randomBytes(24).toString('hex')
    state = {
      createdAtMs: Date.now(),
      deviceId,
      token,
      adminKey,
      seededAtMs: null,
      seed: null,
      runs: [],
    }
    const enrolled = await fetch(`${base}/v1/devices`, {
      method: 'POST',
      headers: { authorization: `Bearer ${adminKey}`, 'content-type': 'application/json' },
      body: JSON.stringify({ device_id: deviceId, token }),
    })
    if (enrolled.status !== 201) {
      throw new Error(`device enrollment failed: ${enrolled.status} ${await enrolled.text()}`)
    }
    await writeState(state)
  }

  const workspace: DogfoodWorkspace = {
    base,
    dataDir: DOGFOOD_DIR,
    dbPath: DOGFOOD_DB,
    state,
    owned: true,
    resumed,
    logTail,
    save: () => writeState(state!),
    stop: async () => {
      if (!child) return
      const current = child
      child = null
      if (current.exitCode === null && current.signalCode === null) {
        current.kill('SIGTERM')
        const exited = await Promise.race([
          new Promise<boolean>((resolve) => current.once('exit', () => resolve(true))),
          delay(5_000).then(() => false),
        ])
        if (!exited) current.kill('SIGKILL')
      }
      await new Promise<void>((resolve) => log?.end(resolve) ?? resolve())
      log = null
    },
  }
  return workspace
}

export function workspaceCommit(): string {
  try {
    return execFileSync('git', ['rev-parse', '--short', 'HEAD'], { cwd: REPO_ROOT })
      .toString()
      .trim()
  } catch {
    return 'unknown'
  }
}
