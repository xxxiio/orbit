import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import type { TaskPriority, TaskStatus } from '@/api/contracts'
import {
  newCommandId,
  queryKeys,
  uncertainMessage,
  useCreateTask,
  useIdempotentAttempt,
  useMilestones,
  useProjectTasks,
  useProjects,
  userMessage,
  type ApiError,
  type CreateTaskVariables,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { Field } from '@/components/ui/field'
import { IconButton } from '@/components/ui/icon-button'
import { ChevronDownIcon, CloseIcon, PlusIcon } from '@/components/ui/icons'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { useToast } from '@/components/ui/toast'
import { cn } from '@/components/ui/utils'
import { FormErrorBanner, LabeledSelect } from '@/components/forms'
import { priorityLabels, statusLabels } from './task-meta'
import { parseDateTimeLocalInput, parseEstimateInput } from './task-drawer/planning-fields'
import { useTaskDrawer } from './use-task-drawer'

const NONE = '__none__'

/** Create only accepts these initial states; the server refuses the rest. */
const INITIAL_STATUSES: readonly TaskStatus[] = ['inbox', 'planned', 'ready', 'deferred']

const INITIAL_PRIORITY: TaskPriority = 'medium'

/** Domain bounds on one create command, mirrored before the request is sent. */
export const MAX_ESTIMATE_MINUTES = 525_600
export const MAX_ACCEPTANCE_CRITERION_LENGTH = 2000
export const MAX_ACCEPTANCE_CRITERIA = 100

/**
 * One create attempt: the command id and the task id are minted when the person
 * submits (never inside `mutationFn`), and reused while the submitted payload
 * stays the same. Pressing Create again after an unconfirmed failure therefore
 * replays the server's receipt instead of creating a second task. Editing any
 * field mints a new attempt, because the server refuses to replay a command id
 * whose payload changed.
 */
interface CreateTaskFormSnapshot {
  title: string
  projectId: string
  description: string
  status: TaskStatus
  priority: TaskPriority
  milestoneId: string
  deadline: string
  estimate: string
  prerequisites: string[]
  criteria: string[]
}

interface CreateTaskAttempt {
  key: string
  commandId: string
  taskId: string
  /** The form exactly as it was when this attempt was sent, for a replay. */
  draft: CreateTaskFormSnapshot
}

interface CriterionDraft {
  /** Stable row identity; reordering or removing rows must not reuse a key. */
  key: string
  text: string
}

interface FieldErrors {
  title?: string
  estimate?: string
  deadline?: string
  prerequisites?: string
  criteria?: string
}

type CreateTaskDraft = Omit<CreateTaskVariables, 'commandId' | 'taskId'>

/**
 * Whole minutes within the domain's 1..=525600 window. `parseEstimateInput`
 * already owns the "whole minutes, at least one" rule; this adds the upper bound
 * the server enforces so a typo is refused beside the field instead of by a
 * round trip.
 */
function parseCreateEstimate(raw: string): { value: number | null; error: string | null } {
  const parsed = parseEstimateInput(raw)
  if (parsed.error || parsed.value === null) return parsed
  if (parsed.value > MAX_ESTIMATE_MINUTES) {
    return {
      value: null,
      error: `Keep the estimate at ${MAX_ESTIMATE_MINUTES} minutes or less.`,
    }
  }
  return parsed
}

export interface CreateTaskDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Preselected when the dialog is opened from a project. */
  defaultProjectId?: string
  /** Preselected when the dialog is opened from a milestone. */
  defaultMilestoneId?: string
}

export function CreateTaskDialog({
  open,
  onOpenChange,
  defaultProjectId,
  defaultMilestoneId,
}: CreateTaskDialogProps) {
  const projects = useProjects()
  const createTask = useCreateTask()
  const queryClient = useQueryClient()
  const { toast } = useToast()
  const { openTask } = useTaskDrawer()

  const [title, setTitle] = useState('')
  const [projectId, setProjectId] = useState<string>(defaultProjectId ?? NONE)
  const [description, setDescription] = useState('')
  const [status, setStatus] = useState<TaskStatus>('inbox')
  const [priority, setPriority] = useState<TaskPriority>(INITIAL_PRIORITY)
  const [milestoneId, setMilestoneId] = useState<string>(
    defaultProjectId ? (defaultMilestoneId ?? NONE) : NONE,
  )
  const [deadline, setDeadline] = useState('')
  const [estimate, setEstimate] = useState('')
  const [prerequisites, setPrerequisites] = useState<string[]>([])
  const [prerequisiteChoice, setPrerequisiteChoice] = useState('')
  const [criteria, setCriteria] = useState<CriterionDraft[]>([])
  const [advancedOpen, setAdvancedOpen] = useState(false)
  const [errors, setErrors] = useState<FieldErrors>({})
  const [confirmDiscard, setConfirmDiscard] = useState(false)

  const titleRef = useRef<HTMLInputElement>(null)
  const criterionSequence = useRef(0)
  const criterionInputs = useRef(new Map<string, HTMLInputElement>())

  const attempt = useIdempotentAttempt<CreateTaskAttempt, CreateTaskFormSnapshot>((key, draft) => ({
    key,
    commandId: newCommandId(),
    taskId: newCommandId(),
    draft: draft ?? {
      title: '',
      projectId: NONE,
      description: '',
      status: 'inbox',
      priority: INITIAL_PRIORITY,
      milestoneId: NONE,
      deadline: '',
      estimate: '',
      prerequisites: [],
      criteria: [],
    },
  }))

  const selectedProject = projectId === NONE ? undefined : projectId
  const milestonesQuery = useMilestones(selectedProject)
  const projectTasks = useProjectTasks(selectedProject)

  useEffect(() => {
    if (!open) return
    // An unconfirmed create keeps its attempt (and the exact form that produced
    // it). Reopening the dialog after closing on an uncertain result restores
    // that draft, so Retry replays the same command/task ids instead of minting
    // a second task.
    if (createTask.disposition === 'uncertain' && attempt.attempt) {
      restoreForm(attempt.attempt.draft)
      setErrors({})
      setConfirmDiscard(false)
      return
    }
    setTitle('')
    setProjectId(defaultProjectId ?? NONE)
    setDescription('')
    setStatus('inbox')
    setPriority(INITIAL_PRIORITY)
    // A milestone only means something inside a project.
    setMilestoneId(defaultProjectId ? (defaultMilestoneId ?? NONE) : NONE)
    setDeadline('')
    setEstimate('')
    setPrerequisites([])
    setPrerequisiteChoice('')
    setCriteria([])
    setAdvancedOpen(false)
    setErrors({})
    setConfirmDiscard(false)
    attempt.clear()
    createTask.reset()
    // Only reset when the dialog opens; `createTask` is a mutation object whose
    // identity changes every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, defaultProjectId, defaultMilestoneId])

  function restoreForm(snapshot: CreateTaskFormSnapshot) {
    setTitle(snapshot.title)
    setProjectId(snapshot.projectId)
    setDescription(snapshot.description)
    setStatus(snapshot.status)
    setPriority(snapshot.priority)
    setMilestoneId(snapshot.milestoneId)
    setDeadline(snapshot.deadline)
    setEstimate(snapshot.estimate)
    setPrerequisites([...snapshot.prerequisites])
    setPrerequisiteChoice('')
    criterionSequence.current = snapshot.criteria.length
    setCriteria(
      snapshot.criteria.map((text, index) => ({
        key: `criterion-restored-${index + 1}`,
        text,
      })),
    )
    setAdvancedOpen(
      snapshot.estimate.length > 0 ||
        snapshot.prerequisites.length > 0 ||
        snapshot.criteria.length > 0,
    )
  }

  const milestoneOptions = useMemo(() => {
    const options = [{ value: NONE, label: 'No milestone' }]
    if (!selectedProject) return options
    const available = milestonesQuery.data ?? []
    const known = available.some((milestone) => milestone.id === milestoneId)
    return [
      ...options,
      ...available.map((milestone) => ({ value: milestone.id, label: milestone.name })),
      // A preselected reference we cannot resolve must never render as a raw id.
      ...(milestoneId !== NONE && !known && !milestonesQuery.isPending
        ? [{ value: milestoneId, label: 'Unknown milestone' }]
        : []),
    ]
  }, [milestoneId, milestonesQuery.data, milestonesQuery.isPending, selectedProject])

  const taskTitles = useMemo(
    () => new Map(projectTasks.map((task) => [task.id, task.title])),
    [projectTasks],
  )

  const dependencyCandidates = useMemo(
    () =>
      projectTasks
        .filter((candidate) => !prerequisites.includes(candidate.id))
        .sort((left, right) => left.title.localeCompare(right.title)),
    [prerequisites, projectTasks],
  )

  const initialProject = defaultProjectId ?? NONE
  const initialMilestone = defaultProjectId ? (defaultMilestoneId ?? NONE) : NONE
  const dirty =
    title.length > 0 ||
    description.length > 0 ||
    projectId !== initialProject ||
    milestoneId !== initialMilestone ||
    deadline.trim().length > 0 ||
    estimate.trim().length > 0 ||
    status !== 'inbox' ||
    priority !== INITIAL_PRIORITY ||
    prerequisites.length > 0 ||
    criteria.some((criterion) => criterion.text.trim().length > 0)

  const error: ApiError | null = createTask.disposition === 'rejected' ? createTask.error : null
  const uncertain = createTask.disposition === 'uncertain'
  const submitting = createTask.isPending

  function requestClose() {
    if (submitting) return
    // An uncertain create never closes silently: the task may already exist, and
    // the person must see that before leaving.
    if (dirty || uncertain) {
      setConfirmDiscard(true)
      return
    }
    onOpenChange(false)
  }

  function discardAndClose() {
    setConfirmDiscard(false)
    // Keep the pinned attempt while the server's answer is unknown so a retry
    // after reopening replays the same ids instead of creating a duplicate.
    if (!uncertain) attempt.clear()
    onOpenChange(false)
  }

  function chooseProject(next: string) {
    setProjectId(next)
    // Milestones and dependencies belong to a project; a project switch must
    // never leave a reference from the previous one in the payload.
    setMilestoneId(NONE)
    setPrerequisites([])
    setPrerequisiteChoice('')
    if (errors.prerequisites) setErrors((current) => ({ ...current, prerequisites: undefined }))
  }

  function addCriterion() {
    if (criteria.length >= MAX_ACCEPTANCE_CRITERIA) return
    criterionSequence.current += 1
    const key = `criterion-${criterionSequence.current}`
    setCriteria((current) => [...current, { key, text: '' }])
    if (errors.criteria) setErrors((current) => ({ ...current, criteria: undefined }))
    requestAnimationFrame(() => criterionInputs.current.get(key)?.focus())
  }

  function removeCriterion(key: string) {
    criterionInputs.current.delete(key)
    setCriteria((current) => current.filter((criterion) => criterion.key !== key))
    if (errors.criteria) setErrors((current) => ({ ...current, criteria: undefined }))
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (submitting) return
    setConfirmDiscard(false)

    const trimmedTitle = title.trim()
    const estimateResult = parseCreateEstimate(estimate)
    const deadlineResult = parseDateTimeLocalInput(deadline)
    const criterionTexts = criteria
      .map((criterion) => criterion.text.trim())
      .filter((text) => text.length > 0)
    const nextErrors: FieldErrors = {}
    if (trimmedTitle.length === 0) nextErrors.title = 'Enter a title for this task.'
    if (estimateResult.error) nextErrors.estimate = estimateResult.error
    if (deadlineResult.error) nextErrors.deadline = deadlineResult.error
    if (criteria.length > MAX_ACCEPTANCE_CRITERIA) {
      nextErrors.criteria = `Use at most ${MAX_ACCEPTANCE_CRITERIA} acceptance criteria.`
    } else if (criterionTexts.some((text) => text.length > MAX_ACCEPTANCE_CRITERION_LENGTH)) {
      nextErrors.criteria = `Keep each acceptance criterion at ${MAX_ACCEPTANCE_CRITERION_LENGTH} characters or less.`
    }
    setErrors(nextErrors)

    if (Object.keys(nextErrors).length > 0) {
      if (nextErrors.estimate || nextErrors.deadline || nextErrors.criteria) setAdvancedOpen(true)
      if (nextErrors.title) titleRef.current?.focus()
      return
    }

    const payload: CreateTaskDraft = {
      title: trimmedTitle,
      ...(selectedProject ? { projectId: selectedProject } : {}),
      ...(selectedProject && milestoneId !== NONE ? { milestoneId } : {}),
      ...(description.trim().length > 0 ? { description } : {}),
      status,
      priority,
      ...(estimateResult.value !== null ? { estimatedMinutes: estimateResult.value } : {}),
      ...(deadlineResult.value !== null ? { deadlineAtMs: deadlineResult.value } : {}),
      ...(criterionTexts.length > 0 ? { acceptanceCriteria: criterionTexts } : {}),
      ...(prerequisites.length > 0 ? { prerequisiteIds: prerequisites } : {}),
    }

    const attemptKey = JSON.stringify(payload)
    const pinned = attempt.ensure(attemptKey, {
      title,
      projectId,
      description,
      status,
      priority,
      milestoneId,
      deadline,
      estimate,
      prerequisites: [...prerequisites],
      criteria: criteria.map((criterion) => criterion.text),
    })

    // Mirrors the server: a task cannot wait on itself. The candidate list is
    // read before the id exists, so this is a guard rather than a likely case.
    if (payload.prerequisiteIds?.includes(pinned.taskId)) {
      setErrors({ prerequisites: 'A task cannot wait on itself.' })
      setAdvancedOpen(true)
      return
    }

    const outcome = await createTask
      .mutateAsync({
        commandId: pinned.commandId,
        taskId: pinned.taskId,
        ...payload,
      })
      .catch(() => null)

    if (!outcome) return
    attempt.clear()
    // The accepted record is already in the cache; a failed refresh must not
    // turn a stored task into a form that looks unsent.
    await queryClient.refetchQueries({ queryKey: queryKeys.snapshot() }).catch(() => undefined)
    toast({
      tone: 'success',
      title: 'Task created',
      description: outcome.task.title,
      duration: 9000,
      action: { label: 'Open task', onClick: () => openTask(outcome.task.id) },
    })
    // The dialog closes with the stored task; the toast keeps the inspector one
    // click away without blocking the page underneath.
    onOpenChange(false)
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          requestClose()
          return
        }
        onOpenChange(true)
      }}
    >
      <DialogContent
        title="New task"
        description="Add what you know now. Details and dependencies can wait."
        size="lg"
        showClose={!submitting}
      >
        <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-5">
          <Field
            label="Title"
            required
            error={errors.title}
            hint="Short and specific: what will be true when this is done?"
          >
            <Input
              ref={titleRef}
              autoFocus
              value={title}
              onChange={(event) => {
                setTitle(event.target.value)
                if (errors.title) setErrors((current) => ({ ...current, title: undefined }))
              }}
              placeholder="Draft the release notes"
              disabled={submitting}
              maxLength={280}
            />
          </Field>

          <LabeledSelect
            label="Project"
            hint="Leave it unassigned to send the task to the Inbox."
            ariaLabel="Project for the new task"
            value={projectId}
            onValueChange={chooseProject}
            disabled={submitting}
            options={[
              { value: NONE, label: 'No project (Inbox)' },
              ...projects.map((project) => ({ value: project.id, label: project.name })),
            ]}
          />

          <Field label="Description" hint="Markdown works here: lists, links, checklists.">
            <Textarea
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              placeholder="Context, links, or the shape of done…"
              rows={3}
              disabled={submitting}
              className="min-h-[5.5rem]"
            />
          </Field>

          <div className="h-px bg-line" />

          <div className="flex flex-col gap-3">
            <p className="text-2xs font-medium uppercase tracking-[0.08em] text-ink-muted">Properties</p>
            <div className="grid gap-x-4 gap-y-3 sm:grid-cols-2">
              <LabeledSelect
                label="Status"
                ariaLabel="Starting status"
                value={status}
                onValueChange={(next) => setStatus(next as TaskStatus)}
                disabled={submitting}
                hint="The state this task starts in."
                options={INITIAL_STATUSES.map((value) => ({
                  value,
                  label: statusLabels[value],
                }))}
              />

              <LabeledSelect
                label="Priority"
                ariaLabel="Task priority"
                value={priority}
                onValueChange={(next) => setPriority(next as TaskPriority)}
                disabled={submitting}
                options={(Object.keys(priorityLabels) as TaskPriority[]).map((value) => ({
                  value,
                  label: priorityLabels[value],
                }))}
              />

              <LabeledSelect
                label="Milestone"
                ariaLabel="Task milestone"
                value={milestoneId}
                onValueChange={setMilestoneId}
                disabled={submitting || !selectedProject || milestonesQuery.isPending}
                placeholder={milestonesQuery.isPending ? 'Loading…' : 'No milestone'}
                hint={
                  selectedProject
                    ? 'Only milestones in this project are offered.'
                    : 'Choose a project before choosing a milestone.'
                }
                options={milestoneOptions}
              />

              <Field label="Deadline" hint="Local time; the server stores the instant." error={errors.deadline}>
                <Input
                  type="datetime-local"
                  value={deadline}
                  onChange={(event) => {
                    setDeadline(event.target.value)
                    if (errors.deadline) setErrors((current) => ({ ...current, deadline: undefined }))
                  }}
                  disabled={submitting}
                />
              </Field>
            </div>
          </div>

          <div className="rounded-md border border-line bg-surface-sunken/30">
            <button
              type="button"
              aria-expanded={advancedOpen}
              onClick={() => setAdvancedOpen((current) => !current)}
              className="flex w-full items-center gap-2 rounded-md px-3 py-2.5 text-left text-sm font-medium text-ink transition-colors hover:bg-surface-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
            >
              <ChevronDownIcon
                size={14}
                className={cn('shrink-0 text-ink-muted transition-transform', advancedOpen && 'rotate-180')}
              />
              <span>Advanced</span>
              <span className="min-w-0 truncate text-2xs font-normal text-ink-muted">
                Estimate, dependencies, acceptance criteria
              </span>
              <span className="ml-auto shrink-0 text-2xs font-normal text-ink-muted">
                {advancedOpen ? 'Hide' : 'Show'}
              </span>
            </button>

            {advancedOpen ? (
              <div className="flex flex-col gap-4 border-t border-line px-3 py-3.5">
                <Field
                  label="Estimate (minutes)"
                  hint="Whole minutes. Leave it empty if you do not know yet."
                  error={errors.estimate}
                >
                  <Input
                    type="number"
                    min={1}
                    step={5}
                    inputMode="numeric"
                    value={estimate}
                    onChange={(event) => {
                      setEstimate(event.target.value)
                      if (errors.estimate) setErrors((current) => ({ ...current, estimate: undefined }))
                    }}
                    disabled={submitting}
                    className="max-w-[12rem]"
                  />
                </Field>

                <div className="flex flex-col gap-1.5">
                  <p className="text-sm font-medium leading-5 text-ink">Dependencies</p>
                  <p className="text-xs leading-4 text-ink-muted">
                    Tasks that must finish first. Only tasks in the same project are offered.
                  </p>
                  {prerequisites.length > 0 ? (
                    <ul className="flex flex-col gap-1" aria-label="Prerequisites for the new task">
                      {prerequisites.map((id) => {
                        const label = taskTitles.get(id) ?? 'another task'
                        return (
                          <li
                            key={id}
                            className="flex items-center gap-2 rounded-sm px-1 py-1 text-sm text-ink-soft"
                          >
                            <span className="min-w-0 flex-1 truncate">{label}</span>
                            <IconButton
                              label={`Remove prerequisite ${label}`}
                              icon={<CloseIcon size={13} />}
                              variant="quiet"
                              size="sm"
                              disabled={submitting}
                              onClick={() =>
                                setPrerequisites((current) => current.filter((taskId) => taskId !== id))
                              }
                            />
                          </li>
                        )
                      })}
                    </ul>
                  ) : null}
                  <div className="flex items-center gap-1.5">
                    <Select
                      value={prerequisiteChoice}
                      onValueChange={setPrerequisiteChoice}
                      disabled={submitting || !selectedProject || dependencyCandidates.length === 0}
                    >
                      <SelectTrigger className="h-8 min-w-0 flex-1 text-sm" aria-label="Add a prerequisite">
                        <SelectValue
                          placeholder={
                            !selectedProject
                              ? 'Choose a project first'
                              : dependencyCandidates.length === 0
                                ? 'No other tasks available'
                                : 'Choose a task'
                          }
                        />
                      </SelectTrigger>
                      <SelectContent>
                        {dependencyCandidates.map((candidate) => (
                          <SelectItem key={candidate.id} value={candidate.id}>
                            {candidate.title}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      disabled={submitting || prerequisiteChoice.length === 0}
                      onClick={() => {
                        if (prerequisiteChoice.length === 0) return
                        setPrerequisites((current) => [...current, prerequisiteChoice])
                        setPrerequisiteChoice('')
                      }}
                    >
                      Add
                    </Button>
                  </div>
                  {errors.prerequisites ? (
                    <p role="alert" className="text-xs leading-4 text-danger">
                      {errors.prerequisites}
                    </p>
                  ) : null}
                </div>

                <div className="flex flex-col gap-1.5">
                  <div className="flex items-center justify-between gap-3">
                    <p className="text-sm font-medium leading-5 text-ink">Acceptance criteria</p>
                    <Button
                      type="button"
                      variant="quiet"
                      size="sm"
                      leadingIcon={<PlusIcon size={13} />}
                      disabled={submitting || criteria.length >= MAX_ACCEPTANCE_CRITERIA}
                      onClick={addCriterion}
                    >
                      Add criterion
                    </Button>
                  </div>
                  {criteria.length === 0 ? (
                    <p className="text-xs leading-4 text-ink-muted">
                      Add the checks that define done. Their order is kept.
                    </p>
                  ) : (
                    <ul className="flex flex-col gap-1.5">
                      {criteria.map((criterion, index) => (
                        <li key={criterion.key} className="flex items-center gap-1.5">
                          <span className="w-5 shrink-0 text-right text-2xs tabular-nums text-ink-faint">
                            {index + 1}
                          </span>
                          <Input
                            ref={(element) => {
                              if (element) criterionInputs.current.set(criterion.key, element)
                              else criterionInputs.current.delete(criterion.key)
                            }}
                            value={criterion.text}
                            onChange={(event) => {
                              const next = event.target.value
                              setCriteria((current) =>
                                current.map((row) =>
                                  row.key === criterion.key ? { ...row, text: next } : row,
                                ),
                              )
                            }}
                            aria-label={`Acceptance criterion ${index + 1}`}
                            placeholder="What must be true?"
                            maxLength={MAX_ACCEPTANCE_CRITERION_LENGTH}
                            disabled={submitting}
                          />
                          <IconButton
                            label={`Remove acceptance criterion ${index + 1}`}
                            icon={<CloseIcon size={13} />}
                            variant="quiet"
                            size="sm"
                            disabled={submitting}
                            onClick={() => removeCriterion(criterion.key)}
                          />
                        </li>
                      ))}
                    </ul>
                  )}
                  {errors.criteria ? (
                    <p role="alert" className="text-xs leading-4 text-danger">
                      {errors.criteria}
                    </p>
                  ) : null}
                </div>
              </div>
            ) : null}
          </div>

          {error ? (
            <FormErrorBanner title="The task was not created" description={userMessage(error)} />
          ) : null}
          {uncertain ? (
            <FormErrorBanner
              className="border-warning/40 bg-warning-soft text-warning-ink"
              title="The task may already exist"
              description={uncertainMessage(createTask.error ?? undefined)}
            />
          ) : null}

          {confirmDiscard ? (
            <div
              role="group"
              aria-label="Discard this draft"
              className="flex flex-col gap-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2.5 text-warning-ink sm:flex-row sm:items-center sm:justify-between"
            >
              <p className="text-xs leading-5">
                {uncertain
                  ? 'The server may already have created this task. Closing now can lead to a duplicate if you create it again; the attempt is kept, so Create task retries the same command and task ids.'
                  : 'Discard this task draft? It has not been sent to the server.'}
              </p>
              <div className="flex shrink-0 items-center gap-2">
                <Button type="button" variant="ghost" size="sm" onClick={() => setConfirmDiscard(false)}>
                  Keep editing
                </Button>
                <Button type="button" variant="danger" size="sm" onClick={discardAndClose}>
                  {uncertain ? 'Close anyway' : 'Discard draft'}
                </Button>
              </div>
            </div>
          ) : null}

          <div className="flex items-center justify-between gap-3 border-t border-line pt-3">
            <p className="hidden text-2xs text-ink-muted sm:block">
              Press <kbd className="command-key">Enter</kbd> to create
            </p>
            <div className="ml-auto flex items-center gap-2">
              <Button type="button" variant="ghost" onClick={requestClose} disabled={submitting}>
                Cancel
              </Button>
              <Button type="submit" loading={submitting}>
                {submitting ? 'Creating…' : 'Create task'}
              </Button>
            </div>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
