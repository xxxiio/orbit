import { useMemo, useState, type ReactNode } from 'react'
import type { Project } from '@/api/adapters'
import type { ApiError } from '@/api/errors'
import {
  useApplyUpgrade,
  useAvailableWorkflows,
  useUpgradePreview,
  useWorkflowDefinition,
  userMessage,
  type Workflow,
  type WorkflowBinding,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import {
  AlertCircleIcon,
  AlertTriangleIcon,
  CheckIcon,
  LoaderIcon,
  SettingsIcon,
} from '@/components/ui/icons'
import { Panel, PanelBody, PanelFooter, PanelHeader } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { cn } from '@/components/ui/utils'
import { humanizeIdentifier } from '@/lib/humanize'
import { projectStatusLabel, workflowCopy, workflowText } from '@/domain'

function IdentityRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[8rem_minmax(0,1fr)] items-start gap-3 border-b border-line py-2 last:border-b-0">
      <dt className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</dt>
      <dd className="min-w-0 text-sm leading-5 text-ink-soft">{children}</dd>
    </div>
  )
}

function WorkflowChoice({
  id,
  version,
  selected,
  pinned,
  onSelect,
}: {
  id: string
  version: number
  selected: boolean
  pinned: boolean
  onSelect: () => void
}) {
  const definition = useWorkflowDefinition(id, version)
  const name = definition.data?.name ?? humanizeIdentifier(id)

  return (
    <label
      className={cn(
        'flex cursor-pointer items-start gap-2.5 rounded-md border px-3 py-2.5 transition-colors',
        selected
          ? 'border-accent bg-accent-soft/50'
          : 'border-line bg-surface hover:border-line-strong hover:bg-surface-hover/40',
      )}
    >
      <input
        type="radio"
        name="target-workflow"
        value={`${id}@${version}`}
        checked={selected}
        onChange={onSelect}
        className="mt-0.5 h-3.5 w-3.5 accent-[hsl(var(--accent))]"
      />
      <span className="min-w-0 flex-1">
        <span className="flex flex-wrap items-center gap-2">
          <span className="text-sm font-medium text-ink">{name}</span>
          <span className="font-mono text-2xs text-ink-muted">v{definition.data?.version ?? version}</span>
          {pinned ? <Badge tone="accent">Pinned</Badge> : selected ? <CheckIcon size={13} className="text-accent" /> : null}
        </span>
        <span className="mt-0.5 block text-xs leading-5 text-ink-muted">
          {workflowCopy(definition.data?.description)?.trim()
            ? workflowCopy(definition.data?.description)
            : definition.error
              ? 'This workflow is listed by the server, but its description could not be read.'
              : 'No description was published for this workflow.'}
        </span>
      </span>
    </label>
  )
}

function ViewList({ workflow, loading }: { workflow: Workflow | undefined; loading: boolean }) {
  if (loading && !workflow) {
    return (
      <div className="flex flex-col gap-2" aria-busy="true">
        <Skeleton className="h-3 w-2/3" />
        <Skeleton className="h-3 w-1/2" />
      </div>
    )
  }
  if (!workflow) {
    return (
      <p className="text-xs leading-5 text-ink-muted">
        No workflow is pinned, so the workspace falls back to Overview and Tasks.
      </p>
    )
  }
  return (
    <ul className="flex flex-col divide-y divide-line rounded-md border border-line">
      {workflow.views.map((view) => (
        <li key={view.id} className="px-3 py-2">
          <p className="text-sm font-medium text-ink">
            {workflowText(view.title) || humanizeIdentifier(view.id)}
          </p>
          <p className="mt-0.5 text-2xs leading-4 text-ink-muted">
            {workflowCopy(view.description) || 'The workflow gives this view no description.'}
          </p>
          <p className="mt-0.5 text-2xs text-ink-faint">
            {view.components.length} panel{view.components.length === 1 ? '' : 's'} ·{' '}
            {view.layout.type} layout
          </p>
        </li>
      ))}
    </ul>
  )
}

function NoticeList({
  tone,
  notices,
}: {
  tone: 'warning' | 'danger'
  notices: Array<{ code: string; message: string; subjects: string[] }>
}) {
  if (notices.length === 0) return null
  const Icon = tone === 'danger' ? AlertCircleIcon : AlertTriangleIcon
  return (
    <ul
      className={cn(
        'flex flex-col gap-1 rounded-md border px-3 py-2',
        tone === 'danger'
          ? 'border-danger/35 bg-danger-soft/60'
          : 'border-warning/35 bg-warning-soft/60',
      )}
    >
      {notices.map((notice) => (
        <li
          key={`${notice.code}-${notice.subjects.join(',')}`}
          className={cn(
            'flex items-start gap-1.5 text-2xs leading-4',
            tone === 'danger' ? 'text-danger-ink' : 'text-warning-ink',
          )}
        >
          <Icon size={12} className="mt-0.5 shrink-0" />
          <span className="min-w-0">
            {notice.message}
            {notice.subjects.length > 0 ? (
              <span className="block text-ink-muted">{notice.subjects.join(', ')}</span>
            ) : null}
          </span>
        </li>
      ))}
    </ul>
  )
}

/**
 * Project settings: identity and the pinned workflow. Every action maps to an
 * API command; anything the API cannot do is stated instead of simulated.
 */
export function ProjectSettings({
  project,
  workflow,
  binding,
  loading,
  error,
  onRetry,
}: {
  project: Project
  workflow: Workflow | undefined
  binding: WorkflowBinding | null
  loading: boolean
  error: ApiError | null
  onRetry: () => void
}) {
  const available = useAvailableWorkflows()
  const previewTarget = useMemo(() => available.data ?? [], [available.data])
  const [selected, setSelected] = useState<{ id: string; version: number } | null>(null)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const { toast } = useToast()

  const target = selected
  const preview = useUpgradePreview(
    target ? { projectId: project.id, workflowId: target.id, version: target.version } : null,
  )
  const targetDefinition = useWorkflowDefinition(target?.id, target?.version)
  const apply = useApplyUpgrade()

  const pinnedId = binding?.workflowId ?? null
  const pinnedVersion = binding?.version ?? null
  const targetIsPinned =
    target !== null && target.id === pinnedId && target.version === pinnedVersion

  const viewTitles = useMemo(() => {
    const map = new Map<string, string>()
    for (const view of targetDefinition.data?.views ?? []) map.set(view.id, view.title)
    return (id: string) => map.get(id) ?? humanizeIdentifier(id)
  }, [targetDefinition.data])

  const blockers = preview.data?.blockers ?? []
  const canApply = Boolean(preview.data) && blockers.length === 0 && !targetIsPinned

  const applyChange = async () => {
    if (!target) return
    try {
      const result = await apply.mutateAsync({
        projectId: project.id,
        workflowId: target.id,
        version: target.version,
      })
      setConfirmOpen(false)
      setSelected(null)
      toast({
        tone: 'success',
        title: result.alreadyApplied ? 'Workflow already pinned' : 'Workflow changed',
        description: result.alreadyApplied
          ? `This project was already on ${
              targetDefinition.data?.name ?? humanizeIdentifier(result.target.workflowId)
            } v${result.target.version}.`
          : `The project now follows ${targetDefinition.data?.name ?? humanizeIdentifier(target.id)} v${target.version}. Its tabs come from the new workflow.`,
      })
    } catch {
      // `apply.error` is shown inside the confirmation dialog.
    }
  }

  return (
    <div className="grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(0,24rem)]">
      <div className="flex min-w-0 flex-col gap-5">
        <Panel>
          <PanelHeader
            title="Identity"
            eyebrow="Project settings"
            description="What the server stores for this project."
          />
          <PanelBody>
            <dl className="flex flex-col">
              <IdentityRow label="Name">{project.name}</IdentityRow>
              <IdentityRow label="Status">{projectStatusLabel(project.status)}</IdentityRow>
              <IdentityRow label="Project ID">
                <span className="break-all font-mono text-2xs text-ink-muted">{project.id}</span>
              </IdentityRow>
              <IdentityRow label="Rename">
                <span className="text-ink-muted">
                  Not supported by the API yet. This build cannot rename a project, so the name and
                  id stay as they are.
                </span>
              </IdentityRow>
            </dl>
          </PanelBody>
          <PanelFooter>
            <p className="text-2xs leading-4 text-ink-muted">
              Creating a differently named project is the only supported way to change identity; its
              work does not move with it.
            </p>
          </PanelFooter>
        </Panel>

        <Panel>
          <PanelHeader
            title="Views in this workflow"
            description="The tabs the workspace shows for this project."
            actions={
              <Badge tone="neutral">
                {workflow ? `${workflow.views.length} view${workflow.views.length === 1 ? '' : 's'}` : 'Fallback views'}
              </Badge>
            }
          />
          <PanelBody>
            {error ? (
              <div className="flex flex-wrap items-center gap-2 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-warning-ink">
                <AlertCircleIcon size={14} className="shrink-0" />
                <span className="min-w-0 flex-1">{userMessage(error)}</span>
                <Button variant="quiet" size="sm" onClick={onRetry}>
                  Retry
                </Button>
              </div>
            ) : (
              <ViewList workflow={workflow} loading={loading} />
            )}
          </PanelBody>
        </Panel>
      </div>

      <div className="flex min-w-0 flex-col gap-5">
        <Panel>
          <PanelHeader
            title="Workflow"
            eyebrow="Pinned"
            description="A project follows one published workflow version."
            actions={
              <Button
                variant="quiet"
                size="sm"
                loading={available.isFetching}
                onClick={() => void available.refetch()}
              >
                Refresh list
              </Button>
            }
          />
          <PanelBody className="flex flex-col gap-3">
            {binding ? (
              <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2">
                <p className="flex flex-wrap items-center gap-2 text-sm font-medium text-ink">
                  {workflow?.name ?? humanizeIdentifier(binding.workflowId)}
                  <span className="font-mono text-2xs text-ink-muted">v{binding.version}</span>
                  <Badge tone="accent">Current</Badge>
                </p>
              </div>
            ) : (
              <p className="text-xs leading-5 text-ink-muted">
                No workflow is pinned. Choose one below to give this project its views.
              </p>
            )}

            <div>
              <p className="text-xs font-semibold text-ink">Choose a workflow version</p>
              <p className="mt-0.5 text-2xs leading-4 text-ink-muted">
                The server lists the newest published version of each workflow. Pinning requires a
                reviewed, confirmed change.
              </p>
              <div className="mt-2 flex flex-col gap-1.5">
                {available.isPending ? (
                  <>
                    <Skeleton className="h-12 rounded-md" />
                    <Skeleton className="h-12 rounded-md" />
                  </>
                ) : available.error ? (
                  <p className="flex items-start gap-1.5 text-xs text-danger-ink">
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                    {userMessage(available.error)}
                  </p>
                ) : previewTarget.length === 0 ? (
                  <p className="text-xs text-ink-muted">The server lists no published workflows.</p>
                ) : (
                  previewTarget.map((option) => (
                    <WorkflowChoice
                      key={`${option.id}@${option.version}`}
                      id={option.id}
                      version={option.version}
                      selected={selected?.id === option.id && selected?.version === option.version}
                      pinned={option.id === pinnedId && option.version === pinnedVersion}
                      onSelect={() => setSelected({ id: option.id, version: option.version })}
                    />
                  ))
                )}
              </div>
            </div>

            {target ? (
              <div className="flex flex-col gap-2 border-t border-line pt-3">
                <p className="text-xs font-semibold text-ink">Change preview</p>
                {preview.error ? (
                  <p className="flex items-start gap-1.5 text-xs text-danger-ink">
                    <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                    {userMessage(preview.error)}
                  </p>
                ) : preview.isPending || !preview.data ? (
                  <div className="flex items-center gap-1.5 text-xs text-ink-muted">
                    <LoaderIcon size={13} className="animate-spin" /> Comparing with the current
                    workflow…
                  </div>
                ) : (
                  <>
                    <p className="text-2xs leading-4 text-ink-muted">
                      {preview.data.current
                        ? `${workflow?.name ?? humanizeIdentifier(preview.data.current.workflowId)} v${preview.data.current.version}`
                        : 'No workflow'}{' '}
                      → {targetDefinition.data?.name ?? humanizeIdentifier(target.id)} v
                      {preview.data.target.version}
                    </p>
                    <p className="break-all font-mono text-2xs text-ink-faint">
                      fingerprint {preview.data.fingerprint}
                    </p>
                    {preview.data.views.added.length > 0 ? (
                      <p className="text-2xs leading-4 text-success-ink">
                        Views added: {preview.data.views.added.map(viewTitles).join(', ')}
                      </p>
                    ) : null}
                    {preview.data.views.removed.length > 0 ? (
                      <p className="text-2xs leading-4 text-warning-ink">
                        Views removed: {preview.data.views.removed.map(viewTitles).join(', ')}
                      </p>
                    ) : null}
                    {preview.data.views.changed.length > 0 ? (
                      <p className="text-2xs leading-4 text-ink-muted">
                        Views changed: {preview.data.views.changed.map((entry) => viewTitles(entry.view)).join(', ')}
                      </p>
                    ) : null}
                    <NoticeList tone="warning" notices={preview.data.warnings} />
                    <NoticeList tone="danger" notices={preview.data.blockers} />
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={!canApply}
                      onClick={() => setConfirmOpen(true)}
                    >
                      Change workflow…
                    </Button>
                    {targetIsPinned ? (
                      <p className="text-2xs text-ink-muted">
                        This project is already pinned to that version.
                      </p>
                    ) : blockers.length > 0 ? (
                      <p className="text-2xs text-danger-ink">
                        Blockers above must be resolved before the server will accept the change.
                      </p>
                    ) : null}
                  </>
                )}
              </div>
            ) : null}
          </PanelBody>
          <PanelFooter>
            <p className="text-2xs leading-4 text-ink-muted">
              Changing a workflow re-pins the project. Existing work items are kept; the server
              refuses the change when one cannot satisfy the target workflow.
            </p>
          </PanelFooter>
        </Panel>

        <Panel>
          <PanelBody className="flex items-start gap-2">
            <SettingsIcon size={15} className="mt-0.5 shrink-0 text-ink-faint" />
            <p className="text-2xs leading-4 text-ink-muted">
              Project archives, deletions and rename are not available over this API, so they are
              not offered here. Editing workflows themselves is a console feature; this screen only
              pins a published version.
            </p>
          </PanelBody>
        </Panel>
      </div>

      <Dialog open={confirmOpen} onOpenChange={(open) => !apply.isPending && setConfirmOpen(open)}>
        <DialogContent
          title="Change this project's workflow?"
          description="The project will follow the target workflow's views immediately after the server accepts the change."
        >
          <div className="flex flex-col gap-3">
            <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-sm">
              <p className="text-ink">
                {binding ? `${workflow?.name ?? humanizeIdentifier(binding.workflowId)} v${binding.version}` : 'No workflow'}{' '}
                <span className="text-ink-faint">→</span>{' '}
                {targetDefinition.data?.name ?? (target ? humanizeIdentifier(target.id) : '')} v
                {target?.version}
              </p>
              <p className="mt-1 break-all font-mono text-2xs text-ink-muted">
                {preview.data?.fingerprint}
              </p>
            </div>

            <p className="text-xs leading-5 text-ink-muted">
              {preview.data?.views.removed.length
                ? `These tabs disappear: ${preview.data.views.removed.map(viewTitles).join(', ')}. Their data is not deleted.`
                : 'The current tabs stay available unless the target workflow does not declare them.'}{' '}
              Nothing is written until you confirm.
            </p>

            <NoticeList tone="danger" notices={blockers} />

            {apply.error ? (
              <p role="alert" className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
                <AlertCircleIcon size={13} className="mt-0.5 shrink-0" />
                <span className="min-w-0 flex-1">{userMessage(apply.error)}</span>
              </p>
            ) : null}

            <div className="flex items-center justify-end gap-2">
              <Button
                variant="secondary"
                size="md"
                onClick={() => setConfirmOpen(false)}
                disabled={apply.isPending}
              >
                Cancel
              </Button>
              <Button size="md" onClick={() => void applyChange()} disabled={!canApply} loading={apply.isPending}>
                Change workflow
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
