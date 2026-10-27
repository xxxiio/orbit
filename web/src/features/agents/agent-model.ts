/**
 * Presentation vocabulary for agent jobs.
 *
 * These helpers name what the server already decided (state, next action,
 * review decision). They never infer task completion: the review response says
 * `task_completed: false`, and the UI repeats that.
 */
import type { BadgeTone } from '@/components/ui/badge'

export const AGENT_STATES = [
  'queued',
  'preparing',
  'launching',
  'running',
  'waiting_input',
  'blocked',
  'completed',
  'failed',
  'interrupted',
  'unreachable',
] as const

/**
 * The only states the server accepts as a resume predecessor. A running job
 * cannot be resumed: the successor is offered after it reaches one of these.
 */
export const AGENT_TERMINAL_STATES = ['completed', 'failed', 'interrupted'] as const

export function isAgentJobTerminal(state: string): boolean {
  return (AGENT_TERMINAL_STATES as readonly string[]).includes(state)
}

const STATE_LABELS: Record<string, string> = {
  queued: 'Queued',
  preparing: 'Preparing',
  launching: 'Launching',
  running: 'Running',
  waiting_input: 'Waiting for input',
  blocked: 'Blocked',
  completed: 'Completed',
  failed: 'Failed',
  interrupted: 'Interrupted',
  unreachable: 'Unreachable',
}

const STATE_TONES: Record<string, BadgeTone> = {
  queued: 'neutral',
  preparing: 'info',
  launching: 'warning',
  running: 'accent',
  waiting_input: 'warning',
  blocked: 'danger',
  completed: 'success',
  failed: 'danger',
  interrupted: 'warning',
  unreachable: 'danger',
}

const NEXT_ACTION_LABELS: Record<string, string> = {
  none: 'Nothing to do',
  // C13: a queued/preparing job with no claim waits for its executor. It is not
  // a person's task, and it must not read as "nothing to do" either.
  await_executor: 'Awaiting an executor',
  respond_to_agent: 'Respond to the agent',
  inspect_runtime: 'Inspect the runtime',
  review_handoff: 'Review the handoff',
  resume_or_rework: 'Resume or rework',
  inspect_missing_handoff: 'Completed without a handoff',
  check_task_acceptance: 'Check task acceptance',
  inspect_and_resume: 'Inspect and resume',
}

export function agentStateLabel(state: string): string {
  return STATE_LABELS[state] ?? state.replace(/_/g, ' ')
}

export function agentStateTone(state: string): BadgeTone {
  return STATE_TONES[state] ?? 'neutral'
}

export function nextActionLabel(action: string): string {
  return NEXT_ACTION_LABELS[action] ?? action.replace(/_/g, ' ')
}

export function agentKindLabel(kind: string): string {
  if (kind === 'codex') return 'Codex'
  if (kind === 'opencode') return 'OpenCode'
  return kind
}

export function reviewDecisionLabel(decision: string | null | undefined): string {
  if (decision === 'approved') return 'Approved'
  if (decision === 'changes_requested') return 'Changes requested'
  return 'Not reviewed'
}

export function shortAgentId(id: string): string {
  return id.length > 18 ? `${id.slice(0, 8)}…${id.slice(-4)}` : id
}

/** What a person can do from the browser for a given next action. */
export function nextActionExplanation(action: string): string {
  switch (action) {
    case 'await_executor':
      return 'No executor has claimed this job yet. It stays queued until an enrolled executor claims it; this browser does not start a model, and there is nothing to click.'
    case 'review_handoff':
      return 'Read the latest handoff and record a review decision below.'
    case 'resume_or_rework':
      return 'The review asked for changes. Resume creates a new job for an executor; it does not launch one here.'
    case 'respond_to_agent':
      return 'The executor owns the conversation with a running agent. This browser cannot send it a prompt.'
    case 'inspect_runtime':
      return 'The runtime session is external. This browser shows its references but cannot inspect or launch it.'
    case 'inspect_missing_handoff':
      return 'The job completed without a handoff, so there is no evidence to review. Check the task separately.'
    case 'check_task_acceptance':
      return 'The handoff is separate from task acceptance. Review it here, then accept the task in its own surface.'
    case 'inspect_and_resume':
      return 'The job did not finish. Resume creates a new job; an executor must claim it before anything runs.'
    default:
      return 'No action is required on this job.'
  }
}

/**
 * What the machine claim means, in words.
 *
 * C13 exposes the machine claim separately from the job state: `null` is a real
 * answer ("no machine has claimed it"), while a payload with no key at all is a
 * server that predates the read. Both must not be conflated, and an `ambiguous`
 * launch is the one case a person must never relaunch.
 */
export interface ClaimStateCopy {
  /** Stable key for `data-claim-state`; `none` is a null claim, not a missing key. */
  kind: 'unreported' | 'none' | 'claimed' | 'confirmed' | 'ambiguous' | 'unrecognised'
  label: string
  detail: string
  tone: BadgeTone
  /**
   * True only when the launch outcome is unknown: queueing or resuming this job
   * again could start a second agent, so no retry affordance is offered.
   */
  blocksRelaunch: boolean
}

export function claimStateCopy({
  reported,
  claimState,
}: {
  reported: boolean
  claimState: string | null | undefined
}): ClaimStateCopy {
  if (!reported) {
    return {
      kind: 'unreported',
      label: 'Claim state not reported',
      detail:
        'This server does not report whether an executor has claimed the job. The state above is still the server’s answer; a claim is not shown here.',
      tone: 'neutral',
      blocksRelaunch: false,
    }
  }
  switch (claimState) {
    case null:
    case undefined:
      return {
        kind: 'none',
        label: 'Waiting for an executor',
        detail:
          'No executor has claimed this job. It stays queued until an enrolled executor claims it; nothing has been started, and this browser does not start a model.',
        tone: 'info',
        blocksRelaunch: false,
      }
    case 'claimed':
      return {
        kind: 'claimed',
        label: 'Claimed by an executor',
        detail:
          'An executor has claimed this job and reported the claim. The launch result is not confirmed yet; the job state changes when the executor reports it.',
        tone: 'warning',
        blocksRelaunch: false,
      }
    case 'confirmed':
      return {
        kind: 'confirmed',
        label: 'Launch confirmed',
        detail:
          'The executor confirmed the launch. From here the job state comes from the executor’s own reports, not from this page.',
        tone: 'success',
        blocksRelaunch: false,
      }
    case 'ambiguous':
      return {
        kind: 'ambiguous',
        label: 'Launch outcome unknown',
        detail:
          'This job’s claim is ambiguous: the launch may or may not have started. Do not relaunch it — a second attempt could start a second agent in the same repository. An executor has to reconcile the claim first.',
        tone: 'danger',
        blocksRelaunch: true,
      }
    default:
      return {
        kind: 'unrecognised',
        label: String(claimState).replace(/_/g, ' '),
        detail:
          'This build does not know this claim state, so the job is treated as unsafe to relaunch. Open it again after the server or this browser is updated.',
        tone: 'warning',
        blocksRelaunch: true,
      }
  }
}
