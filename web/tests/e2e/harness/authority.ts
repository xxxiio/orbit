/**
 * Authoritative reads and writes used by the I1 acceptance specs.
 *
 * Everything goes through the browser's own cookie jar (`page.request`), so a
 * spec reads exactly the server the app talked to, as the signed-in device.
 * Writes carry the same `x-orbit-csrf` header the SPA sends: there is no
 * shortcut around the transport.
 *
 * Helpers return the raw status and parsed body when a spec needs to assert a
 * *refusal*, and a narrow typed shape when it only needs the accepted state.
 */
import type { Page } from '@playwright/test'
import { expect } from './fixtures'
import type { E2EServer } from './server'

const BASE = '/v1/authoritative'

/** The SPA sends this on every non-GET request. */
const CSRF = { 'x-orbit-csrf': '1' }

export interface CallResult {
  status: number
  body: Record<string, any> | null
}

/** One raw call, for specs that assert a refusal instead of a value. */
export async function postAuthority(
  page: Page,
  server: E2EServer,
  path: string,
  body: unknown,
): Promise<CallResult> {
  const response = await page.request.post(`${server.base}${BASE}/${path}`, {
    data: body,
    headers: CSRF,
  })
  return { status: response.status(), body: await jsonBody(response) }
}

export async function getAuthority(
  page: Page,
  server: E2EServer,
  path: string,
): Promise<CallResult> {
  const response = await page.request.get(`${server.base}${BASE}/${path}`)
  return { status: response.status(), body: await jsonBody(response) }
}

async function jsonBody(response: { text(): Promise<string> }): Promise<Record<string, any> | null> {
  const text = await response.text()
  if (text.length === 0) return null
  try {
    return JSON.parse(text) as Record<string, any>
  } catch {
    return { raw: text }
  }
}

/* ------------------------------------------------------------------ */
/* Projects                                                            */
/* ------------------------------------------------------------------ */

export interface AuthoritativeProject {
  id: string
  name: string
  status: string
}

export interface ProjectsRead {
  revision: number
  projects: AuthoritativeProject[]
}

export async function readProjects(page: Page, server: E2EServer): Promise<ProjectsRead> {
  const { status, body } = await getAuthority(page, server, 'projects')
  expect(status, 'GET /v1/authoritative/projects').toBe(200)
  return body as unknown as ProjectsRead
}

export async function projectById(
  page: Page,
  server: E2EServer,
  projectId: string,
): Promise<AuthoritativeProject> {
  const read = await readProjects(page, server)
  const project = read.projects.find((candidate) => candidate.id === projectId)
  expect(project, `project ${projectId} must exist on the server`).toBeTruthy()
  return project as AuthoritativeProject
}

export interface ProjectStatusEdge {
  status: string
  next: string[]
}

/** Every status in domain order with the legal moves the server reports. */
export async function projectStatuses(
  page: Page,
  server: E2EServer,
): Promise<ProjectStatusEdge[]> {
  const { status, body } = await getAuthority(page, server, 'project-statuses')
  expect(status, 'GET /v1/authoritative/project-statuses').toBe(200)
  return (body as { statuses: ProjectStatusEdge[] }).statuses
}

export interface ProjectUpdateInput {
  commandId: string
  projectId: string
  expectedRevision: number
  name?: string
  status?: string
}

/** The same command the identity form sends, from the test's own transport. */
export function updateProject(
  page: Page,
  server: E2EServer,
  input: ProjectUpdateInput,
): Promise<CallResult> {
  return postAuthority(page, server, 'project-commands', {
    command_id: input.commandId,
    expected_revision: input.expectedRevision,
    action: 'update',
    project_id: input.projectId,
    name: input.name ?? '',
    ...(input.status ? { status: input.status } : {}),
  })
}

export interface BlueprintBinding {
  project_id: string
  blueprint_id: string
  version: number
}

/** The workflow a project is pinned to; `null` when it has none. */
export async function blueprintBinding(
  page: Page,
  server: E2EServer,
  projectId: string,
): Promise<BlueprintBinding | null> {
  const { status, body } = await getAuthority(
    page,
    server,
    `blueprints/binding?project_id=${encodeURIComponent(projectId)}`,
  )
  expect(status, 'GET /v1/authoritative/blueprints/binding').toBe(200)
  return body as BlueprintBinding | null
}

/** The authority revision as the task read reports it. */
export async function authorityRevision(page: Page, server: E2EServer): Promise<number> {
  const { status, body } = await getAuthority(page, server, 'tasks')
  expect(status, 'GET /v1/authoritative/tasks').toBe(200)
  return (body as { revision: number }).revision
}

/* ------------------------------------------------------------------ */
/* Tasks                                                               */
/* ------------------------------------------------------------------ */

export interface TaskDetailRecord {
  id: string
  title: string
  status: string
  project_id: string | null
  created_at_ms: number | null
  updated_at_ms: number | null
}

export async function readTaskRecord(
  page: Page,
  server: E2EServer,
  taskId: string,
): Promise<TaskDetailRecord> {
  const { status, body } = await getAuthority(
    page,
    server,
    `tasks/${encodeURIComponent(taskId)}`,
  )
  expect(status, `GET /v1/authoritative/tasks/${taskId}`).toBe(200)
  return body as unknown as TaskDetailRecord
}

/* ------------------------------------------------------------------ */
/* Saved views                                                         */
/* ------------------------------------------------------------------ */

export interface SavedViewRecord {
  id: string
  name: string
  scope: 'project' | 'inbox' | 'home'
  project_id: string | null
  query: string
  display: 'list' | 'board'
  version: number
  created_at_ms: number
  updated_at_ms: number
}

export async function readSavedViews(
  page: Page,
  server: E2EServer,
): Promise<SavedViewRecord[]> {
  const { status, body } = await getAuthority(page, server, 'saved-views')
  expect(status, 'GET /v1/authoritative/saved-views').toBe(200)
  return (body as { views: SavedViewRecord[] }).views
}

export interface SavedViewSaveInput {
  commandId: string
  viewId: string
  name: string
  scope: 'project' | 'inbox' | 'home'
  projectId: string | null
  query: string
  display: 'list' | 'board'
}

export function saveViewViaApi(
  page: Page,
  server: E2EServer,
  input: SavedViewSaveInput,
): Promise<CallResult> {
  return postAuthority(page, server, 'saved-views/commands', {
    type: 'save',
    command_id: input.commandId,
    view_id: input.viewId,
    expected_version: 0,
    name: input.name,
    scope: input.scope,
    project_id: input.projectId,
    query: input.query,
    display: input.display,
  })
}

export function renameViewViaApi(
  page: Page,
  server: E2EServer,
  input: { commandId: string; viewId: string; expectedVersion: number; name: string },
): Promise<CallResult> {
  return postAuthority(page, server, 'saved-views/commands', {
    type: 'rename',
    command_id: input.commandId,
    view_id: input.viewId,
    expected_version: input.expectedVersion,
    name: input.name,
  })
}

export function deleteViewViaApi(
  page: Page,
  server: E2EServer,
  input: { commandId: string; viewId: string; expectedVersion: number },
): Promise<CallResult> {
  return postAuthority(page, server, 'saved-views/commands', {
    type: 'delete',
    command_id: input.commandId,
    view_id: input.viewId,
    expected_version: input.expectedVersion,
  })
}

/* ------------------------------------------------------------------ */
/* Search                                                              */
/* ------------------------------------------------------------------ */

export interface SearchGroupRecord {
  kind: 'project' | 'task' | 'note'
  truncated: boolean
  results: Array<{
    kind: 'project' | 'task' | 'note'
    id: string
    title: string
    subtitle: string | null
    project_id: string | null
    path: string | null
  }>
}

export interface SearchRead {
  revision: number
  query: string
  limit: number
  groups: SearchGroupRecord[]
}

export async function search(
  page: Page,
  server: E2EServer,
  query: string,
): Promise<SearchRead> {
  const { status, body } = await getAuthority(
    page,
    server,
    `search?q=${encodeURIComponent(query)}`,
  )
  expect(status, 'GET /v1/authoritative/search').toBe(200)
  return body as unknown as SearchRead
}

/* ------------------------------------------------------------------ */
/* Knowledge                                                           */
/* ------------------------------------------------------------------ */

export async function noteDocument(
  page: Page,
  server: E2EServer,
  path: string,
): Promise<{ path: string; body: string }> {
  const { status, body } = await getAuthority(
    page,
    server,
    `markdown-document?path=${encodeURIComponent(path)}`,
  )
  expect(status, `GET /v1/authoritative/markdown-document ${path}`).toBe(200)
  return body as unknown as { path: string; body: string }
}
