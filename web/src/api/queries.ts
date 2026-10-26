/**
 * Query keys + the read hooks the UI is allowed to use.
 *
 * Rules encoded here:
 * - one query per server projection; the client never recomputes readiness,
 *   planning, schedules or transitions;
 * - `now_ms` comes from the caller (bucketed to the minute so the key is
 *   stable) because the server evaluates at the instant it is given;
 * - any 401 on any read signs this browser out of the session store, once.
 */
import { useCallback, useEffect } from 'react'
import {
  useQueries,
  useQuery,
  useQueryClient,
  type QueryKey,
  type UseQueryResult,
} from '@tanstack/react-query'
import { handleUnauthorized } from './session-store'
import { sessionQueryKey } from './session'
import { ApiError, ClientStateError, classifyError } from './errors'
import { fetchNextActions, fetchTaskDependencies, fetchTaskReadiness } from './endpoints/dependencies'
import {
  fetchDayPlan,
  fetchPortfolioCapacity,
  fetchProjectDependencies,
  fetchSchedulePlan,
  fetchTaskGraph,
  fetchWorkBrief,
  fetchWorkload,
  type DayPlanParams,
  type PortfolioCapacityParams,
  type ProjectDependenciesParams,
  type SchedulePlanParams,
  type WorkBriefParams,
  type WorkloadParams,
} from './endpoints/planning'
import {
  fetchAgentAttention,
  fetchAgentHandoffs,
  fetchAgentJob,
  fetchAgentOverview,
  fetchAgentResumeContext,
  fetchAgentReview,
} from './endpoints/agents'
import { fetchAttachments, fetchAttachmentContent } from './endpoints/attachments'
import {
  fetchKnowledgeTree,
  fetchMarkdownDocument,
  fetchMarkdownInventory,
  fetchMarkdownRelations,
  searchMarkdown,
} from './endpoints/knowledge'
import { fetchMilestones } from './endpoints/milestones'
import { fetchSnapshot } from './endpoints/snapshot'
import { fetchAcceptanceCriteria, fetchTaskActivity, fetchTaskDetail, fetchTaskTransitions } from './endpoints/tasks'
import {
  fetchBundledWorkflows,
  fetchProjectBinding,
  fetchUpgradePreview,
  fetchWorkItems,
  fetchWorkflow,
  fetchWorkflowVersions,
} from './endpoints/workflows'
import {
  toAcceptanceCriterion,
  toActivityEntry,
  toAgentHandoff,
  toAgentJob,
  toAgentJobOverview,
  toAgentResumeContext,
  toAgentReview,
  toDayPlan,
  toDependency,
  toMilestone,
  toNextActions,
  toPortfolioCapacity,
  toProjectDependencies,
  toSchedulePlan,
  toTaskDetail,
  toTaskGraph,
  toTaskReadiness,
  toWorkBrief,
  toWorkItem,
  toWorkflow,
  toWorkflowBinding,
  toWorkflowUpgradePreview,
  toWorkload,
  toWorkspaceSnapshot,
  type AcceptanceCriterion,
  type ActivityEntry,
  type AgentHandoff,
  type AgentJob,
  type AgentJobOverview,
  type AgentResumeContext,
  type AgentReview,
  type DayPlanView,
  type Dependency,
  type Milestone,
  type NextActionsView,
  type PortfolioCapacityView,
  type ProjectDependencyView,
  type SchedulePlanView,
  type Task,
  type TaskGraphView,
  type TaskReadinessView,
  type WorkBriefView,
  type WorkItem,
  type Workflow,
  type WorkflowBinding,
  type WorkflowUpgradePreview,
  type WorkloadView,
  type WorkspaceSnapshot,
} from './adapters'
import type { Attachment } from './adapters'
import { toAttachment } from './adapters'
import type {
  AttachmentOwnerKind,
  BundledWorkflowDescriptor,
  KnowledgeDocumentInfo,
  KnowledgeNode,
  MilestoneSnapshot,
  TaskStatus,
} from './contracts'

/** The snapshot is the shared basis for almost every screen: keep it warm for 5s. */
export const SNAPSHOT_STALE_TIME = 5_000

export const queryKeys = {
  /** Shared with the scaffold's session module so there is exactly one cache entry. */
  session: () => sessionQueryKey,
  snapshot: () => ['snapshot'] as const,
  task: (id: string) => ['task', id] as const,
  taskAcceptanceCriteria: (id: string) => ['task', id, 'acceptance-criteria'] as const,
  taskActivity: (id: string) => ['task', id, 'activity'] as const,
  taskTransitions: (status: TaskStatus) => ['task-transitions', status] as const,
  taskDependencies: () => ['task-dependencies'] as const,
  milestones: () => ['milestones'] as const,
  taskReadiness: (nowMs: number) => ['task-readiness', nowMs] as const,
  taskReadinessAll: () => ['task-readiness'] as const,
  nextActions: (nowMs: number, limit: number) => ['next-actions', nowMs, limit] as const,
  nextActionsAll: () => ['next-actions'] as const,
  taskGraph: (nowMs: number) => ['task-graph', nowMs] as const,
  taskGraphAll: () => ['task-graph'] as const,
  projectDependencies: (params: ProjectDependenciesParams) => ['project-dependencies', params] as const,
  projectDependenciesAll: () => ['project-dependencies'] as const,
  workBrief: (params: WorkBriefParams) => ['work-brief', params] as const,
  workBriefAll: () => ['work-brief'] as const,
  dayPlan: (params: DayPlanParams) => ['day-plan', params] as const,
  dayPlanAll: () => ['day-plan'] as const,
  schedulePlan: (params: SchedulePlanParams) => ['schedule-plan', params] as const,
  schedulePlanAll: () => ['schedule-plan'] as const,
  portfolioCapacity: (params: PortfolioCapacityParams) => ['portfolio-capacity', params] as const,
  portfolioCapacityAll: () => ['portfolio-capacity'] as const,
  workload: (params: WorkloadParams) => ['workload', params] as const,
  workloadAll: () => ['workload'] as const,
  workflows: () => ['workflows'] as const,
  bundledWorkflows: () => ['bundled-workflows'] as const,
  workflow: (id: string, version: number) => ['workflow', id, version] as const,
  workflowBinding: (projectId: string) => ['workflow-binding', projectId] as const,
  workItems: (projectId: string) => ['work-items', projectId] as const,
  /** Prefix for invalidating every project's item list at once. */
  workItemsAll: () => ['work-items'] as const,
  upgradePreview: (projectId: string, id: string, version: number) =>
    ['workflow-upgrade-preview', projectId, id, version] as const,
  upgradePreviewAll: () => ['workflow-upgrade-preview'] as const,
  knowledge: (path: string) => ['knowledge', path] as const,
  knowledgeAll: () => ['knowledge'] as const,
  knowledgeTree: () => ['knowledge', 'tree'] as const,
  knowledgeInventory: () => ['knowledge', 'inventory'] as const,
  knowledgeDocument: (path: string) => ['knowledge', 'document', path] as const,
  knowledgeRelations: (id: string) => ['knowledge', 'relations', id] as const,
  knowledgeSearch: (query: string, limit: number) => ['knowledge', 'search', query, limit] as const,
  markdownManifest: () => ['knowledge', 'manifest'] as const,
  attachments: (ownerKind: string, ownerId: string) => ['attachments', ownerKind, ownerId] as const,
  attachmentContent: (id: string) => ['attachments', 'content', id] as const,
  agentAll: () => ['agent-jobs'] as const,
  agentOverview: () => ['agent-jobs', 'overview'] as const,
  agentAttention: () => ['agent-jobs', 'attention'] as const,
  agentJob: (id: string) => ['agent-jobs', 'job', id] as const,
  agentHandoffs: (id: string) => ['agent-jobs', 'handoffs', id] as const,
  agentReview: (id: string) => ['agent-jobs', 'review', id] as const,
  agentResumeContext: (id: string) => ['agent-jobs', 'resume-context', id] as const,
}

/** 4xx means "understood and refused": never retry it blindly. */
export function retryUnlessClientError(failureCount: number, error: ApiError): boolean {
  // A `ClientStateError` means the request was never made (missing parameters,
  // missing review). Retrying cannot fix it.
  if (error instanceof ClientStateError) return false
  if (error instanceof ApiError && error.status >= 400 && error.status < 500) return false
  return failureCount < 1
}

export interface ApiQueryOptions<TData, TSelected = TData> {
  queryKey: QueryKey
  queryFn: (context: { signal: AbortSignal }) => Promise<TData>
  enabled?: boolean
  staleTime?: number
  select?: (data: TData) => TSelected
}

/**
 * `useQuery` plus the two behaviours every read here needs: the shared retry
 * policy and a one-shot sign-out when the session expires (401).
 */
export function useApiQuery<TData, TSelected = TData>({
  queryKey,
  queryFn,
  enabled,
  staleTime,
  select,
}: ApiQueryOptions<TData, TSelected>): UseQueryResult<TSelected, ApiError> {
  const queryClient = useQueryClient()
  const query = useQuery<TData, ApiError, TSelected>({
    queryKey,
    queryFn,
    enabled,
    staleTime,
    select,
    retry: retryUnlessClientError,
  })

  useEffect(() => {
    if (query.error && classifyError(query.error) === 'unauthorized') {
      handleUnauthorized(queryClient)
    }
  }, [query.error, queryClient])

  return query
}

/**
 * `now_ms` is bucketed to the minute: the server needs an exact instant, and a
 * literal `Date.now()` in a query key would re-key on every render.
 */
export function defaultNowMs(): number {
  return Math.floor(Date.now() / 60_000) * 60_000
}

/* ------------------------------------------------------------------ */
/* Snapshot                                                            */
/* ------------------------------------------------------------------ */

/** The single workspace projection. Every selector is derived from it. */
export function useWorkspaceSnapshot(): UseQueryResult<WorkspaceSnapshot, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.snapshot(),
    queryFn: ({ signal }) => fetchSnapshot(signal),
    staleTime: SNAPSHOT_STALE_TIME,
    select: toWorkspaceSnapshot,
  })
}

/* ------------------------------------------------------------------ */
/* Task detail: header + acceptance criteria + activity                */
/* ------------------------------------------------------------------ */

export function useTaskRecord(id: string | undefined): UseQueryResult<Task, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.task(id ?? ''),
    queryFn: ({ signal }) => fetchTaskDetail(id as string, signal),
    enabled: Boolean(id),
    select: toTaskDetail,
  })
}

export interface TaskTransitionEdges {
  /**
   * The domain's allowed targets per status. `null` means this status could
   * not be read (pending or failed); callers fall back to offering every
   * status and keep the server's refusal as the honest answer.
   */
  edges: Map<TaskStatus, TaskStatus[] | null>
  pending: boolean
  failed: boolean
}

/**
 * Read the accepted status edges for each status on screen. One request per
 * distinct status, deduplicated by the query cache; no transition table lives
 * in the client.
 */
export function useTaskTransitionEdges(
  statuses: readonly TaskStatus[],
): TaskTransitionEdges {
  const unique: TaskStatus[] = Array.from(new Set(statuses))
  const results = useQueries({
    queries: unique.map((status) => ({
      queryKey: queryKeys.taskTransitions(status),
      queryFn: ({ signal }: { signal: AbortSignal }) => fetchTaskTransitions(status, signal),
      staleTime: 5 * 60_000,
      retry: retryUnlessClientError,
    })),
  })
  const edges = new Map<TaskStatus, TaskStatus[] | null>()
  unique.forEach((status, index) => {
    const result = results[index]
    edges.set(status, result.data ? result.data.next : null)
  })
  return {
    edges,
    pending: results.some((result) => result.isPending),
    failed: results.some((result) => result.isError),
  }
}

/** One status' accepted edges, for a single control (the drawer). */
export function useTaskTransitions(status: TaskStatus): TaskTransitionEdges {
  return useTaskTransitionEdges([status])
}

export function useAcceptanceCriteria(
  id: string | undefined,
): UseQueryResult<AcceptanceCriterion[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.taskAcceptanceCriteria(id ?? ''),
    queryFn: ({ signal }) => fetchAcceptanceCriteria(id as string, signal),
    enabled: Boolean(id),
    select: (records) => records.map(toAcceptanceCriterion),
  })
}

export function useTaskActivity(id: string | undefined): UseQueryResult<ActivityEntry[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.taskActivity(id ?? ''),
    queryFn: ({ signal }) => fetchTaskActivity(id as string, signal),
    enabled: Boolean(id),
    select: (records) => records.map(toActivityEntry),
  })
}

/* ------------------------------------------------------------------ */
/* Milestones + dependencies                                           */
/* ------------------------------------------------------------------ */

export function useMilestones(projectId?: string): UseQueryResult<Milestone[], ApiError> {
  const select = useCallback(
    (snapshot: MilestoneSnapshot) =>
      snapshot.milestones
        .map(toMilestone)
        .filter((milestone) => (projectId ? milestone.projectId === projectId : true)),
    [projectId],
  )

  return useApiQuery({
    queryKey: queryKeys.milestones(),
    queryFn: ({ signal }) => fetchMilestones(signal),
    select,
  })
}

export function useTaskDependencies(): UseQueryResult<Dependency[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.taskDependencies(),
    queryFn: ({ signal }) => fetchTaskDependencies(signal),
    select: (response) => response.edges.map(toDependency),
  })
}

/** Server-derived readiness; the client only displays it. */
export function useTaskReadiness(
  nowMs: number = defaultNowMs(),
  enabled = true,
): UseQueryResult<TaskReadinessView, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.taskReadiness(nowMs),
    queryFn: ({ signal }) => fetchTaskReadiness(nowMs, signal),
    enabled,
    select: toTaskReadiness,
  })
}

export function useNextActions(
  nowMs: number = defaultNowMs(),
  limit = 20,
  enabled = true,
): UseQueryResult<NextActionsView, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.nextActions(nowMs, limit),
    queryFn: ({ signal }) => fetchNextActions(nowMs, limit, signal),
    enabled,
    select: toNextActions,
  })
}

/* ------------------------------------------------------------------ */
/* Dependency graph + project dependencies                             */
/* ------------------------------------------------------------------ */

/** Server-built graph: ordering, depth and "unblocks" counts are not recomputed. */
export function useTaskGraph(nowMs: number, enabled = true): UseQueryResult<TaskGraphView, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.taskGraph(nowMs),
    queryFn: ({ signal }) => fetchTaskGraph(nowMs, signal),
    enabled: enabled && Number.isFinite(nowMs),
    select: toTaskGraph,
  })
}

export function useProjectDependencies(
  params: ProjectDependenciesParams,
  enabled = true,
): UseQueryResult<ProjectDependencyView, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.projectDependencies(params),
    queryFn: ({ signal }) => fetchProjectDependencies(params, signal),
    enabled: enabled && Number.isFinite(params.nowMs),
    select: toProjectDependencies,
  })
}

/* ------------------------------------------------------------------ */
/* Day plan, work brief, workload, capacity                            */
/* ------------------------------------------------------------------ */

/**
 * A day plan only exists once capacity is authored. `available_minutes` is
 * never defaulted: with 0 (or a missing reference time) the query stays idle and
 * nothing is sent.
 */
export function useDayPlan(
  params: DayPlanParams | null,
  enabled = true,
): UseQueryResult<DayPlanView, ApiError> {
  const ready =
    Boolean(params) && Number.isFinite(params?.nowMs) && (params?.availableMinutes ?? 0) > 0

  return useApiQuery({
    queryKey: queryKeys.dayPlan(params ?? { nowMs: 0, availableMinutes: 0 }),
    queryFn: ({ signal }) => {
      if (!params) {
        // Unreachable while `enabled` is false; keeps the queryFn total.
        return Promise.reject(new ApiError(400, 'A day plan needs the available minutes.'))
      }
      return fetchDayPlan(params, signal)
    },
    enabled: enabled && ready,
    select: toDayPlan,
  })
}

export function useWorkBrief(
  params: WorkBriefParams,
  enabled = true,
): UseQueryResult<WorkBriefView, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.workBrief(params),
    queryFn: ({ signal }) => fetchWorkBrief(params, signal),
    enabled: enabled && Number.isFinite(params.nowMs) && Number.isFinite(params.endsAtMs),
    select: toWorkBrief,
  })
}

/**
 * A schedule plan needs an explicit, complete window (both ends). A partial
 * window stays idle: the client never fills in "now + 1 day".
 */
export function useSchedulePlan(
  params: SchedulePlanParams | null,
  enabled = true,
): UseQueryResult<SchedulePlanView, ApiError> {
  const ready =
    Boolean(params) && Number.isFinite(params?.startsAtMs) && Number.isFinite(params?.endsAtMs)

  return useApiQuery({
    queryKey: queryKeys.schedulePlan(params ?? { startsAtMs: 0, endsAtMs: 0 }),
    queryFn: ({ signal }) => {
      if (!params) {
        return Promise.reject(new ApiError(400, 'A schedule plan needs a full planning window.'))
      }
      return fetchSchedulePlan(params, signal)
    },
    enabled: enabled && ready,
    select: toSchedulePlan,
  })
}

export function useWorkload(
  params: WorkloadParams | null,
  enabled = true,
): UseQueryResult<WorkloadView, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.workload(params ?? { nowMs: 0 }),
    queryFn: ({ signal }) => {
      if (!params) {
        return Promise.reject(new ApiError(400, 'A workload report needs a reference time.'))
      }
      return fetchWorkload(params, signal)
    },
    enabled: enabled && Boolean(params) && Number.isFinite(params?.nowMs),
    select: toWorkload,
  })
}

/** Like the day plan: a capacity proposal needs authored minutes. */
export function usePortfolioCapacity(
  params: PortfolioCapacityParams | null,
  enabled = true,
): UseQueryResult<PortfolioCapacityView, ApiError> {
  const ready =
    Boolean(params) && Number.isFinite(params?.nowMs) && (params?.availableMinutes ?? 0) > 0

  return useApiQuery({
    queryKey: queryKeys.portfolioCapacity(params ?? { nowMs: 0, availableMinutes: 0 }),
    queryFn: ({ signal }) => {
      if (!params) {
        return Promise.reject(new ApiError(400, 'A capacity plan needs the available minutes.'))
      }
      return fetchPortfolioCapacity(params, signal)
    },
    enabled: enabled && ready,
    select: toPortfolioCapacity,
  })
}

/* ------------------------------------------------------------------ */
/* Workflows (wire name: blueprints)                                   */
/* ------------------------------------------------------------------ */

export interface AvailableWorkflow {
  id: string
  version: number
}

/** Latest available version of every workflow, sorted by id. */
export function useAvailableWorkflows(): UseQueryResult<AvailableWorkflow[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.workflows(),
    queryFn: ({ signal }) => fetchWorkflowVersions(signal),
    staleTime: 60_000,
    select: (versions) => {
      const latest = new Map<string, number>()
      for (const [id, version] of versions) {
        const known = latest.get(id)
        if (known === undefined || version > known) latest.set(id, version)
      }
      return [...latest.entries()]
        .map(([id, version]) => ({ id, version }))
        .sort((left, right) => left.id.localeCompare(right.id))
    },
  })
}

export function useWorkflowDefinition(
  id: string | undefined,
  version: number | undefined,
): UseQueryResult<Workflow, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.workflow(id ?? '', version ?? 0),
    queryFn: ({ signal }) => fetchWorkflow(id as string, version as number, signal),
    enabled: Boolean(id) && typeof version === 'number',
    staleTime: 5 * 60_000,
    select: toWorkflow,
  })
}

export function useProjectBinding(
  projectId: string | undefined,
): UseQueryResult<WorkflowBinding | null, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.workflowBinding(projectId ?? ''),
    queryFn: ({ signal }) => fetchProjectBinding(projectId as string, signal),
    enabled: Boolean(projectId),
    select: (binding) => (binding ? toWorkflowBinding(binding) : null),
  })
}

export function useWorkItems(projectId: string | undefined): UseQueryResult<WorkItem[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.workItems(projectId ?? ''),
    queryFn: ({ signal }) => fetchWorkItems(projectId as string, signal),
    enabled: Boolean(projectId),
    select: (items) => items.map(toWorkItem),
  })
}

export interface UpgradePreviewParams {
  projectId: string
  workflowId: string
  version: number
}

/** A read: computing the comparison writes nothing (`project_revision` proves it). */
export function useUpgradePreview(
  params: UpgradePreviewParams | null,
): UseQueryResult<WorkflowUpgradePreview, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.upgradePreview(
      params?.projectId ?? '',
      params?.workflowId ?? '',
      params?.version ?? 0,
    ),
    queryFn: ({ signal }) => {
      if (!params) {
        return Promise.reject(new ApiError(400, 'An upgrade preview needs a target version.'))
      }
      return fetchUpgradePreview(params.projectId, params.workflowId, params.version, signal)
    },
    enabled: Boolean(params),
    select: toWorkflowUpgradePreview,
  })
}

/* ------------------------------------------------------------------ */
/* Knowledge (file-authoritative Markdown)                             */
/* ------------------------------------------------------------------ */

export function useKnowledgeTree(): UseQueryResult<KnowledgeNode, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.knowledgeTree(),
    queryFn: ({ signal }) => fetchKnowledgeTree(signal),
    staleTime: 10_000,
  })
}

/** Inventory is the id → path index used for `[[id]]` chips and note titles. */
export function useMarkdownInventory(): UseQueryResult<KnowledgeDocumentInfo[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.knowledgeInventory(),
    queryFn: ({ signal }) => fetchMarkdownInventory(signal),
    staleTime: 10_000,
  })
}

/** A note is a file read: no stale window, so a reload really re-reads the disk. */
export function useMarkdownDocument(
  path: string | undefined,
): UseQueryResult<{ path: string; body: string }, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.knowledgeDocument(path ?? ''),
    queryFn: ({ signal }) => fetchMarkdownDocument(path as string, signal),
    enabled: Boolean(path),
  })
}

export function useMarkdownRelations(
  id: string | undefined,
): UseQueryResult<{ outgoing: string[]; backlinks: string[]; unresolved: string[] }, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.knowledgeRelations(id ?? ''),
    queryFn: ({ signal }) => fetchMarkdownRelations(id as string, signal),
    enabled: Boolean(id),
  })
}

/** Search is a `POST` read; no command is written. */
export function useMarkdownSearch(
  query: string,
  limit = 20,
): UseQueryResult<KnowledgeDocumentInfo[], ApiError> {
  const trimmed = query.trim()
  return useApiQuery({
    queryKey: queryKeys.knowledgeSearch(trimmed, limit),
    queryFn: ({ signal }) => searchMarkdown(trimmed, limit, signal),
    enabled: trimmed.length > 0,
  })
}

/* ------------------------------------------------------------------ */
/* Attachments                                                         */
/* ------------------------------------------------------------------ */

export function useAttachments(
  ownerKind: AttachmentOwnerKind | string | undefined,
  ownerId: string | undefined,
): UseQueryResult<Attachment[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.attachments(ownerKind ?? '', ownerId ?? ''),
    queryFn: ({ signal }) => fetchAttachments(ownerKind as string, ownerId as string, signal),
    enabled: Boolean(ownerKind) && Boolean(ownerId),
    staleTime: 5_000,
    select: (response) => response.attachments.map(toAttachment),
  })
}

/**
 * Image bytes for an inline preview. The `Blob` keeps the response's
 * `Content-Type`, so the object URL an `<img>` receives is the type the server
 * stored. Disabled until the person asks for a preview.
 */
export function useAttachmentContent(
  id: string | undefined,
  enabled: boolean,
): UseQueryResult<Blob, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.attachmentContent(id ?? ''),
    queryFn: ({ signal }) => fetchAttachmentContent(id as string, signal),
    enabled: Boolean(id) && enabled,
    staleTime: Number.POSITIVE_INFINITY,
  })
}

/* ------------------------------------------------------------------ */
/* Agent jobs                                                          */
/* ------------------------------------------------------------------ */

export function useAgentOverview(): UseQueryResult<AgentJobOverview[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.agentOverview(),
    queryFn: ({ signal }) => fetchAgentOverview(signal),
    select: (items) => items.map(toAgentJobOverview),
  })
}

export function useAgentAttention(): UseQueryResult<AgentJob[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.agentAttention(),
    queryFn: ({ signal }) => fetchAgentAttention(signal),
    select: (jobs) => jobs.map(toAgentJob),
  })
}

/**
 * A missing or foreign job answers 403, never 404. The query keeps that error;
 * the detail page turns it into "not visible to this device" rather than
 * pretending the job does not exist.
 */
export function useAgentJob(id: string | undefined): UseQueryResult<AgentJob | null, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.agentJob(id ?? ''),
    queryFn: ({ signal }) => fetchAgentJob(id as string, signal),
    enabled: Boolean(id),
    select: (job) => (job ? toAgentJob(job) : null),
  })
}

export function useAgentHandoffs(
  id: string | undefined,
): UseQueryResult<AgentHandoff[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.agentHandoffs(id ?? ''),
    queryFn: ({ signal }) => fetchAgentHandoffs(id as string, signal),
    enabled: Boolean(id),
    select: (entries) => entries.map(toAgentHandoff),
  })
}

export function useAgentReview(id: string | undefined): UseQueryResult<AgentReview | null, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.agentReview(id ?? ''),
    queryFn: ({ signal }) => fetchAgentReview(id as string, signal),
    enabled: Boolean(id),
    select: (review) => (review ? toAgentReview(review) : null),
  })
}

export function useAgentResumeContext(
  id: string | undefined,
): UseQueryResult<AgentResumeContext, ApiError> {
  return useApiQuery({
    queryKey: queryKeys.agentResumeContext(id ?? ''),
    queryFn: ({ signal }) => fetchAgentResumeContext(id as string, signal),
    enabled: Boolean(id),
    select: toAgentResumeContext,
  })
}

/* ------------------------------------------------------------------ */
/* Bundled workflows                                                   */
/* ------------------------------------------------------------------ */

/**
 * The workflows the server can install from its own package, with the revision
 * this binary ships. Short-lived so a just-installed workflow drops off the
 * list; the install mutation also invalidates both this read and
 * `useAvailableWorkflows`.
 */
export function useBundledWorkflows(): UseQueryResult<BundledWorkflowDescriptor[], ApiError> {
  return useApiQuery({
    queryKey: queryKeys.bundledWorkflows(),
    queryFn: ({ signal }) => fetchBundledWorkflows(signal),
    staleTime: 60_000,
  })
}
