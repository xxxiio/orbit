/**
 * Seed the persistent dogfooding workspace **once**.
 *
 * Split by what the product itself supports, and recorded that way in the
 * session log:
 *
 * - **Bulk volume through the public API** (`seedWorkspaceForServer`): seven
 *   projects, 122 tasks with priorities/deadlines/acceptance criteria/
 *   prerequisites, milestones, markdown notes, attachments and agent jobs. This
 *   is the "existing work" a person would inherit; typing 122 tasks through the
 *   UI would only prove the capture form repeats.
 * - **Capability wiring through the operator API**: installing a trusted
 *   capability and granting it to a project are `operator_device` routes by
 *   design — the SPA has no control for them (`capability.ts` says so), so they
 *   are seeded here and the *execution* is driven through the UI later.
 * - Everything else in the session — projects, tasks, priorities, plans, saved
 *   views, notes, links, references, completion — is done through the browser.
 *
 * A second call is a no-op: the fixture is a fixed command sequence and the
 * database is persistent, so reseeding is only valid on a database at revision
 * 0. The state file records what happened instead.
 */
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { REPO_ROOT } from '../harness/server'
import { seedWorkspaceForServer, type WorkspaceFixture } from '../../fixture/seed'
import { fixtureNowMs } from '../harness/seed'
import type { DogfoodWorkspace, WorkspaceState } from './workspace'

export const CAPABILITY_PROJECT = 'p-dogfood-capability'
export const CAPABILITY_ITEM = 'wi-dogfood-capability-note'
export const REFERENCE_CAPABILITY = 'orbit.reference.work-item-note'
export const REFERENCE_WORKFLOW = 'capability-reference'

export interface SeedReport {
  /** False when this run found the workspace already seeded. */
  seeded: boolean
  fixture: WorkspaceFixture | null
  capability: { projectId: string; itemId: string; created: boolean }
}

const CSRF = { 'x-orbit-csrf': '1' }

async function operator(
  workspace: DogfoodWorkspace,
  path: string,
  body: unknown,
): Promise<{ status: number; body: unknown }> {
  const response = await fetch(`${workspace.base}/v1/authoritative/${path}`, {
    method: 'POST',
    headers: {
      ...CSRF,
      authorization: `Bearer ${workspace.state.token}`,
      'x-orbit-device-id': workspace.state.deviceId,
      'content-type': 'application/json',
    },
    body: JSON.stringify(body),
  })
  const text = await response.text()
  return { status: response.status, body: text.length > 0 ? JSON.parse(text) : null }
}

async function readProjects(
  workspace: DogfoodWorkspace,
): Promise<{ revision: number; projects: Array<{ id: string }> }> {
  const response = await fetch(`${workspace.base}/v1/authoritative/projects`, {
    headers: {
      authorization: `Bearer ${workspace.state.token}`,
      'x-orbit-device-id': workspace.state.deviceId,
    },
  })
  if (response.status !== 200) {
    throw new Error(`GET projects → ${response.status} ${await response.text()}`)
  }
  return (await response.json()) as { revision: number; projects: Array<{ id: string }> }
}

function referenceBlueprint(): Record<string, unknown> {
  return JSON.parse(
    readFileSync(join(REPO_ROOT, 'blueprints', 'examples', 'capability-reference.json'), 'utf8'),
  ) as Record<string, unknown>
}

/** Install + grant once; announce the executor on every run (an announcement is idempotent). */
async function ensureCapability(
  workspace: DogfoodWorkspace,
): Promise<{ projectId: string; itemId: string; created: boolean }> {
  const announce = async () => {
    const announced = await operator(workspace, 'capability-executors/announce', {
      capability_ids: [REFERENCE_CAPABILITY],
    })
    if (announced.status !== 200) {
      throw new Error(
        `capability-executors/announce → ${announced.status} ${JSON.stringify(announced.body)}`,
      )
    }
  }
  const existing = await readProjects(workspace)
  const project = existing.projects.find((candidate) => candidate.id === CAPABILITY_PROJECT)
  if (project) {
    await announce()
    return { projectId: CAPABILITY_PROJECT, itemId: CAPABILITY_ITEM, created: false }
  }

  const installed = await operator(workspace, 'capabilities/install', {
    command_id: 'dogfood-install-reference-capability',
    capability_id: REFERENCE_CAPABILITY,
  })
  if (installed.status !== 200) {
    throw new Error(`capabilities/install → ${installed.status} ${JSON.stringify(installed.body)}`)
  }
  const published = await operator(workspace, 'blueprints', { definition: referenceBlueprint() })
  if (published.status !== 200) {
    throw new Error(`blueprints (reference) → ${published.status} ${JSON.stringify(published.body)}`)
  }
  const created = await operator(workspace, 'project-commands', {
    command_id: 'dogfood-capability-project',
    expected_revision: existing.revision,
    project_id: CAPABILITY_PROJECT,
    name: 'Connected capability demo',
    workflow_id: REFERENCE_WORKFLOW,
  })
  if (created.status !== 200) {
    throw new Error(`project-commands (capability) → ${created.status} ${JSON.stringify(created.body)}`)
  }
  const item = await operator(workspace, 'blueprints/items', {
    id: CAPABILITY_ITEM,
    project_id: CAPABILITY_PROJECT,
    kind: 'note-task',
  })
  if (item.status !== 200) {
    throw new Error(`blueprints/items → ${item.status} ${JSON.stringify(item.body)}`)
  }
  const grant = await operator(workspace, 'capabilities/authorize', {
    command_id: 'dogfood-authorize-reference-capability',
    capability_id: REFERENCE_CAPABILITY,
    project_id: CAPABILITY_PROJECT,
    confirm: true,
  })
  if (grant.status !== 200) {
    throw new Error(`capabilities/authorize → ${grant.status} ${JSON.stringify(grant.body)}`)
  }
  await announce()
  return { projectId: CAPABILITY_PROJECT, itemId: CAPABILITY_ITEM, created: true }
}

export async function ensureSeeded(workspace: DogfoodWorkspace): Promise<SeedReport> {
  const state: WorkspaceState = workspace.state
  if (state.seededAtMs !== null) {
    const capability = await ensureCapability(workspace)
    return { seeded: false, fixture: null, capability }
  }
  if (state.runs.length > 0) {
    throw new Error(
      'workspace.json says runs completed but the seed marker is missing; refusing to seed a database that already holds UI work',
    )
  }

  // Recovery: an earlier attempt may have completed the bulk seed and then
  // failed before writing the marker. The fixture project is proof of that; the
  // fixed command sequence cannot run twice with a different clock.
  const existing = await readProjects(workspace).catch(() => null)
  if (existing?.projects.some((project) => project.id === 'p-orbit-development')) {
    state.seededAtMs = Date.now()
    state.seed = { recoveredFromExistingDatabase: true }
    await workspace.save()
    const capability = await ensureCapability(workspace)
    return { seeded: false, fixture: null, capability }
  }

  const nowMs = fixtureNowMs()
  const fixture = await seedWorkspaceForServer({
    base: workspace.base,
    deviceId: state.deviceId,
    token: state.token,
    nowMs,
  })
  // Record the bulk seed *before* the capability wiring: the fixture is a fixed
  // command sequence and can only run against a database at revision 0, so a
  // later half-failed setup must never look like "seed me again".
  state.seededAtMs = Date.now()
  state.seed = {
    nowMs,
    counts: fixture.counts,
    projects: fixture.projects,
    milestones: fixture.milestones,
    notes: fixture.notes.length,
    agentJobs: fixture.agentJobs,
  }
  await workspace.save()

  const capability = await ensureCapability(workspace)
  ;(state.seed as Record<string, unknown>).capability = {
    projectId: CAPABILITY_PROJECT,
    itemId: CAPABILITY_ITEM,
  }
  await workspace.save()
  return { seeded: true, fixture, capability }
}
