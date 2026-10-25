/**
 * Authoritative reads used by the workspace acceptance specs.
 *
 * The specs assert against the same server the browser talked to; these helpers
 * only issue those reads and narrow the shapes, so no bucket repeats the URL or
 * the response envelope.
 */
import type { Locator, Page } from '@playwright/test'
import { expect } from './fixtures'
import type { E2EServer } from './server'

interface Edge {
  task_id: string
  prerequisite_id: string
}

interface AuthoritativeTask {
  id: string
  status: string
  title: string
}

export type { AuthoritativeTask, Edge }

export async function tasksFor(
  page: Page,
  server: E2EServer,
  projectId: string,
): Promise<AuthoritativeTask[]> {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks`)
  expect(response.status()).toBe(200)
  const body = (await response.json()) as {
    tasks: Array<AuthoritativeTask & { project_id?: string }>
  }
  return body.tasks.filter((task) => task.project_id === projectId)
}

export async function taskDetail(
  page: Page,
  server: E2EServer,
  id: string,
): Promise<AuthoritativeTask> {
  const response = await page.request.get(`${server.base}/v1/authoritative/tasks/${id}`)
  expect(response.status()).toBe(200)
  return (await response.json()) as AuthoritativeTask
}

export async function dependencies(page: Page, server: E2EServer): Promise<Edge[]> {
  const response = await page.request.get(`${server.base}/v1/authoritative/task-dependencies`)
  expect(response.status()).toBe(200)
  return ((await response.json()) as { edges: Edge[] }).edges
}

/** One board column, addressed by its accessible name prefix. */
export function boardColumn(page: Page, label: RegExp): Locator {
  return page.getByRole('region', { name: label })
}
