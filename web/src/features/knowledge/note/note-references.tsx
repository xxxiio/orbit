/**
 * A note's typed references: project and task targets stored in the body as
 * `[project:orbit](orbit://project:orbit)`.
 *
 * Add and remove go through the server's reference command, which reads and
 * publishes the body through the Markdown command path. Nothing here rewrites a
 * token by hand, and an unresolved target is named as broken instead of being
 * dropped or turned into a dead link.
 */
import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import type { TypedReference } from '@/api/contracts'
import {
  findTypedReferences,
  formatTypedReference,
  isReferenceBroken,
  parseTypedReference,
  referenceKindLabel,
  type ReferenceIndex,
} from '../reference-model'
import {
  newCommandId,
  uncertainMessage,
  useIdempotentAttempt,
  useProjects,
  useReferenceCommand,
  useWorkspaceSnapshot,
  userMessage,
  type ApiError,
} from '@/hooks'
import { taskSurfaceHref } from '../knowledge-model'
import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { AlertCircleIcon, LinkIcon, PlusIcon } from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { cn } from '@/components/ui/utils'

interface ReferenceAttempt {
  key: string
  commandId: string
}

export interface NoteReferencesProps {
  noteId: string | null
  /** The note body as the server last returned it. */
  body: string
  /** The project this note is being read from, when it is read from one. */
  projectId?: string
  /** The task this note is being read from, when it is read from one. */
  taskId?: string
  /** The reference command may not run while the editor holds a draft. */
  readOnly?: boolean
  /** Re-reads the note after an accepted reference change. */
  onChanged: () => void
}

export function NoteReferences({
  noteId,
  body,
  projectId,
  taskId,
  readOnly = false,
  onChanged,
}: NoteReferencesProps) {
  const projects = useProjects()
  const snapshot = useWorkspaceSnapshot()
  const command = useReferenceCommand()
  const attempt = useIdempotentAttempt<ReferenceAttempt>((key) => ({
    key,
    commandId: newCommandId(),
  }))
  const [choice, setChoice] = useState('')
  const [active, setActive] = useState<{ target: string; action: 'add' | 'remove' } | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState<string | null>(null)

  const references = useMemo(() => findTypedReferences(body), [body])

  const tasks = snapshot.data?.tasks ?? []
  const indexReady = snapshot.data !== undefined
  const index: ReferenceIndex = useMemo(
    () => ({
      projectIds: new Set(projects.map((project) => project.id)),
      taskIds: new Set(tasks.map((task) => task.id)),
    }),
    [projects, tasks],
  )
  const projectNames = useMemo(
    () => new Map(projects.map((project) => [project.id, project.name])),
    [projects],
  )
  const taskTitles = useMemo(() => new Map(tasks.map((task) => [task.id, task.title])), [tasks])
  const taskProjects = useMemo(() => new Map(tasks.map((task) => [task.id, task.projectId])), [tasks])

  const referenced = useMemo(
    () => new Set(references.map((reference) => formatTypedReference(reference))),
    [references],
  )

  const brokenReferences = useMemo(
    () => (indexReady ? references.filter((reference) => isReferenceBroken(reference, index)) : []),
    [references, indexReady, index],
  )

  const projectChoices = useMemo(
    () =>
      [...projects]
        .filter((project) => !referenced.has(`project:${project.id}`))
        .sort((left, right) => left.name.localeCompare(right.name)),
    [projects, referenced],
  )
  const taskChoices = useMemo(
    () =>
      tasks
        .filter((task) => (projectId ? task.projectId === projectId : true))
        .filter((task) => !referenced.has(`task:${task.id}`))
        .sort((left, right) => left.title.localeCompare(right.title)),
    [tasks, projectId, referenced],
  )

  const pending = command.isPending
  const uncertain = command.disposition === 'uncertain' && attempt.attempt !== null
  const error: ApiError | null =
    command.disposition === 'rejected' || uncertain ? command.error : null
  const unavailable = error?.status === 404

  const run = async (reference: TypedReference, action: 'add' | 'remove') => {
    if (!noteId || pending || readOnly) return
    const target = formatTypedReference(reference)
    const key = `${noteId}\u0000${target}\u0000${action}`
    setActive({ target, action })
    setNotice(null)
    setAnnouncement(null)

    const label = referenceChipText(reference, projectNames, taskTitles)
    const pinned = attempt.attempt
    if (pinned && pinned.key !== key) {
      if (command.disposition === 'uncertain') {
        setNotice(
          'A previous reference change is still unresolved. Retry it unchanged before changing another reference.',
        )
        return
      }
      attempt.clear()
    }
    const replay = pinned?.key === key ? pinned : null
    if (!replay) {
      if (action === 'add' && referenced.has(target)) {
        setNotice(`${label} is already referenced. Nothing was sent.`)
        return
      }
      if (action === 'remove' && !referenced.has(target)) {
        setNotice(`${label} is not referenced by this note. Nothing was sent.`)
        return
      }
    }

    const pinnedNext = replay ?? attempt.ensure(key)
    try {
      const receipt = await command.mutateAsync({
        commandId: pinnedNext.commandId,
        noteId,
        target,
        action,
      })
      attempt.clear()
      setActive(null)
      setAnnouncement(
        receipt.changed
          ? action === 'add'
            ? `Reference to ${label} added.`
            : `Reference to ${label} removed.`
          : `The note already matched that change; nothing was written.`,
      )
      onChanged()
    } catch {
      // The server's answer stays below; the attempt stays pinned for a retry.
    }
  }

  const addChoice = () => {
    const reference = parseTypedReference(choice)
    if (!reference) return
    setChoice('')
    void run(reference, 'add')
  }

  const projectReference = projectId ? { kind: 'project' as const, id: projectId } : null
  const taskReference = taskId ? { kind: 'task' as const, id: taskId } : null

  return (
    <Panel className="mt-4 overflow-hidden" aria-label="Typed references" data-reference-panel>
      <PanelHeader
        title="Project and task references"
        description="A typed reference names a project or a task in the note body. It is stored by the server, not inferred from the text."
        actions={
          <span className="text-2xs text-ink-muted">
            {references.length === 0
              ? 'No typed references'
              : `${references.length} reference${references.length === 1 ? '' : 's'}`}
          </span>
        }
      />
      <PanelBody>
        {!noteId ? (
          <p className="text-xs leading-5 text-ink-muted">
            References are unavailable without an <span className="font-mono">orbit_id</span>.
          </p>
        ) : (
          <div className="flex flex-col gap-4">
            {references.length === 0 ? (
              <p className="text-xs leading-5 text-ink-muted">
                This note does not reference a project or a task yet.
              </p>
            ) : (
              <ul aria-label="References" className="flex flex-wrap gap-1.5" data-reference-group="typed">
                {references.map((reference) => {
                  const target = formatTypedReference(reference)
                  const label = referenceChipText(reference, projectNames, taskTitles)
                  const broken = indexReady && isReferenceBroken(reference, index)
                  const updating = active?.target === target && pending
                  return (
                    <li key={target} className="max-w-full">
                      <span
                        data-reference-target={target}
                        data-reference-state={broken ? 'broken' : indexReady ? 'ok' : 'unknown'}
                        className={cn(
                          'inline-flex max-w-full items-center gap-1.5 rounded-full border px-2 py-0.5 text-2xs',
                          broken
                            ? 'border-warning/40 bg-warning-soft text-warning-ink'
                            : 'border-accent/30 bg-accent-soft text-accent-soft-ink',
                        )}
                      >
                        {broken ? (
                          <AlertCircleIcon size={12} className="shrink-0" aria-hidden="true" />
                        ) : (
                          <LinkIcon size={12} className="shrink-0" aria-hidden="true" />
                        )}
                        {broken || !indexReady ? (
                          <span className="min-w-0 truncate font-medium">{label}</span>
                        ) : (
                          <Link
                            to={
                              reference.kind === 'project'
                                ? `/projects/${reference.id}`
                                : taskHref(taskProjects.get(reference.id) ?? null, reference.id)
                            }
                            className="min-w-0 truncate font-medium underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                            title={
                              reference.kind === 'project'
                                ? `Open the project ${reference.id}`
                                : `Open the task ${reference.id}`
                            }
                          >
                            {label}
                          </Link>
                        )}
                        <span className="shrink-0 font-mono text-ink-faint">{target}</span>
                        {readOnly ? null : (
                          <button
                            type="button"
                            onClick={() => void run(reference, 'remove')}
                            disabled={pending}
                            aria-label={`Remove the reference to ${label}`}
                            data-reference-remove={target}
                            className="shrink-0 rounded-full px-1 text-ink-muted transition-colors hover:text-danger-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 disabled:opacity-50"
                          >
                            ×
                          </button>
                        )}
                        {updating ? <span className="sr-only">Updating</span> : null}
                      </span>
                    </li>
                  )
                })}
              </ul>
            )}

            {!indexReady && references.length > 0 ? (
              snapshot.error ? (
                <p role="alert" className="text-2xs leading-4 text-warning-ink">
                  The project and task list could not be read, so these references cannot be checked
                  for a deleted target. {userMessage(snapshot.error)}
                </p>
              ) : (
                <p className="text-2xs leading-4 text-ink-muted">
                  Waiting for the project and task list, so a reference is not called broken before
                  it can be checked.
                </p>
              )
            ) : null}

            {brokenReferences.length > 0 ? (
              <div
                role="alert"
                data-reference-broken
                className="rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
              >
                <p className="font-medium">
                  Some references name something this workspace does not have.
                </p>
                <ul className="mt-1 list-disc space-y-0.5 pl-4">
                  {brokenReferences.map((reference) => (
                    <li key={formatTypedReference(reference)}>
                      {referenceKindLabel(reference.kind)}{' '}
                      <span className="font-mono">{reference.id}</span> is not in this workspace. It
                      may have been removed from another device. The reference stays in the note
                      until you remove it.
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            {readOnly ? (
              <p className="text-2xs leading-4 text-ink-muted">
                Save or cancel the editor before changing references.
              </p>
            ) : (
              <div className="flex flex-col gap-2 border-t border-line pt-3">
                <div className="flex flex-wrap items-center gap-2">
                  <Select value={choice} onValueChange={setChoice} disabled={pending || unavailable}>
                    <SelectTrigger
                      className="h-8 min-w-0 flex-1 text-sm"
                      aria-label="Choose a project or task to reference"
                      data-field="reference-target"
                    >
                      <SelectValue
                        placeholder={
                          projectChoices.length === 0 && taskChoices.length === 0
                            ? 'Every project and task is already referenced'
                            : 'Choose a project or task'
                        }
                      />
                    </SelectTrigger>
                    <SelectContent>
                      {projectChoices.length > 0 ? (
                        <SelectGroup>
                          <SelectLabel>Projects</SelectLabel>
                          {projectChoices.map((project) => (
                            <SelectItem key={project.id} value={`project:${project.id}`}>
                              {project.name}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      ) : null}
                      {taskChoices.length > 0 ? (
                        <SelectGroup>
                          <SelectLabel>Tasks</SelectLabel>
                          {taskChoices.map((task) => (
                            <SelectItem key={task.id} value={`task:${task.id}`}>
                              {task.title}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      ) : null}
                    </SelectContent>
                  </Select>
                  <Button
                    size="sm"
                    variant="secondary"
                    leadingIcon={<PlusIcon size={14} />}
                    disabled={choice.length === 0 || pending || unavailable}
                    onClick={addChoice}
                    data-action="add-reference"
                  >
                    Add reference
                  </Button>
                  {projectReference && !referenced.has(`project:${projectReference.id}`) ? (
                    <Button
                      size="sm"
                      variant="quiet"
                      onClick={() => void run(projectReference, 'add')}
                      disabled={pending || unavailable}
                      data-action="reference-this-project"
                    >
                      Reference this project
                    </Button>
                  ) : null}
                  {taskReference && !referenced.has(`task:${taskReference.id}`) ? (
                    <Button
                      size="sm"
                      variant="quiet"
                      onClick={() => void run(taskReference, 'add')}
                      disabled={pending || unavailable}
                      data-action="reference-this-task"
                    >
                      Reference this task
                    </Button>
                  ) : null}
                </div>
              </div>
            )}

            {notice ? (
              <p role="alert" className="text-xs leading-5 text-ink-soft">
                {notice}
              </p>
            ) : null}

            {error ? (
              <div
                role="alert"
                className={cn(
                  'rounded-md border px-3 py-2 text-xs leading-5',
                  uncertain
                    ? 'border-warning/40 bg-warning-soft text-warning-ink'
                    : 'border-danger/30 bg-danger-soft text-danger-ink',
                )}
              >
                <p className="font-medium">
                  {unavailable
                    ? 'This server does not accept reference commands yet.'
                    : uncertain
                      ? 'The reference change was not confirmed.'
                      : 'The server refused this reference change.'}
                </p>
                <p className="mt-0.5">
                  {unavailable
                    ? 'The note was not changed. Update the server to reference projects and tasks from a note.'
                    : uncertain
                      ? uncertainMessage(error)
                      : userMessage(error)}
                </p>
                {uncertain && attempt.attempt ? (
                  <Button
                    size="sm"
                    variant="secondary"
                    className="mt-2"
                    disabled={pending}
                    onClick={() => {
                      const reference = active ? parseTypedReference(active.target) : null
                      if (reference && active) void run(reference, active.action)
                    }}
                  >
                    Retry the same change
                  </Button>
                ) : null}
              </div>
            ) : null}

            <p role="status" aria-live="polite" className="min-h-4 text-2xs text-ink-muted">
              {pending ? 'Sending the reference change…' : announcement}
            </p>

            {references.length > 0 && indexReady ? (
              <p className="text-2xs leading-4 text-ink-faint">
                Removing a reference edits the note body through the server, which re-reads the file
                first. A conflict is reported, never retried silently.
              </p>
            ) : null}
          </div>
        )}
      </PanelBody>
    </Panel>
  )
}

function taskHref(projectId: string | null, taskId: string): string {
  return taskSurfaceHref(projectId, taskId)
}

function referenceChipText(
  reference: TypedReference,
  projectNames: ReadonlyMap<string, string>,
  taskTitles: ReadonlyMap<string, string>,
): string {
  const name = reference.kind === 'project' ? projectNames.get(reference.id) : taskTitles.get(reference.id)
  return name && name.trim().length > 0 ? name : reference.id
}
