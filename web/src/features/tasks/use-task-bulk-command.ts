import { useCallback, useMemo, useState } from 'react'
import type { Task } from '@/api/adapters'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import type { CommandMutation, TaskCommandAttempt, TaskMutationOutcome } from '@/hooks'
import { failureMessage, isAmbiguousError, taskCommandAttempt } from '@/hooks'
import { useToast } from '@/components/ui/toast'
import { bulkActionLabel, bulkValueOptions, type BulkAction } from './task-list-model'

interface BulkProgress {
  label: string
  total: number
  done: number
}

/**
 * The bulk command loop shared by list pages: one request per selected task,
 * progress in place, and a partial-failure summary the server's message feeds.
 * Nothing is applied optimistically; the list still shows the server's values.
 *
 * Each iteration mints its own idempotency key and fences on the revision the
 * previous acceptance wrote back into the snapshot cache, so task #2 onwards is
 * not rejected as stale after task #1 advanced the authority.
 */
export function useTaskBulkCommand({
  command,
  projects,
  selectedTasks,
  initialAction,
  includeInbox = false,
  onSucceeded,
}: {
  command: CommandMutation<TaskMutationOutcome, TaskCommandAttempt>
  projects: Array<{ id: string; name: string }>
  selectedTasks: Task[]
  initialAction: BulkAction
  /** Offer "No project (Inbox)" as a bulk value. */
  includeInbox?: boolean
  onSucceeded: () => void
}) {
  const { toast } = useToast()
  const [bulkAction, setBulkAction] = useState<BulkAction>(initialAction)
  const [bulkValue, setBulkValue] = useState('')
  const [bulk, setBulk] = useState<BulkProgress | null>(null)
  const [bulkError, setBulkError] = useState<string | null>(null)

  const selectBulkAction = useCallback((value: BulkAction) => {
    setBulkAction(value)
    setBulkValue('')
    setBulkError(null)
  }, [])

  const bulkOptions = useMemo(
    () => bulkValueOptions(bulkAction, projects, { includeInbox }),
    [bulkAction, projects, includeInbox],
  )

  const applyBulk = useCallback(async () => {
    if (selectedTasks.length === 0 || bulkValue.length === 0 || bulk) return
    const label = bulkActionLabel(bulkAction)

    setBulkError(null)
    setBulk({ label, total: selectedTasks.length, done: 0 })
    let failed = 0
    let firstFailure: unknown = null

    for (const task of selectedTasks) {
      try {
        if (bulkAction === 'status') {
          await command.mutateAsync(
            taskCommandAttempt({
              type: 'change_status',
              taskId: task.id,
              status: bulkValue as TaskStatus,
            }),
          )
        } else if (bulkAction === 'priority') {
          await command.mutateAsync(
            taskCommandAttempt({
              type: 'set_details',
              taskId: task.id,
              description: task.description,
              priority: (bulkValue === 'none' ? null : bulkValue) as TaskPriority | null,
              labels: task.labels,
            }),
          )
        } else {
          await command.mutateAsync(
            taskCommandAttempt({
              type: 'assign_project',
              taskId: task.id,
              projectId: bulkValue === 'none' ? null : bulkValue,
            }),
          )
        }
      } catch (failure) {
        failed += 1
        if (!firstFailure) firstFailure = failure
      }
      setBulk((current) => (current ? { ...current, done: current.done + 1 } : current))
    }

    setBulk(null)
    const succeeded = selectedTasks.length - failed
    if (failed === 0) {
      toast({
        tone: 'success',
        title: `${label} updated on ${succeeded} task${succeeded === 1 ? '' : 's'}`,
      })
      onSucceeded()
      setBulkValue('')
      return
    }

    const ambiguous = firstFailure !== null && isAmbiguousError(firstFailure)
    const summary = ambiguous
      ? `${failed} of ${selectedTasks.length} task${failed === 1 ? '' : 's'} may not have been updated. ${failureMessage(firstFailure)}`
      : `${failed} of ${selectedTasks.length} were not updated. ${failureMessage(firstFailure)}`
    setBulkError(summary)
    toast({
      tone: ambiguous ? 'default' : 'danger',
      title: ambiguous
        ? `Outcome not confirmed for ${failed} task${failed === 1 ? '' : 's'}`
        : `${failed} task${failed === 1 ? '' : 's'} not updated`,
      description: failureMessage(firstFailure),
    })
  }, [bulk, bulkAction, bulkValue, command, onSucceeded, selectedTasks, toast])

  return {
    bulkAction,
    selectBulkAction,
    bulkValue,
    setBulkValue,
    bulk,
    bulkError,
    bulkOptions,
    applyBulk,
  }
}
