import { api } from '../client'
import { withQuery } from '../http'
import type {
  ApplyUpgradeBody,
  BindWorkflowBody,
  BlueprintBindingRecord,
  BlueprintManifest,
  BlueprintUpgradeApplied,
  BlueprintUpgradePreview,
  BlueprintWorkItemRecord,
  BundledWorkflowDescriptor,
  CreateWorkItemBody,
  WorkItemPropertiesBody,
  WorkItemTransitionBody,
} from '../contracts'

const BASE = '/authoritative/blueprints'

/** `[id, version]` pairs of every available manifest, newest version included. */
export function fetchWorkflowVersions(signal?: AbortSignal): Promise<[string, number][]> {
  return api.get<[string, number][]>(BASE, { signal })
}

/** One immutable published version. 404 when it does not exist. */
export function fetchWorkflow(
  id: string,
  version: number,
  signal?: AbortSignal,
): Promise<BlueprintManifest> {
  return api.get<BlueprintManifest>(withQuery(`${BASE}/show`, { id, version }), { signal })
}

/** The project's pinned version, or `null` when it has none. */
export function fetchProjectBinding(
  projectId: string,
  signal?: AbortSignal,
): Promise<BlueprintBindingRecord | null> {
  return api.get<BlueprintBindingRecord | null>(
    withQuery(`${BASE}/binding`, { project_id: projectId }),
    { signal },
  )
}

export function bindWorkflow(body: BindWorkflowBody): Promise<BlueprintBindingRecord> {
  return api.post<BlueprintBindingRecord>(`${BASE}/bind`, body)
}

/**
 * Trap 3, half one: `?id=` returns **one object** (404 when absent).
 * This is deliberately a separate function from `fetchWorkItems`; the route is
 * polymorphic and a single helper that branched on `Array.isArray` would hide
 * the 404 case behind an empty list.
 */
export function fetchWorkItem(id: string, signal?: AbortSignal): Promise<BlueprintWorkItemRecord> {
  return api.get<BlueprintWorkItemRecord>(withQuery(`${BASE}/items`, { id }), { signal })
}

/**
 * Trap 3, half two: `?project_id=` returns an **array** (empty when the project
 * has no items). Never call this with an id, and never call `fetchWorkItem`
 * with a project id.
 */
export function fetchWorkItems(
  projectId: string,
  signal?: AbortSignal,
): Promise<BlueprintWorkItemRecord[]> {
  return api.get<BlueprintWorkItemRecord[]>(
    withQuery(`${BASE}/items`, { project_id: projectId }),
    { signal },
  )
}

export function createWorkItem(body: CreateWorkItemBody): Promise<BlueprintWorkItemRecord> {
  return api.post<BlueprintWorkItemRecord>(`${BASE}/items`, body)
}

/** `expected_revision` is the item's own `u64` revision, not the project's. */
export function transitionWorkItem(body: WorkItemTransitionBody): Promise<BlueprintWorkItemRecord> {
  return api.post<BlueprintWorkItemRecord>(`${BASE}/transition`, body)
}

export function updateWorkItemProperties(
  body: WorkItemPropertiesBody,
): Promise<BlueprintWorkItemRecord> {
  return api.post<BlueprintWorkItemRecord>(`${BASE}/properties`, body)
}

/** Read-only comparison; every diff and blocker is computed by the authority. */
export function fetchUpgradePreview(
  projectId: string,
  id: string,
  version: number,
  signal?: AbortSignal,
): Promise<BlueprintUpgradePreview> {
  return api.get<BlueprintUpgradePreview>(
    withQuery(`${BASE}/upgrade-preview`, { project_id: projectId, id, version }),
    { signal },
  )
}

/**
 * Explicit re-pin. `confirm` must be `true` (the server answers 400
 * "confirmation required" otherwise) and `fingerprint` must name the preview
 * the caller was actually shown (400 when missing, 409 when it no longer
 * matches). The client never derives the fingerprint from the target version:
 * an apply can only target a version the person was shown.
 */
export function applyUpgrade(body: ApplyUpgradeBody): Promise<BlueprintUpgradeApplied> {
  return api.post<BlueprintUpgradeApplied>(`${BASE}/upgrade`, body)
}

/**
 * Trusted workflows this server can install, with the revision this binary
 * ships. Not the same list as `fetchWorkflowVersions` (published revisions):
 * an id may be present here at a newer version than the store holds.
 */
export function fetchBundledWorkflows(signal?: AbortSignal): Promise<BundledWorkflowDescriptor[]> {
  return api.get<BundledWorkflowDescriptor[]>(`${BASE}/bundled`, { signal })
}

/** Publish one bundled workflow. Idempotent: a second install returns the same version. */
export function installBundledWorkflow(id: string): Promise<BlueprintManifest> {
  return api.post<BlueprintManifest>(`${BASE}/install`, { id })
}
