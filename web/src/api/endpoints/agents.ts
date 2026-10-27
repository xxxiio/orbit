import { api } from '../client'
import { withQuery } from '../http'
import type {
  AgentExecutorListResponse,
  AgentHandoffEntry,
  AgentJobCommandRequest,
  AgentJobOverviewItem,
  AgentJobView,
  AgentResumeContext,
  AgentReview,
  QueueSessionPromptRequest,
  ReserveAgentJobRequest,
  ResumeAgentJobRequest,
  ReviewAgentHandoffRequest,
  StartAgentJobRequest,
  SubmitAgentHandoffRequest,
} from '../contracts'

const BASE = '/authoritative/agent-jobs'

/**
 * Devices that can claim a queued job (C10). Owner-scoped and not paginated:
 * one authenticated device sees every enrolled device, and the count is small.
 * An empty list is a valid, honest answer — no executor is configured — not an
 * error. A server older than C10 answers 404; the caller must say so by name
 * instead of showing an empty list.
 */
export function fetchAgentExecutors(signal?: AbortSignal): Promise<AgentExecutorListResponse> {
  return api.get<AgentExecutorListResponse>('/authoritative/agent-executors', { signal })
}

/**
 * A missing or foreign job id answers 403 (`device identity mismatch`), never
 * 404: the ownership check runs before the lookup. Treat 403 as "not yours",
 * not as "server broken".
 */
export function fetchAgentJob(id: string, signal?: AbortSignal): Promise<AgentJobView | null> {
  return api.get<AgentJobView | null>(withQuery(BASE, { id }), { signal })
}

/** Jobs the authority considers to need a person, already owner-filtered. */
export function fetchAgentAttention(signal?: AbortSignal): Promise<AgentJobView[]> {
  return api.get<AgentJobView[]>(`${BASE}/attention`, { signal })
}

/** Cross-project catch-up rows, owner-filtered client-side by the server. */
export function fetchAgentOverview(signal?: AbortSignal): Promise<AgentJobOverviewItem[]> {
  return api.get<AgentJobOverviewItem[]>(`${BASE}/overview`, { signal })
}

export function fetchAgentHandoffs(id: string, signal?: AbortSignal): Promise<AgentHandoffEntry[]> {
  return api.get<AgentHandoffEntry[]>(withQuery(`${BASE}/handoffs`, { id }), { signal })
}

export function fetchAgentReview(id: string, signal?: AbortSignal): Promise<AgentReview | null> {
  return api.get<AgentReview | null>(withQuery(`${BASE}/review`, { id }), { signal })
}

export function fetchAgentResumeContext(
  id: string,
  signal?: AbortSignal,
): Promise<AgentResumeContext> {
  return api.get<AgentResumeContext>(withQuery(`${BASE}/resume-context`, { id }), { signal })
}

export function startAgentJob(body: StartAgentJobRequest): Promise<AgentJobView> {
  return api.post<AgentJobView>(`${BASE}/start`, body)
}

export function reserveAgentJob(body: ReserveAgentJobRequest): Promise<AgentJobView> {
  return api.post<AgentJobView>(`${BASE}/reserve`, body)
}

export function sendAgentJobCommand(body: AgentJobCommandRequest): Promise<AgentJobView> {
  return api.post<AgentJobView>(`${BASE}/command`, body)
}

export function submitAgentHandoff(body: SubmitAgentHandoffRequest): Promise<AgentHandoffEntry[]> {
  return api.post<AgentHandoffEntry[]>(`${BASE}/handoffs`, body)
}

/** Decision is `approved` | `changes_requested`; approving never completes the task. */
export function submitAgentReview(body: ReviewAgentHandoffRequest): Promise<AgentReview> {
  return api.post<AgentReview>(`${BASE}/review`, body)
}

export function resumeAgentJob(body: ResumeAgentJobRequest): Promise<AgentJobView> {
  return api.post<AgentJobView>(`${BASE}/resume`, body)
}

export function queueAgentPrompt(body: QueueSessionPromptRequest): Promise<unknown> {
  return api.post<unknown>(`${BASE}/continue`, body)
}
