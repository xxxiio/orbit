//! Explicit, previewable blueprint version upgrades.
//!
//! Publishing or installing a newer manifest never rebinds a project. Changing
//! a project's pinned version is a separate operation with three fences:
//!
//! 1. a **preview** computed from accepted state only, listing every schema
//!    change and every blocker that makes the target impossible;
//! 2. an **explicit confirmation** (`confirmed`), never implied by another
//!    command;
//! 3. the exact **fingerprint** of the preview the user approved, recomputed
//!    inside the same savepoint that writes the binding.
//!
//! The comparison is a pure function of two manifests plus the project's live
//! work items, so the same accepted state always yields the same report and
//! fingerprint. Nothing here opens a second database connection or writes work
//! items: an upgrade only re-pins the project's single existing binding.
use crate::AuthoritativeHost;
use orbit_application::RepositoryError;
use orbit_blueprint::presentation::{BlueprintView, ViewComponent, ViewLayoutKind};
use orbit_blueprint::{
    Blueprint, BlueprintBinding, BlueprintWorkItem, Field, FieldKind, WorkItemKind, WorkflowAction,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Identifies the canonical fingerprint payload. A future, incompatible
/// payload must use a new version so old fingerprints are never accepted.
pub const UPGRADE_PREVIEW_SCHEMA: &str = "orbit.blueprint-upgrade-preview.v1";

/// Stable warning and blocker codes. Codes are part of the CLI JSON contract
/// and are never derived from a message.
pub mod codes {
    /// Blockers: application is impossible until the reported subjects change.
    pub const KIND_REMOVED_WITH_LIVE_ITEMS: &str = "kind_removed_with_live_items";
    pub const STATE_REMOVED_WITH_LIVE_ITEMS: &str = "state_removed_with_live_items";
    pub const FIELD_REMOVED_WITH_VALUES: &str = "field_removed_with_values";
    pub const FIELD_TYPE_INCOMPATIBLE: &str = "field_type_incompatible";
    pub const FIELD_CHOICE_VALUE_UNSUPPORTED: &str = "field_choice_value_unsupported";
    pub const FIELD_REQUIRED_WITHOUT_DEFAULT: &str = "field_required_without_default";
    pub const FIELD_NOW_REQUIRED_MISSING_VALUE: &str = "field_now_required_missing_value";
    pub const PROPERTY_NOT_IN_TARGET_SCHEMA: &str = "property_not_in_target_schema";
    pub const REQUIRED_CAPABILITY_UNAVAILABLE: &str = "required_capability_unavailable";
    pub const TARGET_VALIDATION_FAILED: &str = "target_validation_failed";

    /// Warnings: informational; application is still possible.
    pub const KIND_ADDED: &str = "kind_added";
    pub const KIND_REMOVED: &str = "kind_removed";
    pub const STATE_ADDED: &str = "state_added";
    pub const STATE_REMOVED: &str = "state_removed";
    pub const INITIAL_STATE_CHANGED: &str = "initial_state_changed";
    pub const FIELD_ADDED: &str = "field_added";
    pub const FIELD_REMOVED: &str = "field_removed";
    pub const FIELD_TYPE_WIDENED: &str = "field_type_widened";
    pub const FIELD_CHOICE_OPTIONS_TRIMMED: &str = "field_choice_options_trimmed";
    pub const FIELD_TYPE_CHANGED_WITHOUT_VALUES: &str = "field_type_changed_without_values";
    pub const FIELD_DEFAULT_CHANGED: &str = "field_default_changed";
    pub const FIELD_NOW_REQUIRED: &str = "field_now_required";
    pub const TRANSITION_ADDED: &str = "transition_added";
    pub const TRANSITION_REMOVED: &str = "transition_removed";
    pub const TRANSITION_CAPABILITY_CHANGED: &str = "transition_capability_changed";
    pub const TRANSITION_GUARDS_CHANGED: &str = "transition_guards_changed";
    pub const ACTION_ADDED: &str = "action_added";
    pub const ACTION_REMOVED: &str = "action_removed";
    pub const ACTION_CHANGED: &str = "action_changed";
    pub const VIEW_ADDED: &str = "view_added";
    pub const VIEW_REMOVED: &str = "view_removed";
    pub const VIEW_CHANGED: &str = "view_changed";
    /// A target older than the current version. Explicit downgrades are
    /// allowed, but the report says so.
    pub const TARGET_VERSION_NOT_NEWER: &str = "target_version_not_newer";
}

/// One warning or blocker. `subjects` names the exact live ids (work items) or
/// identifiers (kinds, fields, capabilities) the finding was computed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpgradeNotice {
    pub code: String,
    pub message: String,
    pub subjects: Vec<String>,
}

/// A published blueprint identity. Never a floating "latest" reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlueprintVersionRef {
    pub blueprint_id: String,
    pub version: u32,
}

impl BlueprintVersionRef {
    fn new(blueprint_id: &str, version: u32) -> Self {
        Self {
            blueprint_id: blueprint_id.to_owned(),
            version,
        }
    }

    fn matches(&self, blueprint_id: &str, version: u32) -> bool {
        self.blueprint_id == blueprint_id && self.version == version
    }
}

/// A field type as it appears in the report. `options` is only populated for
/// choice fields so consumers never need to re-read the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldTypeView {
    pub kind: String,
    pub options: Vec<String>,
}

/// One declared field, in report form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldDefinition {
    pub id: String,
    pub field_type: FieldTypeView,
    pub required: bool,
    pub default: Option<Value>,
}

/// A retained field whose definition changed. `before`/`after` are complete so
/// a caller can render the delta without the manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    pub id: String,
    pub before: FieldDefinition,
    pub after: FieldDefinition,
}

/// A retained transition whose capability requirement or guards changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransitionChange {
    pub transition: String,
    pub required_capability_before: Option<String>,
    pub required_capability_after: Option<String>,
    pub guards_changed: bool,
}

/// One offered capability action, addressed by its transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionRef {
    pub transition: String,
    pub action: String,
    pub capability: String,
}

/// Per-kind schema delta. `added`/`removed` mark whole kinds; the list fields
/// are still populated so a single consumer handles every case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KindUpgradeDiff {
    pub kind: String,
    pub added: bool,
    pub removed: bool,
    pub initial_state_before: Option<String>,
    pub initial_state_after: Option<String>,
    pub states_added: Vec<String>,
    pub states_removed: Vec<String>,
    pub fields_added: Vec<FieldDefinition>,
    pub fields_removed: Vec<FieldDefinition>,
    pub fields_changed: Vec<FieldChange>,
    pub transitions_added: Vec<String>,
    pub transitions_removed: Vec<String>,
    pub transitions_changed: Vec<TransitionChange>,
    pub actions_added: Vec<ActionRef>,
    pub actions_removed: Vec<ActionRef>,
    pub required_capabilities_added: Vec<String>,
    pub required_capabilities_removed: Vec<String>,
}

impl KindUpgradeDiff {
    /// An unchanged kind is omitted from the report entirely.
    fn is_empty(&self) -> bool {
        !self.added
            && !self.removed
            && self.states_added.is_empty()
            && self.states_removed.is_empty()
            && self.fields_added.is_empty()
            && self.fields_removed.is_empty()
            && self.fields_changed.is_empty()
            && self.transitions_added.is_empty()
            && self.transitions_removed.is_empty()
            && self.transitions_changed.is_empty()
            && self.actions_added.is_empty()
            && self.actions_removed.is_empty()
            && self.required_capabilities_added.is_empty()
            && self.required_capabilities_removed.is_empty()
    }
}

/// A retained view whose layout or components changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ViewChange {
    pub view: String,
    pub layout_before: String,
    pub layout_after: String,
    pub components_added: Vec<String>,
    pub components_removed: Vec<String>,
    pub components_changed: Vec<String>,
}

/// Views are presentation-only: a view change never blocks a data upgrade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ViewUpgradeDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<ViewChange>,
}

/// Typed, serde-serializable upgrade preview. Pure read: computing it never
/// writes to the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlueprintUpgradePreview {
    pub schema: String,
    pub project_id: String,
    /// Accepted command-ledger revision the preview was computed at.
    pub project_revision: i64,
    pub current: Option<BlueprintVersionRef>,
    pub target: BlueprintVersionRef,
    pub kinds: Vec<KindUpgradeDiff>,
    pub views: ViewUpgradeDiff,
    pub warnings: Vec<UpgradeNotice>,
    pub blockers: Vec<UpgradeNotice>,
    /// Hex SHA-256 over the canonical preview payload. Changes whenever the
    /// binding, the manifests, the accepted revision or any live item changes.
    pub fingerprint: String,
}

/// Result of a successful (or already-satisfied) upgrade. The preview is
/// flattened so the CLI exposes both the report and the applied binding at the
/// top level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlueprintUpgradeApplied {
    #[serde(flatten)]
    pub preview: BlueprintUpgradePreview,
    pub binding: BlueprintBinding,
    pub previous: Option<BlueprintVersionRef>,
    /// True when the project was already pinned to the target version, so this
    /// replay wrote nothing.
    pub already_applied: bool,
}

/// Everything the comparison reads, captured once from accepted state.
struct PreviewInputs {
    project_revision: i64,
    binding: Option<BlueprintBinding>,
    current: Option<Blueprint>,
    target: Blueprint,
    items: Vec<BlueprintWorkItem>,
    installed: BTreeSet<String>,
}

impl AuthoritativeHost {
    /// Compare a project's pinned blueprint with a published target version
    /// without writing anything. An unpublished target is an error, not an
    /// empty preview: there is nothing to compare against.
    pub fn preview_blueprint_upgrade(
        &self,
        project_id: &str,
        target_id: &str,
        target_version: u32,
    ) -> Result<BlueprintUpgradePreview, RepositoryError> {
        let inputs = self.preview_inputs(project_id, target_id, target_version)?;
        build_preview(project_id, &inputs)
    }

    /// Explicitly re-pin a project. Refuses unless the caller confirmed, the
    /// recomputed preview has no blockers, and the fingerprint matches the
    /// preview the caller saw. The recomputation and the binding write share
    /// one savepoint, so a refusal leaves the binding untouched.
    pub fn apply_blueprint_upgrade(
        &mut self,
        project_id: &str,
        target_id: &str,
        target_version: u32,
        fingerprint: &str,
        confirmed: bool,
    ) -> Result<BlueprintUpgradeApplied, RepositoryError> {
        if !confirmed {
            return Err(RepositoryError::Other(
                "blueprint upgrade requires explicit confirmation (--confirm)".into(),
            ));
        }
        let fingerprint = fingerprint.trim();
        if fingerprint.is_empty() {
            return Err(RepositoryError::Other(
                "blueprint upgrade requires the preview fingerprint".into(),
            ));
        }
        let target = orbit_domain::ProjectId::new(project_id.to_owned())
            .map_err(|_| RepositoryError::Other("invalid project id".into()))?;
        self.authority
            .conn
            .execute_batch("SAVEPOINT orbit_blueprint_upgrade")
            .map_err(upgrade_error)?;
        let outcome = (|| -> Result<BlueprintUpgradeApplied, RepositoryError> {
            let inputs = self.preview_inputs(project_id, target_id, target_version)?;
            let preview = build_preview(project_id, &inputs)?;
            if !preview.blockers.is_empty() {
                return Err(blockers_error(&preview.blockers));
            }
            // Replaying an upgrade that is already in place is idempotent: the
            // report describes the approved change, the binding is unchanged.
            let already_applied = preview
                .current
                .as_ref()
                .is_some_and(|current| current.matches(target_id, target_version));
            if !already_applied && preview.fingerprint != fingerprint {
                return Err(RepositoryError::Other(format!(
                    "blueprint upgrade fingerprint mismatch: recompute the preview and retry (expected {})",
                    preview.fingerprint
                )));
            }
            if !already_applied {
                self.authority.conn.replace_project_blueprint_binding(
                    &target,
                    target_id,
                    target_version,
                )?;
            }
            let binding = self
                .authority
                .conn
                .project_blueprint_binding(project_id)?
                .ok_or(RepositoryError::Conflict)?;
            let previous = preview.current.clone();
            Ok(BlueprintUpgradeApplied {
                preview,
                binding,
                previous,
                already_applied,
            })
        })();
        match outcome {
            Ok(applied) => {
                self.authority
                    .conn
                    .execute_batch("RELEASE SAVEPOINT orbit_blueprint_upgrade")
                    .map_err(upgrade_error)?;
                Ok(applied)
            }
            Err(error) => {
                let _ = self.authority.conn.execute_batch(
                    "ROLLBACK TO SAVEPOINT orbit_blueprint_upgrade; \
                     RELEASE SAVEPOINT orbit_blueprint_upgrade",
                );
                Err(error)
            }
        }
    }

    fn preview_inputs(
        &self,
        project_id: &str,
        target_id: &str,
        target_version: u32,
    ) -> Result<PreviewInputs, RepositoryError> {
        let project_exists: bool = self
            .authority
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [project_id],
                |row| row.get(0),
            )
            .map_err(upgrade_error)?;
        if !project_exists {
            return Err(RepositoryError::Other(format!(
                "unknown project: {project_id}"
            )));
        }
        let target = self
            .authority
            .conn
            .load_blueprint(target_id, target_version)?
            .ok_or_else(|| {
                RepositoryError::Other(format!(
                    "unknown blueprint manifest: {target_id}@{target_version}"
                ))
            })?;
        let binding = self.authority.conn.project_blueprint_binding(project_id)?;
        let current = match &binding {
            Some(binding) => Some(
                self.authority
                    .conn
                    .load_blueprint(&binding.blueprint_id, binding.version)?
                    .ok_or_else(|| {
                        RepositoryError::Other(format!(
                            "missing bound blueprint manifest: {}@{}",
                            binding.blueprint_id, binding.version
                        ))
                    })?,
            ),
            None => None,
        };
        let mut items = self
            .authority
            .conn
            .blueprint_work_items_for_project(project_id)?;
        items.sort_by(|left, right| left.id.cmp(&right.id));
        // A capability counts as installed only through its durable
        // installation record (accepted state), never a caller-supplied list.
        let installed = self
            .authority
            .conn
            .capability_installations()?
            .into_iter()
            .map(|installation| installation.capability_id)
            .collect();
        let project_revision: i64 = self
            .authority
            .conn
            .query_row(
                "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
                [],
                |row| row.get(0),
            )
            .map_err(upgrade_error)?;
        Ok(PreviewInputs {
            project_revision,
            binding,
            current,
            target,
            items,
            installed,
        })
    }
}

fn upgrade_error(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Other(error.to_string())
}

fn blockers_error(blockers: &[UpgradeNotice]) -> RepositoryError {
    let summary = blockers
        .iter()
        .map(|blocker| format!("{}: {}", blocker.code, blocker.message))
        .collect::<Vec<_>>()
        .join("; ");
    RepositoryError::Other(format!("blueprint upgrade blocked: {summary}"))
}

fn notice(code: &str, message: impl Into<String>, subjects: Vec<String>) -> UpgradeNotice {
    UpgradeNotice {
        code: code.to_owned(),
        message: message.into(),
        subjects,
    }
}

fn field_type_view(kind: &FieldKind) -> FieldTypeView {
    match kind {
        FieldKind::Text => FieldTypeView {
            kind: "text".into(),
            options: Vec::new(),
        },
        FieldKind::Boolean => FieldTypeView {
            kind: "boolean".into(),
            options: Vec::new(),
        },
        FieldKind::Integer => FieldTypeView {
            kind: "integer".into(),
            options: Vec::new(),
        },
        FieldKind::Choice { options } => FieldTypeView {
            kind: "choice".into(),
            options: options.clone(),
        },
    }
}

fn field_definition(field: &Field) -> FieldDefinition {
    FieldDefinition {
        id: field.id.clone(),
        field_type: field_type_view(&field.kind),
        required: field.required,
        default: field.default.clone(),
    }
}

fn value_matches(kind: &FieldKind, value: &Value) -> bool {
    match kind {
        FieldKind::Text => value.is_string(),
        FieldKind::Boolean => value.is_boolean(),
        FieldKind::Integer => value.as_i64().is_some(),
        FieldKind::Choice { options } => value
            .as_str()
            .is_some_and(|text| options.iter().any(|option| option == text)),
    }
}

/// A type change is a widening only when every existing string value can still
/// be interpreted: text becomes a choice, or a choice keeps a subset of its
/// values. Everything else (choice -> text/boolean/integer, text -> boolean,
/// ...) needs a migration and is therefore a blocker.
fn is_compatible_widening(before: &FieldKind, after: &FieldKind) -> bool {
    matches!(
        (before, after),
        (FieldKind::Text, FieldKind::Choice { .. })
            | (FieldKind::Choice { .. }, FieldKind::Choice { .. })
    )
}

fn kind_capabilities(kind: &WorkItemKind) -> BTreeSet<String> {
    let mut capabilities = BTreeSet::new();
    for transition in &kind.transitions {
        if let Some(capability) = &transition.required_capability {
            capabilities.insert(capability.clone());
        }
        for action in &transition.actions {
            capabilities.insert(action.capability.clone());
        }
    }
    capabilities
}

fn transition_key(kind: &WorkItemKind, index: usize) -> String {
    let transition = &kind.transitions[index];
    format!("{}->{}", transition.from, transition.to)
}

fn action_key(transition: &str, action: &WorkflowAction) -> String {
    format!("{transition}#{}", action.id)
}

fn action_ref(transition: String, action: &WorkflowAction) -> ActionRef {
    ActionRef {
        transition,
        action: action.id.clone(),
        capability: action.capability.clone(),
    }
}

fn layout_name(kind: ViewLayoutKind) -> &'static str {
    match kind {
        ViewLayoutKind::Vertical => "vertical",
        ViewLayoutKind::Horizontal => "horizontal",
        ViewLayoutKind::Grid => "grid",
    }
}

fn view_components(view: &BlueprintView) -> BTreeMap<&str, &ViewComponent> {
    view.components
        .iter()
        .map(|component| (component.id.as_str(), component))
        .collect()
}

/// Pure comparison of two manifests plus the project's live items.
fn build_preview(
    project_id: &str,
    inputs: &PreviewInputs,
) -> Result<BlueprintUpgradePreview, RepositoryError> {
    let current = inputs.current.as_ref();
    let target = &inputs.target;
    let current_kinds: &[WorkItemKind] =
        current.map_or(&[], |blueprint| blueprint.work_items.as_slice());
    let mut warnings: Vec<UpgradeNotice> = Vec::new();
    let mut blockers: Vec<UpgradeNotice> = Vec::new();

    let mut live_items: BTreeMap<&str, Vec<&BlueprintWorkItem>> = BTreeMap::new();
    for item in &inputs.items {
        live_items.entry(item.kind.as_str()).or_default().push(item);
    }

    let latest_by_kind: BTreeMap<&str, &WorkItemKind> = current_kinds
        .iter()
        .map(|kind| (kind.id.as_str(), kind))
        .collect();
    let target_by_kind: BTreeMap<&str, &WorkItemKind> = target
        .work_items
        .iter()
        .map(|kind| (kind.id.as_str(), kind))
        .collect();
    let mut kind_ids: BTreeSet<&str> = latest_by_kind.keys().copied().collect();
    kind_ids.extend(target_by_kind.keys().copied());

    let mut kinds = Vec::new();
    for kind_id in kind_ids {
        let diff = kind_diff(
            kind_id,
            latest_by_kind.get(kind_id).copied(),
            target_by_kind.get(kind_id).copied(),
            live_items.get(kind_id).map_or(&[][..], Vec::as_slice),
            &mut warnings,
            &mut blockers,
        );
        if !diff.is_empty() {
            kinds.push(diff);
        }
    }

    let current_views: &[BlueprintView] =
        current.map_or(&[], |blueprint| blueprint.views.as_slice());
    let views = view_diff(current_views, &target.views, &mut warnings);

    // Capability gating happens before structural validation so the report
    // names the missing capability even though `validate` would also fail.
    let mut capability_blocked = false;
    for capability in &target.required_capabilities {
        if !inputs.installed.contains(capability) {
            capability_blocked = true;
            blockers.push(notice(
                codes::REQUIRED_CAPABILITY_UNAVAILABLE,
                format!(
                    "target requires capability {capability}, which is not installed in this workspace"
                ),
                vec![capability.clone()],
            ));
        }
    }
    if !capability_blocked && let Err(error) = target.validate(&inputs.installed) {
        blockers.push(notice(
            codes::TARGET_VALIDATION_FAILED,
            format!("target manifest is not valid in this workspace: {error}"),
            vec![format!("{}@{}", target.id, target.version)],
        ));
    }

    if let Some(binding) = &inputs.binding
        && binding.blueprint_id == target.id
        && target.version < binding.version
    {
        warnings.push(notice(
            codes::TARGET_VERSION_NOT_NEWER,
            format!(
                "target {}@{} is older than the pinned version {}",
                target.id, target.version, binding.version
            ),
            vec![format!("{}@{}", target.id, target.version)],
        ));
    }

    let mut preview = BlueprintUpgradePreview {
        schema: UPGRADE_PREVIEW_SCHEMA.to_owned(),
        project_id: project_id.to_owned(),
        project_revision: inputs.project_revision,
        current: inputs
            .binding
            .as_ref()
            .map(|binding| BlueprintVersionRef::new(&binding.blueprint_id, binding.version)),
        target: BlueprintVersionRef::new(&target.id, target.version),
        kinds,
        views,
        warnings,
        blockers,
        fingerprint: String::new(),
    };
    preview.fingerprint = fingerprint(&preview, current, target, &inputs.items)?;
    Ok(preview)
}

fn kind_diff(
    kind_id: &str,
    before: Option<&WorkItemKind>,
    after: Option<&WorkItemKind>,
    items: &[&BlueprintWorkItem],
    warnings: &mut Vec<UpgradeNotice>,
    blockers: &mut Vec<UpgradeNotice>,
) -> KindUpgradeDiff {
    let mut diff = KindUpgradeDiff {
        kind: kind_id.to_owned(),
        added: before.is_none() && after.is_some(),
        removed: before.is_some() && after.is_none(),
        initial_state_before: before.map(|kind| kind.initial_state.clone()),
        initial_state_after: after.map(|kind| kind.initial_state.clone()),
        states_added: Vec::new(),
        states_removed: Vec::new(),
        fields_added: Vec::new(),
        fields_removed: Vec::new(),
        fields_changed: Vec::new(),
        transitions_added: Vec::new(),
        transitions_removed: Vec::new(),
        transitions_changed: Vec::new(),
        actions_added: Vec::new(),
        actions_removed: Vec::new(),
        required_capabilities_added: Vec::new(),
        required_capabilities_removed: Vec::new(),
    };
    let item_ids = |items: &[&BlueprintWorkItem]| -> Vec<String> {
        items.iter().map(|item| item.id.clone()).collect()
    };
    let before_states: BTreeSet<&str> = before
        .map(|kind| kind.states.iter().map(String::as_str).collect())
        .unwrap_or_default();
    let after_states: BTreeSet<&str> = after
        .map(|kind| kind.states.iter().map(String::as_str).collect())
        .unwrap_or_default();
    diff.states_added = after_states
        .difference(&before_states)
        .map(|state| (*state).to_owned())
        .collect();
    diff.states_removed = before_states
        .difference(&after_states)
        .map(|state| (*state).to_owned())
        .collect();

    let before_fields: BTreeMap<&str, &Field> = before
        .map(|kind| {
            kind.fields
                .iter()
                .map(|field| (field.id.as_str(), field))
                .collect()
        })
        .unwrap_or_default();
    let after_fields: BTreeMap<&str, &Field> = after
        .map(|kind| {
            kind.fields
                .iter()
                .map(|field| (field.id.as_str(), field))
                .collect()
        })
        .unwrap_or_default();

    if diff.removed {
        // A kind that disappears cannot hold live items: an upgrade never
        // deletes or re-types a work item.
        if items.is_empty() {
            warnings.push(notice(
                codes::KIND_REMOVED,
                format!("work-item kind {kind_id} is removed from the target"),
                vec![kind_id.to_owned()],
            ));
        } else {
            blockers.push(notice(
                codes::KIND_REMOVED_WITH_LIVE_ITEMS,
                format!(
                    "work-item kind {kind_id} is removed from the target but {} live item(s) use it",
                    items.len()
                ),
                item_ids(items),
            ));
        }
    } else if diff.added {
        warnings.push(notice(
            codes::KIND_ADDED,
            format!("work-item kind {kind_id} is added by the target"),
            vec![kind_id.to_owned()],
        ));
    } else if before.map(|kind| &kind.initial_state) != after.map(|kind| &kind.initial_state) {
        warnings.push(notice(
            codes::INITIAL_STATE_CHANGED,
            format!(
                "initial state of {kind_id} changes from {} to {}; live items keep their state",
                diff.initial_state_before.as_deref().unwrap_or("-"),
                diff.initial_state_after.as_deref().unwrap_or("-")
            ),
            vec![kind_id.to_owned()],
        ));
    }

    if !diff.removed {
        // An added kind already reported as a whole; its states are part of it.
        if !diff.added {
            for state in &diff.states_added {
                warnings.push(notice(
                    codes::STATE_ADDED,
                    format!("state {state} is added to {kind_id}"),
                    vec![kind_id.to_owned(), state.clone()],
                ));
            }
        }
        // Fail closed: a live item may never be silently remapped to a
        // different state.
        for state in &diff.states_removed {
            let affected: Vec<&BlueprintWorkItem> = items
                .iter()
                .copied()
                .filter(|item| item.state == *state)
                .collect();
            if affected.is_empty() {
                warnings.push(notice(
                    codes::STATE_REMOVED,
                    format!("state {state} is removed from {kind_id}"),
                    vec![kind_id.to_owned(), state.clone()],
                ));
            } else {
                blockers.push(notice(
                    codes::STATE_REMOVED_WITH_LIVE_ITEMS,
                    format!(
                        "state {state} is removed from {kind_id} but {} live item(s) are in it",
                        affected.len()
                    ),
                    item_ids(&affected),
                ));
            }
        }
        // Defensive: a live item may already be in a state the target never
        // declared (legacy or imported data). That is the same blocker.
        if let Some(after) = after {
            let orphaned: Vec<&BlueprintWorkItem> = items
                .iter()
                .copied()
                .filter(|item| !after.states.iter().any(|state| state == &item.state))
                .collect();
            if !orphaned.is_empty() && diff.states_removed.is_empty() {
                blockers.push(notice(
                    codes::STATE_REMOVED_WITH_LIVE_ITEMS,
                    format!(
                        "{} live item(s) of {kind_id} are in states the target does not declare",
                        orphaned.len()
                    ),
                    item_ids(&orphaned),
                ));
            }
        }
    }

    let mut field_ids: BTreeSet<&str> = before_fields.keys().copied().collect();
    field_ids.extend(after_fields.keys().copied());
    for field_id in field_ids {
        let old = before_fields.get(field_id).copied();
        let new = after_fields.get(field_id).copied();
        let holders: Vec<&BlueprintWorkItem> = items
            .iter()
            .copied()
            .filter(|item| item.properties.contains_key(field_id))
            .collect();
        match (old, new) {
            (None, Some(field)) => {
                diff.fields_added.push(field_definition(field));
                if field.required && field.default.is_none() && !items.is_empty() {
                    blockers.push(notice(
                        codes::FIELD_REQUIRED_WITHOUT_DEFAULT,
                        format!(
                            "new required field {field_id} in {kind_id} has no default and {} live item(s) lack it",
                            items.len()
                        ),
                        item_ids(items),
                    ));
                } else if diff.added {
                    // Part of the whole-kind addition, not a separate finding.
                } else if !field.required {
                    warnings.push(notice(
                        codes::FIELD_ADDED,
                        format!("field {field_id} is added to {kind_id}"),
                        vec![kind_id.to_owned(), field_id.to_owned()],
                    ));
                } else {
                    warnings.push(notice(
                        codes::FIELD_ADDED,
                        format!(
                            "required field {field_id} is added to {kind_id}; its default is applied on the next item edit"
                        ),
                        vec![kind_id.to_owned(), field_id.to_owned()],
                    ));
                }
            }
            (Some(field), None) => {
                diff.fields_removed.push(field_definition(field));
                // A removed kind already has one whole-kind blocker; its field
                // removals are consequences, not separate findings.
                if diff.removed {
                    continue;
                }
                if holders.is_empty() {
                    warnings.push(notice(
                        codes::FIELD_REMOVED,
                        format!("field {field_id} is removed from {kind_id}"),
                        vec![kind_id.to_owned(), field_id.to_owned()],
                    ));
                } else {
                    // The value would become an undeclared property, so every
                    // later edit of the item would fail. Refuse instead.
                    blockers.push(notice(
                        codes::FIELD_REMOVED_WITH_VALUES,
                        format!(
                            "field {field_id} is removed from {kind_id} but {} live item(s) store a value for it",
                            holders.len()
                        ),
                        item_ids(&holders),
                    ));
                }
            }
            (Some(old), Some(new)) => {
                if old == new {
                    continue;
                }
                diff.fields_changed.push(FieldChange {
                    id: field_id.to_owned(),
                    before: field_definition(old),
                    after: field_definition(new),
                });
                if old.kind != new.kind {
                    if holders.is_empty() {
                        warnings.push(notice(
                            codes::FIELD_TYPE_CHANGED_WITHOUT_VALUES,
                            format!(
                                "type of {kind_id}.{field_id} changes from {} to {}; no live item stores a value",
                                field_type_view(&old.kind).kind,
                                field_type_view(&new.kind).kind
                            ),
                            vec![kind_id.to_owned(), field_id.to_owned()],
                        ));
                    } else if is_compatible_widening(&old.kind, &new.kind)
                        && holders.iter().all(|item| {
                            item.properties
                                .get(field_id)
                                .is_some_and(|value| value_matches(&new.kind, value))
                        })
                    {
                        let code = if matches!(new.kind, FieldKind::Choice { .. })
                            && matches!(old.kind, FieldKind::Choice { .. })
                        {
                            codes::FIELD_CHOICE_OPTIONS_TRIMMED
                        } else {
                            codes::FIELD_TYPE_WIDENED
                        };
                        warnings.push(notice(
                            code,
                            format!(
                                "type of {kind_id}.{field_id} changes from {} to {}; every live value stays valid",
                                field_type_view(&old.kind).kind,
                                field_type_view(&new.kind).kind
                            ),
                            vec![kind_id.to_owned(), field_id.to_owned()],
                        ));
                    } else {
                        let code = if matches!(old.kind, FieldKind::Choice { .. })
                            && matches!(new.kind, FieldKind::Choice { .. })
                        {
                            // Same type, but a live value lost its option.
                            codes::FIELD_CHOICE_VALUE_UNSUPPORTED
                        } else {
                            codes::FIELD_TYPE_INCOMPATIBLE
                        };
                        blockers.push(notice(
                            code,
                            format!(
                                "type of {kind_id}.{field_id} changes incompatibly from {} to {} but {} live item(s) store a value",
                                field_type_view(&old.kind).kind,
                                field_type_view(&new.kind).kind,
                                holders.len()
                            ),
                            item_ids(&holders),
                        ));
                    }
                }
                if !old.required && new.required {
                    let missing: Vec<&BlueprintWorkItem> = items
                        .iter()
                        .copied()
                        .filter(|item| !item.properties.contains_key(field_id))
                        .collect();
                    if !missing.is_empty() {
                        if new.default.is_some() {
                            warnings.push(notice(
                                codes::FIELD_NOW_REQUIRED,
                                format!(
                                    "field {kind_id}.{field_id} is now required; its default fills {} live item(s) on the next edit",
                                    missing.len()
                                ),
                                item_ids(&missing),
                            ));
                        } else {
                            blockers.push(notice(
                                codes::FIELD_NOW_REQUIRED_MISSING_VALUE,
                                format!(
                                    "field {kind_id}.{field_id} is now required but {} live item(s) have no value and the field has no default",
                                    missing.len()
                                ),
                                item_ids(&missing),
                            ));
                        }
                    }
                } else if old.required && !new.required {
                    warnings.push(notice(
                        codes::FIELD_NOW_REQUIRED,
                        format!("field {kind_id}.{field_id} is no longer required"),
                        vec![kind_id.to_owned(), field_id.to_owned()],
                    ));
                }
                if old.default != new.default {
                    warnings.push(notice(
                        codes::FIELD_DEFAULT_CHANGED,
                        format!(
                            "default of {kind_id}.{field_id} changes; live item values are preserved"
                        ),
                        vec![kind_id.to_owned(), field_id.to_owned()],
                    ));
                }
            }
            (None, None) => {}
        }
    }

    // Orphan properties that neither schema declares would also make the next
    // item edit fail; removed fields are already reported above.
    for item in items {
        let orphans: Vec<String> = item
            .properties
            .keys()
            .filter(|key| {
                !after_fields.contains_key(key.as_str())
                    && !before_fields.contains_key(key.as_str())
            })
            .cloned()
            .collect();
        if !orphans.is_empty() {
            blockers.push(notice(
                codes::PROPERTY_NOT_IN_TARGET_SCHEMA,
                format!(
                    "work item {} stores properties the target schema does not declare: {}",
                    item.id,
                    orphans.join(", ")
                ),
                vec![item.id.clone()],
            ));
        }
    }

    let before_transitions: BTreeMap<String, &orbit_blueprint::Transition> = before
        .map(|kind| {
            kind.transitions
                .iter()
                .enumerate()
                .map(|(index, transition)| (transition_key(kind, index), transition))
                .collect()
        })
        .unwrap_or_default();
    let after_transitions: BTreeMap<String, &orbit_blueprint::Transition> = after
        .map(|kind| {
            kind.transitions
                .iter()
                .enumerate()
                .map(|(index, transition)| (transition_key(kind, index), transition))
                .collect()
        })
        .unwrap_or_default();
    for key in after_transitions.keys() {
        if !before_transitions.contains_key(key) {
            diff.transitions_added.push(key.clone());
            if !diff.added {
                warnings.push(notice(
                    codes::TRANSITION_ADDED,
                    format!("transition {kind_id}:{key} is added"),
                    vec![kind_id.to_owned(), key.clone()],
                ));
            }
        }
    }
    for (key, old) in &before_transitions {
        match after_transitions.get(key) {
            None => {
                diff.transitions_removed.push(key.clone());
                if !diff.removed {
                    warnings.push(notice(
                        codes::TRANSITION_REMOVED,
                        format!(
                            "transition {kind_id}:{key} is removed; live items keep their current state"
                        ),
                        vec![kind_id.to_owned(), key.clone()],
                    ));
                }
            }
            Some(new) => {
                if old.required_capability != new.required_capability {
                    diff.transitions_changed.push(TransitionChange {
                        transition: key.clone(),
                        required_capability_before: old.required_capability.clone(),
                        required_capability_after: new.required_capability.clone(),
                        guards_changed: old.guards != new.guards,
                    });
                    warnings.push(notice(
                        codes::TRANSITION_CAPABILITY_CHANGED,
                        format!(
                            "transition {kind_id}:{key} now requires {}",
                            new.required_capability
                                .as_deref()
                                .unwrap_or("no capability")
                        ),
                        vec![kind_id.to_owned(), key.clone()],
                    ));
                } else if old.guards != new.guards {
                    diff.transitions_changed.push(TransitionChange {
                        transition: key.clone(),
                        required_capability_before: old.required_capability.clone(),
                        required_capability_after: new.required_capability.clone(),
                        guards_changed: true,
                    });
                    warnings.push(notice(
                        codes::TRANSITION_GUARDS_CHANGED,
                        format!("guards of transition {kind_id}:{key} change"),
                        vec![kind_id.to_owned(), key.clone()],
                    ));
                }
            }
        }
    }

    let before_actions: BTreeMap<String, (String, &WorkflowAction)> = before
        .map(|kind| {
            let mut actions = BTreeMap::new();
            for (index, transition) in kind.transitions.iter().enumerate() {
                let key = transition_key(kind, index);
                for action in &transition.actions {
                    actions.insert(action_key(&key, action), (key.clone(), action));
                }
            }
            actions
        })
        .unwrap_or_default();
    let after_actions: BTreeMap<String, (String, &WorkflowAction)> = after
        .map(|kind| {
            let mut actions = BTreeMap::new();
            for (index, transition) in kind.transitions.iter().enumerate() {
                let key = transition_key(kind, index);
                for action in &transition.actions {
                    actions.insert(action_key(&key, action), (key.clone(), action));
                }
            }
            actions
        })
        .unwrap_or_default();
    for (key, (transition, action)) in &after_actions {
        match before_actions.get(key) {
            None => {
                diff.actions_added
                    .push(action_ref(transition.clone(), action));
                if !diff.added {
                    warnings.push(notice(
                        codes::ACTION_ADDED,
                        format!("action {key} is added to {kind_id}"),
                        vec![kind_id.to_owned(), key.clone()],
                    ));
                }
            }
            Some((_, old)) if old != action => {
                warnings.push(notice(
                    codes::ACTION_CHANGED,
                    format!("action {key} in {kind_id} changes"),
                    vec![kind_id.to_owned(), key.clone()],
                ));
            }
            Some(_) => {}
        }
    }
    for (key, (transition, action)) in &before_actions {
        if !after_actions.contains_key(key) {
            diff.actions_removed
                .push(action_ref(transition.clone(), action));
            if !diff.removed {
                warnings.push(notice(
                    codes::ACTION_REMOVED,
                    format!(
                        "action {key} is removed; existing requests and receipts are unaffected"
                    ),
                    vec![kind_id.to_owned(), key.clone()],
                ));
            }
        }
    }

    let before_capabilities = before.map(kind_capabilities).unwrap_or_default();
    let after_capabilities = after.map(kind_capabilities).unwrap_or_default();
    diff.required_capabilities_added = after_capabilities
        .difference(&before_capabilities)
        .cloned()
        .collect();
    diff.required_capabilities_removed = before_capabilities
        .difference(&after_capabilities)
        .cloned()
        .collect();
    diff
}

fn view_diff(
    before: &[BlueprintView],
    after: &[BlueprintView],
    warnings: &mut Vec<UpgradeNotice>,
) -> ViewUpgradeDiff {
    let before_by_id: BTreeMap<&str, &BlueprintView> =
        before.iter().map(|view| (view.id.as_str(), view)).collect();
    let after_by_id: BTreeMap<&str, &BlueprintView> =
        after.iter().map(|view| (view.id.as_str(), view)).collect();
    let mut diff = ViewUpgradeDiff::default();
    for id in after_by_id.keys() {
        if !before_by_id.contains_key(id) {
            diff.added.push((*id).to_owned());
            warnings.push(notice(
                codes::VIEW_ADDED,
                format!("view {id} is added"),
                vec![(*id).to_owned()],
            ));
        }
    }
    for (id, old) in &before_by_id {
        match after_by_id.get(id) {
            None => {
                diff.removed.push((*id).to_owned());
                warnings.push(notice(
                    codes::VIEW_REMOVED,
                    format!("view {id} is removed"),
                    vec![(*id).to_owned()],
                ));
            }
            Some(new) if **old != **new => {
                let old_components = view_components(old);
                let new_components = view_components(new);
                let mut components_added = Vec::new();
                let mut components_removed = Vec::new();
                let mut components_changed = Vec::new();
                for component in new_components.keys() {
                    if !old_components.contains_key(component) {
                        components_added.push((*component).to_owned());
                    }
                }
                for (component, old_component) in &old_components {
                    match new_components.get(component) {
                        None => components_removed.push((*component).to_owned()),
                        Some(new_component) if *old_component != *new_component => {
                            components_changed.push((*component).to_owned());
                        }
                        Some(_) => {}
                    }
                }
                diff.changed.push(ViewChange {
                    view: (*id).to_owned(),
                    layout_before: layout_name(old.layout.kind).to_owned(),
                    layout_after: layout_name(new.layout.kind).to_owned(),
                    components_added,
                    components_removed,
                    components_changed,
                });
                warnings.push(notice(
                    codes::VIEW_CHANGED,
                    format!("view {id} changes"),
                    vec![(*id).to_owned()],
                ));
            }
            Some(_) => {}
        }
    }
    diff
}

/// Hex SHA-256 over the canonical preview payload.
///
/// The payload contains everything the report was computed from: the current
/// binding, the target identity, both manifest digests, the accepted revision,
/// the live item ids/states/revisions/properties, and the report itself. Any
/// relevant change therefore changes the fingerprint.
fn fingerprint(
    preview: &BlueprintUpgradePreview,
    current: Option<&Blueprint>,
    target: &Blueprint,
    items: &[BlueprintWorkItem],
) -> Result<String, RepositoryError> {
    #[derive(Serialize)]
    struct FingerprintItem<'a> {
        id: &'a str,
        kind: &'a str,
        state: &'a str,
        revision: u64,
        properties: &'a BTreeMap<String, Value>,
    }
    let items: Vec<FingerprintItem<'_>> = items
        .iter()
        .map(|item| FingerprintItem {
            id: &item.id,
            kind: &item.kind,
            state: &item.state,
            revision: item.revision,
            properties: &item.properties,
        })
        .collect();
    let payload = json!({
        "schema": preview.schema,
        "project_id": preview.project_id,
        "project_revision": preview.project_revision,
        "current": preview.current,
        "target": preview.target,
        "current_manifest_digest": manifest_digest(current)?,
        "target_manifest_digest": manifest_digest(Some(target))?,
        "kinds": preview.kinds,
        "views": preview.views,
        "warnings": preview.warnings,
        "blockers": preview.blockers,
        "items": items,
    });
    let canonical = serde_json::to_string(&payload)
        .map_err(|error| RepositoryError::Other(format!("preview fingerprint: {error}")))?;
    Ok(hex_digest(&canonical))
}

fn manifest_digest(blueprint: Option<&Blueprint>) -> Result<Option<String>, RepositoryError> {
    blueprint
        .map(|blueprint| {
            blueprint
                .to_json()
                .map(|json| hex_digest(&json))
                .map_err(|error| RepositoryError::Other(error.to_string()))
        })
        .transpose()
}

fn hex_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
