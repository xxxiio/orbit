import type { ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { AuthoritySnapshotResponse, TaskRecord } from '@/api/contracts'

export function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

export function plainTextResponse(body: string, status = 400): Response {
  return new Response(body, { status, headers: { 'Content-Type': 'text/plain' } })
}

export function testQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: false },
    },
  })
}

export function queryWrapper(client: QueryClient) {
  return function QueryWrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>
  }
}

/** A minimally complete `GET /snapshot` payload. */
export function snapshotAt(
  revision: number,
  overrides: Partial<AuthoritySnapshotResponse> = {},
): AuthoritySnapshotResponse {
  return {
    revision,
    history_fingerprint: 'a'.repeat(64),
    projects: [],
    tasks: [],
    task_dependencies: [],
    habits: [],
    habit_occurrences: [],
    metrics: [],
    metric_observations: [],
    schedule: [],
    routines: [],
    milestones: [],
    strategies: [],
    ...overrides,
  }
}

/** `TaskRecord` as serialised when both ids are null: the keys are absent. */
export function taskRecord(overrides: Partial<TaskRecord> = {}): TaskRecord {
  return {
    id: 't-1',
    title: 'Ship the release',
    status: 'ready',
    estimated_minutes: null,
    earliest_start_at_ms: null,
    deadline_at_ms: null,
    description: '',
    priority: null,
    labels: [],
    ...overrides,
  }
}

/** Remove keys entirely, which is what `skip_serializing_if` does on the wire. */
export function withoutKeys<T extends object>(value: T, ...keys: Array<keyof T>): T {
  const copy = { ...value }
  for (const key of keys) delete copy[key]
  return copy
}
