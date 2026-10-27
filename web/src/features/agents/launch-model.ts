/**
 * Launch vocabulary: who may be queued, which device can claim it, and what the
 * browser is allowed to promise.
 *
 * Everything here is presentation and form state. None of it decides that a job
 * may start: the authority re-checks the task status, the prerequisites, the
 * one-active-job rule and the executor enrolment, and its refusal is what the
 * dialog shows. Nothing in this module invents progress, and nothing in it calls
 * an `agent-machines/*` route — the browser queues, a machine claims.
 */
import type { AgentExecutor, AgentJobOverview } from '@/api/adapters'
import { userMessage, type ApiError } from '@/api/errors'
import type { TaskStatus } from '@/api/contracts'
import { isAgentJobTerminal } from './agent-model'

/* ------------------------------------------------------------------ */
/* Eligibility                                                         */
/* ------------------------------------------------------------------ */

/**
 * The statuses the storage gate accepts (`start_agent_job_for_device`). Kept in
 * the client only to explain why a launch control is off; the server remains the
 * judge, and its refusal is quoted verbatim when the two disagree.
 */
export const LAUNCHABLE_TASK_STATUSES = ['ready', 'in_progress', 'review'] as const

export type LaunchableTaskStatus = (typeof LAUNCHABLE_TASK_STATUSES)[number]

export function isLaunchableTaskStatus(status: string): status is LaunchableTaskStatus {
  return (LAUNCHABLE_TASK_STATUSES as readonly string[]).includes(status)
}

/** The one job the server would refuse a second start for. */
export interface ActiveAgentJob {
  jobId: string
  state: string
}

export interface LaunchEligibility {
  eligible: boolean
  /** Why the control is off, named. `null` when nothing on this side blocks it. */
  reason: string | null
}

/** The task's non-terminal job, if any (the overview is already owner-scoped). */
export function activeJobForTask(
  jobs: readonly Pick<AgentJobOverview, 'jobId' | 'taskId' | 'state'>[],
  taskId: string,
): ActiveAgentJob | null {
  const job = jobs.find((candidate) => candidate.taskId === taskId && !isAgentJobTerminal(candidate.state))
  return job ? { jobId: job.jobId, state: job.state } : null
}

export function launchEligibility({
  status,
  activeJob,
}: {
  status: TaskStatus | string
  activeJob: ActiveAgentJob | null
}): LaunchEligibility {
  if (!isLaunchableTaskStatus(status)) {
    return {
      eligible: false,
      reason: `Only a task that is ready, in progress or in review can be queued. This task is ${status.replace(/_/g, ' ')}.`,
    }
  }
  if (activeJob) {
    return {
      eligible: false,
      reason: `This task already has an active job in ${activeJob.state.replace(/_/g, ' ')}. The server allows one active job per task.`,
    }
  }
  return { eligible: true, reason: null }
}

/* ------------------------------------------------------------------ */
/* Executors (C10)                                                     */
/* ------------------------------------------------------------------ */

/**
 * The executor read rendered as one of six answers. `empty` and `unsupported`
 * are deliberately different: an empty list is the server telling us there is no
 * executor, while a 404 is a server that cannot answer at all.
 */
export type ExecutorReadState =
  | { kind: 'loading'; active: AgentExecutor[]; all: AgentExecutor[] }
  | { kind: 'unsupported'; active: AgentExecutor[]; all: AgentExecutor[] }
  | { kind: 'failed'; active: AgentExecutor[]; all: AgentExecutor[]; message: string }
  | { kind: 'empty'; active: AgentExecutor[]; all: AgentExecutor[] }
  | { kind: 'inactive'; active: AgentExecutor[]; all: AgentExecutor[] }
  | { kind: 'ready'; active: AgentExecutor[]; all: AgentExecutor[] }

export function executorReadState({
  pending,
  error,
  executors,
}: {
  pending: boolean
  error: ApiError | null
  executors: AgentExecutor[] | undefined
}): ExecutorReadState {
  if (error) {
    if (error.status === 404) {
      return { kind: 'unsupported', active: [], all: [] }
    }
    return { kind: 'failed', active: [], all: [], message: userMessage(error) }
  }
  if (pending && !executors) {
    return { kind: 'loading', active: [], all: [] }
  }
  const all = executors ?? []
  const active = all.filter((executor) => executor.active)
  if (all.length === 0) return { kind: 'empty', active, all }
  if (active.length === 0) return { kind: 'inactive', active, all }
  return { kind: 'ready', active, all }
}

/**
 * Why launching is off, in the words that name the missing thing. `null` means
 * an executor is known and the launch control may be enabled.
 */
export function executorBlocker(state: ExecutorReadState): string | null {
  switch (state.kind) {
    case 'ready':
      return null
    case 'loading':
      return 'Checking which devices are enrolled…'
    case 'unsupported':
      return 'This Orbit server does not offer the executor list, so this page cannot confirm that an executor is enrolled. Launching is disabled here; update the server and reload.'
    case 'failed':
      return `The executor list could not be read, so launching is disabled: ${state.message}`
    case 'empty':
      return 'No executor is enrolled. A queued job would wait for a claim that never comes, so launching is disabled. Enrol a device as an executor first, then reload this page.'
    case 'inactive':
      return 'Every enrolled executor is revoked. A queued job would never be claimed, so launching is disabled. Re-enrol a device as an executor first.'
  }
}

/** One line about what the chosen device is doing, from the server's counts. */
export function executorSummary(executor: AgentExecutor | null | undefined): string | null {
  if (!executor) return null
  if (executor.claimedJobs === 0 && executor.preparingJobs === 0) return 'no jobs assigned'
  const jobs = `${executor.claimedJobs} claimed job${executor.claimedJobs === 1 ? '' : 's'}`
  const preparing = `${executor.preparingJobs} preparing`
  return `${jobs} · ${preparing}`
}

/**
 * The roster's own fact: this device has no claim on record. It is a warning,
 * not a refusal — the roster cannot prove that no runner is active there, so
 * hiding the device would be as dishonest as pretending a queued job has begun.
 */
export function hasNeverClaimed(executor: Pick<AgentExecutor, 'lastClaimAtMs'>): boolean {
  return executor.lastClaimAtMs === null
}

/** The flag itself, shown in the panel and beside the picker. */
export const NEVER_CLAIMED_LABEL = 'has never claimed a job'

/** The one-line explanation that goes with the flag. */
export const NEVER_CLAIMED_EXPLANATION =
  'A job queued for it waits at Preparing until something on that device claims it.'

/**
 * The confirmation's warning, which names the device that was chosen. It states
 * what a queue request can and cannot promise; it never blocks the launch.
 */
export function neverClaimedConfirmation(deviceId: string): { title: string; detail: string } {
  return {
    title: 'This device has never claimed a job.',
    detail: `${deviceId} has not claimed a job before, so this roster cannot show whether a runner is active on it. If none is, the job waits at Preparing indefinitely, and Preparing does not mean work has started. The job stays queued until something on that device claims it.`,
  }
}

/* ------------------------------------------------------------------ */
/* The pinned launch attempt                                           */
/* ------------------------------------------------------------------ */

export const AGENT_KINDS = ['codex', 'opencode'] as const

export type AgentKind = (typeof AGENT_KINDS)[number]

/** C14: the server trims the objective, treats empty as absent, refuses > 4000. */
export const OBJECTIVE_MAX_LENGTH = 4000

/** `validate_ref` on the authority: 1–256 characters, no control characters. */
export const REFERENCE_MAX_LENGTH = 256

export interface LaunchForm {
  taskId: string
  executorId: string
  agentKind: AgentKind
  repositoryRef: string
  /**
   * Optional, exactly like the server field (C14). Making it required here would
   * block a launch the authority accepts and the CLI can already perform, so the
   * form only says what is lost by leaving it blank: the executor's brief then
   * carries the task alone.
   */
  objective: string
  worktree: boolean
}

export function emptyLaunchForm(taskId: string): LaunchForm {
  return {
    taskId,
    executorId: '',
    agentKind: 'codex',
    repositoryRef: '',
    objective: '',
    // The executor's existing default: work in the checkout it already has
    // unless the person asks for an isolated worktree.
    worktree: false,
  }
}

/**
 * What a pinned attempt belongs to. Two forms with the same key are the same
 * request, so they must reuse the same `job_id` and `at_ms`; any visible change
 * to what would be sent is a new attempt.
 */
export function launchAttemptKey(form: LaunchForm): string {
  return [
    form.taskId,
    form.executorId,
    form.agentKind,
    form.repositoryRef.trim(),
    form.objective.trim(),
    form.worktree ? 'worktree' : 'in-place',
  ].join('\u0000')
}

/** The idempotency identity of one launch: both values survive a retry. */
export interface PinnedLaunch {
  key: string
  jobId: string
  atMs: number
}

export interface LaunchFormErrors {
  task?: string
  executor?: string
  repository?: string
  objective?: string
}

/**
 * Client-side checks that mirror the server's own field validation, so an
 * obviously invalid form does not create a job request that can only be refused.
 * Everything else (status, prerequisites, an active sibling job, enrolment) is
 * left to the server.
 */
export function validateLaunchForm(
  form: LaunchForm,
  executors: readonly AgentExecutor[],
): LaunchFormErrors {
  const errors: LaunchFormErrors = {}
  if (form.taskId.trim().length === 0) errors.task = 'Choose the task to queue.'

  const executor = executors.find((candidate) => candidate.deviceId === form.executorId)
  if (!form.executorId) {
    errors.executor = 'Choose the executor that should claim this job.'
  } else if (!executor) {
    errors.executor = 'That device is not an enrolled executor. Choose one from the list.'
  } else if (!executor.active) {
    errors.executor = 'That executor is revoked, so it cannot claim a job. Choose an active device.'
  }

  const repository = form.repositoryRef.trim()
  if (repository.length === 0) {
    errors.repository = 'Enter the repository reference the executor should work in.'
  } else if (repository.length > REFERENCE_MAX_LENGTH) {
    errors.repository = `Keep the repository reference to ${REFERENCE_MAX_LENGTH} characters or fewer.`
  } else if (/[\u0000-\u001f\u007f]/.test(repository)) {
    errors.repository = 'The repository reference cannot contain control characters.'
  }

  if (form.objective.trim().length > OBJECTIVE_MAX_LENGTH) {
    errors.objective = `Keep the objective to ${OBJECTIVE_MAX_LENGTH} characters or fewer.`
  }

  return errors
}

export function hasLaunchErrors(errors: LaunchFormErrors): boolean {
  return Object.values(errors).some((message) => Boolean(message))
}

/**
 * The variables for one pinned attempt. The objective is trimmed and becomes
 * `null` when empty, which the command hook omits from the wire body, so a launch
 * without an objective is byte-identical to a launcher that never sends one.
 */
export function toLaunchRequest(pinned: PinnedLaunch, form: LaunchForm) {
  const objective = form.objective.trim()
  return {
    jobId: pinned.jobId,
    taskId: form.taskId,
    agentKind: form.agentKind,
    machineRef: form.executorId,
    repositoryRef: form.repositoryRef.trim(),
    worktree: form.worktree,
    objective: objective.length > 0 ? objective : null,
    atMs: pinned.atMs,
  }
}

/* ------------------------------------------------------------------ */
/* Refusal copy                                                        */
/* ------------------------------------------------------------------ */

export interface LaunchFailure {
  title: string
  detail: string
}

/**
 * What a failed start means. 400 quotes the server's fixed refusal text (C12);
 * 403 is the owner check; 404 is a server without the endpoint. A 5xx or a lost
 * connection is not described here: those navigate to the job and read it.
 */
export function launchFailureCopy(error: ApiError): LaunchFailure {
  const serverMessage = error.message.trim().length > 0 ? error.message.trim() : null
  switch (error.status) {
    case 400:
      return {
        title: 'The server refused to queue this job.',
        detail: serverMessage
          ? `It answered: “${serverMessage}”. No job was queued for this attempt.`
          : 'No job was queued for this attempt. Adjust the form and try again.',
      }
    case 403:
      return {
        title: 'This device is not allowed to queue a job for that task.',
        detail: serverMessage
          ? `The server answered: “${serverMessage}”. A job belongs to the device that started it.`
          : 'A job belongs to the device that started it. Sign in again on the device that owns this workspace.',
      }
    case 404:
      return {
        title: 'This server does not offer the agent launch endpoint.',
        detail:
          'The launch was not sent. This browser cannot queue a job against a server that predates the endpoint; update the server and reload.',
      }
    case 409:
      return {
        title: 'The job could not be queued as sent.',
        detail: serverMessage
          ? `The server answered: “${serverMessage}”. Reload the task and check for an existing job first.`
          : 'Reload the task and check for an existing job first.',
      }
    case 429:
      return {
        title: 'Too many attempts.',
        detail: 'Wait a moment, then try the same launch again.',
      }
    default:
      return {
        title: 'The job was not queued.',
        detail: userMessage(error),
      }
  }
}
