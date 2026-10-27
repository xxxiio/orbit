/**
 * A deterministic fake executor for the agent-launch acceptance specs.
 *
 * There is no model and no Herdr in this path. The server's machine fence is
 * `job.machine_ref == authenticated device id`, so the device that owns the
 * browser session *is* a real executor once it posts the `agent-machines/*`
 * routes — the fixture seeds its jobs with exactly that id
 * (`web/tests/fixture/seed.ts`, `machineRef: server.deviceId`). No id is
 * invented here.
 *
 * Every call goes through `postAuthority`/`getAuthority`, i.e. the browser's own
 * cookie jar and the same `x-orbit-csrf` header the SPA sends: the transport is
 * the product's, not a shortcut around it. Nothing is mocked and nothing sleeps.
 *
 * The write helpers mirror the canonical recipe in `tests/fixture/seed.ts`
 * (start → claim → launch-result → observe → handoffs → review) and return
 * `{ job_id, revision }` so a spec can feed one step's revision into the next.
 */
import type { Page } from '@playwright/test'
import { getAuthority, postAuthority } from './authority'
import { expect } from './fixtures'
import type { E2EServer } from './server'

const JOBS = 'agent-jobs'
const MACHINES = 'agent-machines'

/** One accepted step: the job it touched and the revision the server answered. */
export interface JobHandle {
  job_id: string
  revision: number
}

export interface JobView {
  id: string
  task_id: string
  project_id: string
  state: string
  revision: number
  agent_kind: string
  machine_ref: string
  repository_ref: string
  runtime_session_ref: string | null
  created_at_ms: number
  updated_at_ms: number
  /** `null` means "no machine has claimed it"; the key is always present. */
  claim_state: string | null
}

export interface OverviewItem {
  job_id: string
  project_id: string
  task_id: string
  state: string
  updated_at_ms: number
  handoff_count: number
  review_decision: string | null
  needs_attention: boolean
  next_action: string
}

export interface ExecutorRecord {
  device_id: string
  active: boolean
  last_claim_at_ms: number | null
  claimed_jobs: number
  preparing_jobs: number
}

export interface HandoffRecord {
  handoff_id: string
  reported_at_ms: number
  payload: Record<string, unknown>
}

export interface ReviewRecord {
  job_id: string
  handoff_id: string
  decision: string
  reviewer: string
  reason: string
  reviewed_at_ms: number
  task_completed: boolean
}

export interface ResumeContextRecord {
  job: JobView
  original_brief: Record<string, unknown>
  latest_handoff: HandoffRecord | null
  handoff_count: number
  review: ReviewRecord | null
  worktree: unknown
  task_completion_inferred: boolean
  runtime_launch_authorized: boolean
}

/* ------------------------------------------------------------------ */
/* Reads                                                               */
/* ------------------------------------------------------------------ */

export async function readJob(
  page: Page,
  server: E2EServer,
  jobId: string,
): Promise<JobView> {
  const { status, body } = await getAuthority(
    page,
    server,
    `${JOBS}?id=${encodeURIComponent(jobId)}`,
  )
  expect(status, `GET ${JOBS}?id=${jobId}`).toBe(200)
  expect(body, `job ${jobId} must be visible to this device`).toBeTruthy()
  return body as unknown as JobView
}

export async function readOverview(page: Page, server: E2EServer): Promise<OverviewItem[]> {
  const { status, body } = await getAuthority(page, server, `${JOBS}/overview`)
  expect(status, `GET ${JOBS}/overview`).toBe(200)
  return body as unknown as OverviewItem[]
}

export async function readExecutors(
  page: Page,
  server: E2EServer,
): Promise<ExecutorRecord[]> {
  const { status, body } = await getAuthority(page, server, 'agent-executors')
  expect(status, 'GET agent-executors').toBe(200)
  return (body as { executors: ExecutorRecord[] }).executors
}

export async function readHandoffs(
  page: Page,
  server: E2EServer,
  jobId: string,
): Promise<HandoffRecord[]> {
  const { status, body } = await getAuthority(
    page,
    server,
    `${JOBS}/handoffs?id=${encodeURIComponent(jobId)}`,
  )
  expect(status, `GET ${JOBS}/handoffs?id=${jobId}`).toBe(200)
  return body as unknown as HandoffRecord[]
}

export async function readReview(
  page: Page,
  server: E2EServer,
  jobId: string,
): Promise<ReviewRecord | null> {
  const { status, body } = await getAuthority(
    page,
    server,
    `${JOBS}/review?id=${encodeURIComponent(jobId)}`,
  )
  expect(status, `GET ${JOBS}/review?id=${jobId}`).toBe(200)
  return body as unknown as ReviewRecord | null
}

export async function readResumeContext(
  page: Page,
  server: E2EServer,
  jobId: string,
): Promise<ResumeContextRecord> {
  const { status, body } = await getAuthority(
    page,
    server,
    `${JOBS}/resume-context?id=${encodeURIComponent(jobId)}`,
  )
  expect(status, `GET ${JOBS}/resume-context?id=${jobId}`).toBe(200)
  return body as unknown as ResumeContextRecord
}

/* ------------------------------------------------------------------ */
/* Owner route: queue a job                                            */
/* ------------------------------------------------------------------ */

export interface StartJobInput {
  jobId: string
  taskId: string
  /**
   * The executor. Defaults to the harness device, which is the enrolled device
   * this browser is signed in as — the same id the server fences on.
   */
  machineRef?: string
  agentKind?: 'codex' | 'opencode'
  repositoryRef?: string
  worktree?: boolean
  objective?: string
  atMs: number
}

/**
 * Queue one job. The server creates it in `preparing` with no claim: nothing
 * starts until an executor posts a claim, and the Orbit task does not move.
 */
export async function startJob(
  page: Page,
  server: E2EServer,
  input: StartJobInput,
): Promise<JobHandle> {
  const { status, body } = await postAuthority(page, server, `${JOBS}/start`, {
    job_id: input.jobId,
    task_id: input.taskId,
    agent_kind: input.agentKind ?? 'codex',
    machine_ref: input.machineRef ?? server.deviceId,
    repository_ref: input.repositoryRef ?? 'orbit',
    worktree: input.worktree ?? false,
    at_ms: input.atMs,
    ...(input.objective === undefined ? {} : { objective: input.objective }),
  })
  expect(status, `POST ${JOBS}/start for ${input.jobId}`).toBe(200)
  const job = body as unknown as JobView
  return { job_id: job.id, revision: job.revision }
}

/* ------------------------------------------------------------------ */
/* Machine routes: the executor side                                   */
/* ------------------------------------------------------------------ */

export interface ClaimInput {
  jobId: string
  /** A freshly started job is at revision 1. */
  expectedRevision?: number
  claimToken: string
  atMs: number
}

/** Claim a prepared job: the durable claim the browser must never retry blindly. */
export async function claim(
  page: Page,
  server: E2EServer,
  input: ClaimInput,
): Promise<JobHandle> {
  const { status, body } = await postAuthority(page, server, `${MACHINES}/claim`, {
    job_id: input.jobId,
    expected_revision: input.expectedRevision ?? 1,
    claim_token: input.claimToken,
    at_ms: input.atMs,
  })
  expect(status, `POST ${MACHINES}/claim for ${input.jobId}`).toBe(200)
  const job = (body as { job: JobView }).job
  return { job_id: job.id, revision: job.revision }
}

export interface LaunchInput {
  jobId: string
  claimToken: string
  /** `null` records an ambiguous launch: the claim state a person must not retry. */
  sessionRef: string | null
  atMs: number
}

export async function reportLaunch(
  page: Page,
  server: E2EServer,
  input: LaunchInput,
): Promise<JobHandle> {
  const { status, body } = await postAuthority(page, server, `${MACHINES}/launch-result`, {
    job_id: input.jobId,
    claim_token: input.claimToken,
    session_ref: input.sessionRef,
    at_ms: input.atMs,
  })
  expect(status, `POST ${MACHINES}/launch-result for ${input.jobId}`).toBe(200)
  const job = body as unknown as JobView
  return { job_id: job.id, revision: job.revision }
}

export interface ObserveInput {
  jobId: string
  expectedRevision: number
  claimToken: string
  sessionRef: string
  activity: 'working' | 'waiting_input' | 'blocked' | 'idle' | 'unreachable' | 'absent'
  atMs: number
}

/** Report what the runtime is doing. `working` is what moves a job to `running`. */
export async function observe(
  page: Page,
  server: E2EServer,
  input: ObserveInput,
): Promise<JobHandle> {
  const { status, body } = await postAuthority(page, server, `${MACHINES}/observe`, {
    job_id: input.jobId,
    expected_revision: input.expectedRevision,
    claim_token: input.claimToken,
    session_ref: input.sessionRef,
    activity: input.activity,
    at_ms: input.atMs,
  })
  expect(status, `POST ${MACHINES}/observe for ${input.jobId}`).toBe(200)
  const job = body as unknown as JobView
  return { job_id: job.id, revision: job.revision }
}

export interface HandoffInput {
  jobId: string
  handoffId: string
  claimToken: string
  sessionRef: string
  /** Must be an object with `summary`, `validation` and `next_steps`. */
  payload: { summary: string; validation: string; next_steps: string[] }
  atMs: number
}

/** Submit evidence from the executor. This is not a review and completes nothing. */
export async function submitHandoff(
  page: Page,
  server: E2EServer,
  input: HandoffInput,
): Promise<JobHandle> {
  const { status } = await postAuthority(page, server, `${MACHINES}/handoff`, {
    job_id: input.jobId,
    claim_token: input.claimToken,
    session_ref: input.sessionRef,
    handoff_id: input.handoffId,
    payload: input.payload,
    at_ms: input.atMs,
  })
  expect(status, `POST ${MACHINES}/handoff for ${input.jobId}`).toBe(200)
  // The route answers the handoff list, so the revision comes from reading the
  // job the server just kept — never from a guess.
  const job = await readJob(page, server, input.jobId)
  return { job_id: job.id, revision: job.revision }
}

/* ------------------------------------------------------------------ */
/* Owner routes: interrupt and review                                  */
/* ------------------------------------------------------------------ */

export interface InterruptInput {
  jobId: string
  expectedRevision: number
  commandId?: string
  atMs: number
}

/**
 * Record a terminal job through the owner command route. `start` reserves
 * queued at 0 and accepts prepare at 1, so an interrupted record is fenced on
 * the state the start route left behind (see the fixture's canonical recipe).
 */
export async function interrupt(
  page: Page,
  server: E2EServer,
  input: InterruptInput,
): Promise<JobHandle> {
  const { status, body } = await postAuthority(page, server, `${JOBS}/command`, {
    command_id: input.commandId ?? `${input.jobId}:interrupt`,
    job_id: input.jobId,
    expected_revision: input.expectedRevision,
    action: 'interrupt',
    at_ms: input.atMs,
  })
  expect(status, `POST ${JOBS}/command interrupt for ${input.jobId}`).toBe(200)
  const job = body as unknown as JobView
  return { job_id: job.id, revision: job.revision }
}

export interface ReviewInput {
  jobId: string
  handoffId: string
  decision: 'approved' | 'changes_requested'
  reason: string
  atMs: number
}

/**
 * Record a review decision. It needs a persisted handoff, is immutable, and the
 * server answers `task_completed: false`: approving never moves the Orbit task.
 */
export async function review(
  page: Page,
  server: E2EServer,
  input: ReviewInput,
): Promise<JobHandle> {
  const { status } = await postAuthority(page, server, `${JOBS}/review`, {
    job_id: input.jobId,
    handoff_id: input.handoffId,
    decision: input.decision,
    reason: input.reason,
    at_ms: input.atMs,
  })
  expect(status, `POST ${JOBS}/review for ${input.jobId}`).toBe(200)
  const job = await readJob(page, server, input.jobId)
  return { job_id: job.id, revision: job.revision }
}

/* ------------------------------------------------------------------ */
/* Clock                                                               */
/* ------------------------------------------------------------------ */

/**
 * One monotonic clock for one job. The authority refuses an `at_ms` that goes
 * backwards, and the browser's clock is pinned to the fixture's, so every step
 * is derived from the job's own `created_at_ms` instead of wall time.
 */
export function jobClock(startMs: number, stepMs = 1_000): { next: () => number } {
  let current = startMs
  return {
    next: () => {
      current += stepMs
      return current
    },
  }
}
