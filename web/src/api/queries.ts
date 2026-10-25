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
import { useQuery, useQueryClient, type QueryKey, type UseQueryResult } from '@tanstack/react-query'
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
import { fetchMilestones } from './endpoints/milestones'
import { fetchSnapshot } from './endpoints/snapshot'
import { fetchAcceptanceCriteria, fetchTaskActivity, fetchTaskDetail } from './endpoints/tasks'
import {
  fetchProjectBinding,
  fetchUpgradePreview,
  fetchWorkItems,
  fetchWorkflow,
  fetchWorkflowVersions,
} from './endpoints/workflows'
import {
  toAcceptanceCriterion,
  toActivityEntry,
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
import type { MilestoneSnapshot } from './contracts'

/** The snapshot is the shared basis for almost every screen: keep it warm for 5s. */
export const SNAPSHOT_STALE_TIME = 5_000

export const queryKeys = {
  /** Shared with the scaffold's session module so there is exactly one cache entry. */
  session: () => sessionQueryKey,
  snapshot: () => ['snapshot'] as const,
  task: (id: string) => ['task', id] as const,
  taskAcceptanceCriteria: (id: string) => ['task', id, 'acceptance-criteria'] as const,
  taskActivity: (id: string) => ['task', id, 'activity'] as const,
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
  markdownManifest: () => ['knowledge', 'manifest'] as const,
  attachments: (ownerKind: string, ownerId: string) => ['attachments', ownerKind, ownerId] as const,
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
