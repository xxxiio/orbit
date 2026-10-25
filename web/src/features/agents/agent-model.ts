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
