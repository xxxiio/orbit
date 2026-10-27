/**
 * The connected capability cycle, driven for acceptance.
 *
 * Two kinds of caller, and they are deliberately different:
 *
 * - **Operator** routes (`capabilities/install`, `capabilities/authorize`) are
 *   `operator_device`: bearer credential plus `x-orbit-device-id`, never the
 *   browser's cookie. A signed-in browser is refused with
 *   `403 operator credential required`. So a spec installs and grants through
 *   this helper — there is no such control in the SPA, and a spec must not look
 *   for one.
 * - **Executor** routes (`capability-executors/announce`, `claim`, `result`)
 *   are `executor_device`: the same bearer credential, and a cookie is refused
 *   with `403 executor credential required`.
 *
 * Everything else here is a browser-legal read or write made with the browser's
 * own cookie jar (`page.request` + `x-orbit-csrf`), i.e. exactly the transport
 * the SPA uses. Nothing is mocked and nothing sleeps.
 *
 * The executor itself is the real CLI (`target/debug/orbit capability serve
 * --once --json`) against the real server and a throwaway replica file. It is
 * awaited with a deadline; a non-zero exit fails loudly with its stderr, never
 * with a retry.
 */
import { spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import type { Page } from '@playwright/test'
import { expect } from './fixtures'
import { REPO_ROOT, type E2EServer } from './server'
import { signInViaApi } from './session'

/** The one bundled capability the reference workflow declares. */
export const REFERENCE_CAPABILITY = 'orbit.reference.work-item-note'
/** Its only action: append a durable note to the item. */
export const REFERENCE_ACTION = 'append_note'
/** The workflow shipped in the repository that declares that action. */
export const REFERENCE_WORKFLOW = 'capability-reference'

const BASE = '/v1/authoritative'
const CSRF = { 'x-orbit-csrf': '1' }

/** One enrolled device that may act as an operator and/or an executor. */
export interface Bearer {
  deviceId: string
  token: string
}

export interface CallResult {
  status: number
  body: Record<string, any> | null
}

export function referenceBlueprint(): Record<string, any> {
  return JSON.parse(
    readFileSync(join(REPO_ROOT, 'blueprints', 'examples', 'capability-reference.json'), 'utf8'),
  ) as Record<string, any>
}

/* ------------------------------------------------------------------ */
/* Transport                                                           */
/* ------------------------------------------------------------------ */

async function jsonBody(response: {
  text(): Promise<string>
}): Promise<Record<string, any> | null> {
  const text = await response.text()
  if (text.length === 0) return null
  try {
    return JSON.parse(text) as Record<string, any>
  } catch {
    return { raw: text }
  }
}

/**
 * One POST through the browser's cookie jar. Passing credentials adds the
 * bearer pair the operator and executor routes demand; the cookie still rides
 * along, which is what makes the cookie-refusal assertions real.
 */
export async function postCapability(
  page: Page,
  server: E2EServer,
  path: string,
  body: unknown,
  credentials?: Bearer,
): Promise<CallResult> {
  const response = await page.request.post(`${server.base}${BASE}/${path}`, {
    data: body,
    headers: {
      ...CSRF,
      ...(credentials
        ? {
            authorization: `Bearer ${credentials.token}`,
            'x-orbit-device-id': credentials.deviceId,
          }
        : {}),
    },
  })
  return { status: response.status(), body: await jsonBody(response) }
}

export async function getCapability(
  page: Page,
  server: E2EServer,
  path: string,
): Promise<CallResult> {
  const response = await page.request.get(`${server.base}${BASE}/${path}`)
  return { status: response.status(), body: await jsonBody(response) }
}

/* ------------------------------------------------------------------ */
/* Operator writes (bearer only)                                       */
/* ------------------------------------------------------------------ */

/** Install the reference contract. It must happen before the workflow is published. */
export async function installReferenceCapability(
  page: Page,
  server: E2EServer,
  credentials: Bearer = server,
): Promise<CallResult> {
  return postCapability(
    page,
    server,
    'capabilities/install',
    { command_id: `install-${randomUUID()}`, capability_id: REFERENCE_CAPABILITY },
    credentials,
  )
}

/** Grant the installed capability to one project. `confirm` is mandatory. */
export async function authorizeCapability(
  page: Page,
  server: E2EServer,
  projectId: string,
  credentials: Bearer = server,
  capabilityId: string = REFERENCE_CAPABILITY,
): Promise<CallResult> {
  return postCapability(
    page,
    server,
    'capabilities/authorize',
    {
      command_id: `authorize-${randomUUID()}`,
      capability_id: capabilityId,
      project_id: projectId,
      confirm: true,
    },
    credentials,
  )
}

/* ------------------------------------------------------------------ */
/* Browser-legal writes                                                */
/* ------------------------------------------------------------------ */

/** Publish a workflow manifest. The reference one is read from the repository. */
export async function publishReferenceWorkflow(
  page: Page,
  server: E2EServer,
  definition: Record<string, any> = referenceBlueprint(),
): Promise<CallResult> {
  return postCapability(page, server, 'blueprints', { definition })
}

/** Create a project already pinned to a published workflow. */
export async function createCapabilityProject(
  page: Page,
  server: E2EServer,
  input: { projectId: string; name: string; workflowId?: string },
): Promise<CallResult> {
  return postCapability(page, server, 'project-commands', {
    command_id: `project-${randomUUID()}`,
    expected_revision: 0,
    project_id: input.projectId,
    name: input.name,
    workflow_id: input.workflowId ?? REFERENCE_WORKFLOW,
  })
}

/** Create one work item of the workflow's `note-task` kind, in `open`. */
export async function createWorkItem(
  page: Page,
  server: E2EServer,
  input: { itemId: string; projectId: string; kind?: string },
): Promise<CallResult> {
  return postCapability(page, server, 'blueprints/items', {
    id: input.itemId,
    project_id: input.projectId,
    kind: input.kind ?? 'note-task',
  })
}

/** Record an intent through the same route the SPA posts to. */
export async function requestCapabilityAction(
  page: Page,
  server: E2EServer,
  input: {
    commandId: string
    itemId: string
    expectedRevision: number
    actionId?: string
    inputs: Record<string, unknown>
    confirm?: boolean
  },
): Promise<CallResult> {
  return postCapability(page, server, 'capability-actions/request', {
    command_id: input.commandId,
    item_id: input.itemId,
    expected_revision: input.expectedRevision,
    action_id: input.actionId ?? REFERENCE_ACTION,
    inputs: input.inputs,
    confirm: input.confirm ?? true,
  })
}

/* ------------------------------------------------------------------ */
/* Executor writes (bearer only)                                       */
/* ------------------------------------------------------------------ */

export async function announceCapability(
  page: Page,
  server: E2EServer,
  credentials: Bearer = server,
  capabilityIds: string[] = [REFERENCE_CAPABILITY],
): Promise<CallResult> {
  return postCapability(
    page,
    server,
    'capability-executors/announce',
    { capability_ids: capabilityIds },
    credentials,
  )
}

export async function claimCapabilityAction(
  page: Page,
  server: E2EServer,
  input: { commandId: string; claimToken: string },
  credentials: Bearer = server,
): Promise<CallResult> {
  return postCapability(
    page,
    server,
    'capability-actions/claim',
    { command_id: input.commandId, claim_token: input.claimToken },
    credentials,
  )
}

export async function reportCapabilityResult(
  page: Page,
  server: E2EServer,
  input: {
    commandId: string
    claimToken: string
    outcome: 'succeeded' | 'failed' | 'uncertain'
    detail?: string
    effect?: Record<string, unknown> | null
  },
  credentials: Bearer = server,
): Promise<CallResult> {
  const body: Record<string, unknown> = {
    command_id: input.commandId,
    claim_token: input.claimToken,
    outcome: input.outcome,
    detail: input.detail ?? 'acceptance run',
  }
  if (input.effect) body.effect = input.effect
  return postCapability(page, server, 'capability-actions/result', body, credentials)
}

/** A declared `work_item_note` effect, shaped exactly as the contract requires. */
export function workItemNote(
  itemId: string,
  note: string,
  tag?: string,
): Record<string, unknown> {
  return {
    kind: 'work_item_note',
    item_id: itemId,
    note,
    ...(tag === undefined ? {} : { tag }),
  }
}

/* ------------------------------------------------------------------ */
/* Reads                                                               */
/* ------------------------------------------------------------------ */

export interface CapabilityOffer {
  action_id: string
  capability_id: string
  from: string
  to: string
  available: boolean
  code: string | null
  reason: string | null
  input_schema: Record<string, any>
}

export interface CapabilityStatusRecord {
  capability_id: string
  installed: boolean
  granted: boolean
  executors: Array<{ device_id: string; announced_at_ms: number }>
}

export interface CapabilityRequestRecord {
  command_id: string
  item_id: string
  item_revision: number
  action_id: string
  capability_id: string
  from: string
  to: string
  state: string
  inputs: Record<string, unknown>
  executor_device_id: string | null
}

export interface CapabilityReceiptRecord {
  outcome: string
  detail: any
  reported_at_ms: number
  reported_by: string
}

export interface WorkItemRecord {
  id: string
  project_id: string
  kind: string
  state: string
  revision: number
  properties: Record<string, unknown>
}

export interface EffectNote {
  command_id: string
  note: string
  tag: string | null
  created_at_ms: number
}

export async function readOffers(
  page: Page,
  server: E2EServer,
  itemId: string,
): Promise<{ item_revision: number; offers: CapabilityOffer[] }> {
  const { status, body } = await getCapability(
    page,
    server,
    `capability-actions?item_id=${encodeURIComponent(itemId)}`,
  )
  expect(status, `GET capability-actions?item_id=${itemId}`).toBe(200)
  return body as unknown as { item_revision: number; offers: CapabilityOffer[] }
}

export async function readOffer(
  page: Page,
  server: E2EServer,
  itemId: string,
  actionId: string = REFERENCE_ACTION,
): Promise<CapabilityOffer> {
  const { offers } = await readOffers(page, server, itemId)
  const offer = offers.find((candidate) => candidate.action_id === actionId)
  expect(offer, `the server must offer ${actionId} for ${itemId}`).toBeTruthy()
  return offer as CapabilityOffer
}

export async function readCapabilityStatus(
  page: Page,
  server: E2EServer,
  projectId: string,
): Promise<CapabilityStatusRecord[]> {
  const { status, body } = await getCapability(
    page,
    server,
    `capabilities?project_id=${encodeURIComponent(projectId)}`,
  )
  expect(status, `GET capabilities?project_id=${projectId}`).toBe(200)
  return (body as { capabilities: CapabilityStatusRecord[] }).capabilities
}

export async function readEffects(
  page: Page,
  server: E2EServer,
  itemId: string,
): Promise<EffectNote[]> {
  const { status, body } = await getCapability(
    page,
    server,
    `capability-effects?item_id=${encodeURIComponent(itemId)}`,
  )
  expect(status, `GET capability-effects?item_id=${itemId}`).toBe(200)
  return (body as { notes: EffectNote[] }).notes
}

export async function readShow(
  page: Page,
  server: E2EServer,
  commandId: string,
): Promise<{ request: CapabilityRequestRecord; receipt: CapabilityReceiptRecord | null }> {
  const { status, body } = await getCapability(
    page,
    server,
    `capability-actions/show?command_id=${encodeURIComponent(commandId)}`,
  )
  expect(status, `GET capability-actions/show?command_id=${commandId}`).toBe(200)
  return body as unknown as {
    request: CapabilityRequestRecord
    receipt: CapabilityReceiptRecord | null
  }
}

export async function readRequests(
  page: Page,
  server: E2EServer,
  itemId: string,
): Promise<CapabilityRequestRecord[]> {
  const { status, body } = await getCapability(
    page,
    server,
    `capability-actions/list?item_id=${encodeURIComponent(itemId)}`,
  )
  expect(status, `GET capability-actions/list?item_id=${itemId}`).toBe(200)
  return (body as { requests: CapabilityRequestRecord[] }).requests
}

/** The item as the server stores it — never as the sheet renders it. */
export async function readWorkItem(
  page: Page,
  server: E2EServer,
  itemId: string,
): Promise<WorkItemRecord> {
  const { status, body } = await getCapability(
    page,
    server,
    `blueprints/items?id=${encodeURIComponent(itemId)}`,
  )
  expect(status, `GET blueprints/items?id=${itemId}`).toBe(200)
  return body as unknown as WorkItemRecord
}

/* ------------------------------------------------------------------ */
/* One workspace built entirely through the product's own HTTP surface */
/* ------------------------------------------------------------------ */

export interface CapabilityWorkspace {
  projectId: string
  itemId: string
  /** A second item in the same project, for a parallel request. */
  secondItemId: string
}

export interface BuildOptions {
  /** Install the contract before publishing. Defaults to `true`. */
  install?: boolean
  /** Grant the capability to the project. Defaults to `true`. */
  authorize?: boolean
  /** Announce the harness device as the executor. Defaults to `true`. */
  announce?: boolean
}

/**
 * install → publish → project → item, in the order the server permits. A
 * publish before the install is refused, so the order is not a convention: it
 * is the server's rule.
 *
 * The browser signs in first: publishing, creating the project and creating the
 * item go through the browser's own cookie jar, exactly like the SPA.
 */
export async function buildCapabilityWorkspace(
  page: Page,
  server: E2EServer,
  options: BuildOptions = {},
): Promise<CapabilityWorkspace> {
  const suffix = randomUUID().slice(0, 8)
  const projectId = `cap-project-${suffix}`
  const itemId = `cap-item-${suffix}`
  const secondItemId = `cap-item-${suffix}-b`

  await signInViaApi(page, server)

  if (options.install !== false) {
    const installed = await installReferenceCapability(page, server)
    expect(installed.status, `POST capabilities/install: ${JSON.stringify(installed.body)}`).toBe(
      200,
    )
  }
  const published = await publishReferenceWorkflow(page, server)
  expect(published.status, `POST blueprints: ${JSON.stringify(published.body)}`).toBe(200)

  const project = await createCapabilityProject(page, server, {
    projectId,
    name: `Capability ${suffix}`,
  })
  expect(project.status, `POST project-commands: ${JSON.stringify(project.body)}`).toBe(200)

  const item = await createWorkItem(page, server, { itemId, projectId })
  expect(item.status, `POST blueprints/items: ${JSON.stringify(item.body)}`).toBe(200)

  if (options.authorize !== false) {
    const grant = await authorizeCapability(page, server, projectId)
    expect(grant.status, `POST capabilities/authorize: ${JSON.stringify(grant.body)}`).toBe(200)
  }
  if (options.announce !== false) {
    const announced = await announceCapability(page, server)
    expect(
      announced.status,
      `POST capability-executors/announce: ${JSON.stringify(announced.body)}`,
    ).toBe(200)
  }
  return { projectId, itemId, secondItemId }
}

/* ------------------------------------------------------------------ */
/* The real executor                                                   */
/* ------------------------------------------------------------------ */

const EXECUTOR_BIN = process.env.ORBIT_CLI_BIN ?? join(REPO_ROOT, 'target', 'debug', 'orbit')
const EXECUTOR_TIMEOUT_MS = 90_000

export interface ExecutorRun {
  exitCode: number
  /** The whole stdout, for a failure message that can be read. */
  stdout: string
  stderr: string
  /** The CLI's `{"type":"authority","result":{…}}` envelope. */
  response: Record<string, any>
  /** `response.result`: `status`, `outcome`, `state`, `receipt`, … */
  result: Record<string, any>
}

/**
 * `orbit capability serve --once --json` against this harness's server, as one
 * enrolled device, with a replica file of its own.
 *
 * The process is awaited with a deadline instead of polled: it exits when it
 * has handled (or found) exactly one request. A non-zero exit or an unreadable
 * answer throws with stderr, so a broken executor fails the spec loudly rather
 * than as a timeout.
 */
export async function runExecutor(
  server: E2EServer,
  credentials: Bearer = server,
  options: { replica?: string } = {},
): Promise<ExecutorRun> {
  const replica = options.replica ?? join(server.dataDir, `executor-${randomUUID()}.db`)
  const env: NodeJS.ProcessEnv = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (key.startsWith('ORBIT_')) continue
    env[key] = value
  }
  Object.assign(env, {
    ORBIT_AUTHORITY_SERVER: server.base,
    ORBIT_SYNC_HTTP_TOKEN: credentials.token,
    ORBIT_AUTHORITY_DEVICE: credentials.deviceId,
    // A throwaway replica: the executor never reads the authority's database.
    ORBIT_AUTHORITY_DB: replica,
  })

  const run = await new Promise<{ code: number | null; stdout: string; stderr: string; timedOut: boolean }>(
    (resolve) => {
      const child = spawn(EXECUTOR_BIN, ['capability', 'serve', '--once', '--json'], {
        env,
        cwd: REPO_ROOT,
        stdio: ['ignore', 'pipe', 'pipe'],
      })
      let stdout = ''
      let stderr = ''
      let timedOut = false
      const timer = setTimeout(() => {
        timedOut = true
        child.kill('SIGKILL')
      }, EXECUTOR_TIMEOUT_MS)
      child.stdout?.on('data', (chunk: Buffer) => {
        stdout += chunk.toString()
      })
      child.stderr?.on('data', (chunk: Buffer) => {
        stderr += chunk.toString()
      })
      child.once('error', (error) => {
        clearTimeout(timer)
        resolve({ code: -1, stdout, stderr: `${stderr}\nspawn failed: ${error.message}`, timedOut })
      })
      child.once('close', (code) => {
        clearTimeout(timer)
        resolve({ code, stdout, stderr, timedOut })
      })
    },
  )

  if (run.timedOut) {
    throw new Error(
      `orbit capability serve did not exit within ${EXECUTOR_TIMEOUT_MS}ms.\nstdout:\n${run.stdout}\nstderr:\n${run.stderr}`,
    )
  }
  if (run.code !== 0) {
    throw new Error(
      `orbit capability serve exited ${run.code}.\nstdout:\n${run.stdout}\nstderr:\n${run.stderr}`,
    )
  }
  let response: Record<string, any>
  try {
    response = JSON.parse(run.stdout) as Record<string, any>
  } catch (error) {
    throw new Error(
      `orbit capability serve printed no JSON (${(error as Error).message}).\nstdout:\n${run.stdout}\nstderr:\n${run.stderr}`,
    )
  }
  const result = (response.result ?? response) as Record<string, any>
  return { exitCode: run.code, stdout: run.stdout, stderr: run.stderr, response, result }
}
