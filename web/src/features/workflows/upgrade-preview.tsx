/**
 * The upgrade comparison, rendered from the server's report.
 *
 * Every line here comes from `BlueprintUpgradePreview`: this component does not
 * diff manifests itself and never decides that a change is safe. A blocker is
 * shown with its code as words and every subject id it was computed from, so a
 * disabled Apply is never unexplained.
 */
import type { ReactNode } from 'react'
import type {
  WorkItemField,
  WorkflowKindUpgrade,
  WorkflowUpgradeApplied,
  WorkflowUpgradeNotice,
  WorkflowUpgradePreview,
  WorkflowViewUpgrade,
} from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { AlertCircleIcon, AlertTriangleIcon, CheckCircleIcon } from '@/components/ui/icons'
import { cn } from '@/components/ui/utils'
import { humanizeIdentifier } from '@/lib/humanize'

const NOTICE_LABELS: Record<string, string> = {
  kind_removed_with_live_items: 'Kind removed while items still use it',
  state_removed_with_live_items: 'State removed while items still use it',
  field_removed_with_values: 'Field removed while items still store values',
  field_type_incompatible: 'Field type cannot hold the stored values',
  field_choice_value_unsupported: 'Stored choice is not in the new options',
  field_required_without_default: 'A new required field has no default',
  field_now_required_missing_value: 'A field became required but items are missing it',
  property_not_in_target_schema: 'An item stores a property the target does not declare',
  required_capability_unavailable: 'A required capability is not installed',
  target_validation_failed: 'The target manifest is not valid in this workspace',
  kind_added: 'Kind added',
  kind_removed: 'Kind removed',
  state_added: 'State added',
  state_removed: 'State removed',
  initial_state_changed: 'Initial state changed',
  field_added: 'Field added',
  field_removed: 'Field removed',
  field_type_widened: 'Field type widened',
  field_choice_options_trimmed: 'Choice options trimmed',
  field_type_changed_without_values: 'Field type changed (no stored values)',
  field_default_changed: 'Field default changed',
  field_now_required: 'Field became required',
  transition_added: 'Transition added',
  transition_removed: 'Transition removed',
  transition_capability_changed: 'Transition capability changed',
  transition_guards_changed: 'Transition guards changed',
  action_added: 'Action added',
  action_removed: 'Action removed',
  action_changed: 'Action changed',
  view_added: 'View added',
  view_removed: 'View removed',
  view_changed: 'View changed',
  target_version_not_newer: 'Target version is not newer',
}

export function noticeLabel(code: string): string {
  return NOTICE_LABELS[code] ?? humanizeIdentifier(code)
}

function fieldTypeText(field: WorkItemField): string {
  if (field.kind.type === 'choice') return `choice: ${field.kind.options.join(', ')}`
  return field.kind.type
}

function fieldSummary(field: WorkItemField): string {
  const parts = [fieldTypeText(field)]
  if (field.required) parts.push('required')
  if (field.default !== null && field.default !== undefined) {
    parts.push(`default ${JSON.stringify(field.default)}`)
  }
  return parts.join(' · ')
}

function transitionName(transition: string): string {
  const [from, to] = transition.split('->')
  return to ? `${humanizeIdentifier(from)} → ${humanizeIdentifier(to)}` : humanizeIdentifier(transition)
}

function ChipList({ items, empty }: { items: string[]; empty: string }) {
  if (items.length === 0) return <span className="text-2xs text-ink-faint">{empty}</span>
  return (
    <span className="flex flex-wrap gap-1">
      {items.map((item) => (
        <span
          key={item}
          className="rounded-sm border border-line bg-surface-sunken px-1.5 py-0.5 font-mono text-2xs text-ink-soft"
        >
          {item}
        </span>
      ))}
    </span>
  )
}

function DiffRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[7.5rem_minmax(0,1fr)] items-start gap-2 border-b border-line py-1.5 last:border-b-0">
      <span className="text-2xs font-semibold uppercase tracking-label text-ink-faint">{label}</span>
      <span className="min-w-0 text-xs leading-5 text-ink-soft">{children}</span>
    </div>
  )
}

function FieldLine({ field, tone }: { field: WorkItemField; tone: 'added' | 'removed' }) {
  return (
    <li className="flex flex-wrap items-baseline gap-1.5">
      <span className={cn('font-mono text-2xs', tone === 'added' ? 'text-success-ink' : 'text-warning-ink')}>
        {tone === 'added' ? '+' : '−'}
      </span>
      <span className="font-medium text-ink">{field.id}</span>
      <span className="text-2xs text-ink-muted">{fieldSummary(field)}</span>
    </li>
  )
}

function KindDiff({ diff, contentWarning = false }: { diff: WorkflowKindUpgrade; contentWarning?: boolean }) {
  const name = humanizeIdentifier(diff.kind)
  const changes =
    diff.statesAdded.length +
    diff.statesRemoved.length +
    diff.fieldsAdded.length +
    diff.fieldsRemoved.length +
    diff.fieldsChanged.length +
    diff.transitionsAdded.length +
    diff.transitionsRemoved.length +
    diff.transitionsChanged.length +
    diff.actionsAdded.length +
    diff.actionsRemoved.length +
    diff.requiredCapabilitiesAdded.length +
    diff.requiredCapabilitiesRemoved.length +
    (diff.initialStateBefore !== diff.initialStateAfter ? 1 : 0)

  return (
    <div
      data-upgrade-kind={diff.kind}
      className="rounded-md border border-line bg-surface px-3 py-2.5"
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <p className="text-sm font-medium text-ink">{name}</p>
        <span className="font-mono text-2xs text-ink-faint">{diff.kind}</span>
        {diff.added ? <Badge tone="success">Added</Badge> : null}
        {diff.removed ? <Badge tone="danger">Removed</Badge> : null}
        {!diff.added && !diff.removed ? <Badge tone="neutral">Changed</Badge> : null}
      </div>
      {changes === 0 ? (
        <p className="mt-1 text-xs text-ink-muted">
          {contentWarning
            ? 'The declaration shape of this kind is unchanged; a content change is listed in the warnings above.'
            : 'No declaration changes in this kind.'}
        </p>
      ) : (
        <div className="mt-1.5">
          {diff.initialStateBefore !== diff.initialStateAfter ? (
            <DiffRow label="Initial state">
              {humanizeIdentifier(diff.initialStateBefore ?? 'none')} →{' '}
              {humanizeIdentifier(diff.initialStateAfter ?? 'none')}
            </DiffRow>
          ) : null}
          {diff.statesAdded.length > 0 ? (
            <DiffRow label="States added">
              <ChipList items={diff.statesAdded.map(humanizeIdentifier)} empty="None" />
            </DiffRow>
          ) : null}
          {diff.statesRemoved.length > 0 ? (
            <DiffRow label="States removed">
              <ChipList items={diff.statesRemoved.map(humanizeIdentifier)} empty="None" />
            </DiffRow>
          ) : null}
          {diff.fieldsAdded.length > 0 || diff.fieldsRemoved.length > 0 || diff.fieldsChanged.length > 0 ? (
            <DiffRow label="Fields">
              <ul className="flex flex-col gap-0.5">
                {diff.fieldsAdded.map((field) => (
                  <FieldLine key={`add-${field.id}`} field={field} tone="added" />
                ))}
                {diff.fieldsRemoved.map((field) => (
                  <FieldLine key={`remove-${field.id}`} field={field} tone="removed" />
                ))}
                {diff.fieldsChanged.map((change) => (
                  <li key={`change-${change.id}`} className="flex flex-col gap-0.5">
                    <span className="font-medium text-ink">{change.id}</span>
                    <span className="text-2xs text-ink-muted">
                      {fieldSummary(change.before)} → {fieldSummary(change.after)}
                    </span>
                  </li>
                ))}
              </ul>
            </DiffRow>
          ) : null}
          {diff.transitionsAdded.length > 0 || diff.transitionsRemoved.length > 0 ? (
            <DiffRow label="Transitions">
              <ul className="flex flex-col gap-0.5">
                {diff.transitionsAdded.map((transition) => (
                  <li key={`ta-${transition}`} className="text-success-ink">
                    + {transitionName(transition)}
                  </li>
                ))}
                {diff.transitionsRemoved.map((transition) => (
                  <li key={`tr-${transition}`} className="text-warning-ink">
                    − {transitionName(transition)}
                  </li>
                ))}
              </ul>
            </DiffRow>
          ) : null}
          {diff.transitionsChanged.length > 0 ? (
            <DiffRow label="Transition rules">
              <ul className="flex flex-col gap-0.5">
                {diff.transitionsChanged.map((change) => (
                  <li key={`tc-${change.transition}`}>
                    <span className="font-medium text-ink">{transitionName(change.transition)}</span>
                    {change.requiredCapabilityBefore !== change.requiredCapabilityAfter ? (
                      <span className="block text-2xs text-ink-muted">
                        Capability: {change.requiredCapabilityBefore ?? 'none'} →{' '}
                        {change.requiredCapabilityAfter ?? 'none'}
                      </span>
                    ) : null}
                    {change.guardsChanged ? (
                      <span className="block text-2xs text-ink-muted">Guards changed.</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            </DiffRow>
          ) : null}
          {diff.actionsAdded.length > 0 || diff.actionsRemoved.length > 0 ? (
            <DiffRow label="Actions">
              <ul className="flex flex-col gap-0.5">
                {diff.actionsAdded.map((action) => (
                  <li key={`aa-${action.transition}-${action.action}`} className="text-success-ink">
                    + {transitionName(action.transition)} · {action.action}{' '}
                    <span className="font-mono text-2xs">{action.capability}</span>
                  </li>
                ))}
                {diff.actionsRemoved.map((action) => (
                  <li key={`ar-${action.transition}-${action.action}`} className="text-warning-ink">
                    − {transitionName(action.transition)} · {action.action}{' '}
                    <span className="font-mono text-2xs">{action.capability}</span>
                  </li>
                ))}
              </ul>
            </DiffRow>
          ) : null}
          {diff.requiredCapabilitiesAdded.length > 0 || diff.requiredCapabilitiesRemoved.length > 0 ? (
            <DiffRow label="Capabilities">
              <ChipList
                items={[
                  ...diff.requiredCapabilitiesAdded.map((capability) => `+ ${capability}`),
                  ...diff.requiredCapabilitiesRemoved.map((capability) => `− ${capability}`),
                ]}
                empty="None"
              />
            </DiffRow>
          ) : null}
        </div>
      )}
    </div>
  )
}

function ViewDiff({ diff, viewTitle }: { diff: WorkflowViewUpgrade; viewTitle: (id: string) => string }) {
  const hasChanges =
    diff.added.length + diff.removed.length + diff.changed.length > 0
  return (
    <div data-upgrade-views="" className="rounded-md border border-line bg-surface px-3 py-2.5">
      <p className="text-sm font-medium text-ink">Views</p>
      {!hasChanges ? (
        <p className="mt-1 text-xs text-ink-muted">No view changes.</p>
      ) : (
        <div className="mt-1.5">
          {diff.added.length > 0 ? (
            <DiffRow label="Added">
              <ChipList items={diff.added.map(viewTitle)} empty="None" />
            </DiffRow>
          ) : null}
          {diff.removed.length > 0 ? (
            <DiffRow label="Removed">
              <ChipList items={diff.removed.map(viewTitle)} empty="None" />
            </DiffRow>
          ) : null}
          {diff.changed.length > 0 ? (
            <DiffRow label="Changed">
              <ul className="flex flex-col gap-1">
                {diff.changed.map((change) => (
                  <li key={change.view}>
                    <span className="font-medium text-ink">{viewTitle(change.view)}</span>
                    <span className="block text-2xs text-ink-muted">
                      Layout {change.layoutBefore} → {change.layoutAfter}
                    </span>
                    {change.componentsAdded.length > 0 ? (
                      <span className="block text-2xs text-success-ink">
                        Panels added: {change.componentsAdded.join(', ')}
                      </span>
                    ) : null}
                    {change.componentsRemoved.length > 0 ? (
                      <span className="block text-2xs text-warning-ink">
                        Panels removed: {change.componentsRemoved.join(', ')}
                      </span>
                    ) : null}
                    {change.componentsChanged.length > 0 ? (
                      <span className="block text-2xs text-ink-muted">
                        Panels changed: {change.componentsChanged.join(', ')}
                      </span>
                    ) : null}
                  </li>
                ))}
              </ul>
            </DiffRow>
          ) : null}
        </div>
      )}
    </div>
  )
}

export function NoticeList({
  tone,
  notices,
  testId,
}: {
  tone: 'warning' | 'danger'
  notices: WorkflowUpgradeNotice[]
  testId?: string
}) {
  if (notices.length === 0) return null
  const Icon = tone === 'danger' ? AlertCircleIcon : AlertTriangleIcon
  return (
    <ul
      data-upgrade-notices={testId ?? tone}
      className={cn(
        'flex flex-col gap-1.5 rounded-md border px-3 py-2',
        tone === 'danger'
          ? 'border-danger/35 bg-danger-soft/60'
          : 'border-warning/35 bg-warning-soft/60',
      )}
    >
      {notices.map((notice) => (
        <li
          key={`${notice.code}-${notice.subjects.join(',')}`}
          data-upgrade-notice={notice.code}
          className={cn(
            'flex items-start gap-1.5 text-xs leading-5',
            tone === 'danger' ? 'text-danger-ink' : 'text-warning-ink',
          )}
        >
          <Icon size={13} className="mt-0.5 shrink-0" />
          <span className="min-w-0">
            <span className="font-semibold">{noticeLabel(notice.code)}.</span> {notice.message}
            {notice.subjects.length > 0 ? (
              <span className="mt-1 flex flex-wrap gap-1">
                {notice.subjects.map((subject) => (
                  <span
                    key={subject}
                    className="rounded-sm border border-current/25 px-1.5 py-0.5 font-mono text-2xs"
                  >
                    {subject}
                  </span>
                ))}
              </span>
            ) : null}
          </span>
        </li>
      ))}
    </ul>
  )
}

export function UpgradeDiff({
  preview,
  viewTitle,
}: {
  preview: WorkflowUpgradePreview
  viewTitle: (id: string) => string
}) {
  return (
    <div className="flex flex-col gap-2" data-upgrade-diff="">
      <NoticeList tone="danger" notices={preview.blockers} testId="blockers" />
      <NoticeList tone="warning" notices={preview.warnings} testId="warnings" />
      {preview.kinds.length === 0 ? (
        <p className="text-xs leading-5 text-ink-muted">
          This version changes no work-item kind.
        </p>
      ) : (
        preview.kinds.map((diff) => (
          <KindDiff
            key={diff.kind}
            diff={diff}
            contentWarning={preview.warnings.some((warning) =>
              warning.subjects.includes(diff.kind),
            )}
          />
        ))
      )}
      <ViewDiff diff={preview.views} viewTitle={viewTitle} />
    </div>
  )
}

/** The applied receipt. `already_applied` is repeated rather than hidden. */
export function UpgradeReceipt({
  applied,
  viewTitle,
}: {
  applied: WorkflowUpgradeApplied
  viewTitle: (id: string) => string
}) {
  const previous = applied.previous
  return (
    <div
      data-upgrade-receipt=""
      className="rounded-md border border-success/30 bg-success-soft/50 px-3 py-2.5"
    >
      <p className="flex items-center gap-1.5 text-sm font-medium text-success-ink">
        <CheckCircleIcon size={15} className="shrink-0" />
        {applied.alreadyApplied ? 'Workflow already pinned' : 'Workflow change applied'}
      </p>
      <p className="mt-1 text-xs leading-5 text-ink-soft">
        {previous
          ? `${humanizeIdentifier(previous.workflowId)} v${previous.version}`
          : 'No workflow'}{' '}
        → {humanizeIdentifier(applied.binding.workflowId)} v{applied.binding.version}
        {applied.alreadyApplied
          ? ' — the server reported that this version was already in place.'
          : ' — the project now follows the target workflow.'}
      </p>
      <p className="mt-1 break-all font-mono text-2xs text-ink-muted">
        fingerprint {applied.fingerprint}
      </p>
      <details className="mt-1.5">
        <summary className="cursor-pointer text-2xs text-ink-muted">
          What the applied preview contained
        </summary>
        <div className="mt-2">
          <UpgradeDiff preview={applied} viewTitle={viewTitle} />
        </div>
      </details>
    </div>
  )
}
