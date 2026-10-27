//! Contract-only capability data: what a capability *is*, with no way to run it.
//!
//! The trusted local composition ([`crate::capability_composition::TrustedComposition`])
//! owns providers, so it can only be built by the process that is allowed to
//! execute. A connected server needs the same contracts (descriptor, permission,
//! requirements, action ids, input schemas, external-effect flags) to plan,
//! validate and persist an intent, but it must never be able to load or call a
//! provider. This module is that subset: data plus deterministic validation,
//! with no registry, no executor and no `AuthoritativeHost` access.
//!
//! The constructors here reuse the exact same contract constructors the local
//! composition registers ([`crate::capability_composition::reference_capability`]
//! and [`crate::habit_capability::habit_recurrence_capability`]) rather than a
//! copy of their JSON, so a schema change cannot drift between the two
//! deployment modes.
use crate::capability_composition::{TrustedCapability, reference_capability};
use crate::habit_capability::habit_recurrence_capability;
use orbit_application::RepositoryError;
use orbit_blueprint::{
    Blueprint, BlueprintWorkItem, CapabilityCatalog, CapabilityDescriptor, CapabilityPermission,
};
use orbit_storage::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The provider identity recorded by the bundled contracts. Both deployment
/// modes must write the same installation row, so the value is stated once.
pub const BUILTIN_PROVIDER: &str = "orbit";

/// One action a capability advertises to offers, status and the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityActionSpec {
    pub action_id: String,
    pub description: String,
    /// JSON-schema subset: `{"type":"object","properties":{...},"required":[..]}`.
    pub input_schema: Value,
    pub external_effect: bool,
    /// Other capabilities this single action needs installed. Empty today; kept
    /// per action so a future action can carry a narrower requirement than its
    /// capability without changing the wire shape. Omitted from the durable
    /// contract JSON when empty so already-installed rows stay byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

impl CapabilityActionSpec {
    /// Presentation never decides authority, but the CLI still refuses an
    /// unconfirmed invocation of an effectful or explicitly destructive action.
    pub fn is_destructive(&self) -> bool {
        self.external_effect || self.input_schema.get("destructive") == Some(&Value::Bool(true))
    }
}

/// The one durable effect a capability's contract declares.
///
/// It is compile-time data, keyed by capability id: the payload an executor
/// reports back must be described by *this* schema, and no request field can
/// supply, rename or widen it. The schema is deliberately declarative rather
/// than free-form JSON Schema, because the authority only ever persists one
/// kind of row and must never be told a table, a path or a provider:
///
/// ```text
/// { "kind": "work_item_note",
///   "target": "item_id",
///   "required": ["note"],
///   "optional": ["tag"],
///   "max_note": 2000,
///   "max_tag": 64 }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEffectSpec {
    /// The single effect kind this capability declares. A reported effect must
    /// carry exactly this value.
    pub kind: String,
    /// `required` / `optional` key names, plus one `max_<key>` length bound per
    /// key that has one.
    pub schema: Value,
}

impl CapabilityEffectSpec {
    /// The key names listed under `required` or `optional`, in declaration
    /// order. Unknown or non-string entries are simply not declared.
    pub fn keys(&self, field: &str) -> Vec<String> {
        self.schema
            .get(field)
            .and_then(Value::as_array)
            .map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn required_keys(&self) -> Vec<String> {
        self.keys("required")
    }

    pub fn optional_keys(&self) -> Vec<String> {
        self.keys("optional")
    }

    /// The declared character bound for one key, if the schema states one.
    pub fn max_len(&self, key: &str) -> Option<u64> {
        self.schema
            .get(format!("max_{key}"))
            .and_then(Value::as_u64)
    }

    /// The payload key that names the effect's target, if the schema states one.
    pub fn target_key(&self) -> Option<&str> {
        self.schema.get("target").and_then(Value::as_str)
    }
}

/// The effect kind of the bundled reference capability.
pub const REFERENCE_EFFECT_KIND: &str = "work_item_note";

/// The declared effect of a capability id, if this build declares one.
///
/// The reference capability is the only bundled capability that changes the
/// authority, so it is the only one with an effect contract. Everything else
/// answers `None` and therefore may never report an effect at all.
pub fn declared_effect(capability_id: &str) -> Option<CapabilityEffectSpec> {
    if capability_id != crate::capability_composition::REFERENCE_CAPABILITY_ID {
        return None;
    }
    Some(CapabilityEffectSpec {
        kind: REFERENCE_EFFECT_KIND.to_owned(),
        schema: json!({
            "kind": REFERENCE_EFFECT_KIND,
            "target": "item_id",
            "required": ["note"],
            "optional": ["tag"],
            // Both bounds are the action schema's own constants, so the offer
            // and the effect contract cannot drift apart.
            "max_note": crate::capability_composition::REFERENCE_NOTE_MAX_CHARS,
            "max_tag": crate::capability_composition::REFERENCE_TAG_MAX_CHARS,
        }),
    })
}

/// A declared effect payload that passed its contract's schema. Only this type
/// reaches the storage writer, so an unchecked document can never be persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedCapabilityEffect {
    pub kind: String,
    pub item_id: String,
    pub note: String,
    pub tag: Option<String>,
}

/// The three fixed refusals for a reported effect. The transport maps each to
/// `400` with exactly this text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityEffectRefusal {
    /// The capability's contract declares no effect at all.
    NotDeclared,
    /// A malformed payload: wrong kind, wrong target, unknown key, wrong type,
    /// or an empty required string.
    Invalid,
    /// A string payload longer than its declared bound.
    TooLong,
}

impl CapabilityEffectRefusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::NotDeclared => "capability does not declare an effect",
            Self::Invalid => "invalid capability effect",
            Self::TooLong => "capability effect exceeds its declared length",
        }
    }
}

/// Validate a reported effect against the capability's compile-time declaration.
///
/// The order is part of the contract: an effect on a non-declaring capability is
/// refused as such before its payload is looked at, and a type error is never
/// reported as a length error.
pub fn validate_declared_effect(
    declared: Option<&CapabilityEffectSpec>,
    item_id: &str,
    payload: &Value,
) -> Result<ValidatedCapabilityEffect, CapabilityEffectRefusal> {
    let Some(declared) = declared else {
        return Err(CapabilityEffectRefusal::NotDeclared);
    };
    let object = payload
        .as_object()
        .ok_or(CapabilityEffectRefusal::Invalid)?;
    // The payload may never rename the effect.
    if object.get("kind").and_then(Value::as_str) != Some(declared.kind.as_str()) {
        return Err(CapabilityEffectRefusal::Invalid);
    }
    // The effect's target must be the request's own item.
    let target_key = declared.target_key().unwrap_or("item_id");
    if object.get(target_key).and_then(Value::as_str) != Some(item_id) {
        return Err(CapabilityEffectRefusal::Invalid);
    }
    let required = declared.required_keys();
    let optional = declared.optional_keys();
    // The authority can only persist one shape: a `note` body plus an optional
    // `tag`. A declaration this build cannot honour is refused here rather than
    // producing a row the storage writer would have to reject later.
    if !required.iter().any(|key| key == "note")
        || required.iter().any(|key| key != "note")
        || optional.iter().any(|key| key != "tag")
    {
        return Err(CapabilityEffectRefusal::Invalid);
    }
    for key in object.keys() {
        if key == "kind" || key == target_key || required.contains(key) || optional.contains(key) {
            continue;
        }
        return Err(CapabilityEffectRefusal::Invalid);
    }
    // Types before lengths: a number where a string belongs is invalid, not long.
    for key in &required {
        let Some(text) = object.get(key).and_then(Value::as_str) else {
            return Err(CapabilityEffectRefusal::Invalid);
        };
        if text.trim().is_empty() {
            return Err(CapabilityEffectRefusal::Invalid);
        }
    }
    for key in &optional {
        match object.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::String(_)) => {}
            Some(_) => return Err(CapabilityEffectRefusal::Invalid),
        }
    }
    for key in required.iter().chain(optional.iter()) {
        let Some(max) = declared.max_len(key) else {
            continue;
        };
        if object
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|text| text.chars().count() as u64 > max)
        {
            return Err(CapabilityEffectRefusal::TooLong);
        }
    }
    let note = object
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let tag = match object.get("tag") {
        None | Some(Value::Null) => None,
        Some(Value::String(tag)) if tag.trim().is_empty() => None,
        Some(Value::String(tag)) => Some(tag.clone()),
        // Already refused above; kept total rather than unreachable.
        Some(_) => return Err(CapabilityEffectRefusal::Invalid),
    };
    Ok(ValidatedCapabilityEffect {
        kind: declared.kind.clone(),
        item_id: item_id.to_owned(),
        note,
        tag,
    })
}

/// The durable contract of one capability: everything a transport may know
/// about it, and nothing that can execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityContract {
    pub capability_id: String,
    pub name: String,
    /// `read` | `mutate` | `external_effect`, the vocabulary the durable
    /// installation rows already constrain themselves to.
    pub permission: String,
    pub requires: Vec<String>,
    pub actions: Vec<CapabilityActionSpec>,
    /// The one durable effect this capability may report. `None` means the
    /// contract declares no effect, so a reported effect is refused outright.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<CapabilityEffectSpec>,
}

impl CapabilityContract {
    /// Build the contract view of a trusted composition member. Registration
    /// data is the only input; the executor stays with the caller.
    pub fn from_trusted(capability: &TrustedCapability) -> Self {
        Self {
            capability_id: capability.descriptor.id.clone(),
            // The composition does not carry a presentation name; the contract
            // catalog below supplies one for the bundled capabilities.
            name: capability.descriptor.id.clone(),
            permission: permission_label(capability.descriptor.permission).to_owned(),
            requires: capability.requires.iter().cloned().collect(),
            actions: capability.actions.clone(),
            // A capability's effect is declared next to its contract, by id.
            effect: declared_effect(&capability.descriptor.id),
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_owned();
        self
    }

    /// The descriptor used for blueprint planning and publication checks. The
    /// permission string is already constrained to the three known labels. A
    /// contract-only catalog names no provider code, so it reports the bundled
    /// provider identity the durable installation row also carries.
    pub fn descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: self.capability_id.clone(),
            provider: BUILTIN_PROVIDER.to_owned(),
            permission: permission_from_label(&self.permission),
            enabled: true,
        }
    }

    pub fn action(&self, action_id: &str) -> Option<&CapabilityActionSpec> {
        self.actions.iter().find(|spec| spec.action_id == action_id)
    }

    /// Whether this capability's contract makes an action effectful enough to
    /// require explicit confirmation. An unknown action is treated as effectful
    /// by the caller (fail closed), so this only answers for known actions.
    pub fn requires_confirmation(&self, action: &CapabilityActionSpec) -> bool {
        action.is_destructive() || self.permission == "external_effect"
    }

    /// The exact durable installation row this contract installs. Shared with
    /// the local composition so both deployment modes write identical rows.
    pub fn installation(
        &self,
        provider: &str,
        at_ms: i64,
    ) -> Result<orbit_storage::capability_installs::CapabilityInstallation, RepositoryError> {
        Ok(orbit_storage::capability_installs::CapabilityInstallation {
            capability_id: self.capability_id.clone(),
            provider: provider.to_owned(),
            permission: self.permission.clone(),
            actions_json: serde_json::to_string(&self.actions)
                .map_err(|error| RepositoryError::Other(error.to_string()))?,
            requires_json: serde_json::to_string(&self.requires)
                .map_err(|error| RepositoryError::Other(error.to_string()))?,
            installed_at_ms: at_ms,
        })
    }
}

pub fn permission_label(permission: CapabilityPermission) -> &'static str {
    match permission {
        CapabilityPermission::Read => "read",
        CapabilityPermission::Mutate => "mutate",
        CapabilityPermission::ExternalEffect => "external_effect",
    }
}

fn permission_from_label(label: &str) -> CapabilityPermission {
    match label {
        "read" => CapabilityPermission::Read,
        "external_effect" => CapabilityPermission::ExternalEffect,
        _ => CapabilityPermission::Mutate,
    }
}

/// A compile-time, contract-only catalog. It has no executor registry, so it
/// cannot run anything even by mistake.
#[derive(Debug, Clone, Default)]
pub struct ContractCatalog {
    contracts: BTreeMap<String, CapabilityContract>,
}

impl ContractCatalog {
    pub fn new(contracts: impl IntoIterator<Item = CapabilityContract>) -> Self {
        Self {
            contracts: contracts
                .into_iter()
                .map(|contract| (contract.capability_id.clone(), contract))
                .collect(),
        }
    }

    /// The capabilities this build ships a provider for, as data only.
    pub fn builtin() -> Self {
        Self::new(contracts())
    }

    pub fn contract(&self, capability_id: &str) -> Option<&CapabilityContract> {
        self.contracts.get(capability_id)
    }

    pub fn contracts(&self) -> impl Iterator<Item = &CapabilityContract> {
        self.contracts.values()
    }

    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty()
    }

    /// Descriptors for blueprint planning and publication. Only enabled
    /// contracts are available, exactly like the local composition's catalog.
    pub fn capability_catalog(&self) -> CapabilityCatalog {
        let mut catalog = CapabilityCatalog::default();
        for contract in self.contracts.values() {
            let _ = catalog.register(contract.descriptor());
        }
        catalog
    }
}

/// The catalog of this build. Calls the same contract constructors the local
/// composition registers and discards the executors, so the two modes can never
/// describe a capability differently.
pub fn contracts() -> Vec<CapabilityContract> {
    vec![
        CapabilityContract::from_trusted(&reference_capability()).with_name("Reference notes"),
        CapabilityContract::from_trusted(&habit_recurrence_capability())
            .with_name("Habit recurrence"),
    ]
}

/// Look one contract up without building an executor.
pub fn contract(capability_id: &str) -> Option<CapabilityContract> {
    ContractCatalog::builtin().contract(capability_id).cloned()
}

/// What a capability registry can answer about contracts and about whether some
/// executor can run one. The local composition answers "registered provider";
/// the connected server answers "active enrolled device announced it". Nothing
/// else differs between the two availability computations.
pub trait CapabilityRegistryView {
    fn capability_contract(&self, capability_id: &str) -> Option<CapabilityContract>;
    /// A registered contract may be disabled. Disabled is distinct from missing
    /// so availability can report `capability_disabled`, not "never heard of it".
    fn capability_enabled(&self, capability_id: &str) -> bool;
    fn executor_available(&self, capability_id: &str) -> bool;
    /// Enabled capability ids used to validate a blueprint's required set.
    fn enabled_capabilities(&self) -> BTreeSet<String>;
}

impl CapabilityRegistryView for crate::capability_composition::TrustedComposition {
    fn capability_contract(&self, capability_id: &str) -> Option<CapabilityContract> {
        self.capability(capability_id)
            .map(CapabilityContract::from_trusted)
    }

    fn capability_enabled(&self, capability_id: &str) -> bool {
        self.capability(capability_id)
            .is_some_and(|capability| capability.descriptor.enabled)
    }

    fn executor_available(&self, capability_id: &str) -> bool {
        self.has_executor(capability_id)
    }

    fn enabled_capabilities(&self) -> BTreeSet<String> {
        self.catalog().available()
    }
}

/// Which deployment is asking. It only changes the reason prose a client shows;
/// the availability *codes* are identical so the browser never parses prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityScope {
    /// Same-process trusted composition: an executor is registered here.
    Local,
    /// Connected server: an enrolled device announced the capability.
    Connected,
}

impl CapabilityScope {
    fn missing_reason(self, capability_id: &str) -> String {
        match self {
            Self::Local => {
                format!("capability {capability_id} is not registered in the trusted composition")
            }
            Self::Connected => {
                format!("capability {capability_id} is not in the server capability catalog")
            }
        }
    }

    fn executor_reason(self, capability_id: &str, action_id: &str) -> String {
        match self {
            Self::Local => format!(
                "trusted composition has no executable action {action_id} for {capability_id}"
            ),
            Self::Connected => {
                format!("no active enrolled executor announced {capability_id}")
            }
        }
    }
}

/// Why an offered action cannot be requested or executed right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CapabilityActionAvailability {
    Available,
    Unavailable { code: String, reason: String },
}

impl CapabilityActionAvailability {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Available => None,
            Self::Unavailable { code, .. } => Some(code),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferedCapabilityAction {
    pub action_id: String,
    pub capability: String,
    pub description: String,
    pub from: String,
    pub to: String,
    pub availability: CapabilityActionAvailability,
    pub input_schema: Value,
    pub declared_parameters: Value,
    pub external_effect: bool,
    pub requires: Vec<String>,
}

/// The durable facts one availability decision depends on. Every field comes
/// from storage or from the compile-time contract; a caller cannot supply any
/// of them.
pub struct AvailabilityInputs<'a> {
    pub scope: CapabilityScope,
    pub capability_id: &'a str,
    pub action_id: &'a str,
    pub contract: Option<&'a CapabilityContract>,
    pub enabled: bool,
    pub action: Option<&'a CapabilityActionSpec>,
    pub executor_available: bool,
    pub installed: &'a BTreeSet<String>,
    pub configured: &'a BTreeSet<String>,
    pub authorized: &'a BTreeSet<String>,
    pub blueprint_requires: &'a [String],
    pub project_id: &'a str,
}

/// The single availability order used by both deployment modes. The order is
/// part of the contract: a client that receives `capability_not_installed` must
/// never be told to authorize first.
pub fn action_availability(inputs: AvailabilityInputs<'_>) -> CapabilityActionAvailability {
    let AvailabilityInputs {
        scope,
        capability_id,
        action_id,
        contract,
        enabled,
        action,
        executor_available,
        installed,
        configured,
        authorized,
        blueprint_requires,
        project_id,
    } = inputs;
    let unavailable = |code: &str, reason: String| CapabilityActionAvailability::Unavailable {
        code: code.to_owned(),
        reason,
    };
    let Some(contract) = contract else {
        return unavailable("capability_missing", scope.missing_reason(capability_id));
    };
    if !enabled {
        return unavailable(
            "capability_disabled",
            format!("capability {capability_id} is disabled"),
        );
    }
    if !installed.contains(capability_id) {
        return unavailable(
            "capability_not_installed",
            format!("capability {capability_id} is not installed"),
        );
    }
    if action.is_none() || !executor_available {
        return unavailable(
            "executor_missing",
            scope.executor_reason(capability_id, action_id),
        );
    }
    // Both the contract's own requirements and the bound workflow's required
    // set are reported with the same code and prose the local path always used;
    // `capability_status` is the read that names the specific missing id.
    let contract_requirement_missing = contract
        .requires
        .iter()
        .chain(action.iter().flat_map(|spec| spec.requires.iter()))
        .any(|required| !installed.contains(required));
    let blueprint_requirement_missing = blueprint_requires
        .iter()
        .any(|required| !installed.contains(required) || !configured.contains(required));
    if contract_requirement_missing || blueprint_requirement_missing {
        return unavailable(
            "requirement_missing",
            format!("capability {capability_id} has unsatisfied requirements"),
        );
    }
    if !authorized.contains(capability_id) {
        return unavailable(
            "not_authorized",
            format!("capability {capability_id} is not authorized for project {project_id}"),
        );
    }
    CapabilityActionAvailability::Available
}

/// Every action offered by a transition whose `from` is the item's current
/// state, with availability computed from durable state and the registry only.
///
/// Ordering is preserved from the blueprint: offers are listed edge by edge, in
/// declaration order.
pub fn offers_for_item(
    conn: &SqliteStore,
    item: &BlueprintWorkItem,
    registry: &dyn CapabilityRegistryView,
    scope: CapabilityScope,
) -> Result<Vec<OfferedCapabilityAction>, RepositoryError> {
    let binding = conn
        .project_blueprint_binding(&item.project_id)?
        .ok_or(RepositoryError::Conflict)?;
    let blueprint: Blueprint = conn
        .load_blueprint(&binding.blueprint_id, binding.version)?
        .ok_or(RepositoryError::Conflict)?;
    let installed: BTreeSet<String> = conn
        .capability_installations()?
        .into_iter()
        .map(|installation| installation.capability_id)
        .collect();
    let authorized = conn.authorized_capabilities(&item.project_id)?;
    let configured = registry.enabled_capabilities();
    let kind = blueprint
        .item_kind(&item.kind)
        .map_err(|error| RepositoryError::Other(error.to_string()))?;
    let mut offers = Vec::new();
    for edge in kind
        .transitions
        .iter()
        .filter(|edge| edge.from == item.state)
    {
        for action in &edge.actions {
            let contract = registry.capability_contract(&action.capability);
            let spec = contract
                .as_ref()
                .and_then(|contract| contract.action(&action.id));
            let availability = action_availability(AvailabilityInputs {
                scope,
                capability_id: &action.capability,
                action_id: &action.id,
                contract: contract.as_ref(),
                enabled: registry.capability_enabled(&action.capability),
                action: spec,
                executor_available: registry.executor_available(&action.capability),
                installed: &installed,
                configured: &configured,
                authorized: &authorized,
                blueprint_requires: &blueprint.required_capabilities,
                project_id: &item.project_id,
            });
            offers.push(OfferedCapabilityAction {
                action_id: action.id.clone(),
                capability: action.capability.clone(),
                description: action.description.clone(),
                from: edge.from.clone(),
                to: edge.to.clone(),
                availability,
                input_schema: spec
                    .map(|spec| spec.input_schema.clone())
                    .unwrap_or_else(|| json!({})),
                declared_parameters: serde_json::to_value(&action.parameters)
                    .map_err(|error| RepositoryError::Other(error.to_string()))?,
                // Unknown contracts are treated as effectful so a caller cannot
                // skip confirmation by removing the contract.
                external_effect: spec.map(|spec| spec.external_effect).unwrap_or(true),
                requires: contract
                    .map(|contract| contract.requires.clone())
                    .unwrap_or_default(),
            });
        }
    }
    Ok(offers)
}

/// Merge the manifest-declared defaults with explicit caller inputs. Only the
/// persisted invocation is authoritative; the merge itself grants nothing.
pub fn merge_parameters(
    offer: &OfferedCapabilityAction,
    inputs: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut parameters: BTreeMap<String, Value> = offer
        .declared_parameters
        .as_object()
        .map(|declared| {
            declared
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (key, value) in inputs {
        parameters.insert(key.clone(), value.clone());
    }
    parameters
}

/// JSON-schema subset used by trusted capability actions. Only the declared
/// contract is enforced: unknown keys never become capability parameters.
pub fn validate_capability_inputs(
    schema: &Value,
    parameters: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let object = schema
        .as_object()
        .ok_or_else(|| "input schema is not an object".to_owned())?;
    let properties = object.get("properties").and_then(Value::as_object);
    if let Some(required) = object.get("required").and_then(Value::as_array) {
        for key in required {
            let key = key
                .as_str()
                .ok_or_else(|| "invalid required list".to_owned())?;
            if !parameters.contains_key(key) {
                return Err(format!("missing required parameter: {key}"));
            }
        }
    }
    for (key, value) in parameters {
        let Some(property) = properties.and_then(|properties| properties.get(key)) else {
            continue;
        };
        let Some(kind) = property.get("type").and_then(Value::as_str) else {
            continue;
        };
        let valid = match kind {
            "string" => value.as_str().is_some_and(|text| {
                let length = text.chars().count() as u64;
                property
                    .get("minLength")
                    .and_then(Value::as_u64)
                    .is_none_or(|min| length >= min)
                    && property
                        .get("maxLength")
                        .and_then(Value::as_u64)
                        .is_none_or(|max| length <= max)
            }),
            "integer" => value.as_i64().is_some_and(|number| {
                property
                    .get("minimum")
                    .and_then(Value::as_i64)
                    .is_none_or(|min| number >= min)
                    && property
                        .get("maximum")
                        .and_then(Value::as_i64)
                        .is_none_or(|max| number <= max)
            }),
            "boolean" => value.is_boolean(),
            _ => true,
        };
        if !valid {
            return Err(format!("invalid value for {key}"));
        }
    }
    Ok(())
}

/// Command ids are caller-supplied idempotency keys, not free text.
pub fn valid_command_id(command_id: &str) -> bool {
    !command_id.is_empty()
        && command_id.len() <= 128
        && command_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contracts_reuse_the_registered_constructors() {
        let contracts = contracts();
        assert_eq!(contracts.len(), 2, "{contracts:?}");
        let reference = contract(crate::capability_composition::REFERENCE_CAPABILITY_ID)
            .expect("reference contract");
        let registered = reference_capability();
        assert_eq!(reference.permission, "mutate");
        assert_eq!(
            reference.capability_id,
            crate::capability_composition::REFERENCE_CAPABILITY_ID
        );
        assert_eq!(reference.actions, registered.actions);
        assert_eq!(
            reference.requires,
            registered.requires.iter().cloned().collect::<Vec<_>>()
        );
        let habit = contract(crate::habit_capability::CAPABILITY_ID).expect("habit contract");
        let registered_habit = habit_recurrence_capability();
        assert_eq!(habit.permission, "read");
        assert_eq!(habit.actions, registered_habit.actions);
        assert_eq!(habit.name, "Habit recurrence");
        // Requesting a contract cannot require an executor: none is reachable
        // from this module at all, and an unknown id is simply absent.
        assert!(contract("orbit.not.a.capability").is_none());
    }

    #[test]
    fn action_requires_is_omitted_from_the_durable_contract_when_empty() {
        let spec = CapabilityActionSpec {
            action_id: "append_note".into(),
            description: "Append".into(),
            input_schema: json!({"type": "object"}),
            external_effect: false,
            requires: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&spec).expect("serialize"),
            r#"{"action_id":"append_note","description":"Append","input_schema":{"type":"object"},"external_effect":false}"#
        );
        // An installed row written before the field existed still decodes.
        let decoded: CapabilityActionSpec = serde_json::from_str(
            r#"{"action_id":"append_note","description":"Append","input_schema":{},"external_effect":false}"#,
        )
        .expect("legacy action spec");
        assert!(decoded.requires.is_empty());
    }

    #[test]
    fn availability_order_is_installed_before_executor_before_grant() {
        let contract = contract(crate::capability_composition::REFERENCE_CAPABILITY_ID)
            .expect("reference contract");
        let action = contract.action("append_note").expect("action");
        let installed = BTreeSet::new();
        let empty = BTreeSet::new();
        let availability = action_availability(AvailabilityInputs {
            scope: CapabilityScope::Connected,
            capability_id: &contract.capability_id,
            action_id: "append_note",
            contract: Some(&contract),
            enabled: true,
            action: Some(action),
            executor_available: false,
            installed: &installed,
            configured: &empty,
            authorized: &empty,
            blueprint_requires: &[],
            project_id: "project-1",
        });
        assert_eq!(
            availability.code(),
            Some("capability_not_installed"),
            "installation is checked before executor enrollment and grants"
        );

        let installed = BTreeSet::from([contract.capability_id.clone()]);
        let availability = action_availability(AvailabilityInputs {
            scope: CapabilityScope::Connected,
            capability_id: &contract.capability_id,
            action_id: "append_note",
            contract: Some(&contract),
            enabled: true,
            action: Some(action),
            executor_available: false,
            installed: &installed,
            configured: &empty,
            authorized: &empty,
            blueprint_requires: &[],
            project_id: "project-1",
        });
        assert_eq!(availability.code(), Some("executor_missing"));
        match availability {
            CapabilityActionAvailability::Unavailable { reason, .. } => assert_eq!(
                reason,
                "no active enrolled executor announced orbit.reference.work-item-note"
            ),
            CapabilityActionAvailability::Available => panic!("expected unavailable"),
        }
    }

    #[test]
    fn local_scope_keeps_the_existing_reason_prose() {
        assert_eq!(
            CapabilityScope::Local.missing_reason("orbit.reference.work-item-note"),
            "capability orbit.reference.work-item-note is not registered in the trusted composition"
        );
        assert_eq!(
            CapabilityScope::Local.executor_reason("orbit.reference.work-item-note", "append_note"),
            "trusted composition has no executable action append_note for orbit.reference.work-item-note"
        );
    }

    #[test]
    fn input_validation_is_the_declared_contract_only() {
        let schema = json!({
            "type": "object",
            "properties": {
                "note": {"type": "string", "minLength": 1, "maxLength": 4},
                "count": {"type": "integer", "minimum": 1},
            },
            "required": ["note"]
        });
        let mut parameters = BTreeMap::new();
        assert!(validate_capability_inputs(&schema, &parameters).is_err());
        parameters.insert("note".into(), json!("ok"));
        assert!(validate_capability_inputs(&schema, &parameters).is_ok());
        parameters.insert("note".into(), json!(""));
        assert_eq!(
            validate_capability_inputs(&schema, &parameters),
            Err("invalid value for note".to_owned())
        );
        parameters.insert("note".into(), json!("toolong"));
        assert!(validate_capability_inputs(&schema, &parameters).is_err());
        parameters.insert("note".into(), json!("ok"));
        parameters.insert("count".into(), json!(0));
        assert!(validate_capability_inputs(&schema, &parameters).is_err());
        // Undeclared keys are not schema-checked here; the request path rejects
        // them before this point.
        parameters.insert("shell".into(), json!("rm -rf /"));
        assert!(validate_capability_inputs(&schema, &parameters).is_err());
        parameters.remove("count");
        assert!(validate_capability_inputs(&schema, &parameters).is_ok());
    }

    #[test]
    fn confirmation_is_required_for_effectful_or_destructive_actions() {
        let mut contract = contract(crate::capability_composition::REFERENCE_CAPABILITY_ID)
            .expect("reference contract");
        // The bundled reference action is a local write with no external effect.
        assert!(!contract.requires_confirmation(&contract.actions[0].clone()));

        let effectful = CapabilityActionSpec {
            external_effect: true,
            ..contract.actions[0].clone()
        };
        assert!(contract.requires_confirmation(&effectful));
        // A `destructive` schema marker also needs explicit consent.
        let destructive = CapabilityActionSpec {
            external_effect: false,
            input_schema: json!({"type": "object", "destructive": true}),
            ..contract.actions[0].clone()
        };
        assert!(contract.requires_confirmation(&destructive));
        // An external-effect capability is destructive even when the action is not.
        contract.permission = "external_effect".to_owned();
        assert!(contract.requires_confirmation(&contract.actions[0].clone()));
    }

    #[test]
    fn only_the_reference_capability_declares_an_effect() {
        let reference = contract(crate::capability_composition::REFERENCE_CAPABILITY_ID)
            .expect("reference contract");
        let effect = reference.effect.as_ref().expect("declared effect");
        assert_eq!(effect.kind, REFERENCE_EFFECT_KIND);
        assert_eq!(effect.kind, "work_item_note");
        assert_eq!(effect.required_keys(), vec!["note".to_owned()]);
        assert_eq!(effect.optional_keys(), vec!["tag".to_owned()]);
        assert_eq!(effect.max_len("note"), Some(2_000));
        assert_eq!(effect.max_len("tag"), Some(64));
        assert_eq!(effect.target_key(), Some("item_id"));
        // The declaration travels with the contract, from the id alone.
        assert_eq!(
            reference.effect,
            declared_effect(crate::capability_composition::REFERENCE_CAPABILITY_ID)
        );
        // A capability with no effect keeps `None`, and its durable contract
        // JSON omits the field entirely so old host contracts stay byte-equal.
        let habit = contract(crate::habit_capability::CAPABILITY_ID).expect("habit contract");
        assert!(habit.effect.is_none());
        assert!(
            !serde_json::to_string(&habit)
                .expect("serialize")
                .contains("\"effect\"")
        );
    }

    #[test]
    fn the_reference_action_bounds_match_the_declared_effect() {
        let reference = contract(crate::capability_composition::REFERENCE_CAPABILITY_ID)
            .expect("reference contract");
        let effect = reference.effect.as_ref().expect("declared effect");
        let action = reference.action("append_note").expect("action");
        let properties = action.input_schema["properties"]
            .as_object()
            .expect("input schema properties");

        // Every bounded effect key must be bounded in the offer by exactly the
        // same number. An input the offer leaves unbounded is one the authority
        // refuses only after the executor has already run it, which strands the
        // request as `claimed` with no receipt.
        for key in effect
            .required_keys()
            .into_iter()
            .chain(effect.optional_keys())
        {
            let Some(declared) = effect.max_len(&key) else {
                continue;
            };
            let offered = properties
                .get(&key)
                .and_then(|property| property.get("maxLength"))
                .and_then(Value::as_u64);
            assert_eq!(
                offered,
                Some(declared),
                "the offer must bound `{key}` exactly as the effect declaration does"
            );
        }

        // The tag bound is visible, not merely implied: a 65-character tag cannot
        // be requested at all, so it can never reach the executor.
        assert_eq!(
            properties.get("tag").and_then(|tag| tag.get("maxLength")),
            Some(&json!(64))
        );
        let mut parameters = BTreeMap::new();
        parameters.insert("note".into(), json!("hello"));
        parameters.insert("tag".into(), json!("t".repeat(65)));
        assert_eq!(
            validate_capability_inputs(&action.input_schema, &parameters),
            Err("invalid value for tag".to_owned())
        );
        parameters.insert("tag".into(), json!("t".repeat(64)));
        assert!(
            validate_capability_inputs(&action.input_schema, &parameters).is_ok(),
            "the declared bound is inclusive"
        );
    }

    #[test]
    fn a_reported_effect_is_validated_against_the_declared_schema_only() {
        let declared = declared_effect(crate::capability_composition::REFERENCE_CAPABILITY_ID);
        let declared = declared.as_ref();
        // The happy shape, with and without the optional tag.
        assert_eq!(
            validate_declared_effect(
                declared,
                "item-1",
                &json!({"kind": "work_item_note", "item_id": "item-1", "note": "hello", "tag": "demo"}),
            ),
            Ok(ValidatedCapabilityEffect {
                kind: "work_item_note".to_owned(),
                item_id: "item-1".to_owned(),
                note: "hello".to_owned(),
                tag: Some("demo".to_owned()),
            })
        );
        assert_eq!(
            validate_declared_effect(
                declared,
                "item-1",
                &json!({"kind": "work_item_note", "item_id": "item-1", "note": "hello"}),
            ),
            Ok(ValidatedCapabilityEffect {
                kind: "work_item_note".to_owned(),
                item_id: "item-1".to_owned(),
                note: "hello".to_owned(),
                tag: None,
            })
        );

        let refusal =
            |payload: Value| validate_declared_effect(declared, "item-1", &payload).unwrap_err();
        // No declaration at all: refused before the payload is even read.
        assert_eq!(
            validate_declared_effect(None, "item-1", &json!({"note": "hello"})),
            Err(CapabilityEffectRefusal::NotDeclared)
        );
        // A payload may not rename the effect or retarget it.
        assert_eq!(
            refusal(json!({"kind": "file_write", "item_id": "item-1", "note": "x"})),
            CapabilityEffectRefusal::Invalid
        );
        assert_eq!(
            refusal(json!({"kind": "work_item_note", "item_id": "item-2", "note": "x"})),
            CapabilityEffectRefusal::Invalid
        );
        assert_eq!(
            refusal(json!({"kind": "work_item_note", "note": "x"})),
            CapabilityEffectRefusal::Invalid
        );
        // Missing required, wrong type, an invented key, and a flat non-object.
        assert_eq!(
            refusal(json!({"kind": "work_item_note", "item_id": "item-1"})),
            CapabilityEffectRefusal::Invalid
        );
        assert_eq!(
            refusal(json!({"kind": "work_item_note", "item_id": "item-1", "note": 7})),
            CapabilityEffectRefusal::Invalid
        );
        assert_eq!(
            refusal(json!({"kind": "work_item_note", "item_id": "item-1", "note": "x", "tag": 7})),
            CapabilityEffectRefusal::Invalid
        );
        assert_eq!(
            refusal(
                json!({"kind": "work_item_note", "item_id": "item-1", "note": "x", "path": "/etc/passwd"})
            ),
            CapabilityEffectRefusal::Invalid
        );
        assert_eq!(refusal(json!("note")), CapabilityEffectRefusal::Invalid);
        // An empty required string is malformed, not long.
        assert_eq!(
            refusal(json!({"kind": "work_item_note", "item_id": "item-1", "note": "  "})),
            CapabilityEffectRefusal::Invalid
        );
        // Lengths are the declared ones, and length wins only over a type-valid
        // string.
        assert_eq!(
            refusal(
                json!({"kind": "work_item_note", "item_id": "item-1", "note": "x".repeat(2_001)})
            ),
            CapabilityEffectRefusal::TooLong
        );
        assert_eq!(
            refusal(
                json!({"kind": "work_item_note", "item_id": "item-1", "note": "x", "tag": "t".repeat(65)})
            ),
            CapabilityEffectRefusal::TooLong
        );
        assert!(
            validate_declared_effect(
                declared,
                "item-1",
                &json!({"kind": "work_item_note", "item_id": "item-1", "note": "x".repeat(2_000)})
            )
            .is_ok(),
            "the declared bound is inclusive"
        );
        // Every refusal has exactly one fixed text.
        assert_eq!(
            CapabilityEffectRefusal::Invalid.message(),
            "invalid capability effect"
        );
        assert_eq!(
            CapabilityEffectRefusal::TooLong.message(),
            "capability effect exceeds its declared length"
        );
        assert_eq!(
            CapabilityEffectRefusal::NotDeclared.message(),
            "capability does not declare an effect"
        );
    }

    #[test]
    fn command_ids_are_strict() {
        assert!(valid_command_id("cap-cmd-1"));
        assert!(valid_command_id("a:b.c_d-e"));
        assert!(!valid_command_id(""));
        assert!(!valid_command_id("has space"));
        assert!(!valid_command_id(&"x".repeat(129)));
    }
}
