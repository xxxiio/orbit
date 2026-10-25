/**
 * Agent job rows: the job, the device-wide overview item, its handoffs, the
 * recorded review and the resume context.
 *
 * A job's state, revision and next action are the server's; this module never
 * infers that a review completed a task.
 */
import type {
  AgentHandoffEntry,
  AgentJobOverviewItem,
  AgentJobView,
  AgentResumeContext as AgentResumeContextRecord,
  AgentReview as AgentReviewRecord,
} from '../contracts'

export interface AgentJob {
  id: string
  requestId: string
  projectId: string
  taskId: string
  state: string
  revision: number
  agentKind: string
  machineRef: string
  runtimeKind: string
  repositoryRef: string
  runtimeSessionRef: string | null
  createdAtMs: number
  updatedAtMs: number
}

export function toAgentJob(view: AgentJobView): AgentJob {
  return {
    id: view.id,
    requestId: view.request_id,
    projectId: view.project_id,
    taskId: view.task_id,
    state: view.state,
    revision: view.revision,
    agentKind: view.agent_kind,
    machineRef: view.machine_ref,
    runtimeKind: view.runtime_kind,
    repositoryRef: view.repository_ref,
    runtimeSessionRef: view.runtime_session_ref,
    createdAtMs: view.created_at_ms,
    updatedAtMs: view.updated_at_ms,
  }
}

export interface AgentJobOverview {
  jobId: string
  projectId: string
  taskId: string
  state: string
  updatedAtMs: number
  handoffCount: number
  reviewDecision: string | null
  needsAttention: boolean
  nextAction: string
}

export function toAgentJobOverview(item: AgentJobOverviewItem): AgentJobOverview {
  return {
    jobId: item.job_id,
    projectId: item.project_id,
    taskId: item.task_id,
    state: item.state,
    updatedAtMs: item.updated_at_ms,
    handoffCount: item.handoff_count,
    reviewDecision: item.review_decision,
    needsAttention: item.needs_attention,
    nextAction: item.next_action,
  }
}

export interface AgentHandoff {
  handoffId: string
  reportedAtMs: number
  payload: unknown
}

export function toAgentHandoff(entry: AgentHandoffEntry): AgentHandoff {
  return {
    handoffId: entry.handoff_id,
    reportedAtMs: entry.reported_at_ms,
    payload: entry.payload,
  }
}

export interface AgentReview {
  jobId: string
  handoffId: string
  decision: string
  reviewer: string
  reason: string
  reviewedAtMs: number
  taskCompleted: boolean
}

export function toAgentReview(review: AgentReviewRecord): AgentReview {
  return {
    jobId: review.job_id,
    handoffId: review.handoff_id,
    decision: review.decision,
    reviewer: review.reviewer,
    reason: review.reason,
    reviewedAtMs: review.reviewed_at_ms,
    taskCompleted: review.task_completed,
  }
}

export interface AgentResumeContext {
  job: AgentJob
  originalBrief: unknown
  latestHandoff: unknown
  handoffCount: number
  review: AgentReview | null
  worktree: unknown
  taskCompletionInferred: boolean
  runtimeLaunchAuthorized: boolean
}

export function toAgentResumeContext(context: AgentResumeContextRecord): AgentResumeContext {
  return {
    job: toAgentJob(context.job),
    originalBrief: context.original_brief,
    latestHandoff: context.latest_handoff,
    handoffCount: context.handoff_count,
    review: context.review ? toAgentReview(context.review) : null,
    worktree: context.worktree,
    taskCompletionInferred: context.task_completion_inferred,
    runtimeLaunchAuthorized: context.runtime_launch_authorized,
  }
}
