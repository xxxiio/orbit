/**
 * Install or update a workflow the server ships with.
 *
 * A clean server has published no non-default workflow, so the only way a
 * person can choose Development (or any other bundled workflow) is to publish
 * it first. This control compares the **version** the store holds with the
 * version this binary ships: an id that exists at an older revision is offered
 * again as an update, because a published version is immutable and the bundle
 * may have gained fields or views. The install mutation invalidates the
 * available-workflow read, so the choice appears without a page reload.
 */
import { useMemo, useState } from 'react'
import { LoaderIcon } from '@/components/ui/icons'
import { Button } from '@/components/ui/button'
import { useToast } from '@/components/ui/toast'
import {
  useAvailableWorkflows,
  useBundledWorkflows,
  useInstallBundledWorkflow,
  userMessage,
  type AvailableWorkflow,
} from '@/hooks'
import type { BundledWorkflowDescriptor } from '@/api/contracts'

export interface InstallableBundledWorkflow {
  workflow: BundledWorkflowDescriptor
  /** The revision the store holds, or `null` when the id is not published. */
  storedVersion: number | null
}

/**
 * The bundled workflows this server still needs to publish. An id is only
 * hidden when the store already holds the same or a newer revision; an older
 * stored version is offered as an update because published revisions are
 * immutable and the bundle may have gained fields or views.
 */
export function installableBundledWorkflows(
  bundled: readonly BundledWorkflowDescriptor[] | undefined,
  available: readonly AvailableWorkflow[] | undefined,
): InstallableBundledWorkflow[] {
  const stored = new Map<string, number>()
  for (const workflow of available ?? []) stored.set(workflow.id, workflow.version)
  return (bundled ?? [])
    .map((workflow) => ({
      workflow,
      storedVersion: stored.get(workflow.id) ?? null,
    }))
    .filter(
      ({ workflow, storedVersion }) =>
        storedVersion === null || storedVersion < workflow.version,
    )
    .sort((left, right) => left.workflow.name.localeCompare(right.workflow.name))
}

export function BundledWorkflowInstall({ compact = false }: { compact?: boolean }) {
  const available = useAvailableWorkflows()
  const bundled = useBundledWorkflows()
  const install = useInstallBundledWorkflow()
  const { toast } = useToast()
  const [installing, setInstalling] = useState<string | null>(null)

  const candidates = useMemo(
    () => installableBundledWorkflows(bundled.data, available.data),
    [bundled.data, available.data],
  )

  const run = async (id: string) => {
    setInstalling(id)
    try {
      const result = await install.mutateAsync({ id })
      toast({
        tone: 'success',
        title: 'Workflow installed',
        description: `${result.name} v${result.version} is now available to choose.`,
      })
    } catch {
      // `install.error` is rendered below with the server's message.
    } finally {
      setInstalling(null)
    }
  }

  if (bundled.isPending) {
    return (
      <p className="flex items-center gap-1.5 text-xs text-ink-muted" aria-busy="true">
        <LoaderIcon size={13} className="animate-spin" /> Reading the server&apos;s bundled workflows…
      </p>
    )
  }

  if (bundled.error) {
    return (
      <p className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
        <span className="min-w-0 flex-1">
          The bundled workflows could not be listed. {userMessage(bundled.error)}
        </span>
        <Button variant="quiet" size="sm" onClick={() => void bundled.refetch()}>
          Retry
        </Button>
      </p>
    )
  }

  return (
    <div className="flex flex-col gap-2" data-bundled-workflows="">
      <p className="text-xs font-medium text-ink">Bundled workflows</p>
      {candidates.length === 0 ? (
        <p className="text-xs leading-5 text-ink-muted">
          Every workflow this server ships with is installed.
        </p>
      ) : (
        <>
          <p className="text-xs leading-5 text-ink-muted">
            These ship with the server but are not installed yet, or the server holds an older
            revision. Install one to make it available in this list; nothing is chosen until you
            select it.
          </p>
          <ul className="flex flex-col gap-1.5">
            {candidates.map(({ workflow, storedVersion }) => {
              const name = workflow.name
              const updating = storedVersion !== null
              return (
                <li
                  key={workflow.id}
                  className={
                    compact
                      ? 'flex items-center justify-between gap-2'
                      : 'flex items-center justify-between gap-2 rounded-md border border-line bg-surface px-3 py-2'
                  }
                >
                  <span className="min-w-0 truncate text-sm text-ink">
                    {name}
                    <span className="ml-1.5 font-mono text-2xs text-ink-muted">
                      v{workflow.version}
                    </span>
                    {updating ? (
                      <span className="ml-1.5 text-2xs text-ink-muted">
                        (installed v{storedVersion})
                      </span>
                    ) : null}
                  </span>
                  <Button
                    variant="quiet"
                    size="sm"
                    loading={installing === workflow.id}
                    disabled={install.isPending && installing !== workflow.id}
                    aria-label={`${updating ? 'Update' : 'Install'} ${name}`}
                    onClick={() => void run(workflow.id)}
                  >
                    {updating ? 'Update' : 'Install'}
                  </Button>
                </li>
              )
            })}
          </ul>
        </>
      )}
      {install.disposition === 'rejected' && install.error ? (
        <p role="alert" className="text-xs leading-5 text-danger-ink">
          Install failed. {userMessage(install.error)}
        </p>
      ) : null}
    </div>
  )
}
