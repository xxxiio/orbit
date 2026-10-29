/**
 * Accepted-write helpers: one home for "the server answered; make the cache
 * agree without ever going backwards".
 *
 * Every command in this product fences on a revision. A response that arrives
 * late, or a replayed receipt, can therefore carry a revision older than the
 * tip this browser already knows. Writing it back blindly would rewind the
 * cache and make the *next* command fence on a version the server has already
 * left behind. So the rules live here, once:
 *
 * - a ledger revision is raised with `Math.max` in both snapshots;
 * - an accepted task record is written only when its command is not older than
 *   the cached snapshot;
 * - an accepted work-item record replaces older cached copies but never a newer
 *   one, and a create appends only to its own project's list.
 *
 * This is cache coherence, not an optimistic update: it runs only after the
 * server accepted the write, and it stores exactly what the server returned.
 */
import type { QueryClient, QueryKey } from '@tanstack/react-query'
import type { Project, SavedView, Task, WorkItem } from './adapters'
import type {
  AuthoritySnapshotResponse,
  BlueprintWorkItemRecord,
  TaskDetailResponse,
} from './contracts'
import { ClientStateError } from './errors'
import { queryKeys } from './queries'

/**
 * Write the revision an accepted response carries back into the snapshot it
 * fences on, never lowering what is already cached: a duplicate receipt carries
 * the revision the *original* command had, which can be older than the tip this
 * browser already knows about.
 *
 * Without this, two commands in one interaction (a bulk loop, status then
 * priority in the drawer) would both fence on the pre-write revision and the
 * server would correctly reject the second as stale.
 */
export function acceptRevision(
  queryClient: QueryClient,
  queryKey: QueryKey,
  revision: number,
): void {
  queryClient.setQueryData<{ revision: number }>(queryKey, (previous) =>
    previous ? { ...previous, revision: Math.max(previous.revision, revision) } : previous,
  )
}

/**
 * Every ledger write advances the *global* authority revision, and both cached
 * snapshots (`snapshot` and `milestones`) read the same `MAX(cursor)`. Advancing
 * only one of them lets the other fence a later command against an older
 * revision and 409 "stale" even though nothing it reads changed. So every
 * accepted ledger write calls this once, and neither cached revision is ever
 * lowered (a replayed receipt carries the original, older revision).
 */
export function acceptLedgerRevision(queryClient: QueryClient, revision: number): void {
  acceptRevision(queryClient, queryKeys.snapshot(), revision)
  acceptRevision(queryClient, queryKeys.milestones(), revision)
  acceptRevision(queryClient, queryKeys.epics(), revision)
  // An accepted ledger write can change the item set (and therefore the
  // upgrade fingerprint) any open preview was computed from. Refreshing it here
  // keeps Apply from ever confirming a comparison the person was not shown.
  void queryClient.invalidateQueries({ queryKey: queryKeys.upgradePreviewAll() })
}


/* ------------------------------------------------------------------ */
/* Saved views                                                         */
/* ------------------------------------------------------------------ */

/**
 * Write an accepted saved-view row into the cached list, newest first (the
 * server's own order). A replay or out-of-order answer carrying an older
 * version leaves the newer row alone, so a rename is never rewound.
 */
export function acceptSavedView(queryClient: QueryClient, view: SavedView): void {
  queryClient.setQueryData<SavedView[]>(queryKeys.savedViews(), (previous) => {
    if (!previous) return previous
    const current = previous.find((candidate) => candidate.id === view.id)
    if (current && current.version > view.version) return previous
    return [view, ...previous.filter((candidate) => candidate.id !== view.id)]
  })
}

/**
 * Remove an accepted deleted row. The version guard prevents a late delete
 * answer from removing a row that a newer rename has already replaced.
 */
export function removeSavedView(
  queryClient: QueryClient,
  viewId: string,
  version: number,
): void {
  queryClient.setQueryData<SavedView[]>(queryKeys.savedViews(), (previous) => {
    if (!previous) return previous
    const current = previous.find((candidate) => candidate.id === viewId)
    if (!current || current.version > version) return previous
    return previous.filter((candidate) => candidate.id !== viewId)
  })
}

/* ------------------------------------------------------------------ */
/* Project records                                                     */
/* ------------------------------------------------------------------ */

/**
 * Write an accepted project row into the cached snapshot. Every project label
 * on screen (sidebar, header, project list, recent destinations, command
 * palette) reads this projection, so the rename is visible before the refetch
 * settles. The id is the key; a row the cache does not hold is left alone.
 */
export function acceptProjectRecord(
  queryClient: QueryClient,
  project: Project,
  revision: number = Number.POSITIVE_INFINITY,
): void {
  const snapshot = queryClient.getQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot())
  // A replayed receipt carries the revision the original command had. If a
  // newer accepted write has already advanced the snapshot, its record wins.
  if (snapshot && snapshot.revision > revision) return
  queryClient.setQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot(), (previous) => {
    if (!previous) return previous
    let changed = false
    const projects = previous.projects.map((record) => {
      if (record.id !== project.id) return record
      changed = true
      return { ...record, name: project.name, status: project.status }
    })
    return changed ? { ...previous, projects } : previous
  })
}

/* ------------------------------------------------------------------ */
/* Task records                                                        */
/* ------------------------------------------------------------------ */

/** The record fields a task command response carries (detail wire shape). */
function taskRecordFields(task: Task) {
  return {
    title: task.title,
    status: task.status,
    project_id: task.projectId,
    milestone_id: task.milestoneId,
    estimated_minutes: task.estimatedMinutes,
    earliest_start_at_ms: task.earliestStartAtMs,
    deadline_at_ms: task.deadlineAtMs,
    description: task.description,
    priority: task.priority,
    labels: [...task.labels],
  }
}

/** The same fields in the snapshot's shape, where absent ids are omitted. */
function taskSnapshotFields(task: Task) {
  return {
    ...taskRecordFields(task),
    project_id: task.projectId ?? undefined,
    milestone_id: task.milestoneId ?? undefined,
  }
}

/**
 * Write an accepted task command's response back into the read caches.
 *
 * The task drawer keeps a full record in memory and sends `set_details` as a
 * *full* record (description, priority and labels together). Without a write
 * back, a second edit made in the window between "server accepted" and "refetch
 * settled" would be built from the pre-write record and silently revert the
 * first edit.
 *
 * `revision` is the ledger revision the accepted command carried. A response
 * older than the cached snapshot is a replay or an out-of-order arrival: its
 * record is skipped so a newer edit is never rewound. Callers without a
 * revision (a local cache test) pass the cached tip and still write.
 */
export function acceptTaskRecord(
  queryClient: QueryClient,
  task: Task,
  revision: number = Number.POSITIVE_INFINITY,
): void {
  const snapshot = queryClient.getQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot())
  if (snapshot && snapshot.revision > revision) return

  queryClient.setQueryData<TaskDetailResponse>(queryKeys.task(task.id), (previous) =>
    previous ? { ...previous, ...taskRecordFields(task) } : previous,
  )

  queryClient.setQueryData<AuthoritySnapshotResponse>(queryKeys.snapshot(), (previous) => {
    if (!previous) return previous
    let changed = false
    const tasks = previous.tasks.map((record) => {
      if (record.id !== task.id) return record
      changed = true
      return { ...record, ...taskSnapshotFields(task) }
    })
    return changed ? { ...previous, tasks } : previous
  })
}

/* ------------------------------------------------------------------ */
/* Work-item records                                                   */
/* ------------------------------------------------------------------ */

/**
 * One cached work-item record, whatever the cached shape: the project list is an
 * array, a single-item snapshot is the record itself.
 */
type CachedWorkItem = BlueprintWorkItemRecord | BlueprintWorkItemRecord[] | undefined

function findCachedWorkItem(cached: CachedWorkItem, id: string): BlueprintWorkItemRecord | undefined {
  if (!cached) return undefined
  if (Array.isArray(cached)) return cached.find((candidate) => candidate.id === id)
  return cached.id === id ? cached : undefined
}

/** An accepted work item as the cached list stores it (the wire record). */
export function workItemRecord(item: WorkItem): BlueprintWorkItemRecord {
  return {
    id: item.id,
    project_id: item.projectId,
    kind: item.kind,
    state: item.state,
    properties: item.properties,
    revision: item.revision,
  }
}

/**
 * Write the record an accepted work-item response carries back into every
 * cached copy of it: the project list(s) that hold it and any single-item
 * snapshot. A cached copy is never rewound: a replayed or out-of-order answer
 * carrying an older revision leaves the newer record in place, so the next
 * command fences on the tip this browser has already seen.
 */
export function acceptWorkItemRecord(queryClient: QueryClient, item: WorkItem): void {
  const record = workItemRecord(item)
  const cached = queryClient.getQueriesData<CachedWorkItem>({
    queryKey: queryKeys.workItemsAll(),
  })
  for (const [key, data] of cached) {
    if (!data) continue
    if (Array.isArray(data)) {
      const index = data.findIndex((candidate) => candidate.id === record.id)
      if (index >= 0) {
        if (data[index].revision > record.revision) continue
        const next = [...data]
        next[index] = record
        queryClient.setQueryData(key, next)
        continue
      }
      // A create accepted before the list's first refetch: append to *this*
      // project's list only (the key is `['work-items', projectId]`).
      if (key[1] === item.projectId) {
        queryClient.setQueryData(key, [...data, record])
      }
      continue
    }
    if (data.id !== record.id || data.revision > record.revision) continue
    queryClient.setQueryData(key, record)
  }
}

/** Read the freshest cached revision for a work item, for its next fence. */
export function workItemRevision(queryClient: QueryClient, id: string): number {
  const cached = queryClient.getQueriesData<CachedWorkItem>({
    queryKey: queryKeys.workItemsAll(),
  })
  let revision: number | null = null
  for (const [, data] of cached) {
    const item = findCachedWorkItem(data, id)
    if (!item) continue
    revision = revision === null ? item.revision : Math.max(revision, item.revision)
  }
  if (revision === null) {
    throw new ClientStateError('Reload this project before changing its work items.')
  }
  return revision
}
