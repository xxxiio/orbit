/**
 * Real `orbit-server` process for acceptance tests.
 *
 * Each harness instance owns:
 *  - a free loopback port,
 *  - a throwaway directory `${os.tmpdir()}/orbit-web-e2e/<uuid>/` holding
 *    `data/server.db` (authoritative SQLite state, its knowledge vault and the
 *    attachment object store) and `server.log`,
 *  - one enrolled browser device whose credentials the specs use.
 *
 * The app under test is the *built* SPA served by that same server
 * (`ORBIT_WEB_DIST=web/dist`). That is deliberate: the API has no CORS layer, so
 * a separate origin (vite dev/preview) cannot call it without a proxy. Serving
 * the real build from the real server is both simpler and closer to production.
 *
 * No mocked API, no fixed sleeps: readiness is `GET /health` polled until a
 * deadline, and failures surface the tail of `server.log`.
 */
import { spawn, type ChildProcessByStdio } from 'node:child_process'
import { randomBytes, randomUUID } from 'node:crypto'
import { createWriteStream, type WriteStream } from 'node:fs'
import { mkdir, readFile, rm, stat } from 'node:fs/promises'
import { createServer } from 'node:net'
import type { Readable } from 'node:stream'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

/** `web/tests/e2e/harness/server.ts` → repository root. */
export const REPO_ROOT = fileURLToPath(new URL('../../../../', import.meta.url))
export const WEB_DIST = join(REPO_ROOT, 'web', 'dist')
export const DEFAULT_SERVER_BIN = join(REPO_ROOT, 'target', 'debug', 'orbit-server')

/** `spawn` with stdin ignored and stdout/stderr piped. */
type ServerProcess = ChildProcessByStdio<null, Readable, Readable>

export interface E2EServer {
  /** `http://127.0.0.1:<port>` — the app and the API share this origin. */
  base: string
  /** Enrolled device id (browser credentials). */
  deviceId: string
  /** Enrolled device token (≥32 chars; also valid as `Authorization: Bearer`). */
  token: string
  /** Server admin key, for admin-only endpoints (`/v1/devices`). */
  adminKey: string
  /** Temp directory that holds `data/` and `server.log`. */
  dataDir: string
  /** Last lines of `server.log`; safe to call after exit. */
  logTail(lines?: number): Promise<string>
  /** Terminate and reap the process, then delete the temp directory. */
  stop(): Promise<void>
}

export interface StartServerOptions {
  /**
   * Extra environment for the server. `ORBIT_*` variables from the caller's
   * environment are always stripped first, so a developer's real configuration
   * cannot leak into the test database.
   */
  env?: Record<string, string>
  /** Deadline for `GET /health` to answer 200. Defaults to 30s. */
  healthTimeoutMs?: number
  /** Override the binary (CI builds it elsewhere). */
  binary?: string
}

const CLEAN_ENV_PREFIX = 'ORBIT_'

export async function startServer(options: StartServerOptions = {}): Promise<E2EServer> {
  const binary = options.binary ?? process.env.ORBIT_SERVER_BIN ?? DEFAULT_SERVER_BIN
  await assertBinary(binary)

  const port = await findFreePort()
  const base = `http://127.0.0.1:${port}`
  const dataDir = join(tmpdir(), 'orbit-web-e2e', randomUUID())
  const dbPath = join(dataDir, 'data', 'server.db')
  const logPath = join(dataDir, 'server.log')
  await mkdir(join(dataDir, 'data'), { recursive: true })

  const adminKey = randomBytes(32).toString('hex')
  const env = cleanEnv({
    ...options.env,
    ORBIT_SERVER_DB: dbPath,
    ORBIT_SERVER_BIND: `127.0.0.1:${port}`,
    ORBIT_SERVER_ADMIN_KEY: adminKey,
    ORBIT_WEB_DIST: WEB_DIST,
  })

  const log = createWriteStream(logPath, { flags: 'a' })
  const child: ServerProcess = spawn(binary, [], {
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
    cwd: REPO_ROOT,
  })
  // Keep a small in-memory tail as well as the file: a process that dies before
  // startup has flushed its stdio would otherwise produce an empty log exactly
  // when the reason matters most.
  const tail: string[] = []
  const record = (chunk: Buffer) => {
    const text = chunk.toString()
    tail.push(text)
    if (tail.length > 200) tail.shift()
    log.write(text)
  }
  child.stdout.on('data', record)
  child.stderr.on('data', record)

  let stopping = false
  const server: E2EServer = {
    base,
    deviceId: `e2e-${randomUUID().slice(0, 8)}-browser`,
    token: randomBytes(24).toString('hex'),
    adminKey,
    dataDir,
    logTail: async (lines = 80) => {
      const buffered = tail.join('')
      if (buffered.trim().length > 0) return lastLines(buffered, lines)
      return readLogTail(logPath, lines)
    },
    stop: async () => {
      if (stopping) return
      stopping = true
      await stopChild(child, log)
      await removeState(dataDir)
    },
  }

  try {
    await waitForHealth(base, options.healthTimeoutMs ?? 30_000, child, () => server.logTail(80))
    await enrollDeviceWithAdminKey(server, { deviceId: server.deviceId, token: server.token })
  } catch (error) {
    await server.stop()
    throw error
  }

  const exitGuard = () => {
    if (stopping) return
    // A crash after startup must not be silent: it surfaces as a connection
    // error in the next call, so print the reason while it is still fresh.
    void server.logTail(40).then((excerpt) => {
      if (excerpt.trim().length > 0) {
        console.warn(`orbit-server exited unexpectedly:\n${excerpt}`)
      }
    })
  }
  child.once('exit', exitGuard)

  return server
}

async function assertBinary(binary: string): Promise<void> {
  try {
    const info = await stat(binary)
    if (!info.isFile()) throw new Error('not a regular file')
  } catch {
    throw new Error(
      [
        `orbit-server binary not found at ${binary}.`,
        'Build it first:',
        `  cargo build -p orbit-server`,
        'or point ORBIT_SERVER_BIN at an existing binary.',
      ].join('\n'),
    )
  }
}

/** Bind 127.0.0.1:0 to let the kernel choose, then release the port. */
function findFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer()
    probe.unref()
    probe.once('error', reject)
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address()
      const port = typeof address === 'object' && address !== null ? address.port : 0
      probe.close(() => (port > 0 ? resolve(port) : reject(new Error('no free port found'))))
    })
  })
}

function cleanEnv(overrides: Record<string, string>): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (key.startsWith(CLEAN_ENV_PREFIX)) continue
    env[key] = value
  }
  for (const [key, value] of Object.entries(overrides)) {
    if (value === '') delete env[key]
    else env[key] = value
  }
  return env
}

async function waitForHealth(
  base: string,
  deadlineMs: number,
  child: ServerProcess,
  logTail: () => Promise<string>,
): Promise<void> {
  const started = Date.now()
  let lastError = 'no response yet'
  while (Date.now() - started < deadlineMs) {
    if (child.exitCode !== null) {
      throw new Error(
        `orbit-server exited with code ${child.exitCode} before becoming healthy.\n${await logTail()}`,
      )
    }
    try {
      const response = await fetch(`${base}/health`, { signal: AbortSignal.timeout(2_000) })
      if (response.ok) return
      lastError = `GET /health → ${response.status}`
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error)
    }
    await delay(50)
  }
  throw new Error(
    `orbit-server did not become healthy at ${base} within ${deadlineMs}ms (${lastError}).\n${await logTail()}`,
  )
}

/** Credentials for one enrolled browser device. */
export interface DeviceCredentials {
  deviceId: string
  /** The server requires at least 32 characters. */
  token: string
}

/** Enroll an additional browser device through the admin API. */
export async function enrollDeviceWithAdminKey(
  server: E2EServer,
  device: DeviceCredentials,
): Promise<void> {
  const response = await fetch(`${server.base}/v1/devices`, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${server.adminKey}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify({ device_id: device.deviceId, token: device.token }),
  })
  if (response.status !== 201) {
    throw new Error(
      `device enrollment failed: POST /v1/devices → ${response.status} ${await response.text()}\n${await server.logTail()}`,
    )
  }
}

/** Revoke a device with the admin key; live browser sessions die with it. */
export async function revokeDevice(server: E2EServer, deviceId: string): Promise<void> {
  const response = await fetch(`${server.base}/v1/devices/${encodeURIComponent(deviceId)}`, {
    method: 'DELETE',
    headers: { authorization: `Bearer ${server.adminKey}` },
  })
  if (response.status !== 204) {
    throw new Error(
      `device revoke failed: DELETE /v1/devices/${deviceId} → ${response.status} ${await response.text()}`,
    )
  }
}

async function stopChild(
  child: ServerProcess,
  log: WriteStream,
): Promise<void> {
  if (child.exitCode === null && child.signalCode === null) {
    child.kill('SIGTERM')
    const exited = await waitForExit(child, 5_000)
    if (!exited) {
      child.kill('SIGKILL')
      await waitForExit(child, 2_000)
    }
  }
  await new Promise<void>((resolve) => log.end(resolve))
}

function waitForExit(child: ServerProcess, deadlineMs: number): Promise<boolean> {
  return new Promise((resolve) => {
    if (child.exitCode !== null || child.signalCode !== null) {
      resolve(true)
      return
    }
    const timer = setTimeout(() => {
      child.removeListener('exit', onExit)
      resolve(false)
    }, deadlineMs)
    const onExit = () => {
      clearTimeout(timer)
      resolve(true)
    }
    child.once('exit', onExit)
  })
}

async function removeState(dataDir: string): Promise<void> {
  if (process.env.ORBIT_KEEP_E2E_STATE === '1') return
  await rm(dataDir, { recursive: true, force: true }).catch(() => undefined)
}

async function readLogTail(logPath: string, lines: number): Promise<string> {
  const raw = await readFile(logPath, 'utf8').catch(() => '')
  return raw.length === 0 ? '' : lastLines(raw, lines)
}

function lastLines(raw: string, lines: number): string {
  return raw.split('\n').slice(-lines).join('\n')
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
