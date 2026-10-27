import { useEffect, useMemo, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import type { ProjectStatus } from '@/api/contracts'
import type { Project } from '@/api/adapters'
import {
  classifyError,
  newCommandId,
  uncertainMessage,
  useIdempotentAttempt,
  queryKeys,
  useProjectStatuses,
  useUpdateProject,
  userMessage,
} from '@/hooks'
import { projectStatusLabel } from '@/domain'
import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { InlineFieldError } from '@/components/forms'
import { AlertCircleIcon, LoaderIcon } from '@/components/ui/icons'
import { planProjectUpdate, projectNameError, projectStatusChoices } from './project-edit'

interface PinnedCommand {
  key: string
  commandId: string
}

/**
 * The editable half of project identity: name and status.
 *
 * The project id is shown by `ProjectIdentity` and is never an input. Every
 * save is one fenced command; a refusal is quoted where it happened, a stale
 * revision refreshes the record and keeps the draft so the person can retry.
 */
export function ProjectIdentityForm({ project }: { project: Project }) {
  const statuses = useProjectStatuses()
  const update = useUpdateProject()
  const queryClient = useQueryClient()
  const attempt = useIdempotentAttempt<PinnedCommand>((key) => ({
    key,
    commandId: newCommandId(),
  }))
  const { toast } = useToast()

  const [draft, setDraft] = useState<{ name: string; status: ProjectStatus }>({
    name: project.name,
    status: project.status,
  })
  const baseline = useRef({ name: project.name, status: project.status })

  // A refetch can bring a newer record (a stale fence, or another device). Keep
  // a draft the person is holding, but never leave an untouched form showing
  // the old value.
  useEffect(() => {
    const previous = baseline.current
    baseline.current = { name: project.name, status: project.status }
    setDraft((current) =>
      current.name === previous.name && current.status === previous.status
        ? { name: project.name, status: project.status }
        : current,
    )
  }, [project.name, project.status])

  const { choices, read: statusRead } = useMemo(
    () => projectStatusChoices(statuses.data, project.status),
    [statuses.data, project.status],
  )

  // A target that the refreshed server no longer allows goes back to the stored
  // status instead of lingering as a value the form cannot submit.
  useEffect(() => {
    if (!statuses.data) return
    setDraft((current) =>
      choices.includes(current.status) ? current : { ...current, status: project.status },
    )
  }, [choices, project.status, statuses.data])

  const nameError = projectNameError(draft.name)
  const plan = planProjectUpdate(project, draft)
  const changed = draft.name.trim() !== project.name || draft.status !== project.status
  const pending = update.isPending
  const canSave = plan !== null && nameError === null && !pending

  const error = update.disposition === 'rejected' ? update.error : null
  const conflicting = error !== null && classifyError(error) === 'conflict'
  const uncertain = update.disposition === 'uncertain'

  const save = async () => {
    if (!plan || nameError !== null || pending) return
    const key = JSON.stringify({
      projectId: project.id,
      name: plan.name,
      status: plan.status ?? '',
    })
    const pinned = attempt.ensure(key)
    try {
      const outcome = await update.mutateAsync({
        commandId: pinned.commandId,
        projectId: project.id,
        name: plan.name,
        status: plan.status,
      })
      attempt.clear()
      baseline.current = { name: outcome.project.name, status: outcome.project.status }
      setDraft({ name: outcome.project.name, status: outcome.project.status })
      update.reset()
      toast({
        tone: 'success',
        title: 'Project updated',
        description:
          plan.status !== undefined && plan.name !== ''
            ? 'The server saved the new name and status.'
            : plan.status !== undefined
              ? 'The server saved the new status.'
              : 'The server saved the new name.',
      })
    } catch {
      // `update.error` is rendered inline below; the pinned command id is kept
      // so a retry after a lost answer replays the same receipt.
    }
  }

  const cancel = () => {
    setDraft({ name: project.name, status: project.status })
    attempt.clear()
    update.reset()
  }

  // A conflict or an unconfirmed answer is repaired by reading the record
  // again, not by retrying into the dark.
  const refreshProject = () => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot() })
    void queryClient.invalidateQueries({ queryKey: queryKeys.milestones() })
    void queryClient.invalidateQueries({ queryKey: queryKeys.searchAll() })
  }

  return (
    <form
      className="flex flex-col gap-4"
      data-project-identity=""
      onSubmit={(event) => {
        event.preventDefault()
        void save()
      }}
      noValidate
    >
      <Field
        label="Project name"
        hint="The name people see in lists, the sidebar and search. The project id stays the same."
        error={nameError ?? undefined}
      >
        <Input
          data-project-name-input=""
          value={draft.name}
          autoComplete="off"
          disabled={pending}
          onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
        />
      </Field>

      <div className="flex flex-col gap-1.5">
        <span className="text-sm font-medium leading-5 text-ink" id="project-status-label">
          Status
        </span>
        {statuses.isPending && !statuses.data ? (
          <div className="flex items-center gap-2" aria-busy="true">
            <Skeleton className="h-9 w-full max-w-xs" />
            <span className="text-2xs text-ink-muted">Reading allowed status changes…</span>
          </div>
        ) : statuses.error ? (
          <div
            data-project-statuses-error=""
            role="alert"
            className="flex flex-wrap items-start gap-x-2 gap-y-1 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
          >
            <AlertCircleIcon size={14} className="mt-0.5 shrink-0" />
            <p className="min-w-0 flex-1">
              The allowed status changes could not be read, so the status stays read-only.{' '}
              {classifyError(statuses.error) === 'notFound'
                ? 'This server does not report project status transitions yet.'
                : userMessage(statuses.error)}
            </p>
            <Button
              type="button"
              variant="quiet"
              size="sm"
              onClick={() => void statuses.refetch()}
              loading={statuses.isFetching}
            >
              Retry
            </Button>
            <span className="w-full text-2xs text-ink-muted">
              Stored status: {projectStatusLabel(project.status)}
            </span>
          </div>
        ) : !statusRead ? (
          <p
            role="alert"
            data-project-statuses-error=""
            className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
          >
            The server did not report transitions for{' '}
            <span className="font-medium">{projectStatusLabel(project.status)}</span>, so the status
            stays read-only.
          </p>
        ) : (
          <>
            <Select
              value={draft.status}
              onValueChange={(value) =>
                setDraft((current) => ({ ...current, status: value as ProjectStatus }))
              }
              disabled={pending}
            >
              <SelectTrigger
                data-project-status-select=""
                aria-labelledby="project-status-label"
                aria-describedby="project-status-hint"
                disabled={pending || choices.length <= 1}
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {choices.map((status) => (
                  <SelectItem key={status} value={status}>
                    {projectStatusLabel(status)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p id="project-status-hint" className="text-xs leading-4 text-ink-muted">
              {choices.length > 1
                ? 'Only the moves the server reports as allowed are offered. A refusal keeps the stored status.'
                : 'The server reports no further status changes from here. The status stays as stored.'}
            </p>
          </>
        )}
      </div>

      {error ? (
        <div data-project-identity-error="" className="text-xs leading-5 text-danger-ink">
          <InlineFieldError
            error={
              conflicting
                ? 'This project changed on another device. Refresh the project, review the form, and save again.'
                : userMessage(error)
            }
          />
          {conflicting ? (
            <div className="mt-1 flex flex-wrap items-center gap-2">
              <p className="text-ink-muted">
                The form keeps your draft; saving again fences on the refreshed revision.
              </p>
              <Button type="button" variant="quiet" size="sm" onClick={refreshProject}>
                Refresh project
              </Button>
            </div>
          ) : null}
        </div>
      ) : uncertain && update.error ? (
        <div role="alert" data-project-identity-error="" className="text-xs leading-5 text-danger-ink">
          <div className="flex flex-wrap items-center gap-2">
            <p className="min-w-0 flex-1">{uncertainMessage(update.error)}</p>
            <Button type="button" variant="quiet" size="sm" onClick={refreshProject}>
              Refresh project
            </Button>
          </div>
        </div>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <Button type="submit" size="sm" disabled={!canSave} loading={pending} data-project-save="">
          Save changes
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          onClick={cancel}
          disabled={pending || (!changed && !error && !uncertain)}
          data-project-cancel=""
        >
          Cancel
        </Button>
        {pending ? (
          <span className="flex items-center gap-1.5 text-2xs text-ink-muted">
            <LoaderIcon size={12} className="animate-spin" /> Waiting for the server…
          </span>
        ) : changed ? (
          <span className="text-2xs text-ink-muted">Unsaved changes</span>
        ) : (
          <span className="text-2xs text-ink-muted">Everything is saved.</span>
        )}
      </div>
    </form>
  )
}
