import { useEffect, useMemo, useState, type ReactNode } from 'react'
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
  type WorkflowUpgradeApplied,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import {
  AlertCircleIcon,
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
import { UpgradeDiff, UpgradeReceipt } from '@/features/workflows/upgrade-preview'

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
  const targets = useMemo(() => available.data ?? [], [available.data])
  const [selected, setSelected] = useState<{ id: string; version: number } | null>(null)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [confirmed, setConfirmed] = useState(false)
  const [lastApplied, setLastApplied] = useState<WorkflowUpgradeApplied | null>(null)
  const { toast } = useToast()

  const pinnedId = binding?.workflowId ?? null
  const pinnedVersion = binding?.version ?? null
  const newestSameId = useMemo(
    () => (pinnedId ? (targets.find((option) => option.id === pinnedId) ?? null) : null),
    [targets, pinnedId],
  )

  // The default entry point is the newest published version of the *same*
  // workflow. Choosing another workflow is a separate, explicit re-pin below.
  useEffect(() => {
    // After an apply, the receipt is the story; do not re-select the version we
    // just applied while the binding refetch catches up.
    if (selected || lastApplied || !newestSameId || pinnedVersion === null) return
    if (newestSameId.version > pinnedVersion) setSelected(newestSameId)
  }, [selected, lastApplied, newestSameId, pinnedVersion])

  const target = selected
  const preview = useUpgradePreview(
    target ? { projectId: project.id, workflowId: target.id, version: target.version } : null,
  )
  const targetDefinition = useWorkflowDefinition(target?.id, target?.version)
  const apply = useApplyUpgrade()

  const mode: 'upgrade' | 'repin' | 'pin' | null = !target
    ? null
    : !binding
      ? 'pin'
      : target.id === pinnedId
        ? 'upgrade'
        : 'repin'
  const targetIsPinned =
    target !== null && target.id === pinnedId && target.version === pinnedVersion
  const upgradeAvailable =
    Boolean(newestSameId && pinnedVersion !== null && newestSameId.version > pinnedVersion)

  const viewTitles = useMemo(() => {
    const map = new Map<string, string>()
    for (const view of workflow?.views ?? []) map.set(view.id, view.title)
    for (const view of targetDefinition.data?.views ?? []) map.set(view.id, view.title)
    return (id: string) => map.get(id) ?? humanizeIdentifier(id)
  }, [workflow, targetDefinition.data])

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
      setConfirmed(false)
      setSelected(null)
      setLastApplied(result)
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

  const selectTarget = (option: { id: string; version: number }) => {
    setSelected(option)
    setConfirmed(false)
    setLastApplied(null)
  }

  const triggerLabel = mode === 'upgrade' ? 'Upgrade workflow…' : 'Change workflow…'
  const confirmLabel = mode === 'upgrade' ? 'Apply upgrade' : 'Change workflow'
  const targetName = targetDefinition.data?.name ?? (target ? humanizeIdentifier(target.id) : '')
  const currentName = workflow?.name ?? (binding ? humanizeIdentifier(binding.workflowId) : 'No workflow')

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
            description="A project follows one published workflow version. A newer version is applied only after a reviewed, confirmed preview."
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
                  {currentName}
                  <span className="font-mono text-2xs text-ink-muted">v{binding.version}</span>
                  <Badge tone="accent">Current</Badge>
                </p>
              </div>
            ) : (
              <p className="text-xs leading-5 text-ink-muted">
                No workflow is pinned. Choose one below to give this project its views.
              </p>
            )}

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
            ) : (
              <>
                {upgradeAvailable && newestSameId ? (
                  <div
                    data-upgrade-available={newestSameId.version}
                    className="rounded-md border border-accent/30 bg-accent-soft/40 px-3 py-2"
                  >
                    <p className="text-xs font-semibold text-accent-soft-ink">
                      A newer {currentName} version is published
                    </p>
                    <p className="mt-0.5 text-2xs leading-4 text-ink-muted">
                      This project follows v{pinnedVersion ?? '—'} of the same workflow id. The
                      preview below compares it with v{newestSameId.version}. Nothing is applied
                      automatically.
                    </p>
                  </div>
                ) : binding ? (
                  <p className="text-xs leading-5 text-ink-muted">
                    This project is on the newest published version of {currentName}. No upgrade is
                    pending.
                  </p>
                ) : null}

                <details className="border-t border-line pt-3" data-change-workflow="" open>
                  <summary className="cursor-pointer text-xs font-semibold text-ink">
                    Re-pin to a different workflow
                  </summary>
                  <p className="mt-1 text-2xs leading-4 text-ink-muted">
                    A different workflow id is not an upgrade. The same preview, fingerprint and
                    confirmation apply, and the server refuses the change when an existing item
                    cannot satisfy the target.
                  </p>
                  <div className="mt-2 flex flex-col gap-1.5">
                    {targets.map((option) => (
                      <WorkflowChoice
                        key={`${option.id}@${option.version}`}
                        id={option.id}
                        version={option.version}
                        selected={selected?.id === option.id && selected?.version === option.version}
                        pinned={option.id === pinnedId && option.version === pinnedVersion}
                        onSelect={() => selectTarget(option)}
                      />
                    ))}
                  </div>
                </details>
              </>
            )}
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

      {target || lastApplied ? (
        <Panel className="xl:col-span-2" data-upgrade-panel="">
          <PanelHeader
            title={
              !target && lastApplied
                ? 'Applied workflow change'
                : mode === 'upgrade'
                  ? 'Version upgrade preview'
                  : mode === 'repin'
                    ? 'Workflow re-pin preview'
                    : 'Workflow pin preview'
            }
            eyebrow="Workflow change"
            description={
              !target && lastApplied
                ? 'The server accepted this change. Its receipt is below.'
                : 'The server computed this comparison from accepted state. Nothing is written until you confirm.'
            }
            actions={
              lastApplied ? (
                <Badge tone="success">Applied</Badge>
              ) : blockers.length > 0 ? (
                <Badge tone="danger">
                  {blockers.length} blocker{blockers.length === 1 ? '' : 's'}
                </Badge>
              ) : (
                <Badge tone="neutral">No blockers</Badge>
              )
            }
          />
          <PanelBody className="flex flex-col gap-3">
            {lastApplied ? (
              <UpgradeReceipt applied={lastApplied} viewTitle={viewTitles} />
            ) : null}
            {target ? (
              <div className="flex flex-col gap-3" data-upgrade-preview="">
                    <p className="text-2xs leading-4 text-ink-muted">
                      {binding
                        ? `${currentName} v${pinnedVersion}`
                        : 'No workflow'}{' '}
                      → {targetName} v{target.version}
                    </p>
                    <p className="break-all font-mono text-2xs text-ink-faint">
                      fingerprint {preview.data?.fingerprint ?? '…'}
                    </p>

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
                        <UpgradeDiff preview={preview.data} viewTitle={viewTitles} />
                        <Button
                          size="sm"
                          variant="secondary"
                          className="self-start"
                          disabled={!canApply}
                          onClick={() => setConfirmOpen(true)}
                          data-upgrade-apply=""
                        >
                          {triggerLabel}
                        </Button>
                        {targetIsPinned ? (
                          <p className="text-2xs text-ink-muted">
                            This project is already pinned to that version.
                          </p>
                        ) : blockers.length > 0 ? (
                          <p className="text-2xs text-danger-ink">
                            Apply is disabled until every blocker above is resolved.
                          </p>
                        ) : (
                          <p className="text-2xs text-ink-muted">
                            Nothing is written until you review the confirmation.
                          </p>
                        )}
                      </>
                    )}
              </div>
            ) : null}
          </PanelBody>
        </Panel>
      ) : null}

      <Dialog
        open={confirmOpen}
        onOpenChange={(open) => {
          if (apply.isPending) return
          setConfirmOpen(open)
          if (!open) setConfirmed(false)
        }}
      >
        <DialogContent
          title={mode === 'upgrade' ? 'Upgrade this workflow?' : "Change this project's workflow?"}
          description={
            mode === 'upgrade'
              ? 'The project will follow the target version immediately after the server accepts the change.'
              : 'The project will follow the target workflow immediately after the server accepts the change.'
          }
        >
          <div className="flex flex-col gap-3">
            <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2 text-sm">
              <p className="text-ink">
                {binding ? `${currentName} v${binding.version}` : 'No workflow'}{' '}
                <span className="text-ink-faint">→</span> {targetName} v{target?.version}
              </p>
              <p className="mt-1 break-all font-mono text-2xs text-ink-muted">
                fingerprint {preview.data?.fingerprint}
              </p>
            </div>

            <p className="text-xs leading-5 text-ink-muted">
              {preview.data?.views.removed.length
                ? `These tabs disappear: ${preview.data.views.removed.map(viewTitles).join(', ')}. Their data is not deleted.`
                : 'The current tabs stay available unless the target workflow does not declare them.'}{' '}
              Nothing is written until you confirm.
            </p>

            {preview.data ? (
              <details className="rounded-md border border-line px-3 py-2">
                <summary className="cursor-pointer text-xs font-medium text-ink">
                  Review the complete preview again
                </summary>
                <div className="mt-2 max-h-[40vh] overflow-y-auto">
                  <UpgradeDiff preview={preview.data} viewTitle={viewTitles} />
                </div>
              </details>
            ) : null}

            {mode === 'upgrade' && target ? (
              <Checkbox
                checked={confirmed}
                onCheckedChange={(value) => setConfirmed(value === true)}
                label={`I reviewed the preview for ${targetName} v${target.version} and want to upgrade this project.`}
              />
            ) : null}

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
              <Button
                size="md"
                onClick={() => void applyChange()}
                disabled={!canApply || (mode === 'upgrade' && !confirmed)}
                loading={apply.isPending}
              >
                {confirmLabel}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}
