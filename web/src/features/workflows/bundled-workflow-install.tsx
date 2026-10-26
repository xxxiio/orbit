/**
 * Install a workflow the server ships with.
 *
 * A clean server has published no non-default workflow, so the only way a
 * person can choose Development (or any other bundled workflow) is to publish
 * it first. This control lists the server's bundled ids that are not installed
 * yet and installs one on request. The install mutation invalidates the
 * available-workflow read, so the choice appears without a page reload.
 */
import { useMemo, useState } from 'react'
import { LoaderIcon } from '@/components/ui/icons'
import { Button } from '@/components/ui/button'
import { useToast } from '@/components/ui/toast'
import { humanizeIdentifier } from '@/lib/humanize'
import {
  useAvailableWorkflows,
  useBundledWorkflows,
  useInstallBundledWorkflow,
  userMessage,
} from '@/hooks'

export function BundledWorkflowInstall({ compact = false }: { compact?: boolean }) {
  const available = useAvailableWorkflows()
  const bundled = useBundledWorkflows()
  const install = useInstallBundledWorkflow()
  const { toast } = useToast()
  const [installing, setInstalling] = useState<string | null>(null)

  const installed = useMemo(
    () => new Set((available.data ?? []).map((workflow) => workflow.id)),
    [available.data],
  )
  const candidates = useMemo(
    () => (bundled.data ?? []).filter((id) => !installed.has(id)).sort((a, b) => a.localeCompare(b)),
    [bundled.data, installed],
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
            These ship with the server but are not installed yet. Install one to make it available
            in this list; nothing is chosen until you select it.
          </p>
          <ul className="flex flex-col gap-1.5">
            {candidates.map((id) => {
              const name = humanizeIdentifier(id)
              return (
                <li
                  key={id}
                  className={
                    compact
                      ? 'flex items-center justify-between gap-2'
                      : 'flex items-center justify-between gap-2 rounded-md border border-line bg-surface px-3 py-2'
                  }
                >
                  <span className="min-w-0 truncate text-sm text-ink">{name}</span>
                  <Button
                    variant="quiet"
                    size="sm"
                    loading={installing === id}
                    disabled={install.isPending && installing !== id}
                    aria-label={`Install ${name}`}
                    onClick={() => void run(id)}
                  >
                    Install
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
