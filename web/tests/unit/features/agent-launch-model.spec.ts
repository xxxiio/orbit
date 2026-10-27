import { describe, expect, it } from 'vitest'
import { ApiError } from '@/api/errors'
import type { AgentExecutor, AgentJobOverview } from '@/api/adapters'
import {
  NEVER_CLAIMED_EXPLANATION,
  NEVER_CLAIMED_LABEL,
  OBJECTIVE_MAX_LENGTH,
  REFERENCE_MAX_LENGTH,
  activeJobForTask,
  emptyLaunchForm,
  executorBlocker,
  executorReadState,
  executorSummary,
  hasLaunchErrors,
  hasNeverClaimed,
  launchAttemptKey,
  launchEligibility,
  launchFailureCopy,
  neverClaimedConfirmation,
  toLaunchRequest,
  validateLaunchForm,
  type LaunchForm,
} from '@/features/agents/launch-model'

function executor(overrides: Partial<AgentExecutor> = {}): AgentExecutor {
  return {
    deviceId: 'dev-1',
    active: true,
    lastClaimAtMs: null,
    claimedJobs: 0,
    preparingJobs: 0,
    ...overrides,
  }
}

function job(overrides: Partial<AgentJobOverview> = {}): AgentJobOverview {
  return {
    jobId: 'job-1',
    projectId: 'p-1',
    taskId: 't-1',
    state: 'preparing',
    updatedAtMs: 1,
    handoffCount: 0,
    reviewDecision: null,
    needsAttention: false,
    nextAction: 'await_executor',
    ...overrides,
  }
}

function form(overrides: Partial<LaunchForm> = {}): LaunchForm {
  return {
    ...emptyLaunchForm('t-1'),
    executorId: 'dev-1',
    repositoryRef: 'orbit',
    objective: 'Fix the login redirect.',
    ...overrides,
  }
}

describe('launch eligibility', () => {
  it('offers a launch for the three statuses the storage gate accepts', () => {
    for (const status of ['ready', 'in_progress', 'review'] as const) {
      expect(launchEligibility({ status, activeJob: null })).toEqual({
        eligible: true,
        reason: null,
      })
    }
  })

  it('names the task status when the gate would refuse it', () => {
    const blocked = launchEligibility({ status: 'waiting', activeJob: null })
    expect(blocked.eligible).toBe(false)
    expect(blocked.reason).toContain('ready, in progress or in review')
    expect(blocked.reason).toContain('waiting')
  })

  it('treats any non-terminal job as the active job for that task', () => {
    const jobs = [
      job({ jobId: 'old', taskId: 't-1', state: 'completed' }),
      job({ jobId: 'live', taskId: 't-1', state: 'unreachable' }),
      job({ jobId: 'other', taskId: 't-2', state: 'running' }),
    ]
    expect(activeJobForTask(jobs, 't-1')).toEqual({ jobId: 'live', state: 'unreachable' })
    expect(activeJobForTask(jobs, 't-2')).toEqual({ jobId: 'other', state: 'running' })
    expect(activeJobForTask([jobs[0]], 't-1')).toBeNull()
  })

  it('explains the one-active-job refusal without deciding it', () => {
    const result = launchEligibility({
      status: 'in_progress',
      activeJob: { jobId: 'live', state: 'running' },
    })
    expect(result.eligible).toBe(false)
    expect(result.reason).toContain('one active job per task')
    expect(result.reason).toContain('running')
  })
})

describe('executor read rendering', () => {
  it('renders an empty list as "no executor", never as loading or an error', () => {
    const state = executorReadState({ pending: false, error: null, executors: [] })
    expect(state.kind).toBe('empty')
    expect(executorBlocker(state)).toContain('No executor is enrolled')
    expect(executorBlocker(state)).toContain('disabled')
  })

  it('names the server gap for a 404 instead of showing an empty list', () => {
    const state = executorReadState({
      pending: false,
      error: new ApiError(404, 'not found'),
      executors: undefined,
    })
    expect(state.kind).toBe('unsupported')
    expect(executorBlocker(state)).toContain('does not offer the executor list')
  })

  it('keeps a pending read pending and a failed read failed', () => {
    expect(executorReadState({ pending: true, error: null, executors: undefined }).kind).toBe(
      'loading',
    )
    const failed = executorReadState({
      pending: false,
      error: new ApiError(500, 'storage unavailable'),
      executors: undefined,
    })
    expect(failed.kind).toBe('failed')
    expect(executorBlocker(failed)).toContain('could not be read')
  })

  it('offers only active devices and blocks when every one is revoked', () => {
    const mixed = executorReadState({
      pending: false,
      error: null,
      executors: [executor(), executor({ deviceId: 'dev-2', active: false })],
    })
    expect(mixed.kind).toBe('ready')
    expect(mixed.active.map((entry) => entry.deviceId)).toEqual(['dev-1'])
    expect(executorBlocker(mixed)).toBeNull()

    const revoked = executorReadState({
      pending: false,
      error: null,
      executors: [executor({ active: false })],
    })
    expect(revoked.kind).toBe('inactive')
    expect(executorBlocker(revoked)).toContain('revoked')
    expect(revoked.active).toEqual([])
  })

  it('states the chosen device’s counts without deriving them', () => {
    expect(executorSummary(executor({ claimedJobs: 1, preparingJobs: 2 }))).toBe(
      '1 claimed job · 2 preparing',
    )
    expect(executorSummary(executor())).toBe('no jobs assigned')
    expect(executorSummary(null)).toBeNull()
    // A device that has claimed before is never described as unclaimed.
    expect(
      executorSummary(executor({ lastClaimAtMs: 1_761_432_000_000, claimedJobs: 3 })),
    ).not.toContain('no claim')
  })
})

describe('never-claimed executors', () => {
  it('reads the roster fact from last_claim_at_ms alone', () => {
    expect(hasNeverClaimed(executor())).toBe(true)
    expect(hasNeverClaimed(executor({ lastClaimAtMs: 1_761_432_000_000 }))).toBe(false)
    // A job in preparing is not a claim: the device may still have never claimed.
    expect(hasNeverClaimed(executor({ preparingJobs: 4 }))).toBe(true)
  })

  it('keeps a never-claimed device selectable and the launch enabled', () => {
    const state = executorReadState({
      pending: false,
      error: null,
      executors: [executor({ deviceId: 'never-claimed' })],
    })
    expect(state.kind).toBe('ready')
    expect(state.active.map((entry) => entry.deviceId)).toEqual(['never-claimed'])
    // The roster cannot prove that no runner is active, so nothing blocks.
    expect(executorBlocker(state)).toBeNull()
    expect(validateLaunchForm(form({ executorId: 'never-claimed' }), state.active)).toEqual({})
  })

  it('names the flag and explains what a queued job would do', () => {
    expect(NEVER_CLAIMED_LABEL).toBe('has never claimed a job')
    expect(NEVER_CLAIMED_EXPLANATION).toContain('Preparing')
    expect(NEVER_CLAIMED_EXPLANATION).toContain('claims it')
  })

  it('warns at the confirmation about the device that was chosen', () => {
    const warning = neverClaimedConfirmation('dev-1')
    expect(warning.title).toBe('This device has never claimed a job.')
    expect(warning.detail).toContain('dev-1')
    expect(warning.detail).toContain('has not claimed a job before')
    expect(warning.detail).toContain('Preparing')
    expect(warning.detail).toContain('does not mean work has started')
    // It is a caveat, not a refusal: it never says the launch is blocked.
    expect(warning.detail.toLowerCase()).not.toContain('cannot queue')
    expect(warning.detail.toLowerCase()).not.toContain('disabled')
  })
})

describe('pinned launch attempt', () => {
  it('keeps one identity for an unchanged payload, including whitespace-only edits', () => {
    const pinnedForm = form()
    const sameRequest = form({ objective: '  Fix the login redirect.  ', repositoryRef: ' orbit ' })
    expect(launchAttemptKey(sameRequest)).toBe(launchAttemptKey(pinnedForm))
  })

  it('treats a changed executor, objective or worktree as a new attempt', () => {
    const base = launchAttemptKey(form())
    expect(launchAttemptKey(form({ executorId: 'dev-2' }))).not.toBe(base)
    expect(launchAttemptKey(form({ objective: 'Something else.' }))).not.toBe(base)
    expect(launchAttemptKey(form({ worktree: true }))).not.toBe(base)
    expect(launchAttemptKey(form({ taskId: 't-2' }))).not.toBe(base)
  })

  it('reuses the pinned job id and timestamp and omits an empty objective', () => {
    const pinned = { key: 'k', jobId: 'job-9', atMs: 1_761_432_000_000 }
    const first = toLaunchRequest(pinned, form())
    const retry = toLaunchRequest(pinned, form())

    expect(first).toEqual(retry)
    expect(first.jobId).toBe('job-9')
    expect(first.atMs).toBe(1_761_432_000_000)
    expect(first.objective).toBe('Fix the login redirect.')
    expect(first.machineRef).toBe('dev-1')
    expect(first.repositoryRef).toBe('orbit')
    expect(first.worktree).toBe(false)

    const withoutObjective = toLaunchRequest(pinned, form({ objective: '   ' }))
    expect(withoutObjective.objective).toBeNull()
  })
})

describe('launch form validation', () => {
  it('accepts a complete form with one active executor', () => {
    const errors = validateLaunchForm(form(), [executor()])
    expect(hasLaunchErrors(errors)).toBe(false)
  })

  it('requires a repository reference and refuses one the authority cannot accept', () => {
    expect(validateLaunchForm(form({ repositoryRef: '  ' }), [executor()]).repository).toContain(
      'Enter the repository reference',
    )
    expect(
      validateLaunchForm(form({ repositoryRef: 'x'.repeat(REFERENCE_MAX_LENGTH + 1) }), [
        executor(),
      ]).repository,
    ).toContain(`${REFERENCE_MAX_LENGTH} characters`)
    expect(
      validateLaunchForm(form({ repositoryRef: 'orbit\nrm -rf' }), [executor()]).repository,
    ).toContain('control characters')
  })

  it('refuses an executor that is not an active enrolled device', () => {
    expect(validateLaunchForm(form({ executorId: '' }), []).executor).toContain(
      'Choose the executor',
    )
    expect(
      validateLaunchForm(form({ executorId: 'ghost' }), [executor()]).executor,
    ).toContain('not an enrolled executor')
    expect(
      validateLaunchForm(form(), [executor({ active: false })]).executor,
    ).toContain('revoked')
  })

  it('caps the objective at the server limit', () => {
    const tooLong = 'x'.repeat(OBJECTIVE_MAX_LENGTH + 1)
    expect(validateLaunchForm(form({ objective: tooLong }), [executor()]).objective).toContain(
      `${OBJECTIVE_MAX_LENGTH} characters`,
    )
    expect(
      validateLaunchForm(form({ objective: 'x'.repeat(OBJECTIVE_MAX_LENGTH) }), [executor()])
        .objective,
    ).toBeUndefined()
  })
})

describe('launch refusal copy', () => {
  it('quotes the server’s preserved refusal text', () => {
    const refused = launchFailureCopy(
      new ApiError(400, 'task not eligible for connected start'),
    )
    expect(refused.title).toBe('The server refused to queue this job.')
    expect(refused.detail).toContain('task not eligible for connected start')
  })

  it('names the owner check and the missing endpoint by status', () => {
    expect(launchFailureCopy(new ApiError(403, 'device identity mismatch')).detail).toContain(
      'device identity mismatch',
    )
    expect(launchFailureCopy(new ApiError(404, 'not found')).title).toContain(
      'does not offer the agent launch endpoint',
    )
  })
})
