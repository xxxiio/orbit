/**
 * Accepted task write-back — a feature-side alias.
 *
 * The implementation lives in `@/api/accepted-write` with the work-item and
 * ledger write-backs, so the revision discipline is one rule. This module keeps
 * the task kernel's import path (`@/features/tasks` re-exports it) and passes
 * the command's accepted revision through, so a late or replayed answer can
 * never rewind a newer cached record.
 */
import type { QueryClient } from '@tanstack/react-query'
import type { Task } from '@/api/adapters'
import { acceptTaskRecord } from '@/api/accepted-write'

export function writeAcceptedTask(
  queryClient: QueryClient,
  task: Task,
  revision?: number,
): void {
  acceptTaskRecord(queryClient, task, revision)
}
