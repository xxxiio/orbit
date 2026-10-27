/**
 * Agent-job commands: queue a job, record a review, resume a finished job.
 *
 * None of them launches an executor and none completes the Orbit task. Starting
 * queues a job an enrolled device has to claim; the server answers
 * `task_completed: false` on approval and queues a job on resume.
 */
import { queryKeys } from '../queries'
import { resumeAgentJob, startAgentJob, submitAgentReview } from '../endpoints/agents'
import type { StartAgentJobRequest } from '../contracts'
import { toAgentJob, toAgentReview, type AgentJob, type AgentReview } from '../adapters'
import { useApiMutation, type CommandMutation } from './core'

export interface StartAgentJobVariables {
  /** Pinned before the first attempt; an identical retry must reuse it. */
  jobId: string
  taskId: string
  agentKind: string
  /** An enrolled executor's device id, chosen from the executor read. */
  machineRef: string
  repositoryRef: string
  worktree: boolean
  /** Already trimmed by the caller; `null` means "no objective" and is omitted. */
  objective: string | null
  atMs: number
}

/**
 * Queue one agent job for an enrolled executor (C14).
 *
 * The browser owns only the queueing: the server creates a `preparing` job, and
 * nothing runs until the named executor claims it. `job_id` is the launch
 * identity and the idempotency key, so an unconfirmed attempt must be checked
 * by reading that job before anything is sent again.
 */
export function useStartAgentJob(): CommandMutation<AgentJob, StartAgentJobVariables> {
  return useApiMutation<AgentJob, StartAgentJobVariables>({
    uncertainOnError: true,
    mutationFn: ({ jobId, taskId, agentKind, machineRef, repositoryRef, worktree, objective, atMs }) => {
      const body: StartAgentJobRequest = {
        job_id: jobId,
        task_id: taskId,
        agent_kind: agentKind,
        machine_ref: machineRef,
        repository_ref: repositoryRef,
        worktree,
        at_ms: atMs,
      }
      if (objective !== null && objective.length > 0) body.objective = objective
      return startAgentJob(body).then(toAgentJob)
    },
    affectedKeys: (variables) => [
      queryKeys.agentJob(variables.jobId),
      queryKeys.agentOverview(),
      queryKeys.agentAttention(),
      // The executor's `preparing_jobs` count changes with the new job.
      queryKeys.agentExecutors(),
    ],
  })
}

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
