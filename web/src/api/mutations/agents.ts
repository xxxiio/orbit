/**
 * Agent-job commands: record a review, resume a finished job.
 *
 * Neither launches an executor and neither completes the Orbit task; the
 * server answers `task_completed: false` on approval and queues a job an
 * executor must claim on resume.
 */
import { queryKeys } from '../queries'
import { resumeAgentJob, submitAgentReview } from '../endpoints/agents'
import { toAgentJob, toAgentReview, type AgentJob, type AgentReview } from '../adapters'
import { useApiMutation, type CommandMutation } from './core'

export interface SubmitAgentReviewVariables {
  jobId: string
  handoffId: string
  decision: 'approved' | 'changes_requested'
  reason: string
  /** Minted with the attempt so an identical retry replays the same review. */
  atMs: number
}

/**
 * A review is a write, not a launch. The server records the decision and always
 * returns `task_completed: false`; approving never moves the Orbit task. The
 * caller keeps `atMs` stable across retries because the server refuses a
 * conflicting second review for the same handoff.
 */
export function useSubmitAgentReview(): CommandMutation<AgentReview, SubmitAgentReviewVariables> {
  return useApiMutation<AgentReview, SubmitAgentReviewVariables>({
    uncertainOnError: true,
    mutationFn: ({ jobId, handoffId, decision, reason, atMs }) =>
      submitAgentReview({
        job_id: jobId,
        handoff_id: handoffId,
        decision,
        reason,
        at_ms: atMs,
      }).then(toAgentReview),
    affectedKeys: (variables) => [
      queryKeys.agentJob(variables.jobId),
      queryKeys.agentHandoffs(variables.jobId),
      queryKeys.agentReview(variables.jobId),
      queryKeys.agentResumeContext(variables.jobId),
      queryKeys.agentOverview(),
      queryKeys.agentAttention(),
    ],
  })
}

export interface ResumeAgentJobVariables {
  priorJobId: string
  /** Minted before the first attempt; an identical retry must reuse it. */
  newJobId: string
  atMs: number
}

/**
 * Resume queues a new, authority-owned job that an executor can claim. It does
 * not launch anything from the browser, and it does not infer task completion.
 */
export function useResumeAgentJob(): CommandMutation<AgentJob, ResumeAgentJobVariables> {
  return useApiMutation<AgentJob, ResumeAgentJobVariables>({
    uncertainOnError: true,
    mutationFn: ({ priorJobId, newJobId, atMs }) =>
      resumeAgentJob({
        prior_job_id: priorJobId,
        new_job_id: newJobId,
        at_ms: atMs,
      }).then(toAgentJob),
    affectedKeys: (variables) => [
      queryKeys.agentJob(variables.priorJobId),
      queryKeys.agentJob(variables.newJobId),
      queryKeys.agentResumeContext(variables.priorJobId),
      queryKeys.agentOverview(),
      queryKeys.agentAttention(),
    ],
  })
}
