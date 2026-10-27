/**
 * The one place the browser queues an agent job.
 *
 * Two steps, one dialog: the form, then a confirmation that says what queueing
 * does and does not do. The launch identity (`job_id`) and `at_ms` are pinned per
 * payload, so pressing the button again after a refusal replays the same request
 * instead of queueing a second job. An unconfirmed answer is never re-sent: the
 * dialog opens the job id it already used and lets the job page read it.
 *
 * The browser never calls an `agent-machines/*` route, never starts a model and
 * never completes a task from here.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import type { AgentExecutor, Task } from '@/api/adapters'
import {
  isAmbiguousError,
  newCommandId,
  useAgentOverview,
  useIdempotentAttempt,
  useStartAgentJob,
  useWorkspaceSnapshot,
  type ApiError,
} from '@/hooks'
import { statusLabel, statusTones } from '@/features/tasks/task-meta'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { LabeledSelect } from '@/components/forms/labeled-select'
import { Textarea } from '@/components/ui/textarea'
import { AlertCircleIcon, AgentsIcon, ArrowLeftIcon, ClockIcon, FolderIcon, LoaderIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { agentKindLabel } from '../agent-model'
import {
  AGENT_KINDS,
  NEVER_CLAIMED_EXPLANATION,
  NEVER_CLAIMED_LABEL,
  OBJECTIVE_MAX_LENGTH,
  REFERENCE_MAX_LENGTH,
  activeJobForTask,
  emptyLaunchForm,
  executorSummary,
  hasLaunchErrors,
  hasNeverClaimed,
  launchAttemptKey,
  launchEligibility,
  launchFailureCopy,
  neverClaimedConfirmation,
  toLaunchRequest,
  validateLaunchForm,
  type LaunchForm,
  type LaunchFormErrors,
  type PinnedLaunch,
} from '../launch-model'
import { useExecutorState } from './use-executor-state'

export interface LaunchAgentJobDialogProps {
  /** The fixed task (task drawer). `null`/absent means the person chooses one. */
  fixedTask?: Task | null
  onClose: () => void
}

export function LaunchAgentJobDialog({ fixedTask = null, onClose }: LaunchAgentJobDialogProps) {
  const navigate = useNavigate()
  const snapshot = useWorkspaceSnapshot()
  const overview = useAgentOverview()
  const executorState = useExecutorState()
  const start = useStartAgentJob()
  const attempt = useIdempotentAttempt<PinnedLaunch>((key) => ({
    key,
    jobId: newCommandId(),
    atMs: Date.now(),
  }))

  const [form, setForm] = useState<LaunchForm>(() => emptyLaunchForm(fixedTask?.id ?? ''))
  const [step, setStep] = useState<'form' | 'confirm'>('form')
  const [errors, setErrors] = useState<LaunchFormErrors>({})
  const [announcement, setAnnouncement] = useState<string | null>(null)

  // The dialog is mounted by the click that opened it, so this is the trigger.
  const returnFocusRef = useRef<HTMLElement | null>(
    typeof document !== 'undefined' && document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null,
  )
  const confirmPrimaryRef = useRef<HTMLButtonElement>(null)
  const formPrimaryRef = useRef<HTMLButtonElement>(null)
  const firstRender = useRef(true)

  useEffect(() => {
    if (firstRender.current) {
      firstRender.current = false
      return
    }
    // Switching steps replaces the focused control; put focus on the new
    // primary action instead of dropping it on the dialog body.
    if (step === 'confirm') confirmPrimaryRef.current?.focus()
    else formPrimaryRef.current?.focus()
  }, [step])

  const activeExecutors = executorState.state.active

  /**
   * Preselect the executor only when there is exactly one active device: with
   * several, choosing one silently would hide the decision. The read usually
   * lands after the dialog mounts, hence an effect rather than the initial state.
   */
  useEffect(() => {
    if (form.executorId) return
    if (activeExecutors.length !== 1) return
    const only = activeExecutors[0].deviceId
    setForm((current) => (current.executorId ? current : { ...current, executorId: only }))
  }, [activeExecutors, form.executorId])

  /** Tasks that could be queued, for the chooser. The server still decides. */
  const candidates = useMemo(() => {
    const jobs = overview.data ?? []
    return (snapshot.data?.tasks ?? [])
      .filter((task) => launchEligibility({
        status: task.status,
        activeJob: activeJobForTask(jobs, task.id),
      }).eligible)
      .sort((left, right) => left.title.localeCompare(right.title))
  }, [overview.data, snapshot.data?.tasks])

  const selectedTask = useMemo(() => {
    if (fixedTask) return fixedTask
    return candidates.find((task) => task.id === form.taskId) ?? null
  }, [candidates, fixedTask, form.taskId])

  const projectName = useMemo(() => {
    if (!selectedTask?.projectId) return null
    return (
      snapshot.data?.projects.find((project) => project.id === selectedTask.projectId)?.name ?? null
    )
  }, [selectedTask?.projectId, snapshot.data?.projects])

  const selectedExecutor: AgentExecutor | null =
    activeExecutors.find((executor) => executor.deviceId === form.executorId) ?? null

  const blocker = executorState.blocker
  const pending = start.isPending
  const failure: ApiError | null = start.disposition === 'rejected' ? start.error : null
  const workspacePending =
    (snapshot.isPending && !snapshot.data) || (overview.isPending && !overview.data)
  const launchable = Boolean(selectedTask) && activeExecutors.length > 0 && !blocker
  const pinned = attempt.attempt
  const executorHint = selectedExecutor
    ? executorSummary(selectedExecutor)
    : 'The device that has to claim this job before anything runs.'
  /**
   * The roster's one honest caveat about the chosen device: it has no claim on
   * record. It is repeated at the confirmation, because that is the step a
   * person reads immediately before queueing. It never disables the launch: the
   * roster cannot prove that no runner is active there.
   */
  const executorNeverClaimed = selectedExecutor ? hasNeverClaimed(selectedExecutor) : false
  const neverClaimedWarning = selectedExecutor
    ? neverClaimedConfirmation(selectedExecutor.deviceId)
    : null

  const patch = (next: Partial<LaunchForm>) => {
    setForm((current) => ({ ...current, ...next }))
    setErrors({})
    // A refusal belongs to the payload that was sent; changing the payload
    // starts a new attempt, so the old answer must not sit beside the new form.
    if (start.error) start.reset()
    setAnnouncement(null)
  }

  const review = () => {
    const nextErrors = validateLaunchForm(form, activeExecutors)
    setErrors(nextErrors)
    if (hasLaunchErrors(nextErrors)) {
      setAnnouncement('Some fields need attention before this can be queued.')
      return
    }
    // Mint the launch identity now so the confirmation shows the real job id.
    attempt.ensure(launchAttemptKey(form))
    setAnnouncement(null)
    setStep('confirm')
  }

  const submit = async () => {
    if (!selectedTask || pending) return
    // Re-check the form: an executor can be revoked while the confirmation is
    // open, and a request we already know the server refuses is not worth sending.
    const nextErrors = validateLaunchForm(form, activeExecutors)
    if (hasLaunchErrors(nextErrors)) {
      setErrors(nextErrors)
      setStep('form')
      setAnnouncement('The form needs attention before this can be queued.')
      return
    }
    const pinnedAttempt = attempt.ensure(launchAttemptKey(form))
    setAnnouncement('Queueing the job…')
    try {
      const job = await start.mutateAsync(toLaunchRequest(pinnedAttempt, form))
      attempt.clear()
      setAnnouncement(`Queued ${job.id}. Opening it…`)
      // Read the job the server just created rather than predicting its state.
      navigate(`/agents/${encodeURIComponent(job.id)}`, { replace: true })
      onClose()
    } catch (error) {
      if (isAmbiguousError(error)) {
        // The answer never arrived. Do not send a second request with a new id:
        // open the job id this attempt already used and let its page answer.
        navigate(`/agents/${encodeURIComponent(pinnedAttempt.jobId)}`, {
          replace: true,
          state: { launchUnconfirmed: true },
        })
        onClose()
        return
      }
      setAnnouncement(null)
    }
  }

  return (
    <Dialog open onOpenChange={(next) => (next ? undefined : onClose())}>
      <DialogContent
        size="lg"
        data-launch-dialog=""
        data-launch-step={step}
        title="Queue an agent job"
        description="This queues a job for an enrolled executor. It does not start a model and it does not complete the task."
        className="max-h-[min(46rem,92dvh)] overflow-y-auto"
        onCloseAutoFocus={(event) => {
          event.preventDefault()
          const target = returnFocusRef.current
          if (target?.isConnected) target.focus({ preventScroll: true })
        }}
      >
        <p className="sr-only" role="status">
          {step === 'confirm'
            ? `Confirmation step. This queues a job for the executor; nothing starts until it claims the job.${
                neverClaimedWarning ? ` ${neverClaimedWarning.title} ${neverClaimedWarning.detail}` : ''
              }`
            : ''}
        </p>
        {step === 'form' ? (
          <div className="flex flex-col gap-4">
            {fixedTask ? (
              <TaskSummary task={selectedTask} projectName={projectName} />
            ) : (
              <LabeledSelect
                label="Task"
                required
                value={form.taskId}
                onValueChange={(taskId) => patch({ taskId })}
                options={candidates.map((task) => ({ value: task.id, label: task.title }))}
                placeholder={candidates.length > 0 ? 'Choose a task' : 'No task can be queued'}
                hint="A task is queueable while it is ready, in progress or in review, and has no active job. The server re-checks both."
                error={errors.task}
              />
            )}
            {!fixedTask && selectedTask ? (
              <TaskSummary task={selectedTask} projectName={projectName} />
            ) : null}
            {!fixedTask && candidates.length === 0 ? (
              <p className="rounded-md border border-line bg-surface-sunken/50 px-3 py-2 text-xs leading-5 text-ink-muted">
                {workspacePending
                  ? 'Checking which tasks can be queued…'
                  : 'Nothing can be queued right now. Open a task that is ready, in progress or in review, and queue it from that task’s drawer.'}
              </p>
            ) : null}

            <LabeledSelect
              label="Executor"
              required
              value={form.executorId}
              onValueChange={(executorId) => patch({ executorId })}
              options={activeExecutors.map((executor) => ({
                value: executor.deviceId,
                label: hasNeverClaimed(executor)
                  ? `${executor.deviceId} (${NEVER_CLAIMED_LABEL})`
                  : executor.deviceId,
              }))}
              placeholder={
                activeExecutors.length > 0 ? 'Choose an enrolled device' : 'No active executor'
              }
              hint={executorHint}
              error={errors.executor}
              disabled={activeExecutors.length === 0 || pending}
            />
            {selectedExecutor && executorNeverClaimed ? (
              <p
                role="status"
                data-executor-never-claimed=""
                data-launch-executor-warning=""
                className="-mt-1 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-2xs leading-4 text-warning-ink"
              >
                <ClockIcon size={11} className="mr-1 inline-block align-[-1px]" />
                <span className="font-semibold">
                  {selectedExecutor.deviceId} {NEVER_CLAIMED_LABEL}.
                </span>{' '}
                {NEVER_CLAIMED_EXPLANATION}
              </p>
            ) : null}
            <p className="-mt-1 text-2xs leading-4 text-ink-faint">
              The executor is an enrolled device. This form does not accept a machine name typed by
              hand, and the browser cannot claim the job itself.
            </p>

            <Field
              id="agent-launch-objective"
              label="Objective"
              hint={`Optional. What the executor should do, in a few lines. It is stored with the job and carried into a resume; ${OBJECTIVE_MAX_LENGTH} characters at most.`}
              error={errors.objective}
              labelExtra={
                <span className="text-2xs tabular-nums text-ink-faint">
                  {form.objective.trim().length}/{OBJECTIVE_MAX_LENGTH}
                </span>
              }
            >
              <Textarea
                value={form.objective}
                onChange={(event) => patch({ objective: event.target.value })}
                placeholder="Fix the login redirect and add a regression test."
                rows={3}
                disabled={pending}
              />
            </Field>

            <div className="grid gap-4 sm:grid-cols-2">
              <LabeledSelect
                label="Agent"
                value={form.agentKind}
                onValueChange={(kind) =>
                  patch({ agentKind: kind === 'opencode' ? 'opencode' : 'codex' })
                }
                options={AGENT_KINDS.map((kind) => ({ value: kind, label: agentKindLabel(kind) }))}
                hint="The executor's own runtime decides how the agent is started."
                disabled={pending}
              />
              <Field
                id="agent-launch-repository"
                label="Repository"
                required
                hint="How the executor finds the repository: a name or an absolute path. This browser does not read it."
                error={errors.repository}
              >
                <Input
                  value={form.repositoryRef}
                  onChange={(event) => patch({ repositoryRef: event.target.value })}
                  placeholder="orbit"
                  maxLength={REFERENCE_MAX_LENGTH}
                  autoComplete="off"
                  spellCheck={false}
                  className="font-mono text-sm"
                  disabled={pending}
                />
              </Field>
            </div>

            <Checkbox
              id="agent-launch-worktree"
              checked={form.worktree}
              onCheckedChange={(checked) => patch({ worktree: checked === true })}
              label="Run in a separate worktree"
              description="The executor isolates the work in its own worktree instead of the checkout it already has open."
              disabled={pending}
            />

            {failure ? <LaunchRefusal failure={launchFailureCopy(failure)} /> : null}

            <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4">
              <Button
                ref={formPrimaryRef}
                data-launch-review=""
                onClick={review}
                disabled={!launchable || pending}
                aria-describedby={launchable ? undefined : 'agent-launch-blocked'}
              >
                Review and queue
              </Button>
              <Button variant="quiet" onClick={onClose} disabled={pending}>
                Cancel
              </Button>
              {!launchable ? (
                <p
                  id="agent-launch-blocked"
                  data-launch-blocked=""
                  className="min-w-0 flex-1 text-2xs leading-4 text-ink-muted"
                >
                  {blocker ??
                    (selectedTask
                      ? 'This launch cannot be prepared yet.'
                      : 'Choose a task to queue.')}
                </p>
              ) : null}
            </div>
            <LaunchStatus announcement={announcement} pending={pending} />
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <div
              data-launch-confirm=""
              className="rounded-md border border-line bg-surface-sunken/40 px-3 py-3"
            >
              <p className="text-sm font-medium text-ink">
                Queueing this job does not start it.
              </p>
              <p className="mt-1 text-xs leading-5 text-ink-soft">
                Orbit creates a queued job that waits for an executor.{' '}
                <span className="font-mono text-2xs">{form.executorId}</span> must claim it before
                anything runs. Until then the job is queued and nothing is running.
              </p>
              <p className="mt-1.5 text-xs leading-5 text-ink-soft">
                This browser does not start a model. Approving the handoff later records a review
                and does not complete the task.
              </p>
            </div>

            {executorNeverClaimed && neverClaimedWarning ? (
              <div
                id="agent-launch-never-claimed"
                data-launch-never-claimed=""
                className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
              >
                <p className="font-medium">{neverClaimedWarning.title}</p>
                <p className="mt-1">{neverClaimedWarning.detail}</p>
              </div>
            ) : null}

            <dl className="grid gap-x-4 gap-y-2 text-xs sm:grid-cols-2" data-launch-summary="">
              <SummaryRow label="Task" value={selectedTask?.title ?? form.taskId} />
              <SummaryRow label="Project" value={projectName ?? selectedTask?.projectId ?? 'No project'} />
              <SummaryRow
                label="Task status"
                value={selectedTask ? statusLabel(selectedTask.status) : 'Unknown'}
              />
              <SummaryRow label="Executor" value={form.executorId} mono />
              <SummaryRow label="Agent" value={agentKindLabel(form.agentKind)} />
              <SummaryRow label="Repository" value={form.repositoryRef.trim()} mono />
              <SummaryRow
                label="Worktree"
                value={form.worktree ? 'Separate worktree' : 'Existing checkout'}
              />
              <SummaryRow
                label="Objective"
                value={form.objective.trim() || 'None: the executor gets the task brief only'}
              />            </dl>

            <p className="text-2xs leading-4 text-ink-faint">
              Job id{' '}
              <span className="font-mono" data-launch-job-id="">
                {pinned?.jobId ?? 'assigned when you queue'}
              </span>
              . A retry after a refusal reuses this id, so Orbit cannot queue the same work twice.
            </p>

            {failure ? <LaunchRefusal failure={launchFailureCopy(failure)} /> : null}

            <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4">
              <Button
                ref={confirmPrimaryRef}
                data-launch-submit=""
                onClick={() => void submit()}
                loading={pending}
                disabled={pending}
                aria-describedby={
                  executorNeverClaimed ? 'agent-launch-never-claimed' : undefined
                }
              >
                {failure ? 'Retry the same launch' : 'Queue the job'}
              </Button>
              <Button
                variant="secondary"
                leadingIcon={<ArrowLeftIcon size={14} />}
                onClick={() => setStep('form')}
                disabled={pending}
              >
                Back to the form
              </Button>
            </div>
            <LaunchStatus announcement={announcement} pending={pending} />
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}

function TaskSummary({ task, projectName }: { task: Task | null; projectName: string | null }) {
  if (!task) return null
  return (
    <div
      className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2"
      data-launch-task={task.id}
    >
      <div className="flex flex-wrap items-center gap-2">
        <AgentsIcon size={13} className="text-ink-muted" />
        <span className="min-w-0 flex-1 break-words text-sm font-medium text-ink">{task.title}</span>
        <Badge tone={statusTones[task.status]} dot>
          {statusLabel(task.status)}
        </Badge>
      </div>
      <p className="mt-1 flex items-center gap-1 text-2xs text-ink-muted">
        <FolderIcon size={11} className="opacity-70" />
        {projectName ?? task.projectId ?? 'No project'}
      </p>
    </div>
  )
}

function SummaryRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0">
      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</dt>
      <dd className={cn('break-words text-sm leading-5 text-ink-soft', mono && 'font-mono text-xs')}>
        {value}
      </dd>
    </div>
  )
}

function LaunchRefusal({ failure }: { failure: { title: string; detail: string } }) {
  return (
    <div
      role="alert"
      data-launch-refusal=""
      className="rounded-md border border-danger/30 bg-danger-soft px-3 py-2 text-xs leading-5 text-danger-ink"
    >
      <p className="flex items-start gap-1.5 font-medium">
        <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
        <span>{failure.title}</span>
      </p>
      <p className="mt-1 pl-[1.375rem]">{failure.detail}</p>
    </div>
  )
}

/**
 * The async state of the dialog. It is announced, not just drawn: a screen
 * reader hears "Queueing the job…" and then either the refusal above or the
 * navigation to the job.
 */
function LaunchStatus({ announcement, pending }: { announcement: string | null; pending: boolean }) {
  return (
    <p role="status" aria-live="polite" className="min-h-4 text-2xs leading-4 text-ink-muted">
      {pending ? (
        <span className="inline-flex items-center gap-1.5">
          <LoaderIcon size={12} className="animate-spin text-accent" />
          Queueing the job…
        </span>
      ) : (
        announcement
      )}
    </p>
  )
}
