import { useEffect, useMemo, useState } from 'react'
import type { Epic, Task, WorkItem } from '@/api/adapters'
import type { EpicStatus } from '@/api/contracts'
import { newCommandId, useEpicCommand, userMessage } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { AlertCircleIcon, ArrowRightIcon, TrashIcon } from '@/components/ui/icons'
import { useToast } from '@/components/ui/toast'
import type { EpicDraft } from '@/api/endpoints/epics'
import {
  EPIC_STATUS_LABELS,
  epicNextStatuses,
  epicProgressLabel,
  formatEpicDate,
} from './epic-model'
import { EpicDraftFields } from './create-epic-dialog'

function toDraft(epic: Epic): EpicDraft {
  return {
    title: epic.title,
    description: epic.description,
    priority: epic.priority,
    owner: epic.owner,
    startAtMs: epic.startAtMs,
    targetAtMs: epic.targetAtMs,
  }
}

/**
 * One epic's details, children and history. Everything here goes through the
 * epic command family; task status is never edited from this dialog.
 */
export function EpicDetailDialog({
  epic,
  tasks,
  workItems = [],
  open,
  onOpenChange,
  onOpenTask,
}: {
  epic: Epic | null
  tasks: Task[]
  workItems?: WorkItem[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onOpenTask?: (taskId: string) => void
}) {
  const command = useEpicCommand()
  const { toast } = useToast()
  const [draft, setDraft] = useState<EpicDraft>(() =>
    epic ? toDraft(epic) : { title: '', description: '', priority: null, owner: null, startAtMs: null, targetAtMs: null },
  )
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (epic) setDraft(toDraft(epic))
    setError(null)
  }, [epic?.id, epic?.updatedAtMs])

  const childTasks = useMemo(() => {
    if (!epic) return []
    const ids = new Set(epic.taskIds)
    return tasks.filter((task) => ids.has(task.id))
  }, [epic, tasks])
  const childWorkItems = useMemo(() => {
    if (!epic) return []
    const ids = new Set(epic.workItemIds)
    return workItems.filter((item) => ids.has(item.id))
  }, [epic, workItems])

  if (!epic) return null

  const dirty =
    draft.title !== epic.title ||
    draft.description !== epic.description ||
    draft.priority !== epic.priority ||
    draft.owner !== epic.owner ||
    draft.startAtMs !== epic.startAtMs ||
    draft.targetAtMs !== epic.targetAtMs

  const nextStatuses = epicNextStatuses(epic.status)

  const save = async () => {
    if (draft.title.trim().length === 0) {
      setError('Give the epic a title.')
      return
    }
    setSaving(true)
    setError(null)
    try {
      await command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'update', epicId: epic.id, draft },
      })
      toast({ tone: 'success', title: 'Epic updated' })
    } catch (failure) {
      setError(userMessage(failure))
    } finally {
      setSaving(false)
    }
  }

  const changeStatus = async (status: EpicStatus) => {
    setError(null)
    try {
      await command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'change_status', epicId: epic.id, status },
      })
      toast({ tone: 'success', title: `Epic moved to ${EPIC_STATUS_LABELS[status]}` })
    } catch (failure) {
      setError(userMessage(failure))
    }
  }

  const removeTask = async (taskId: string) => {
    setError(null)
    try {
      await command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'unassign_task', taskId },
      })
    } catch (failure) {
      setError(userMessage(failure))
    }
  }

  const removeWorkItem = async (itemId: string) => {
    setError(null)
    try {
      await command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'unassign_work_item', itemId },
      })
    } catch (failure) {
      setError(userMessage(failure))
    }
  }

  const remove = async () => {
    setError(null)
    try {
      await command.mutateAsync({
        commandId: newCommandId(),
        input: { type: 'delete', epicId: epic.id },
      })
      toast({ tone: 'success', title: 'Epic deleted' })
      onOpenChange(false)
    } catch (failure) {
      setError(userMessage(failure))
    }
  }

  const hasWork = epic.taskIds.length > 0 || epic.workItemIds.length > 0

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={epic.title}
        description={`Epic in this project · ${epicProgressLabel(epic)}`}
        className="max-w-2xl"
      >
        <div className="flex flex-col gap-5">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={epic.status === 'completed' ? 'success' : epic.status === 'blocked' ? 'danger' : 'accent'} dot>
              {EPIC_STATUS_LABELS[epic.status]}
            </Badge>
            <span className="text-xs text-ink-muted">{epicProgressLabel(epic)}</span>
            {epic.targetAtMs ? (
              <span className="text-xs text-ink-muted">Target {formatEpicDate(epic.targetAtMs)}</span>
            ) : null}
            <span className="h-1.5 w-32 overflow-hidden rounded-full bg-line">
              <span
                className="block h-full rounded-full bg-accent"
                style={{ width: `${epic.progress.percent}%` }}
              />
            </span>
            <span className="text-xs tabular-nums text-ink-muted">{epic.progress.percent}%</span>
          </div>

          <div className="grid gap-4">
            <EpicDraftFields draft={draft} onChange={setDraft} disabled={saving} />
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Button size="sm" onClick={() => void save()} loading={saving} disabled={!dirty}>
                  Save epic
                </Button>
                <Button
                  size="sm"
                  variant="quiet"
                  onClick={() => setDraft(toDraft(epic))}
                  disabled={!dirty || saving}
                >
                  Reset
                </Button>
              </div>
              <div className="flex items-center gap-2">
                <Select
                  value={epic.status}
                  onValueChange={(value) => void changeStatus(value as EpicStatus)}
                  disabled={nextStatuses.length === 0 || command.isPending}
                >
                  <SelectTrigger className="h-9 w-[11rem]" aria-label="Epic status">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={epic.status}>
                      {EPIC_STATUS_LABELS[epic.status]}
                    </SelectItem>
                    {nextStatuses.map((status) => (
                      <SelectItem key={status} value={status}>
                        {EPIC_STATUS_LABELS[status]}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Button
                  size="sm"
                  variant="danger"
                  leadingIcon={<TrashIcon size={14} />}
                  disabled={hasWork}
                  title={hasWork ? 'Move its work out before deleting this epic' : undefined}
                  onClick={() => void remove()}
                >
                  Delete
                </Button>
              </div>
            </div>
            {hasWork ? (
              <p className="text-2xs leading-4 text-ink-muted">
                This epic still owns work. Complete or cancel it, or move the work out, before
                deleting.
              </p>
            ) : null}
          </div>

          {error ? (
            <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
              <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
              {error}
            </p>
          ) : null}

          <section className="border-t border-line pt-4">
            <h3 className="text-sm font-semibold text-ink">Tasks ({childTasks.length})</h3>
            {childTasks.length === 0 ? (
              <p className="mt-1 text-xs text-ink-muted">
                Assign a task from its details, or from the Backlog.
              </p>
            ) : (
              <ul className="mt-2 divide-y divide-line rounded-md border border-line">
                {childTasks.map((task) => (
                  <li key={task.id} className="flex items-center gap-2 px-3 py-2 text-sm">
                    <span className="min-w-0 flex-1 truncate">{task.title}</span>
                    <span className="text-2xs text-ink-muted">{task.status}</span>
                    {onOpenTask ? (
                      <Button
                        variant="quiet"
                        size="sm"
                        onClick={() => {
                          onOpenChange(false)
                          onOpenTask(task.id)
                        }}
                        aria-label={`Open ${task.title}`}
                      >
                        <ArrowRightIcon size={14} />
                      </Button>
                    ) : null}
                    <Button
                      variant="quiet"
                      size="sm"
                      onClick={() => void removeTask(task.id)}
                      aria-label={`Remove ${task.title} from the epic`}
                    >
                      Remove
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          {childWorkItems.length > 0 ? (
            <section className="border-t border-line pt-4">
              <h3 className="text-sm font-semibold text-ink">
                Work items ({childWorkItems.length})
              </h3>
              <ul className="mt-2 divide-y divide-line rounded-md border border-line">
                {childWorkItems.map((item) => (
                  <li key={item.id} className="flex items-center gap-2 px-3 py-2 text-sm">
                    <span className="min-w-0 flex-1 truncate">{typeof item.properties.title === 'string' ? item.properties.title : item.id}</span>
                    <span className="text-2xs text-ink-muted">{item.state}</span>
                    <Button
                      variant="quiet"
                      size="sm"
                      onClick={() => void removeWorkItem(item.id)}
                      aria-label={`Remove ${typeof item.properties.title === 'string' ? item.properties.title : item.id} from the epic`}
                    >
                      Remove
                    </Button>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          <section className="border-t border-line pt-4">
            <h3 className="text-sm font-semibold text-ink">Activity</h3>
            {epic.activities.length === 0 ? (
              <p className="mt-1 text-xs text-ink-muted">No recorded activity yet.</p>
            ) : (
              <ul className="mt-2 flex flex-col gap-1.5">
                {epic.activities.slice(0, 8).map((entry) => (
                  <li key={entry.id} className="flex items-start gap-2 text-xs text-ink-soft">
                    <span className="mt-1 h-1.5 w-1.5 shrink-0 rounded-full bg-line-strong" />
                    <span className="min-w-0 flex-1">{entry.summary}</span>
                    <time className="shrink-0 text-2xs text-ink-muted">
                      {formatEpicDate(entry.occurredAtMs)}
                    </time>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      </DialogContent>
    </Dialog>
  )
}

/** Select the epic a task belongs to, or clear it. */
export function EpicAssignmentField({
  epics,
  selectedEpicId,
  taskId,
  kind = 'task',
  onChanged,
  disabled = false,
}: {
  epics: Epic[]
  selectedEpicId: string | null
  taskId: string
  /** Work Items share the same membership commands under their own ids. */
  kind?: 'task' | 'work_item'
  onChanged?: () => void
  disabled?: boolean
}) {
  const command = useEpicCommand()
  const [error, setError] = useState<string | null>(null)
  const assign = async (value: string) => {
    setError(null)
    try {
      if (value === 'none') {
        if (!selectedEpicId) return
        await command.mutateAsync({
          commandId: newCommandId(),
          input:
            kind === 'task'
              ? { type: 'unassign_task', taskId }
              : { type: 'unassign_work_item', itemId: taskId },
        })
      } else {
        await command.mutateAsync({
          commandId: newCommandId(),
          input:
            kind === 'task'
              ? { type: 'assign_task', epicId: value, taskId }
              : { type: 'assign_work_item', epicId: value, itemId: taskId },
        })
      }
      onChanged?.()
    } catch (failure) {
      setError(userMessage(failure))
    }
  }
  return (
    <Field label="Epic" hint="Groups related work; milestones are separate checkpoints.">
      <Select
        value={selectedEpicId ?? 'none'}
        onValueChange={(value) => void assign(value)}
        disabled={disabled || command.isPending}
      >
        <SelectTrigger aria-label="Epic">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="none">No epic</SelectItem>
          {epics.map((epic) => (
            <SelectItem key={epic.id} value={epic.id}>
              {epic.title}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {error ? (
        <p role="alert" className="mt-1 flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
          <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
          {error}
        </p>
      ) : null}
    </Field>
  )
}
