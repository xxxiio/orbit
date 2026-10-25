/**
 * Declared workflows (wire name: blueprints), their views, kinds and the
 * upgrade preview.
 *
 * A view component keeps every declared field, including the ones this build
 * cannot render: the renderer decides what to show and what to refuse, so the
 * adapter must not drop a declaration on the way in.
 */
import type {
  BlueprintBindingRecord,
  BlueprintKindDefinition,
  BlueprintManifest,
  BlueprintUpgradeApplied,
  BlueprintUpgradePreview,
  BlueprintView,
  BlueprintWorkItemRecord,
  FieldKind,
  KindUpgradeDiff,
  Transition,
  TransitionGuard,
  UpgradeNotice,
  ViewComponent,
  ViewUpgradeDiff,
  WorkflowActionDefinition,
} from '../contracts'

export interface WorkItemField {
  id: string
  kind: FieldKind
  required: boolean
  default: unknown
}

export interface WorkflowAction {
  id: string
  capability: string
  description: string
  parameters: Record<string, unknown>
}

export interface WorkItemTransition {
  from: string
  to: string
  requiredCapability: string | null
  guards: TransitionGuard[]
  actions: WorkflowAction[]
}

export interface WorkItemKind {
  id: string
  name: string
  initialState: string
  states: string[]
  fields: WorkItemField[]
  transitions: WorkItemTransition[]
}

export interface WorkflowViewComponent {
  id: string
  type: string
  title: string | null
  source: string | null
  aggregate: string | null
  field: string | null
  groupBy: string | null
  filter: Record<string, string>
  sort: string | null
  descending: boolean
  limit: number | null
  columns: string[]
  text: string | null
  actions: string[]
  options: Record<string, unknown>
}

export interface WorkflowView {
  id: string
  title: string
  description: string
  layout: { type: 'vertical' | 'horizontal' | 'grid'; columns: number }
  components: WorkflowViewComponent[]
}

export interface Workflow {
  id: string
  version: number
  name: string
  description: string
  kinds: WorkItemKind[]
  requiredCapabilities: string[]
  templates: Array<{ id: string; name: string; items: Array<{ key: string; kind: string; properties: Record<string, unknown>; prerequisites: string[] }> }>
  views: WorkflowView[]
}

export interface WorkflowBinding {
  projectId: string
  workflowId: string
  version: number
}

export interface WorkItem {
  id: string
  projectId: string
  kind: string
  state: string
  properties: Record<string, unknown>
  revision: number
}

export interface WorkflowVersionRef {
  workflowId: string
  version: number
}

function toWorkItemField(field: { id: string; kind: FieldKind; required: boolean; default: unknown }): WorkItemField {
  return { id: field.id, kind: field.kind, required: field.required, default: field.default }
}

function toWorkflowAction(action: WorkflowActionDefinition): WorkflowAction {
  return {
    id: action.id,
    capability: action.capability,
    description: action.description,
    parameters: action.parameters,
  }
}

function toWorkItemTransition(transition: Transition): WorkItemTransition {
  return {
    from: transition.from,
    to: transition.to,
    requiredCapability: transition.required_capability,
    guards: transition.guards,
    actions: transition.actions.map(toWorkflowAction),
  }
}

export function toWorkItemKind(kind: BlueprintKindDefinition): WorkItemKind {
  return {
    id: kind.id,
    name: kind.name,
    initialState: kind.initial_state,
    states: kind.states,
    fields: kind.fields.map(toWorkItemField),
    transitions: kind.transitions.map(toWorkItemTransition),
  }
}

function toWorkflowViewComponent(component: ViewComponent): WorkflowViewComponent {
  return {
    id: component.id,
    type: component.type,
    title: component.title,
    source: component.source,
    aggregate: component.aggregate,
    field: component.field,
    groupBy: component.group_by,
    filter: component.filter,
    sort: component.sort,
    descending: component.descending,
    limit: component.limit,
    columns: component.columns,
    text: component.text,
    actions: component.actions,
    options: component.options,
  }
}

export function toWorkflowView(view: BlueprintView): WorkflowView {
  return {
    id: view.id,
    title: view.title,
    description: view.description,
    layout: view.layout,
    components: view.components.map(toWorkflowViewComponent),
  }
}

export function toWorkflow(manifest: BlueprintManifest): Workflow {
  return {
    id: manifest.id,
    version: manifest.version,
    name: manifest.name,
    description: manifest.description,
    kinds: manifest.work_items.map(toWorkItemKind),
    requiredCapabilities: manifest.required_capabilities,
    templates: manifest.templates.map((template) => ({
      id: template.id,
      name: template.name,
      items: template.items.map((item) => ({
        key: item.key,
        kind: item.kind,
        properties: item.properties,
        prerequisites: item.prerequisites,
      })),
    })),
    views: manifest.views.map(toWorkflowView),
  }
}

export function toWorkflowBinding(binding: BlueprintBindingRecord): WorkflowBinding {
  return {
    projectId: binding.project_id,
    workflowId: binding.blueprint_id,
    version: binding.version,
  }
}

export function toWorkItem(item: BlueprintWorkItemRecord): WorkItem {
  return {
    id: item.id,
    projectId: item.project_id,
    kind: item.kind,
    state: item.state,
    properties: item.properties,
    revision: item.revision,
  }
}

/* ---- Upgrade preview ---- */

export interface WorkflowUpgradeNotice {
  code: string
  message: string
  subjects: string[]
}

export interface WorkflowActionRef {
  transition: string
  action: string
  capability: string
}

export interface WorkflowKindUpgrade {
  kind: string
  added: boolean
  removed: boolean
  initialStateBefore: string | null
  initialStateAfter: string | null
  statesAdded: string[]
  statesRemoved: string[]
  fieldsAdded: WorkItemField[]
  fieldsRemoved: WorkItemField[]
  fieldsChanged: Array<{ id: string; before: WorkItemField; after: WorkItemField }>
  transitionsAdded: string[]
  transitionsRemoved: string[]
  transitionsChanged: Array<{
    transition: string
    requiredCapabilityBefore: string | null
    requiredCapabilityAfter: string | null
    guardsChanged: boolean
  }>
  actionsAdded: WorkflowActionRef[]
  actionsRemoved: WorkflowActionRef[]
  requiredCapabilitiesAdded: string[]
  requiredCapabilitiesRemoved: string[]
}

export interface WorkflowViewUpgrade {
  added: string[]
  removed: string[]
  changed: Array<{
    view: string
    layoutBefore: string
    layoutAfter: string
    componentsAdded: string[]
    componentsRemoved: string[]
    componentsChanged: string[]
  }>
}

export interface WorkflowUpgradePreview {
  schema: string
  projectId: string
  projectRevision: number
  current: WorkflowVersionRef | null
  target: WorkflowVersionRef
  kinds: WorkflowKindUpgrade[]
  views: WorkflowViewUpgrade
  warnings: WorkflowUpgradeNotice[]
  blockers: WorkflowUpgradeNotice[]
  fingerprint: string
}

export interface WorkflowUpgradeApplied extends WorkflowUpgradePreview {
  binding: WorkflowBinding
  previous: WorkflowVersionRef | null
  alreadyApplied: boolean
}

function toNotice(notice: UpgradeNotice): WorkflowUpgradeNotice {
  return { code: notice.code, message: notice.message, subjects: notice.subjects }
}

function toKindUpgrade(diff: KindUpgradeDiff): WorkflowKindUpgrade {
  return {
    kind: diff.kind,
    added: diff.added,
    removed: diff.removed,
    initialStateBefore: diff.initial_state_before,
    initialStateAfter: diff.initial_state_after,
    statesAdded: diff.states_added,
    statesRemoved: diff.states_removed,
    fieldsAdded: diff.fields_added.map((field) => ({
      id: field.id,
      kind: { type: field.field_type.kind as FieldKind['type'], options: field.field_type.options } as FieldKind,
      required: field.required,
      default: field.default,
    })),
    fieldsRemoved: diff.fields_removed.map((field) => ({
      id: field.id,
      kind: { type: field.field_type.kind as FieldKind['type'], options: field.field_type.options } as FieldKind,
      required: field.required,
      default: field.default,
    })),
    fieldsChanged: diff.fields_changed.map((change) => ({
      id: change.id,
      before: {
        id: change.before.id,
        kind: {
          type: change.before.field_type.kind as FieldKind['type'],
          options: change.before.field_type.options,
        } as FieldKind,
        required: change.before.required,
        default: change.before.default,
      },
      after: {
        id: change.after.id,
        kind: {
          type: change.after.field_type.kind as FieldKind['type'],
          options: change.after.field_type.options,
        } as FieldKind,
        required: change.after.required,
        default: change.after.default,
      },
    })),
    transitionsAdded: diff.transitions_added,
    transitionsRemoved: diff.transitions_removed,
    transitionsChanged: diff.transitions_changed.map((change) => ({
      transition: change.transition,
      requiredCapabilityBefore: change.required_capability_before,
      requiredCapabilityAfter: change.required_capability_after,
      guardsChanged: change.guards_changed,
    })),
    actionsAdded: diff.actions_added.map((action) => ({ ...action })),
    actionsRemoved: diff.actions_removed.map((action) => ({ ...action })),
    requiredCapabilitiesAdded: diff.required_capabilities_added,
    requiredCapabilitiesRemoved: diff.required_capabilities_removed,
  }
}

function toViewUpgrade(diff: ViewUpgradeDiff): WorkflowViewUpgrade {
  return {
    added: diff.added,
    removed: diff.removed,
    changed: diff.changed.map((change) => ({
      view: change.view,
      layoutBefore: change.layout_before,
      layoutAfter: change.layout_after,
      componentsAdded: change.components_added,
      componentsRemoved: change.components_removed,
      componentsChanged: change.components_changed,
    })),
  }
}

function toPreviewFields(preview: BlueprintUpgradePreview): WorkflowUpgradePreview {
  return {
    schema: preview.schema,
    projectId: preview.project_id,
    projectRevision: preview.project_revision,
    current: preview.current
      ? { workflowId: preview.current.blueprint_id, version: preview.current.version }
      : null,
    target: { workflowId: preview.target.blueprint_id, version: preview.target.version },
    kinds: preview.kinds.map(toKindUpgrade),
    views: toViewUpgrade(preview.views),
    warnings: preview.warnings.map(toNotice),
    blockers: preview.blockers.map(toNotice),
    fingerprint: preview.fingerprint,
  }
}

export function toWorkflowUpgradePreview(preview: BlueprintUpgradePreview): WorkflowUpgradePreview {
  return toPreviewFields(preview)
}

export function toWorkflowUpgradeApplied(applied: BlueprintUpgradeApplied): WorkflowUpgradeApplied {
  return {
    ...toPreviewFields(applied),
    binding: toWorkflowBinding(applied.binding),
    previous: applied.previous
      ? { workflowId: applied.previous.blueprint_id, version: applied.previous.version }
      : null,
    alreadyApplied: applied.already_applied,
  }
}
