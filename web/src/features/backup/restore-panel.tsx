/**
 * Restore panel.
 *
 * Two modes, deliberately unequal:
 *
 * - **Isolated copy** (recommended): restores into a new folder on the server
 *   and verifies it there. The active workspace is never modified. Starting it
 *   requires the person to type the confirmation word.
 * - **Replace authority**: a maintenance operation. The browser can only ask an
 *   administrator-credentialed server route to write the offline plan and return
 *   the operator commands. The live database is never replaced from a web
 *   request.
 *
 * Verification is never presented as restoration, and a restore that has not
 * been confirmed is never started implicitly.
 */
import { useEffect, useState, type FormEvent } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  AlertCircleIcon,
  AlertTriangleIcon,
  CheckCircleIcon,
  LayersIcon,
  RefreshIcon,
} from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import type {
  BackupSummary,
  IsolatedRestoreResponse,
  ReplacePlanResponse,
  RestoreMode,
  RestorePreflightResponse,
} from '@/api/contracts'
import {
  backupErrorMessage,
  backupMoment,
  backupName,
  countsSummary,
  formatBytes,
  formatTimestamp,
  isRestoreConfirmationValid,
  isWorkspaceBusy,
  planFacts,
  preflightBlockedCopy,
  RESTORE_CONFIRMATION,
  RESTORE_MODE_COPY,
  verificationCopy,
} from './backup-model'
import { VerificationReportView } from './verification-report-view'
import { useIsolatedRestore, useReplacePlan, useRestorePreflight } from './use-backup'

function backupOptionLabel(backup: BackupSummary): string {
  const verification = verificationCopy(backup.verification)
  return `${backupName(backup)} · ${backupMoment(backup)} · ${verification.label}`
}

function ModeChoice({
  mode,
  selected,
  onSelect,
}: {
  mode: RestoreMode
  selected: RestoreMode
  onSelect: (mode: RestoreMode) => void
}) {
  const copy = RESTORE_MODE_COPY[mode]
  const active = selected === mode
  const isolated = mode === 'isolated'
  return (
    <label
      data-restore-mode={mode}
      className={`flex cursor-pointer flex-col gap-1.5 rounded-lg border p-3 transition-colors ${
        active
          ? isolated
            ? 'border-accent/50 bg-accent-soft/40'
            : 'border-danger/40 bg-danger-soft/30'
          : 'border-line bg-surface hover:border-line-strong'
      }`}
    >
      <span className="flex items-center gap-2">
        <input
          type="radio"
          name="restore-mode"
          value={mode}
          checked={active}
          onChange={() => onSelect(mode)}
          className="h-3.5 w-3.5 accent-[hsl(var(--accent))]"
        />
        <span className="text-sm font-medium text-ink">{copy.title}</span>
        <Badge tone={isolated ? 'success' : 'danger'}>{isolated ? 'Recommended' : 'Maintenance'}</Badge>
      </span>
      <span className="pl-5.5 text-xs leading-5 text-ink-muted">{copy.description}</span>
    </label>
  )
}

function ProblemList({
  title,
  problems,
  tone,
}: {
  title: string
  problems: { code: string; detail: string }[]
  tone: 'danger' | 'warning'
}) {
  if (problems.length === 0) return null
  return (
    <div>
      <p className={`text-xs font-semibold ${tone === 'danger' ? 'text-danger-ink' : 'text-warning-ink'}`}>
        {title}
      </p>
      <ul aria-label={title} className="mt-1 flex flex-col gap-1">
        {problems.map((problem, index) => (
          <li
            key={`${problem.code}-${index}`}
            className={`flex items-start gap-1.5 text-xs leading-5 ${
              tone === 'danger' ? 'text-danger-ink' : 'text-warning-ink'
            }`}
          >
            <AlertCircleIcon size={12} className="mt-0.5 shrink-0" />
            <span className="min-w-0 flex-1">{problem.detail}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

function PreflightPlan({ preflight }: { preflight: RestorePreflightResponse }) {
  const facts = planFacts(preflight)
  const blocked = preflightBlockedCopy(preflight)
  const plan = preflight.plan
  return (
    <div
      data-restore-preflight={plan.mode}
      className="flex flex-col gap-3 rounded-lg border border-line bg-surface-sunken/30 p-4"
    >
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={preflight.admissible ? 'success' : 'danger'} dot>
          {preflight.admissible ? 'Preflight allows this restore' : 'Preflight blocks this restore'}
        </Badge>
        <span className="text-xs text-ink-muted">
          {plan.mode === 'isolated' ? 'Isolated copy' : 'Replace authority'}
        </span>
      </div>
      {blocked ? (
        <p role="alert" className="rounded-md border border-danger/30 bg-danger-soft/40 px-3 py-2 text-xs leading-5 text-danger-ink">
          {blocked}
        </p>
      ) : null}
      <dl className="grid grid-cols-1 gap-x-4 gap-y-2 text-xs sm:grid-cols-2">
        {facts.map((fact) => (
          <div key={fact.label}>
            <dt className="text-ink-muted">{fact.label}</dt>
            <dd
              className={`mt-0.5 ${
                fact.tone === 'danger'
                  ? 'text-danger-ink'
                  : fact.tone === 'warning'
                    ? 'text-warning-ink'
                    : fact.tone === 'success'
                      ? 'text-success-ink'
                      : 'text-ink'
              }`}
            >
              {fact.value}
            </dd>
          </div>
        ))}
      </dl>
      <ProblemList title="Blockers" problems={preflight.blockers} tone="danger" />
      <ProblemList title="Warnings" problems={preflight.warnings} tone="warning" />
      {plan.connected_client_warnings.length > 0 ? (
        <div>
          <p className="text-xs font-semibold text-warning-ink">Connected clients</p>
          <ul aria-label="Connected client warnings" className="mt-1 flex flex-col gap-1">
            {plan.connected_client_warnings.map((warning, index) => (
              <li key={index} className="text-xs leading-5 text-warning-ink">
                {warning}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {plan.external_dependencies.length > 0 ? (
        <div>
          <p className="text-xs font-semibold text-ink-soft">Remains external after restore</p>
          <ul aria-label="External dependencies" className="mt-1 flex flex-col gap-1">
            {plan.external_dependencies.map((dependency, index) => (
              <li key={index} className="text-xs leading-5 text-ink-muted">
                {dependency}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      <div className="border-t border-line pt-3">
        <VerificationReportView report={preflight.verification} />
      </div>
    </div>
  )
}

function IsolatedSuccess({ result }: { result: IsolatedRestoreResponse }) {
  return (
    <div
      data-isolated-restore-result={result.target_name}
      className="flex flex-col gap-2 rounded-lg border border-success/30 bg-success-soft/30 p-4"
    >
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={result.verified ? 'success' : 'danger'} dot>
          {result.verified ? 'Isolated copy verified' : 'Isolated copy failed verification'}
        </Badge>
        <p className="min-w-0 flex-1 truncate text-sm font-medium text-ink">
          {result.target_name}
        </p>
      </div>
      <p className="text-xs leading-5 text-ink-soft">{result.message}</p>
      <p className="text-xs text-ink-muted">{countsSummary(result.counts)}</p>
      <p className="text-2xs leading-4 text-ink-faint">
        Verification covers the archive and the restored files. It is not proof that every external
        dependency (agent sessions, credentials, repositories) is available.
      </p>
    </div>
  )
}

function ReplacementPlanResult({
  plan,
  onReset,
}: {
  plan: ReplacePlanResponse
  onReset: () => void
}) {
  return (
    <div data-replace-plan="" className="flex flex-col gap-3 rounded-lg border border-danger/30 bg-danger-soft/20 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone="warning" dot>
          Maintenance required
        </Badge>
        <p className="min-w-0 flex-1 text-sm font-medium text-ink">Offline replacement plan</p>
      </div>
      <p role="note" className="rounded-md border border-warning/40 bg-warning-soft/40 px-3 py-2 text-xs leading-5 text-warning-ink">
        The live workspace was NOT replaced from the browser. Run these commands with the server
        stopped; the CLI preserves the previous database and vault and never deletes the preserved
        copy automatically.
      </p>
      <ol className="flex flex-col gap-3">
        {plan.steps.map((step, index) => (
          <li key={step.title} className="flex gap-2.5">
            <span className="mt-0.5 grid h-5 w-5 shrink-0 place-items-center rounded-full bg-surface-sunken text-2xs font-semibold text-ink-soft">
              {index + 1}
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-ink">{step.title}</p>
              <p className="mt-0.5 text-xs leading-5 text-ink-muted">{step.detail}</p>
              {step.command ? (
                <div className="mt-1.5">
                  <p className="text-2xs font-medium text-ink-faint">Operator command</p>
                  <code className="mt-0.5 block overflow-x-auto whitespace-pre rounded border border-line bg-canvas px-2 py-1.5 font-mono text-[0.6875rem] text-ink-soft">
                    {step.command}
                  </code>
                </div>
              ) : null}
            </div>
          </li>
        ))}
      </ol>
      <div className="flex flex-wrap items-center gap-2 border-t border-line pt-3">
        <p className="min-w-0 flex-1 text-2xs leading-4 text-ink-faint">
          Plan written to the server's pending-restore file · archive sha256{' '}
          <span className="font-mono">{plan.archive_sha256.slice(0, 12)}…</span>
        </p>
        <Button variant="quiet" size="sm" onClick={onReset}>
          Clear plan
        </Button>
      </div>
    </div>
  )
}

export function RestorePanel({ backups }: { backups: BackupSummary[] }) {
  const [selectedId, setSelectedId] = useState<string>(
    backups.find((backup) => backup.verification === 'verified')?.backup_id ??
      backups[0]?.backup_id ??
      '',
  )
  const [mode, setMode] = useState<RestoreMode>('isolated')
  const [confirmation, setConfirmation] = useState('')
  const [administratorKey, setAdministratorKey] = useState('')

  const preflight = useRestorePreflight()
  const isolated = useIsolatedRestore()
  const replacePlan = useReplacePlan()

  useEffect(() => {
    if (selectedId && backups.some((backup) => backup.backup_id === selectedId)) return
    setSelectedId(
      backups.find((backup) => backup.verification === 'verified')?.backup_id ??
        backups[0]?.backup_id ??
        '',
    )
  }, [backups, selectedId])

  const resetPlan = () => {
    preflight.reset()
    isolated.reset()
    replacePlan.reset()
    setConfirmation('')
  }

  const chooseBackup = (backupId: string) => {
    setSelectedId(backupId)
    resetPlan()
  }

  const chooseMode = (next: RestoreMode) => {
    setMode(next)
    resetPlan()
  }

  const activePreflight =
    preflight.data &&
    preflight.variables?.backupId === selectedId &&
    preflight.variables?.mode === mode
      ? preflight.data
      : null

  const checkPlan = (event: FormEvent) => {
    event.preventDefault()
    if (!selectedId) return
    isolated.reset()
    replacePlan.reset()
    setConfirmation('')
    preflight.mutate({ backupId: selectedId, mode })
  }

  const confirmationValid = isRestoreConfirmationValid(confirmation)
  const canRestore =
    Boolean(activePreflight) &&
    activePreflight!.admissible &&
    activePreflight!.verification.ok &&
    confirmationValid

  const startIsolatedRestore = () => {
    if (!activePreflight || !canRestore) return
    isolated.mutate({
      backup_id: selectedId,
      name: activePreflight.plan.target.target_name,
    })
  }

  const prepareReplacement = () => {
    if (administratorKey.trim().length === 0) return
    replacePlan.mutate({ backupId: selectedId, administratorKey: administratorKey.trim() })
  }

  return (
    <Panel as="section" data-restore-panel="">
      <PanelHeader
        eyebrow="Destructive action"
        title="Restore"
        description="Restoring is not the same as verifying. Choose an isolated copy for a safe drill, or prepare an offline maintenance plan to replace the authority."
        actions={<Badge tone="warning">Requires confirmation</Badge>}
      />
      <PanelBody className="flex flex-col gap-4">
        {backups.length === 0 ? (
          <EmptyState
            compact
            headingLevel="p"
            icon={<LayersIcon size={18} />}
            title="No backup to restore from"
            description="Create and verify a backup first. Restore never runs against an unknown file."
          />
        ) : (
          <form onSubmit={checkPlan} className="flex flex-col gap-4">
            <Field label="Backup">
              <Select value={selectedId} onValueChange={chooseBackup}>
                <SelectTrigger aria-label="Backup to restore">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {backups.map((backup) => (
                    <SelectItem key={backup.backup_id} value={backup.backup_id}>
                      {backupOptionLabel(backup)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>

            <fieldset className="flex flex-col gap-2">
              <legend className="mb-1 text-sm font-medium text-ink">Restore mode</legend>
              <div className="grid gap-2 sm:grid-cols-2">
                <ModeChoice mode="isolated" selected={mode} onSelect={chooseMode} />
                <ModeChoice mode="replace" selected={mode} onSelect={chooseMode} />
              </div>
            </fieldset>

            <div className="flex flex-wrap items-center gap-2">
              <Button
                type="submit"
                variant={mode === 'isolated' ? 'secondary' : 'danger'}
                leadingIcon={<RefreshIcon size={15} />}
                loading={preflight.isPending}
                disabled={preflight.isPending || !selectedId}
              >
                {RESTORE_MODE_COPY[mode].action}
              </Button>
              <span className="text-2xs text-ink-faint">
                Preflight reads the archive and the target; it changes nothing.
              </span>
            </div>
          </form>
        )}

        {preflight.error ? (
          <p role="alert" className="rounded-md border border-danger/30 bg-danger-soft/40 px-3 py-2 text-xs leading-5 text-danger-ink">
            {isWorkspaceBusy(preflight.error)
              ? 'The workspace is busy: a writer holds the database fence. Retry the preflight in a moment.'
              : backupErrorMessage(preflight.error)}
          </p>
        ) : null}

        {activePreflight ? <PreflightPlan preflight={activePreflight} /> : null}

        {activePreflight && mode === 'isolated' ? (
          <div className="flex flex-col gap-3 rounded-lg border border-line p-4">
            <p className="flex items-start gap-2 text-xs leading-5 text-ink-soft">
              <CheckCircleIcon size={14} className="mt-0.5 shrink-0 text-accent" />
              <span>
                This creates a new workspace copy in the server's restore folder and verifies it.
                The active workspace is not modified. The copy is not opened by the server.
              </span>
            </p>
            <Field
              label={`Type ${RESTORE_CONFIRMATION} to confirm`}
              hint="The button stays disabled until the word matches exactly."
            >
              <Input
                value={confirmation}
                onChange={(event) => setConfirmation(event.target.value)}
                placeholder={RESTORE_CONFIRMATION}
                autoComplete="off"
                spellCheck={false}
              />
            </Field>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                variant="primary"
                disabled={!canRestore || isolated.isPending}
                loading={isolated.isPending}
                onClick={startIsolatedRestore}
              >
                {isolated.isPending ? 'Creating isolated copy…' : 'Create isolated copy'}
              </Button>
              {!activePreflight.verification.ok ? (
                <span className="text-xs text-danger-ink">
                  Verification failed; this backup must not be restored.
                </span>
              ) : null}
            </div>
            {isolated.error ? (
              <p role="alert" className="text-xs leading-5 text-danger-ink">
                {backupErrorMessage(isolated.error)} The active workspace was not modified; any
                preserved copy stays on the server and is never deleted automatically.
              </p>
            ) : null}
            {isolated.data ? <IsolatedSuccess result={isolated.data} /> : null}
          </div>
        ) : null}

        {activePreflight && mode === 'replace' ? (
          <div className="flex flex-col gap-3 rounded-lg border border-danger/30 p-4">
            <p role="note" className="flex items-start gap-2 text-xs leading-5 text-danger-ink">
              <AlertTriangleIcon size={14} className="mt-0.5 shrink-0" />
              <span>
                Replacement requires administrator authorization and a server maintenance window.
                The browser never replaces the live database: it writes the offline plan and shows
                the operator commands. A failed replacement preserves the previous workspace; that
                copy is never deleted automatically.
              </span>
            </p>
            {replacePlan.data ? (
              <ReplacementPlanResult plan={replacePlan.data} onReset={resetPlan} />
            ) : (
              <>
                <Field
                  label="Administrator key"
                  hint="Sent once with this request; it is never stored by the page."
                >
                  <Input
                    type="password"
                    value={administratorKey}
                    onChange={(event) => setAdministratorKey(event.target.value)}
                    autoComplete="off"
                    spellCheck={false}
                  />
                </Field>
                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    variant="danger"
                    disabled={administratorKey.trim().length === 0 || replacePlan.isPending}
                    loading={replacePlan.isPending}
                    onClick={prepareReplacement}
                  >
                    {replacePlan.isPending ? 'Preparing plan…' : 'Prepare replacement plan'}
                  </Button>
                  <span className="text-2xs text-ink-faint">
                    Administrator credential required. This does not replace the live workspace.
                  </span>
                </div>
              </>
            )}
            {replacePlan.error ? (
              <p role="alert" className="text-xs leading-5 text-danger-ink">
                {backupErrorMessage(replacePlan.error)}
              </p>
            ) : null}
          </div>
        ) : null}

        {activePreflight ? null : (
          <p className="flex items-start gap-1.5 text-2xs leading-4 text-ink-faint">
            <CheckCircleIcon size={12} className="mt-0.5 shrink-0 text-success" />
            A restore only starts after a preflight you can read. Nothing is restored implicitly.
          </p>
        )}

        {activePreflight && activePreflight.plan.free_space_bytes !== null &&
        activePreflight.plan.free_space_bytes < activePreflight.plan.totals.content_bytes ? (
          <p className="flex items-start gap-1.5 text-xs leading-5 text-danger-ink">
            <AlertTriangleIcon size={13} className="mt-0.5 shrink-0" />
            Insufficient free space: the target has {formatBytes(activePreflight.plan.free_space_bytes)}{' '}
            free but the restore needs {formatBytes(activePreflight.plan.totals.content_bytes)}.
          </p>
        ) : null}

        {activePreflight && activePreflight.plan.created_at ? (
          <p className="text-2xs text-ink-faint">
            Selected recovery point created {formatTimestamp(activePreflight.plan.created_at)}.
          </p>
        ) : null}
      </PanelBody>
    </Panel>
  )
}
