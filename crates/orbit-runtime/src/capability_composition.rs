//! Trusted capability composition: the only path from an installed capability
//! descriptor to a durable, explicit, authorized execution.
//!
//! Installation, authorization, request and claim are separate facts. A
//! blueprint manifest never grants execution authority, and a transport never
//! dispatches a capability: [`TrustedComposition`] is constructed by the host
//! process, and execution is only reachable through the host API in this
//! module. Connected/HTTP parity is tracked separately (M25 EC-004); connected
//! transports must fail closed rather than queue a request that cannot execute.
use crate::AuthoritativeHost;
use crate::blueprint_execution::{
    BlueprintCapabilityExecutor, BlueprintExecutorRegistry, ExecutionOutcome,
};
use crate::capability_contracts::{
    CapabilityContract, CapabilityScope, merge_parameters, offers_for_item, valid_command_id,
    validate_capability_inputs,
};
use orbit_application::RepositoryError;
use orbit_blueprint::{
    ActionInvocation, CapabilityCatalog, CapabilityDescriptor, CapabilityPermission,
};
use orbit_storage::blueprint_actions::{BlueprintActionIntent, BlueprintActionRequest};
use orbit_storage::capability_installs::{
    CapabilityGrant, CapabilityInstallation, CapabilityReceipt,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The contract vocabulary lives in [`crate::capability_contracts`] so a
/// connected server can describe a capability without loading a provider. These
/// re-exports keep the local composition path source compatible.
pub use crate::capability_contracts::{
    CapabilityActionAvailability, CapabilityActionSpec, ContractCatalog, OfferedCapabilityAction,
    contract, contracts,
};

/// The deterministic, I/O-free capability used to prove the whole composition
/// path end to end.
pub const REFERENCE_CAPABILITY_ID: &str = "orbit.reference.work-item-note";

/// The declared character bound of the reference action's required `note`.
///
/// The action's input schema and the capability's effect contract
/// ([`crate::capability_contracts::declared_effect`]) both read this constant, so
/// an offer can never advertise a longer note than the authority will accept.
pub const REFERENCE_NOTE_MAX_CHARS: u64 = 2_000;

/// The declared character bound of the reference action's optional `tag`.
///
/// Stated once for the same reason as [`REFERENCE_NOTE_MAX_CHARS`]: an input the
/// offer does not bound is an input the authority's effect declaration refuses,
/// which would strand a claimed request with no receipt. The bound is therefore
/// visible in the offer (and enforced by the request form) before anything is
/// recorded.
pub const REFERENCE_TAG_MAX_CHARS: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedCapability {
    pub descriptor: CapabilityDescriptor,
    pub actions: Vec<CapabilityActionSpec>,
    /// Other capabilities that must be installed before this one is usable.
    pub requires: BTreeSet<String>,
}

impl TrustedCapability {
    pub fn action(&self, action_id: &str) -> Option<&CapabilityActionSpec> {
        self.actions.iter().find(|spec| spec.action_id == action_id)
    }
}

/// Process-local composition. It is not global mutable state: the host builds
/// one and passes it to the entry point that needs it.
#[derive(Default)]
pub struct TrustedComposition {
    capabilities: BTreeMap<String, TrustedCapability>,
    executors: BlueprintExecutorRegistry,
}

impl TrustedComposition {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The bundled reference composition: deterministic, no network, no Python,
    /// no Herdr, no shell. It also registers the opt-in habit recurrence package;
    /// registration never installs or authorizes it.
    pub fn with_builtin_reference() -> Self {
        let mut composition = Self::empty();
        composition
            .register(
                reference_capability(),
                Box::new(ReferenceWorkItemNoteExecutor),
            )
            .expect("built-in reference capability is a valid composition member");
        crate::habit_capability::register_habit_capability(&mut composition)
            .expect("built-in habit recurrence capability is a valid composition member");
        composition
    }

    /// Registration is trusted composition, never blueprint-controlled. A
    /// duplicate capability, a duplicate executor, or an executor that does not
    /// serve the capability it is registered with is rejected.
    pub fn register(
        &mut self,
        capability: TrustedCapability,
        executor: Box<dyn BlueprintCapabilityExecutor>,
    ) -> Result<(), String> {
        let id = capability.descriptor.id.clone();
        if id.is_empty() || self.capabilities.contains_key(&id) {
            return Err(format!("duplicate or empty capability id: {id}"));
        }
        if executor.capability_id() != id {
            return Err(format!(
                "executor {} cannot serve capability {id}",
                executor.capability_id()
            ));
        }
        let mut action_ids = BTreeSet::new();
        for action in &capability.actions {
            if action.action_id.trim().is_empty() || !action_ids.insert(action.action_id.clone()) {
                return Err(format!("duplicate or empty action id for {id}"));
            }
            if !action.input_schema.is_object() {
                return Err(format!("action {} has no input schema", action.action_id));
            }
        }
        self.executors.register(executor)?;
        self.capabilities.insert(id, capability);
        Ok(())
    }

    pub fn capability(&self, id: &str) -> Option<&TrustedCapability> {
        self.capabilities.get(id)
    }

    pub fn capabilities(&self) -> impl Iterator<Item = &TrustedCapability> {
        self.capabilities.values()
    }

    pub fn has_executor(&self, id: &str) -> bool {
        self.executors.contains(id)
    }

    /// Descriptors are registered even when disabled so availability can report
    /// `capability_disabled` distinctly from `capability_missing`. Only enabled
    /// descriptors appear in the catalog's available set.
    pub fn catalog(&self) -> CapabilityCatalog {
        let mut catalog = CapabilityCatalog::default();
        for capability in self.capabilities.values() {
            let _ = catalog.register(capability.descriptor.clone());
        }
        catalog
    }
}

/// The built-in reference capability contract.
pub fn reference_capability() -> TrustedCapability {
    TrustedCapability {
        descriptor: CapabilityDescriptor {
            id: REFERENCE_CAPABILITY_ID.to_owned(),
            provider: "orbit".to_owned(),
            permission: CapabilityPermission::Mutate,
            enabled: true,
        },
        actions: vec![CapabilityActionSpec {
            action_id: "append_note".to_owned(),
            description: "Append a durable note to this blueprint work item".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "note": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": REFERENCE_NOTE_MAX_CHARS,
                        "description": "Note body (1..2000 characters)"
                    },
                    "tag": {
                        "type": "string",
                        "maxLength": REFERENCE_TAG_MAX_CHARS,
                        "description": "Optional label for the note"
                    }
                },
                "required": ["note"]
            }),
            external_effect: false,
            requires: Vec::new(),
        }],
        requires: BTreeSet::new(),
    }
}

struct ReferenceWorkItemNoteExecutor;

impl BlueprintCapabilityExecutor for ReferenceWorkItemNoteExecutor {
    fn capability_id(&self) -> &str {
        REFERENCE_CAPABILITY_ID
    }

    fn execute(
        &mut self,
        host: &mut AuthoritativeHost,
        request: &BlueprintActionRequest,
        invocation: &ActionInvocation,
    ) -> ExecutionOutcome {
        // The persisted envelope must describe exactly this dispatch.
        if invocation.capability != REFERENCE_CAPABILITY_ID
            || invocation.command_id != request.command_id
            || invocation.action_id != request.action_id
        {
            return ExecutionOutcome::Failed;
        }
        let Some(note) = invocation.parameters.get("note").and_then(Value::as_str) else {
            return ExecutionOutcome::Failed;
        };
        let tag = match invocation.parameters.get("tag") {
            None | Some(Value::Null) => None,
            Some(Value::String(tag)) => Some(tag.as_str()),
            _ => return ExecutionOutcome::Failed,
        };
        // Defense in depth: the durable request must still point at a real item
        // in a project that is currently authorized for this capability.
        let item = match host.authority.conn.blueprint_work_item(&request.item_id) {
            Ok(Some(item)) => item,
            _ => return ExecutionOutcome::Failed,
        };
        let authorized = match host
            .authority
            .conn
            .authorized_capabilities(&item.project_id)
        {
            Ok(authorized) => authorized,
            Err(_) => return ExecutionOutcome::Failed,
        };
        if !authorized.contains(REFERENCE_CAPABILITY_ID) {
            return ExecutionOutcome::Failed;
        }
        let Some(now_ms) = now_ms() else {
            return ExecutionOutcome::Failed;
        };
        // The command ID is the idempotency key: a replay can never append a
        // second note for the same action.
        match host.authority.conn.record_reference_work_item_note(
            &request.command_id,
            &item.id,
            note,
            tag,
            now_ms,
        ) {
            Ok(_) => ExecutionOutcome::Succeeded,
            Err(_) => ExecutionOutcome::Failed,
        }
    }
}

fn now_ms() -> Option<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    i64::try_from(elapsed.as_millis()).ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityStatusRow {
    pub capability_id: String,
    pub provider: String,
    pub permission: String,
    pub installed: bool,
    pub executor_registered: bool,
    pub actions: Vec<String>,
    pub requires: Vec<String>,
    pub authorized_projects: Vec<String>,
    pub available: bool,
    /// Why this capability is unavailable in the current composition/durable
    /// state, using the same codes as action offers. `None` means available.
    pub unavailable_reason: Option<String>,
}

/// A user's explicit request to invoke one offered capability action. It is not
/// an execution instruction: only [`AuthoritativeHost::execute_capability_action`]
/// may claim and dispatch it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityActionRequest {
    pub item_id: String,
    pub expected_revision: u64,
    pub action_id: String,
    pub command_id: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityActionState {
    pub request: BlueprintActionRequest,
    pub receipt: Option<CapabilityReceipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityReconcileDecision {
    ConfirmSucceeded,
    ConfirmFailed,
}

impl CapabilityReconcileDecision {
    fn outcome(self) -> &'static str {
        match self {
            Self::ConfirmSucceeded => "succeeded",
            Self::ConfirmFailed => "failed",
        }
    }
}

impl AuthoritativeHost {
    /// Installation view joined with the current trusted composition. Never
    /// derives a permission from a blueprint manifest.
    pub fn capability_status(
        &self,
        composition: &TrustedComposition,
        project_id: Option<&str>,
    ) -> Result<Vec<CapabilityStatusRow>, RepositoryError> {
        let installed: BTreeMap<String, CapabilityInstallation> = self
            .authority
            .conn
            .capability_installations()?
            .into_iter()
            .map(|row| (row.capability_id.clone(), row))
            .collect();
        let grants = self.authority.conn.capability_grants(project_id)?;
        let mut ids: BTreeSet<String> = composition
            .capabilities()
            .map(|capability| capability.descriptor.id.clone())
            .collect();
        ids.extend(installed.keys().cloned());
        let mut rows = Vec::new();
        for id in ids {
            let trusted = composition.capability(&id);
            let row = installed.get(&id);
            let actions = match trusted {
                Some(capability) => capability
                    .actions
                    .iter()
                    .map(|action| action.action_id.clone())
                    .collect(),
                None => row
                    .and_then(|row| serde_json::from_str::<Vec<Value>>(&row.actions_json).ok())
                    .map(|actions| {
                        actions
                            .iter()
                            .filter_map(|action| {
                                action
                                    .get("action_id")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            let requires: Vec<String> = match trusted {
                Some(capability) => capability.requires.iter().cloned().collect(),
                None => row
                    .and_then(|row| serde_json::from_str::<Vec<String>>(&row.requires_json).ok())
                    .unwrap_or_default(),
            };
            let provider = row
                .map(|row| row.provider.clone())
                .or_else(|| trusted.map(|capability| capability.descriptor.provider.clone()))
                .unwrap_or_default();
            let permission = row
                .map(|row| row.permission.clone())
                .or_else(|| {
                    trusted.map(|capability| {
                        match capability.descriptor.permission {
                            CapabilityPermission::Read => "read",
                            CapabilityPermission::Mutate => "mutate",
                            CapabilityPermission::ExternalEffect => "external_effect",
                        }
                        .to_owned()
                    })
                })
                .unwrap_or_default();
            let enabled = trusted.is_some_and(|capability| capability.descriptor.enabled);
            let authorized_projects: Vec<String> = grants
                .iter()
                .filter(|grant| grant.capability_id == id)
                .map(|grant| grant.project_id.clone())
                .collect();
            let available = row.is_some()
                && enabled
                && composition.has_executor(&id)
                && requires.iter().all(|required| installed.contains_key(required))
                // With a project filter, "available" additionally means
                // authorized for exactly that project.
                && (project_id.is_none() || !authorized_projects.is_empty());
            let unavailable_reason = if row.is_none() {
                Some(format!("capability_not_installed: {id} is not installed"))
            } else if !enabled {
                Some(format!("capability_disabled: {id} is disabled"))
            } else if !composition.has_executor(&id) {
                Some(format!("executor_missing: no executor registered for {id}"))
            } else if let Some(missing) = requires
                .iter()
                .find(|required| !installed.contains_key(*required))
            {
                Some(format!(
                    "requirement_missing: {id} requires {missing}, which is not installed"
                ))
            } else if project_id.is_some() && authorized_projects.is_empty() {
                Some(format!(
                    "not_authorized: {id} is not authorized for the requested project"
                ))
            } else {
                None
            };
            rows.push(CapabilityStatusRow {
                capability_id: id.clone(),
                provider,
                permission,
                installed: row.is_some(),
                executor_registered: composition.has_executor(&id),
                actions,
                requires,
                authorized_projects,
                available,
                unavailable_reason,
            });
        }
        Ok(rows)
    }

    /// The set of capabilities that publication may treat as available: durable
    /// installation AND an enabled descriptor in the current composition. A
    /// manifest that requires a capability is unpublishable until this set
    /// contains it.
    pub fn installed_capability_ids(
        &self,
        composition: &TrustedComposition,
    ) -> Result<BTreeSet<String>, RepositoryError> {
        let available = composition.catalog().available();
        let installed: BTreeSet<String> = self
            .authority
            .conn
            .capability_installations()?
            .into_iter()
            .map(|installation| installation.capability_id)
            .collect();
        Ok(installed.intersection(&available).cloned().collect())
    }

    /// Installation records the trusted composition's contract. An id that the
    /// composition does not know is rejected rather than invented. The row is
    /// built from the shared contract type so a connected server writes exactly
    /// the same installation without constructing a composition.
    pub fn install_trusted_capability(
        &mut self,
        composition: &TrustedComposition,
        capability_id: &str,
        at_ms: i64,
    ) -> Result<CapabilityInstallation, RepositoryError> {
        let capability = composition
            .capability(capability_id)
            .ok_or(RepositoryError::Conflict)?;
        let contract = CapabilityContract::from_trusted(capability);
        let installation = contract.installation(&capability.descriptor.provider, at_ms)?;
        self.authority.conn.install_capability(&installation)
    }

    pub fn uninstall_trusted_capability(
        &mut self,
        capability_id: &str,
    ) -> Result<bool, RepositoryError> {
        self.authority.conn.uninstall_capability(capability_id)
    }

    /// The only operation that creates a project-scoped grant. Requires an
    /// existing installation and an existing project.
    pub fn authorize_trusted_capability(
        &mut self,
        capability_id: &str,
        project_id: &str,
        actor: &str,
        at_ms: i64,
    ) -> Result<CapabilityGrant, RepositoryError> {
        self.authority
            .conn
            .grant_capability(capability_id, project_id, actor, at_ms)
    }

    pub fn revoke_trusted_capability(
        &mut self,
        capability_id: &str,
        project_id: &str,
    ) -> Result<bool, RepositoryError> {
        self.authority
            .conn
            .revoke_capability(capability_id, project_id)
    }

    /// Every action offered by a transition whose `from` is the item's current
    /// state, with availability computed from durable state only.
    pub fn capability_action_offers(
        &self,
        composition: &TrustedComposition,
        item_id: &str,
    ) -> Result<Vec<OfferedCapabilityAction>, RepositoryError> {
        let item = self
            .authority
            .conn
            .blueprint_work_item(item_id)?
            .ok_or(RepositoryError::Conflict)?;
        self.offers_for_item(composition, &item)
    }

    fn offers_for_item(
        &self,
        composition: &TrustedComposition,
        item: &orbit_blueprint::BlueprintWorkItem,
    ) -> Result<Vec<OfferedCapabilityAction>, RepositoryError> {
        // The availability algorithm is shared with the connected server; only
        // the executor question differs (registered provider here, announced
        // enrolled device there).
        offers_for_item(
            &self.authority.conn,
            item,
            composition,
            CapabilityScope::Local,
        )
    }

    /// Persist an explicit request. Authorization, installation, requirements,
    /// revision and inputs are all re-checked here: an earlier "Available"
    /// offer is never treated as permission.
    pub fn request_capability_action(
        &mut self,
        composition: &TrustedComposition,
        request: CapabilityActionRequest,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        if !valid_command_id(&request.command_id) || request.item_id.trim().is_empty() {
            return Err(RepositoryError::Conflict);
        }
        let item = self
            .authority
            .conn
            .blueprint_work_item(&request.item_id)?
            .ok_or(RepositoryError::Conflict)?;
        if item.revision != request.expected_revision {
            return Err(RepositoryError::Conflict);
        }
        let offers = self.offers_for_item(composition, &item)?;
        let offer = offers
            .iter()
            .find(|offer| offer.action_id == request.action_id)
            .ok_or(RepositoryError::Conflict)?;
        match &offer.availability {
            CapabilityActionAvailability::Available => {}
            CapabilityActionAvailability::Unavailable { code, reason } => {
                return Err(RepositoryError::Other(format!(
                    "capability action unavailable: {code}: {reason}"
                )));
            }
        }
        // Input validity is part of the capability's trusted contract, not the
        // manifest's. Validate the merged invocation (defaults + user values)
        // before anything is persisted, and allow exactly the inputs the
        // capability's own schema declares.
        let mut allowed_inputs = std::collections::BTreeSet::new();
        if let Some(spec) = composition
            .capability(&offer.capability)
            .and_then(|capability| capability.action(&offer.action_id))
        {
            let parameters = merge_parameters(offer, &request.inputs);
            validate_capability_inputs(&spec.input_schema, &parameters).map_err(|reason| {
                RepositoryError::Other(format!("invalid capability action input: {reason}"))
            })?;
            allowed_inputs.extend(
                spec.input_schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default(),
            );
        }
        let catalog = composition.catalog();
        let grants = self
            .authority
            .conn
            .authorized_capabilities(&item.project_id)?;
        self.authority
            .conn
            .request_blueprint_action_on_transition_allowing(
                BlueprintActionIntent {
                    item_id: &request.item_id,
                    expected_revision: request.expected_revision,
                    from: &offer.from,
                    action_id: &request.action_id,
                    command_id: &request.command_id,
                },
                &offer.to,
                &request.inputs,
                &allowed_inputs,
                &catalog,
                &grants,
            )
    }

    /// Claim and execute exactly once. Delegates the single-claim fence to the
    /// existing executor registry and then records a durable receipt. Replaying
    /// after a recorded outcome returns that receipt instead of re-executing.
    pub fn execute_capability_action(
        &mut self,
        composition: &mut TrustedComposition,
        command_id: &str,
        at_ms: i64,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        if let Some(receipt) = self.authority.conn.capability_receipt(command_id)? {
            return Ok(receipt);
        }
        let request = self
            .authority
            .conn
            .blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        // Provider identity comes from the durable installation, never from a
        // request body. A missing provider conflicts before any claim.
        let installation = self
            .authority
            .conn
            .capability_installation(&request.capability)?
            .ok_or(RepositoryError::Conflict)?;
        let executed = composition.executors.execute_claimed(self, command_id)?;
        let detail = json!({"state": executed.state});
        self.authority
            .conn
            .record_capability_receipt(&CapabilityReceipt {
                command_id: request.command_id,
                item_id: request.item_id,
                capability: request.capability,
                action_id: request.action_id,
                provider: installation.provider,
                outcome: executed.state,
                detail,
                recorded_at_ms: at_ms,
            })
    }

    pub fn capability_action_receipt(
        &self,
        command_id: &str,
    ) -> Result<Option<CapabilityActionState>, RepositoryError> {
        let Some(request) = self.authority.conn.blueprint_action_request(command_id)? else {
            return Ok(None);
        };
        let receipt = self.authority.conn.capability_receipt(command_id)?;
        Ok(Some(CapabilityActionState { request, receipt }))
    }

    pub fn list_capability_actions(
        &self,
        item_id: Option<&str>,
        state: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BlueprintActionRequest>, RepositoryError> {
        self.authority
            .conn
            .capability_action_requests(item_id, state, limit)
    }

    /// Durable effects of the built-in reference capability for one work item.
    pub fn reference_work_item_notes(
        &self,
        item_id: &str,
    ) -> Result<Vec<orbit_storage::capability_installs::ReferenceWorkItemNote>, RepositoryError>
    {
        self.authority.conn.reference_work_item_notes(item_id)
    }

    /// Explicitly settle a request whose outcome is unknown to the authority.
    ///
    /// Supported states are `uncertain` (the provider reported an ambiguous
    /// result) and `claimed` with no receipt (the process died between the
    /// durable claim and the recorded result, so no acknowledgement was ever
    /// received). An explicit operator decision is what resolves them; this
    /// never dispatches a capability, never re-claims, and never rewrites a
    /// `succeeded`/`failed` outcome. A `pending` request has not executed and is
    /// refused: it must go through `execute_capability_action`.
    ///
    /// The state transition and the receipt write commit atomically. Replaying
    /// the identical decision/actor is idempotent; a different decision or actor
    /// conflicts.
    pub fn reconcile_capability_action(
        &mut self,
        command_id: &str,
        decision: CapabilityReconcileDecision,
        actor: &str,
        at_ms: i64,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        self.reconcile_capability_action_with_note(command_id, decision, actor, "", at_ms)
    }

    /// See [`Self::reconcile_capability_action`]. `note` is the authenticated
    /// actor's own explanation, recorded inside the receipt detail so a remote
    /// operator's decision is attributable after the fact. An empty note keeps
    /// the historically recorded detail object byte-identical.
    pub fn reconcile_capability_action_with_note(
        &mut self,
        command_id: &str,
        decision: CapabilityReconcileDecision,
        actor: &str,
        note: &str,
        at_ms: i64,
    ) -> Result<CapabilityReceipt, RepositoryError> {
        if actor.trim().is_empty() || actor.len() > 128 || note.len() > 1_000 {
            return Err(RepositoryError::Conflict);
        }
        let note = note.trim();
        let request = self
            .authority
            .conn
            .blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        let outcome = decision.outcome();
        let existing = self.authority.conn.capability_receipt(command_id)?;
        match request.state.as_str() {
            // Lost acknowledgement: the claim is durable but nothing recorded
            // the provider's result, so an operator decision is the only truth.
            "claimed" => {
                let detail = reconcile_detail(actor, "claimed", note);
                match &existing {
                    // Defensive replay of a claim already reconciled.
                    Some(receipt)
                        if receipt.outcome == outcome
                            && receipt.detail.get("reconciled_by").and_then(Value::as_str)
                                == Some(actor) =>
                    {
                        Ok(receipt.clone())
                    }
                    // A recorded outcome can never be overwritten by a claim.
                    Some(receipt) if receipt.outcome != "uncertain" => {
                        Err(RepositoryError::Conflict)
                    }
                    Some(_) | None => {
                        // Provider identity stays the durable installation's,
                        // never an operator-supplied string.
                        let provider = self
                            .authority
                            .conn
                            .capability_installation(&request.capability)?
                            .ok_or(RepositoryError::Conflict)?
                            .provider;
                        self.authority
                            .conn
                            .settle_blueprint_action(command_id, outcome, &provider, &detail, at_ms)
                    }
                }
            }
            "uncertain" => {
                let receipt = existing.ok_or(RepositoryError::Conflict)?;
                if receipt.outcome != "uncertain" {
                    return Err(RepositoryError::Conflict);
                }
                let detail = reconcile_detail(actor, "uncertain", note);
                self.authority.conn.settle_blueprint_action(
                    command_id,
                    outcome,
                    &receipt.provider,
                    &detail,
                    at_ms,
                )
            }
            "succeeded" | "failed" => {
                let receipt = existing.ok_or(RepositoryError::Conflict)?;
                if receipt.outcome != outcome
                    || receipt.detail.get("reconciled_by").and_then(Value::as_str) != Some(actor)
                    || receipt.detail.get("reconciled") != Some(&Value::Bool(true))
                {
                    return Err(RepositoryError::Conflict);
                }
                Ok(receipt)
            }
            // `pending` and any unknown state are never reconcilable.
            _ => Err(RepositoryError::Conflict),
        }
    }
}

/// The receipt detail of an explicit reconciliation. The operator's note is
/// recorded only when supplied, so a decision taken without one keeps the
/// historic shape (and therefore replays identically).
fn reconcile_detail(actor: &str, recovered_from: &str, note: &str) -> Value {
    let mut detail = json!({
        "reconciled": true,
        "reconciled_by": actor,
        "recovered_from": recovered_from,
    });
    if !note.is_empty()
        && let Some(object) = detail.as_object_mut()
    {
        object.insert("note".to_owned(), Value::String(note.to_owned()));
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_application::ProjectRepository;
    use orbit_blueprint::Blueprint;
    use orbit_domain::{Project, ProjectId};

    const DEMO_MANIFEST: &str =
        include_str!("../../../blueprints/examples/capability-reference.json");

    fn project(host: &mut AuthoritativeHost, id: &str) {
        let project = Project::new(ProjectId::new(id).unwrap(), "Reference").expect("project");
        ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
    }

    fn demo_manifest() -> Blueprint {
        Blueprint::from_json(
            DEMO_MANIFEST,
            &BTreeSet::from([REFERENCE_CAPABILITY_ID.to_owned()]),
        )
        .expect("demo manifest")
    }

    /// Install -> publish -> bind -> create item, in the only order the
    /// publication rule allows.
    fn bound_demo(host: &mut AuthoritativeHost, composition: &TrustedComposition) {
        project(host, "project-1");
        host.install_trusted_capability(composition, REFERENCE_CAPABILITY_ID, 1)
            .unwrap();
        let installed = host.installed_capability_ids(composition).unwrap();
        host.publish_project_blueprint(&demo_manifest(), &installed)
            .unwrap();
        host.activate_project_blueprint("project-1", "capability-reference", 1)
            .unwrap();
        host.create_project_work_item("item-1", "project-1", "note-task", BTreeMap::new())
            .unwrap();
    }

    fn demo_host() -> (AuthoritativeHost, TrustedComposition) {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let composition = TrustedComposition::with_builtin_reference();
        bound_demo(&mut host, &composition);
        (host, composition)
    }

    fn single_offer(
        host: &AuthoritativeHost,
        composition: &TrustedComposition,
    ) -> OfferedCapabilityAction {
        let offers = host
            .capability_action_offers(composition, "item-1")
            .unwrap();
        assert_eq!(offers.len(), 1, "one offered action: {offers:?}");
        offers.into_iter().next().unwrap()
    }

    fn request(command_id: &str, note: &str) -> CapabilityActionRequest {
        CapabilityActionRequest {
            item_id: "item-1".into(),
            expected_revision: 0,
            action_id: "append_note".into(),
            command_id: command_id.into(),
            inputs: BTreeMap::from([("note".to_owned(), json!(note))]),
        }
    }

    fn authorize(host: &mut AuthoritativeHost) {
        host.authorize_trusted_capability(REFERENCE_CAPABILITY_ID, "project-1", "tester", 2)
            .unwrap();
    }

    struct OutcomeExecutor {
        id: &'static str,
        outcome: ExecutionOutcome,
    }

    impl BlueprintCapabilityExecutor for OutcomeExecutor {
        fn capability_id(&self) -> &str {
            self.id
        }
        fn execute(
            &mut self,
            _host: &mut AuthoritativeHost,
            _request: &BlueprintActionRequest,
            _invocation: &ActionInvocation,
        ) -> ExecutionOutcome {
            self.outcome
        }
    }

    fn test_manifest(capability: &str) -> Blueprint {
        serde_json::from_value(json!({
            "id": "test-capability", "version": 1, "name": "Test capability",
            "required_capabilities": [capability],
            "work_items": [{
                "id": "task", "name": "Task", "initial_state": "open",
                "states": ["open", "closed"],
                "transitions": [{
                    "from": "open", "to": "closed",
                    "actions": [{
                        "id": "run", "capability": capability,
                        "description": "Run the test capability",
                        "parameters": {}
                    }]
                }]
            }]
        }))
        .expect("test manifest")
    }

    fn custom_host(
        id: &'static str,
        outcome: ExecutionOutcome,
    ) -> (AuthoritativeHost, TrustedComposition) {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        project(&mut host, "project-1");
        let mut composition = TrustedComposition::empty();
        composition
            .register(
                TrustedCapability {
                    descriptor: CapabilityDescriptor {
                        id: id.into(),
                        provider: "test".into(),
                        permission: CapabilityPermission::Mutate,
                        enabled: true,
                    },
                    actions: vec![CapabilityActionSpec {
                        action_id: "run".into(),
                        description: "Run".into(),
                        input_schema: json!({"type": "object", "properties": {}}),
                        external_effect: false,
                        requires: Vec::new(),
                    }],
                    requires: BTreeSet::new(),
                },
                Box::new(OutcomeExecutor { id, outcome }),
            )
            .unwrap();
        host.install_trusted_capability(&composition, id, 1)
            .unwrap();
        let installed = host.installed_capability_ids(&composition).unwrap();
        host.publish_project_blueprint(&test_manifest(id), &installed)
            .unwrap();
        host.activate_project_blueprint("project-1", "test-capability", 1)
            .unwrap();
        host.create_project_work_item("item-1", "project-1", "task", BTreeMap::new())
            .unwrap();
        host.authorize_trusted_capability(id, "project-1", "tester", 2)
            .unwrap();
        (host, composition)
    }

    fn custom_request(command_id: &str) -> CapabilityActionRequest {
        CapabilityActionRequest {
            item_id: "item-1".into(),
            expected_revision: 0,
            action_id: "run".into(),
            command_id: command_id.into(),
            inputs: BTreeMap::new(),
        }
    }

    #[test]
    fn demo_blueprint_cannot_be_published_before_install() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        project(&mut host, "project-1");
        let composition = TrustedComposition::with_builtin_reference();
        let installed = host.installed_capability_ids(&composition).unwrap();
        assert!(installed.is_empty());
        assert!(
            host.publish_project_blueprint(&demo_manifest(), &installed)
                .is_err(),
            "publication must fail before the capability is installed"
        );
        host.install_trusted_capability(&composition, REFERENCE_CAPABILITY_ID, 1)
            .unwrap();
        let installed = host.installed_capability_ids(&composition).unwrap();
        assert_eq!(
            installed,
            BTreeSet::from([REFERENCE_CAPABILITY_ID.to_owned()])
        );
        host.publish_project_blueprint(&demo_manifest(), &installed)
            .unwrap();
        assert!(
            host.project_blueprint("capability-reference", 1)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn offers_report_every_availability_code_from_durable_state() {
        let (mut host, composition) = demo_host();
        // Installed, not authorized.
        let offer = single_offer(&host, &composition);
        assert_eq!(offer.availability.code(), Some("not_authorized"));
        assert_eq!(offer.from, "open");
        assert_eq!(offer.to, "closed");
        assert!(!offer.external_effect);
        assert_eq!(offer.declared_parameters, json!({"note": ""}));
        assert_eq!(offer.input_schema["required"], json!(["note"]));

        authorize(&mut host);
        assert_eq!(
            single_offer(&host, &composition).availability,
            CapabilityActionAvailability::Available
        );

        // Unknown to the composition.
        let empty = TrustedComposition::empty();
        assert_eq!(
            single_offer(&host, &empty).availability.code(),
            Some("capability_missing")
        );

        // A disabled descriptor is distinct from a missing one.
        let mut disabled = TrustedComposition::empty();
        disabled
            .register(
                TrustedCapability {
                    descriptor: CapabilityDescriptor {
                        id: REFERENCE_CAPABILITY_ID.into(),
                        provider: "orbit".into(),
                        permission: CapabilityPermission::Mutate,
                        enabled: false,
                    },
                    actions: reference_capability().actions,
                    requires: BTreeSet::new(),
                },
                Box::new(OutcomeExecutor {
                    id: REFERENCE_CAPABILITY_ID,
                    outcome: ExecutionOutcome::Succeeded,
                }),
            )
            .unwrap();
        assert_eq!(
            single_offer(&host, &disabled).availability.code(),
            Some("capability_disabled")
        );

        // A composition without the advertised action cannot execute it.
        let mut no_action = TrustedComposition::empty();
        no_action
            .register(
                TrustedCapability {
                    actions: Vec::new(),
                    ..reference_capability()
                },
                Box::new(OutcomeExecutor {
                    id: REFERENCE_CAPABILITY_ID,
                    outcome: ExecutionOutcome::Succeeded,
                }),
            )
            .unwrap();
        assert_eq!(
            single_offer(&host, &no_action).availability.code(),
            Some("executor_missing")
        );

        // Uninstalling the capability invalidates the bound item's offer.
        host.uninstall_trusted_capability(REFERENCE_CAPABILITY_ID)
            .unwrap();
        assert_eq!(
            single_offer(&host, &composition).availability.code(),
            Some("capability_not_installed")
        );
    }

    #[test]
    fn offers_report_missing_requirements() {
        let (mut host, _) = demo_host();
        let mut composition = TrustedComposition::empty();
        composition
            .register(
                TrustedCapability {
                    requires: BTreeSet::from(["orbit.some.requirement".to_owned()]),
                    ..reference_capability()
                },
                Box::new(OutcomeExecutor {
                    id: REFERENCE_CAPABILITY_ID,
                    outcome: ExecutionOutcome::Succeeded,
                }),
            )
            .unwrap();
        assert_eq!(
            single_offer(&host, &composition).availability.code(),
            Some("requirement_missing")
        );
        // Installing the requirement alone does not authorize the action.
        host.authorize_trusted_capability(REFERENCE_CAPABILITY_ID, "project-1", "tester", 2)
            .unwrap();
        assert_eq!(
            single_offer(&host, &composition).availability.code(),
            Some("requirement_missing")
        );
    }

    #[test]
    fn request_fails_closed_without_authorization() {
        let (mut host, composition) = demo_host();
        let error = host
            .request_capability_action(&composition, request("action-1", "hello"))
            .unwrap_err();
        assert!(
            matches!(&error, RepositoryError::Other(message) if message.contains("not_authorized")),
            "{error:?}"
        );
        assert!(
            host.list_capability_actions(None, None, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn request_rejects_unsupported_action_revision_and_inputs() {
        let (mut host, composition) = demo_host();
        authorize(&mut host);
        let mut unsupported = request("action-1", "hello");
        unsupported.action_id = "delete_everything".into();
        assert!(
            host.request_capability_action(&composition, unsupported)
                .is_err()
        );

        let mut stale = request("action-2", "hello");
        stale.expected_revision = 9;
        assert!(host.request_capability_action(&composition, stale).is_err());

        // A caller can never invent capability parameters.
        let mut injected = request("action-3", "hello");
        injected.inputs.insert("shell".into(), json!("rm -rf /"));
        assert!(
            host.request_capability_action(&composition, injected)
                .is_err()
        );

        // An empty note is refused by the input schema's min length.
        let mut blank = request("action-4", "");
        blank.inputs.insert("note".into(), json!(""));
        assert!(host.request_capability_action(&composition, blank).is_err());
        assert!(
            host.list_capability_actions(None, None, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn request_accepts_schema_declared_inputs_and_enforces_max_length() {
        let (mut host, composition) = demo_host();
        authorize(&mut host);
        // `tag` is declared by the capability's input schema but not by the
        // manifest defaults: optional inputs must not need a fake default.
        let mut tagged = request("action-tag", "hello");
        tagged.inputs.insert("tag".into(), json!("review"));
        let stored = host
            .request_capability_action(&composition, tagged)
            .unwrap();
        let plan: ActionInvocation = serde_json::from_str(&stored.plan_json).unwrap();
        assert_eq!(plan.parameters["note"], json!("hello"));
        assert_eq!(plan.parameters["tag"], json!("review"));

        // maxLength is enforced before a request is persisted.
        let oversized = "x".repeat(2001);
        let mut too_long = request("action-too-long", &oversized);
        too_long.inputs.insert("note".into(), json!(oversized));
        let error = host
            .request_capability_action(&composition, too_long)
            .unwrap_err();
        assert!(
            matches!(&error, RepositoryError::Other(message)
                if message.contains("invalid capability action input")),
            "{error:?}"
        );
        // The same bound is declared for the optional tag: an over-long tag is
        // refused before a request is persisted, so it can never reach an
        // executor and strand the request as `claimed` with no receipt.
        let mut tag_too_long = request("action-tag-too-long", "hello");
        tag_too_long
            .inputs
            .insert("tag".into(), json!("t".repeat(65)));
        let error = host
            .request_capability_action(&composition, tag_too_long)
            .unwrap_err();
        assert!(
            matches!(&error, RepositoryError::Other(message)
                if message.contains("invalid capability action input")),
            "{error:?}"
        );
        let stored = host.list_capability_actions(None, None, 10).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].command_id, "action-tag");
    }

    #[test]
    fn request_is_idempotent_per_command_and_fenced_per_action() {
        let (mut host, composition) = demo_host();
        authorize(&mut host);
        let first = host
            .request_capability_action(&composition, request("action-1", "hello"))
            .unwrap();
        assert_eq!(
            host.request_capability_action(&composition, request("action-1", "hello"))
                .unwrap(),
            first
        );
        // A different command ID for the same (item, revision, action) conflicts.
        assert!(
            host.request_capability_action(&composition, request("action-2", "hello"))
                .is_err()
        );
        assert_eq!(
            host.list_capability_actions(Some("item-1"), Some("pending"), 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn single_claim_fence_rejects_a_second_claim() {
        let (mut host, composition) = demo_host();
        authorize(&mut host);
        host.request_capability_action(&composition, request("action-1", "hello"))
            .unwrap();
        host.claim_project_work_item_action("action-1").unwrap();
        assert!(host.claim_project_work_item_action("action-1").is_err());
        // A claimed but unresolved request never duplicates its effect.
        assert!(
            host.execute_capability_action(
                &mut TrustedComposition::with_builtin_reference(),
                "action-1",
                5
            )
            .is_err()
        );
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .unwrap(),
            0
        );
    }

    #[test]
    fn execute_writes_one_note_and_replays_the_same_receipt() {
        let (mut host, mut composition) = demo_host();
        authorize(&mut host);
        let request = host
            .request_capability_action(&composition, request("action-1", "hello"))
            .unwrap();
        assert_eq!(request.state, "pending");
        let receipt = host
            .execute_capability_action(&mut composition, "action-1", 5)
            .unwrap();
        assert_eq!(receipt.outcome, "succeeded");
        assert_eq!(receipt.provider, "orbit");
        assert_eq!(receipt.item_id, "item-1");
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_notes("item-1")
                .unwrap()
                .len(),
            1
        );
        // Replay returns the stored receipt and never duplicates the effect.
        assert_eq!(
            host.execute_capability_action(&mut composition, "action-1", 6)
                .unwrap(),
            receipt
        );
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .unwrap(),
            1
        );
        let state = host
            .capability_action_receipt("action-1")
            .unwrap()
            .expect("action state");
        assert_eq!(state.request.state, "succeeded");
        assert_eq!(state.receipt.unwrap().outcome, "succeeded");
        assert!(host.capability_action_receipt("missing").unwrap().is_none());
    }

    #[test]
    fn execute_requires_an_installed_provider_before_claiming() {
        let (mut host, composition) = demo_host();
        authorize(&mut host);
        host.request_capability_action(&composition, request("action-1", "hello"))
            .unwrap();
        let mut empty = TrustedComposition::empty();
        assert_eq!(
            host.execute_capability_action(&mut empty, "action-1", 5),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(
            host.project_work_item_action("action-1")
                .unwrap()
                .unwrap()
                .state,
            "pending",
            "a missing provider must not claim the request"
        );
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .unwrap(),
            0
        );
    }

    #[test]
    fn executor_failure_records_failed_without_writing() {
        let (mut host, mut composition) = custom_host("test.failing", ExecutionOutcome::Failed);
        host.request_capability_action(&composition, custom_request("action-f"))
            .unwrap();
        let receipt = host
            .execute_capability_action(&mut composition, "action-f", 7)
            .unwrap();
        assert_eq!(receipt.outcome, "failed");
        assert_eq!(receipt.provider, "test");
        assert_eq!(
            host.project_work_item_action("action-f")
                .unwrap()
                .unwrap()
                .state,
            "failed"
        );
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .unwrap(),
            0
        );
    }

    #[test]
    fn claimed_without_receipt_is_recoverable_without_re_execution() {
        let (mut host, mut composition) = demo_host();
        authorize(&mut host);
        host.request_capability_action(&composition, request("action-lost", "hello"))
            .unwrap();
        // Simulate a process crash between the durable claim and the recorded
        // result: the request is `claimed` and no acknowledgement exists.
        host.claim_project_work_item_action("action-lost").unwrap();
        // The normal path can neither re-claim nor invent an outcome.
        assert_eq!(
            host.execute_capability_action(&mut composition, "action-lost", 5),
            Err(RepositoryError::Conflict)
        );
        assert_eq!(
            host.project_work_item_action("action-lost")
                .unwrap()
                .unwrap()
                .state,
            "claimed",
            "a refused execution must not settle or re-claim the request"
        );
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .unwrap(),
            0
        );

        let receipt = host
            .reconcile_capability_action(
                "action-lost",
                CapabilityReconcileDecision::ConfirmFailed,
                "operator",
                6,
            )
            .unwrap();
        assert_eq!(receipt.outcome, "failed");
        assert_eq!(receipt.provider, "orbit");
        assert_eq!(receipt.recorded_at_ms, 6);
        assert_eq!(
            receipt.detail,
            json!({
                "reconciled": true,
                "reconciled_by": "operator",
                "recovered_from": "claimed",
            })
        );
        assert_eq!(
            host.project_work_item_action("action-lost")
                .unwrap()
                .unwrap()
                .state,
            "failed"
        );
        // Replaying the same decision is idempotent; a different decision or a
        // different actor conflicts.
        assert_eq!(
            host.reconcile_capability_action(
                "action-lost",
                CapabilityReconcileDecision::ConfirmFailed,
                "operator",
                7,
            )
            .unwrap(),
            receipt
        );
        assert!(
            host.reconcile_capability_action(
                "action-lost",
                CapabilityReconcileDecision::ConfirmSucceeded,
                "operator",
                7,
            )
            .is_err()
        );
        assert!(
            host.reconcile_capability_action(
                "action-lost",
                CapabilityReconcileDecision::ConfirmFailed,
                "someone-else",
                7,
            )
            .is_err()
        );
        // Recovery never dispatched the executor, and a later execute returns
        // the recorded receipt instead of running it now.
        assert_eq!(
            host.execute_capability_action(&mut composition, "action-lost", 8)
                .unwrap(),
            receipt
        );
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .unwrap(),
            0,
            "a recovered claim must never write the reference effect"
        );
    }

    #[test]
    fn pending_request_cannot_be_reconciled() {
        let (mut host, composition) = demo_host();
        authorize(&mut host);
        host.request_capability_action(&composition, request("action-pending", "hello"))
            .unwrap();
        // Pending has not executed: it must go through execute, never reconcile.
        assert!(
            host.reconcile_capability_action(
                "action-pending",
                CapabilityReconcileDecision::ConfirmFailed,
                "operator",
                6,
            )
            .is_err()
        );
        assert!(
            host.capability_action_receipt("action-pending")
                .unwrap()
                .unwrap()
                .receipt
                .is_none()
        );
        assert_eq!(
            host.project_work_item_action("action-pending")
                .unwrap()
                .unwrap()
                .state,
            "pending"
        );
    }

    #[test]
    fn claimed_with_a_disagreeing_receipt_cannot_be_reconciled() {
        let (mut host, composition) = demo_host();
        authorize(&mut host);
        host.request_capability_action(&composition, request("action-stuck", "hello"))
            .unwrap();
        host.claim_project_work_item_action("action-stuck").unwrap();
        // Cannot arise through the supported path (the receipt and the state
        // commit together), but if it ever does, the recorded outcome wins.
        let recorded = CapabilityReceipt {
            command_id: "action-stuck".into(),
            item_id: "item-1".into(),
            capability: REFERENCE_CAPABILITY_ID.into(),
            action_id: "append_note".into(),
            provider: "orbit".into(),
            outcome: "succeeded".into(),
            detail: json!({"state": "succeeded"}),
            recorded_at_ms: 5,
        };
        host.authority
            .conn
            .record_capability_receipt(&recorded)
            .unwrap();
        assert!(
            host.reconcile_capability_action(
                "action-stuck",
                CapabilityReconcileDecision::ConfirmFailed,
                "operator",
                6,
            )
            .is_err()
        );
        // Even an agreeing, never-reconciled receipt is not silently confirmed.
        assert!(
            host.reconcile_capability_action(
                "action-stuck",
                CapabilityReconcileDecision::ConfirmSucceeded,
                "operator",
                6,
            )
            .is_err()
        );
        assert_eq!(
            host.authority
                .conn
                .capability_receipt("action-stuck")
                .unwrap()
                .unwrap(),
            recorded
        );
        assert_eq!(
            host.project_work_item_action("action-stuck")
                .unwrap()
                .unwrap()
                .state,
            "claimed"
        );
    }

    #[test]
    fn uncertain_outcome_can_only_be_reconciled_explicitly() {
        let (mut host, mut composition) =
            custom_host("test.uncertain", ExecutionOutcome::Uncertain);
        host.request_capability_action(&composition, custom_request("action-u"))
            .unwrap();
        let receipt = host
            .execute_capability_action(&mut composition, "action-u", 7)
            .unwrap();
        assert_eq!(receipt.outcome, "uncertain");

        // Reconciling before the executor is done is not possible: the request
        // is already resolved for execution purposes (no re-claim, no re-run).
        let reconciled = host
            .reconcile_capability_action(
                "action-u",
                CapabilityReconcileDecision::ConfirmSucceeded,
                "tester",
                8,
            )
            .unwrap();
        assert_eq!(reconciled.outcome, "succeeded");
        assert_eq!(
            reconciled.detail,
            json!({
                "reconciled": true,
                "reconciled_by": "tester",
                "recovered_from": "uncertain",
            })
        );
        assert_eq!(
            host.project_work_item_action("action-u")
                .unwrap()
                .unwrap()
                .state,
            "succeeded"
        );
        // Idempotent replay of the same decision.
        assert_eq!(
            host.reconcile_capability_action(
                "action-u",
                CapabilityReconcileDecision::ConfirmSucceeded,
                "tester",
                9,
            )
            .unwrap(),
            reconciled
        );
        // A contradictory decision conflicts, and nothing is executed again.
        assert!(
            host.reconcile_capability_action(
                "action-u",
                CapabilityReconcileDecision::ConfirmFailed,
                "tester",
                9,
            )
            .is_err()
        );
        assert!(
            composition
                .executors
                .execute_claimed(&mut host, "action-u")
                .is_err(),
            "reconciled actions must never be re-claimed"
        );
    }

    #[test]
    fn status_reports_installation_grants_and_executors() {
        let (mut host, composition) = demo_host();
        let before = host
            .capability_status(&composition, Some("project-1"))
            .unwrap();
        assert_eq!(
            before.len(),
            2,
            "reference plus habit recurrence: {before:?}"
        );
        let reference = before
            .iter()
            .find(|row| row.capability_id == REFERENCE_CAPABILITY_ID)
            .expect("reference row");
        assert!(reference.installed);
        assert!(reference.executor_registered);
        assert!(!reference.available, "not authorized yet");
        assert_eq!(reference.permission, "mutate");
        assert_eq!(reference.actions, vec!["append_note".to_owned()]);
        assert_eq!(reference.authorized_projects, Vec::<String>::new());
        assert!(
            reference
                .unavailable_reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("not_authorized"))
        );
        // Without a project filter, `available` only reports composability.
        let unfiltered = host.capability_status(&composition, None).unwrap();
        assert!(
            unfiltered
                .iter()
                .find(|row| row.capability_id == REFERENCE_CAPABILITY_ID)
                .unwrap()
                .available
        );
        authorize(&mut host);
        let after = host
            .capability_status(&composition, Some("project-1"))
            .unwrap();
        let reference = after
            .iter()
            .find(|row| row.capability_id == REFERENCE_CAPABILITY_ID)
            .unwrap();
        assert!(reference.available);
        assert_eq!(reference.authorized_projects, vec!["project-1".to_owned()]);
        assert!(reference.unavailable_reason.is_none());
        assert!(
            host.capability_status(&composition, Some("other"))
                .unwrap()
                .iter()
                .find(|row| row.capability_id == REFERENCE_CAPABILITY_ID)
                .unwrap()
                .authorized_projects
                .is_empty()
        );
    }

    #[test]
    fn guarded_transition_uses_durable_project_grants() {
        let (mut host, _) = demo_host();
        // The demo edge is unguarded, so it works without a grant, while the
        // authorized helper reads grants from the project rather than a caller.
        let item = host
            .transition_project_work_item_authorized("item-1", "closed", 0)
            .unwrap();
        assert_eq!(item.state, "closed");
    }

    #[test]
    fn composition_rejects_duplicate_or_mismatched_members() {
        let mut composition = TrustedComposition::with_builtin_reference();
        assert!(
            composition
                .register(
                    reference_capability(),
                    Box::new(OutcomeExecutor {
                        id: REFERENCE_CAPABILITY_ID,
                        outcome: ExecutionOutcome::Succeeded,
                    })
                )
                .is_err()
        );
        assert!(
            composition
                .register(
                    TrustedCapability {
                        descriptor: CapabilityDescriptor {
                            id: "other.capability".into(),
                            provider: "test".into(),
                            permission: CapabilityPermission::Read,
                            enabled: true,
                        },
                        actions: Vec::new(),
                        requires: BTreeSet::new(),
                    },
                    Box::new(OutcomeExecutor {
                        id: REFERENCE_CAPABILITY_ID,
                        outcome: ExecutionOutcome::Succeeded,
                    })
                )
                .is_err()
        );
        assert!(composition.has_executor(REFERENCE_CAPABILITY_ID));
        assert!(!composition.has_executor("other.capability"));
        assert_eq!(composition.capabilities().count(), 2);
    }
}
