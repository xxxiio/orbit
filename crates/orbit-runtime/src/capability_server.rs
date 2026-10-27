//! Server-owned capability service for the connected (self-hosted) deployment.
//!
//! # Where provider code runs, and why it cannot run here
//!
//! Provider code runs only in a separately enrolled **executor** process (for
//! example `orbit capability serve --once`). That process owns a
//! [`crate::capability_composition::TrustedComposition`] and calls the provider
//! through it after claiming a durable intent.
//!
//! This module is the server half of that split, and it deliberately has no way
//! to cross it:
//!
//! * its capability knowledge comes only from [`crate::capability_contracts`]
//!   (compile-time descriptors, action specs, input schemas, permissions and
//!   requirements) and from durable installation/grant/enrollment rows;
//! * there is no executor registry and no `TrustedComposition` value anywhere in
//!   this module or in its transport, so `provider.execute` /
//!   `execute_claimed` are not even nameable;
//! * nothing a caller sends can name a provider, a permission, an action list, a
//!   target state or a status. Those come from the contract or from the durable
//!   intent.
//!
//! So the server's job is exactly: authorize, fence, persist, and let an
//! enrolled executor claim and run the intent. A claim is never reset (not even
//! across restarts), and `uncertain` is settled only by an explicit reconcile.
use crate::AuthoritativeHost;
use crate::capability_composition::CapabilityReconcileDecision;
use crate::capability_contracts::{
    CapabilityActionAvailability, CapabilityActionSpec, CapabilityContract,
    CapabilityEffectRefusal, CapabilityRegistryView, CapabilityScope, ContractCatalog,
    ValidatedCapabilityEffect, merge_parameters, offers_for_item, valid_command_id,
    validate_capability_inputs, validate_declared_effect,
};
use orbit_application::RepositoryError;
use orbit_blueprint::ActionInvocation;
use orbit_storage::blueprint_actions::{BlueprintActionEnvelope, BlueprintActionIntent};
use orbit_storage::capability_installs::{CapabilityEffectRecord, CapabilityInstallation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The frozen refusal or acceptance of one connected capability operation.
///
/// The transport maps the variant to a status; the message is already the exact
/// client-facing text. Availability refusals keep their code so the browser can
/// branch on a value instead of parsing prose.
#[derive(Debug, PartialEq, Eq)]
pub enum CapabilityServerError {
    /// 400 with this exact message.
    BadRequest(&'static str),
    /// 400 `invalid capability action input: <reason>`.
    InvalidInput { reason: String },
    /// 400 `capability action unavailable: <code>: <reason>`.
    Unavailable { code: String, reason: String },
    /// 403 with this exact message.
    Forbidden(&'static str),
    /// 404 with this exact message.
    NotFound(&'static str),
    /// 409 with this exact message.
    Conflict(&'static str),
    /// Storage or internal failure. Never carries SQL, a path or a debug dump to
    /// a client; the transport logs it and answers opaquely.
    Storage(RepositoryError),
}

impl From<RepositoryError> for CapabilityServerError {
    fn from(error: RepositoryError) -> Self {
        Self::Storage(error)
    }
}

/// The outcome an executor reports. `uncertain` is a real, durable outcome, not
/// a failure to be retried: only an explicit reconcile may leave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityOutcome {
    Succeeded,
    Failed,
    Uncertain,
}

impl CapabilityOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Uncertain => "uncertain",
        }
    }
}

/// Body of an explicit request. It cannot name a capability, a provider, a
/// permission, an action list, a target state or a status: the action offer and
/// the durable contract decide all of that.
#[derive(Debug, Clone)]
pub struct ServerCapabilityRequest {
    pub command_id: String,
    pub item_id: String,
    pub expected_revision: u64,
    pub action_id: String,
    pub inputs: BTreeMap<String, Value>,
    pub confirm: bool,
}

/// One action of a contract, as the status document exposes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityActionContractDto {
    pub id: String,
    pub description: String,
    pub input_schema: Value,
    pub external_effect: bool,
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityExecutorDto {
    pub device_id: String,
    pub announced_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityStatusEntry {
    pub capability_id: String,
    pub name: String,
    pub permission: String,
    pub requires: Vec<String>,
    pub installed: bool,
    pub granted: bool,
    pub executors: Vec<CapabilityExecutorDto>,
    pub actions: Vec<CapabilityActionContractDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityStatusResponse {
    pub capabilities: Vec<CapabilityStatusEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityOffer {
    pub action_id: String,
    pub capability_id: String,
    pub from: String,
    pub to: String,
    pub description: String,
    pub input_schema: Value,
    pub external_effect: bool,
    pub available: bool,
    /// `null` when available, otherwise the stable availability code.
    pub code: Option<String>,
    /// `null` when available, otherwise the human reason for `code`.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityOffersResponse {
    pub item_id: String,
    pub item_revision: u64,
    pub offers: Vec<CapabilityOffer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityActionDto {
    pub command_id: String,
    pub item_id: String,
    pub item_revision: u64,
    pub action_id: String,
    pub capability_id: String,
    pub from: String,
    pub to: String,
    pub state: String,
    pub inputs: BTreeMap<String, Value>,
    pub created_at_ms: i64,
    pub executor_device_id: Option<String>,
    pub claim_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityRequestResponse {
    pub request: CapabilityActionDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityClaimResponse {
    /// The persisted intent plus its fence. Never a provider, a path or code.
    pub request: CapabilityActionDto,
    /// The frozen invocation envelope the runner replays through its own local
    /// composition. The server never interprets it.
    pub execution_plan: ActionInvocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityReceiptDto {
    pub outcome: String,
    /// The stored receipt detail document, verbatim.
    pub detail: Value,
    pub reported_at_ms: i64,
    pub reported_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityResultResponse {
    pub request: CapabilityActionDto,
    pub receipt: CapabilityReceiptDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityShowResponse {
    pub request: CapabilityActionDto,
    pub receipt: Option<CapabilityReceiptDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityListResponse {
    pub requests: Vec<CapabilityActionDto>,
}

/// One durable effect row, as a browser or an acceptance test reads it back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityEffectNoteDto {
    pub command_id: String,
    pub note: String,
    pub tag: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityEffectsResponse {
    pub item_id: String,
    pub notes: Vec<CapabilityEffectNoteDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityAnnouncementDto {
    pub capability_id: String,
    pub announced_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityAnnounceResponse {
    pub device_id: String,
    pub capabilities: Vec<CapabilityAnnouncementDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityExecutorRosterEntry {
    pub device_id: String,
    pub capability_id: String,
    pub announced_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityExecutorRosterResponse {
    pub executors: Vec<CapabilityExecutorRosterEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityInstallResponse {
    pub capability_id: String,
    pub provider: String,
    pub permission: String,
    pub installed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityGrantResponse {
    pub capability_id: String,
    pub project_id: String,
    pub granted_by: String,
    pub granted_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityRevokeResponse {
    pub capability_id: String,
    pub project_id: String,
    pub revoked: bool,
}

/// The contract catalog plus the enrollment fact. Only enrolled *active* devices
/// count as executors; a registered provider in some other process never does.
struct EnrolledContracts<'a> {
    catalog: &'a ContractCatalog,
    announced: BTreeSet<String>,
}

impl CapabilityRegistryView for EnrolledContracts<'_> {
    fn capability_contract(&self, capability_id: &str) -> Option<CapabilityContract> {
        self.catalog.contract(capability_id).cloned()
    }

    fn capability_enabled(&self, capability_id: &str) -> bool {
        self.catalog.contract(capability_id).is_some()
    }

    fn executor_available(&self, capability_id: &str) -> bool {
        self.announced.contains(capability_id)
    }

    fn enabled_capabilities(&self) -> BTreeSet<String> {
        self.catalog.capability_catalog().available()
    }
}

fn enrolled_contracts<'a>(
    host: &AuthoritativeHost,
    catalog: &'a ContractCatalog,
) -> Result<EnrolledContracts<'a>, CapabilityServerError> {
    Ok(EnrolledContracts {
        catalog,
        announced: host.authority.conn.announced_capability_ids()?,
    })
}

/// The durable installed set, intersected with the contracts this build knows.
/// Publication uses exactly this set, so a manifest cannot be published by
/// declaring its own capabilities in the request body.
pub fn installed_capability_ids(
    host: &AuthoritativeHost,
) -> Result<BTreeSet<String>, RepositoryError> {
    let enabled = ContractCatalog::builtin().capability_catalog().available();
    let installed: BTreeSet<String> = host
        .authority
        .conn
        .capability_installations()?
        .into_iter()
        .map(|installation| installation.capability_id)
        .collect();
    Ok(installed.intersection(&enabled).cloned().collect())
}

/// Record that an authenticated device can execute these capabilities.
///
/// Re-announcing is idempotent and refreshes the timestamp; an id the catalog
/// does not know is refused rather than stored as an inert row.
pub fn announce_executors(
    host: &mut AuthoritativeHost,
    device_id: &str,
    capability_ids: &[String],
    at_ms: i64,
) -> Result<CapabilityAnnounceResponse, CapabilityServerError> {
    if !host.device_active(device_id).map_err(storage_error)? {
        return Err(CapabilityServerError::Forbidden(
            "executor credential required",
        ));
    }
    let catalog = ContractCatalog::builtin();
    let mut capabilities = Vec::new();
    for capability_id in capability_ids {
        if catalog.contract(capability_id).is_none() {
            return Err(CapabilityServerError::BadRequest("unknown capability"));
        }
    }
    for capability_id in capability_ids {
        let announcement = host
            .authority
            .conn
            .announce_capability_executor(device_id, capability_id, at_ms)
            .map_err(|error| conflict_or_storage(error, "capability enrollment conflict"))?;
        capabilities.push(announcement);
    }
    capabilities.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));
    Ok(CapabilityAnnounceResponse {
        device_id: device_id.to_owned(),
        capabilities: capabilities
            .into_iter()
            .map(|announcement| CapabilityAnnouncementDto {
                capability_id: announcement.capability_id,
                announced_at_ms: announcement.announced_at_ms,
            })
            .collect(),
    })
}

/// The roster of active enrolled executors, ordered by device then capability.
pub fn executor_roster(
    host: &AuthoritativeHost,
) -> Result<CapabilityExecutorRosterResponse, CapabilityServerError> {
    Ok(CapabilityExecutorRosterResponse {
        executors: host
            .authority
            .conn
            .capability_executor_announcements()?
            .into_iter()
            .map(|announcement| CapabilityExecutorRosterEntry {
                device_id: announcement.device_id,
                capability_id: announcement.capability_id,
                announced_at_ms: announcement.announced_at_ms,
            })
            .collect(),
    })
}

/// Contracts joined with durable installation, grants and enrollment. Every
/// field is a durable fact; a registered provider is never consulted.
///
/// `granted` is scoped to `project_id` when one is supplied. Without a project
/// filter it answers "authorized for at least one project", which is the only
/// honest reading of a project-scoped grant in a workspace-wide document.
pub fn status(
    host: &AuthoritativeHost,
    project_id: Option<&str>,
) -> Result<CapabilityStatusResponse, CapabilityServerError> {
    let conn = &host.authority.conn;
    let catalog = ContractCatalog::builtin();
    let installations: BTreeMap<String, CapabilityInstallation> = conn
        .capability_installations()?
        .into_iter()
        .map(|row| (row.capability_id.clone(), row))
        .collect();
    let grants = conn.capability_grants(project_id)?;
    let mut ids: BTreeSet<String> = catalog
        .contracts()
        .map(|contract| contract.capability_id.clone())
        .collect();
    ids.extend(installations.keys().cloned());
    let mut capabilities = Vec::new();
    for capability_id in ids {
        let contract = catalog.contract(&capability_id);
        let installation = installations.get(&capability_id);
        let (name, permission, requires, actions) = contract.map_or_else(
            || {
                // Installed by an older build the catalog no longer describes:
                // report the installation's own contract rather than inventing
                // a name or a permission.
                let row = installation;
                (
                    capability_id.clone(),
                    row.map(|row| row.permission.clone()).unwrap_or_default(),
                    row.and_then(|row| {
                        serde_json::from_str::<Vec<String>>(&row.requires_json).ok()
                    })
                    .unwrap_or_default(),
                    row.and_then(|row| {
                        serde_json::from_str::<Vec<CapabilityActionSpec>>(&row.actions_json).ok()
                    })
                    .unwrap_or_default(),
                )
            },
            |contract| {
                (
                    contract.name.clone(),
                    contract.permission.clone(),
                    contract.requires.clone(),
                    contract.actions.clone(),
                )
            },
        );
        let executors = conn
            .capability_executor_announcements_for(&capability_id)?
            .into_iter()
            .map(|announcement| CapabilityExecutorDto {
                device_id: announcement.device_id,
                announced_at_ms: announcement.announced_at_ms,
            })
            .collect();
        capabilities.push(CapabilityStatusEntry {
            granted: grants
                .iter()
                .any(|grant| grant.capability_id == capability_id),
            capability_id,
            name,
            permission,
            requires,
            installed: installation.is_some(),
            executors,
            actions: actions.iter().map(action_contract_dto).collect(),
        });
    }
    Ok(CapabilityStatusResponse { capabilities })
}

fn action_contract_dto(action: &CapabilityActionSpec) -> CapabilityActionContractDto {
    CapabilityActionContractDto {
        id: action.action_id.clone(),
        description: action.description.clone(),
        input_schema: action.input_schema.clone(),
        external_effect: action.external_effect,
        requires: action.requires.clone(),
    }
}

/// The actions the bound workflow offers from the item's current state.
///
/// `executor_missing` here means no active device announced the capability — a
/// durable fact about enrollment, not a process-local registry lookup.
pub fn offers(
    host: &AuthoritativeHost,
    item_id: &str,
) -> Result<CapabilityOffersResponse, CapabilityServerError> {
    let conn = &host.authority.conn;
    let item = conn
        .blueprint_work_item(item_id)?
        .ok_or(CapabilityServerError::NotFound("work item not found"))?;
    let catalog = ContractCatalog::builtin();
    let registry = enrolled_contracts(host, &catalog)?;
    let offers = offers_for_item(conn, &item, &registry, CapabilityScope::Connected)?;
    Ok(CapabilityOffersResponse {
        item_id: item.id,
        item_revision: item.revision,
        offers: offers.iter().map(offer_dto).collect(),
    })
}

fn offer_dto(offer: &crate::capability_contracts::OfferedCapabilityAction) -> CapabilityOffer {
    let (available, code, reason) = match &offer.availability {
        CapabilityActionAvailability::Available => (true, None, None),
        CapabilityActionAvailability::Unavailable { code, reason } => {
            (false, Some(code.clone()), Some(reason.clone()))
        }
    };
    CapabilityOffer {
        action_id: offer.action_id.clone(),
        capability_id: offer.capability.clone(),
        from: offer.from.clone(),
        to: offer.to.clone(),
        description: offer.description.clone(),
        input_schema: offer.input_schema.clone(),
        external_effect: offer.external_effect,
        available,
        code,
        reason,
    }
}

/// Persist an authorized, fenced intent. The order is the local one: envelope,
/// item/revision, offer availability, declared inputs only, schema/length, then
/// the durable project grant — and the storage writer re-checks all of it.
pub fn request(
    host: &mut AuthoritativeHost,
    body: ServerCapabilityRequest,
) -> Result<CapabilityRequestResponse, CapabilityServerError> {
    if !valid_command_id(&body.command_id) || body.item_id.trim().is_empty() {
        return Err(CapabilityServerError::Conflict(
            "capability request conflict",
        ));
    }
    let item = host
        .authority
        .conn
        .blueprint_work_item(&body.item_id)?
        .ok_or(CapabilityServerError::NotFound("work item not found"))?;
    if item.revision != body.expected_revision {
        return Err(CapabilityServerError::Conflict(
            "capability request conflict",
        ));
    }
    let catalog = ContractCatalog::builtin();
    let registry = enrolled_contracts(host, &catalog)?;
    let offers = offers_for_item(
        &host.authority.conn,
        &item,
        &registry,
        CapabilityScope::Connected,
    )?;
    let offer = offers
        .iter()
        .find(|offer| offer.action_id == body.action_id)
        .ok_or(CapabilityServerError::Conflict(
            "capability request conflict",
        ))?;
    if let CapabilityActionAvailability::Unavailable { code, reason } = &offer.availability {
        return Err(CapabilityServerError::Unavailable {
            code: code.clone(),
            reason: reason.clone(),
        });
    }
    let contract = catalog
        .contract(&offer.capability)
        .ok_or(CapabilityServerError::BadRequest("unknown capability"))?;
    let spec = contract
        .action(&offer.action_id)
        .ok_or(CapabilityServerError::Conflict(
            "capability request conflict",
        ))?;
    // Confirmation is part of consent, not of availability: an effectful or
    // destructive action is never recorded on an unconfirmed request.
    if contract.requires_confirmation(spec) && !body.confirm {
        return Err(CapabilityServerError::BadRequest(
            "capability action requires confirmation",
        ));
    }
    // A caller may fill in inputs the capability's own schema declares; it can
    // never invent one. Unknown keys are refused before schema validation so the
    // refusal names the actual client mistake.
    let allowed_inputs: BTreeSet<String> = spec
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default();
    for key in body.inputs.keys() {
        if !allowed_inputs.contains(key) && offer.declared_parameters.get(key).is_none() {
            return Err(CapabilityServerError::InvalidInput {
                reason: format!("undeclared input: {key}"),
            });
        }
    }
    let parameters = merge_parameters(offer, &body.inputs);
    validate_capability_inputs(&spec.input_schema, &parameters)
        .map_err(|reason| CapabilityServerError::InvalidInput { reason })?;
    // The project grant is the only thing that authorizes execution, and it is
    // re-read here rather than trusted from the offer the caller saw.
    let grants = host
        .authority
        .conn
        .authorized_capabilities(&item.project_id)?;
    let stored = host
        .authority
        .conn
        .request_blueprint_action_on_transition_allowing(
            BlueprintActionIntent {
                item_id: &body.item_id,
                expected_revision: body.expected_revision,
                from: &offer.from,
                action_id: &body.action_id,
                command_id: &body.command_id,
            },
            &offer.to,
            &body.inputs,
            &allowed_inputs,
            &catalog.capability_catalog(),
            &grants,
        )
        .map_err(request_error)?;
    let envelope = host
        .authority
        .conn
        .blueprint_action_envelope(&stored.command_id)?
        .ok_or(CapabilityServerError::Conflict(
            "capability request conflict",
        ))?;
    Ok(CapabilityRequestResponse {
        request: action_dto(&envelope)?,
    })
}

/// Claim one pending intent for an enrolled executor.
///
/// The claim is a single `pending -> claimed` write carrying the device and the
/// claim token. A replay by the same device with the same token returns the same
/// claim; any other contender is refused. Nothing resets a claim, not even a
/// restart, so a runner can never run the same intent twice.
pub fn claim(
    host: &mut AuthoritativeHost,
    device_id: &str,
    command_id: &str,
    claim_token: &str,
) -> Result<CapabilityClaimResponse, CapabilityServerError> {
    if !valid_command_id(command_id) || !valid_claim_token(claim_token) {
        return Err(CapabilityServerError::BadRequest(
            "invalid capability action claim",
        ));
    }
    let envelope = host
        .authority
        .conn
        .blueprint_action_envelope(command_id)?
        .ok_or(CapabilityServerError::NotFound(
            "capability action not found",
        ))?;
    require_announced(host, device_id, &envelope.request.capability)?;
    match envelope.request.state.as_str() {
        "pending" => {}
        "claimed"
            if envelope.executor_device_id.as_deref() == Some(device_id)
                && envelope.claim_token.as_deref() == Some(claim_token) =>
        {
            return claim_response(&envelope);
        }
        "claimed" => {
            return Err(CapabilityServerError::Conflict(
                "capability action already claimed",
            ));
        }
        _ => {
            return Err(CapabilityServerError::Conflict(
                "capability action already settled",
            ));
        }
    }
    if host
        .authority
        .conn
        .claim_token_in_use(claim_token, command_id)?
    {
        return Err(CapabilityServerError::Conflict(
            "capability claim token already used",
        ));
    }
    let claimed = host
        .authority
        .conn
        .claim_blueprint_action_for_executor(device_id, command_id, claim_token)
        .map_err(|error| match error {
            // Another executor won the same pending intent.
            RepositoryError::Conflict => {
                CapabilityServerError::Conflict("capability action already claimed")
            }
            other => CapabilityServerError::Storage(other),
        })?;
    claim_response(&claimed)
}

fn claim_response(
    envelope: &BlueprintActionEnvelope,
) -> Result<CapabilityClaimResponse, CapabilityServerError> {
    Ok(CapabilityClaimResponse {
        request: action_dto(envelope)?,
        execution_plan: execution_plan(envelope)?,
    })
}

fn execution_plan(
    envelope: &BlueprintActionEnvelope,
) -> Result<ActionInvocation, CapabilityServerError> {
    serde_json::from_str(&envelope.request.plan_json).map_err(corrupt_plan)
}

/// Record what the claiming executor reported.
///
/// The state transition, the receipt and — when the capability declares one and
/// the executor reported it — the **durable effect** commit in one savepoint, so
/// a settled intent cannot be left without its receipt and an effect cannot be
/// recorded without the settlement that states it. A replay of the same outcome
/// returns the original receipt; a different outcome on a settled intent is
/// refused. `uncertain` is stored as-is and never retried.
///
/// The effect is validated against the capability's **compile-time declaration**.
/// Nothing in the request can name an effect kind, a target, a path, a table or a
/// provider: the payload either matches the declared schema or is refused.
#[allow(clippy::too_many_arguments)]
pub fn result(
    host: &mut AuthoritativeHost,
    device_id: &str,
    command_id: &str,
    claim_token: &str,
    outcome: CapabilityOutcome,
    detail: &str,
    effect: Option<&Value>,
    at_ms: i64,
) -> Result<CapabilityResultResponse, CapabilityServerError> {
    if detail.chars().count() > 2_000 {
        return Err(CapabilityServerError::BadRequest(
            "invalid capability action result",
        ));
    }
    let envelope = host
        .authority
        .conn
        .blueprint_action_envelope(command_id)?
        .ok_or(CapabilityServerError::NotFound(
            "capability action not found",
        ))?;
    require_announced(host, device_id, &envelope.request.capability)?;
    if envelope.executor_device_id.as_deref() != Some(device_id)
        || envelope.claim_token.as_deref() != Some(claim_token)
    {
        return Err(CapabilityServerError::Conflict(
            "capability action already claimed",
        ));
    }
    // The declaration is compile-time data for this capability id. An installed
    // row from an older build that the catalog no longer describes therefore
    // declares no effect, and cannot be handed one.
    let declared = ContractCatalog::builtin()
        .contract(&envelope.request.capability)
        .and_then(|contract| contract.effect.clone());
    let (reported, statement) = match effect {
        Some(payload) => {
            let validated =
                validate_declared_effect(declared.as_ref(), &envelope.request.item_id, payload)
                    .map_err(|refusal: CapabilityEffectRefusal| {
                        CapabilityServerError::BadRequest(refusal.message())
                    })?;
            (Some(validated), None)
        }
        None => {
            // Absent is allowed, and says so honestly: the receipt is the record
            // of what the authority did, including "nothing".
            let reason = if declared.is_some() {
                "no effect declared by the executor"
            } else {
                "the capability's contract declares no effect"
            };
            (None, Some(reason))
        }
    };
    // Provider identity comes from the durable installation, never from the
    // executor's report.
    let provider = host
        .authority
        .conn
        .capability_installation(&envelope.request.capability)?
        .ok_or(CapabilityServerError::Conflict(
            "capability action already claimed",
        ))?
        .provider;
    let statement = match (&reported, statement) {
        // The persisted identity of the effect is its own row key plus the kind:
        // (kind, item_id, command_id). That is what makes the receipt a durable
        // statement that the authority changed rather than a claim about a
        // scratch database.
        (Some(effect), _) => serde_json::json!({
            "applied": true,
            "kind": effect.kind,
            "item_id": effect.item_id,
            "command_id": command_id,
        }),
        (None, Some(reason)) => serde_json::json!({
            "applied": false,
            "reason": reason,
        }),
        (None, None) => serde_json::json!({ "applied": false }),
    };
    let stored_detail = serde_json::json!({
        "state": outcome.as_str(),
        "detail": detail,
        "reported_by": device_id,
        "effect": statement,
    });
    let record = reported.as_ref().map(effect_record);
    host.authority
        .conn
        .report_blueprint_action_result(
            command_id,
            outcome.as_str(),
            &provider,
            &stored_detail,
            record.as_ref(),
            at_ms,
        )
        .map_err(|error| match error {
            RepositoryError::Conflict => {
                CapabilityServerError::Conflict("capability action already settled")
            }
            other => CapabilityServerError::Storage(other),
        })?;
    let envelope = host
        .authority
        .conn
        .blueprint_action_envelope(command_id)?
        .ok_or(CapabilityServerError::NotFound(
            "capability action not found",
        ))?;
    let receipt = host.authority.conn.capability_receipt(command_id)?.ok_or(
        CapabilityServerError::Storage(RepositoryError::CorruptData {
            detail: "settled capability action has no receipt".to_owned(),
        }),
    )?;
    Ok(CapabilityResultResponse {
        request: action_dto(&envelope)?,
        receipt: receipt_dto(&receipt, &envelope),
    })
}

/// The storage-side view of a validated effect. It borrows from the validated
/// value, so an unvalidated document cannot reach the writer.
fn effect_record(effect: &ValidatedCapabilityEffect) -> CapabilityEffectRecord<'_> {
    CapabilityEffectRecord {
        kind: &effect.kind,
        item_id: &effect.item_id,
        note: &effect.note,
        tag: effect.tag.as_deref(),
    }
}

/// The durable effects recorded for one work item, in creation order.
///
/// This is a read of rows the authority already committed: it is how a browser
/// or an acceptance test proves the durable result without reading the database.
pub fn effects(
    host: &AuthoritativeHost,
    item_id: &str,
) -> Result<CapabilityEffectsResponse, CapabilityServerError> {
    if host.authority.conn.blueprint_work_item(item_id)?.is_none() {
        return Err(CapabilityServerError::NotFound("work item not found"));
    }
    let notes = host.authority.conn.reference_work_item_notes(item_id)?;
    Ok(CapabilityEffectsResponse {
        item_id: item_id.to_owned(),
        notes: notes
            .into_iter()
            .map(|note| CapabilityEffectNoteDto {
                command_id: note.command_id,
                note: note.note,
                tag: note.tag,
                created_at_ms: note.created_at_ms,
            })
            .collect(),
    })
}

/// One intent and its receipt, as an operator or browser reads them.
pub fn show(
    host: &AuthoritativeHost,
    command_id: &str,
) -> Result<CapabilityShowResponse, CapabilityServerError> {
    let envelope = host
        .authority
        .conn
        .blueprint_action_envelope(command_id)?
        .ok_or(CapabilityServerError::NotFound(
            "capability action not found",
        ))?;
    let receipt = host.authority.conn.capability_receipt(command_id)?;
    Ok(CapabilityShowResponse {
        request: action_dto(&envelope)?,
        receipt: receipt
            .as_ref()
            .map(|receipt| receipt_dto(receipt, &envelope)),
    })
}

/// Durable intents, ordered by creation moment then command id.
pub fn list(
    host: &AuthoritativeHost,
    item_id: Option<&str>,
    state: Option<&str>,
) -> Result<CapabilityListResponse, CapabilityServerError> {
    let envelopes = host
        .authority
        .conn
        .blueprint_action_envelopes(item_id, state, LIST_LIMIT)?;
    let mut requests = Vec::with_capacity(envelopes.len());
    for envelope in &envelopes {
        requests.push(action_dto(envelope)?);
    }
    Ok(CapabilityListResponse { requests })
}

const LIST_LIMIT: usize = 200;

/// Explicitly settle a claimed or uncertain intent under the authenticated
/// operator's identity. This never dispatches anything, and `pending` is
/// refused because it has not executed yet.
pub fn reconcile(
    host: &mut AuthoritativeHost,
    device_id: &str,
    command_id: &str,
    decision: CapabilityReconcileDecision,
    detail: &str,
    at_ms: i64,
) -> Result<CapabilityShowResponse, CapabilityServerError> {
    if detail.chars().count() > 1_000 {
        return Err(CapabilityServerError::BadRequest(
            "invalid capability action reconciliation",
        ));
    }
    let envelope = host
        .authority
        .conn
        .blueprint_action_envelope(command_id)?
        .ok_or(CapabilityServerError::NotFound(
            "capability action not found",
        ))?;
    if envelope.request.state == "pending" {
        return Err(CapabilityServerError::Conflict(
            "capability action cannot be reconciled",
        ));
    }
    host.reconcile_capability_action_with_note(command_id, decision, device_id, detail, at_ms)
        .map_err(|error| match error {
            RepositoryError::Conflict => {
                CapabilityServerError::Conflict("capability action cannot be reconciled")
            }
            other => CapabilityServerError::Storage(other),
        })?;
    show(host, command_id)
}

/// Install a catalog contract. A re-install of the identical contract replays; a
/// changed contract conflicts rather than silently rewriting an immutable row.
pub fn install(
    host: &mut AuthoritativeHost,
    capability_id: &str,
    at_ms: i64,
) -> Result<CapabilityInstallResponse, CapabilityServerError> {
    let contract = ContractCatalog::builtin()
        .contract(capability_id)
        .cloned()
        .ok_or(CapabilityServerError::BadRequest("unknown capability"))?;
    let installation =
        contract.installation(crate::capability_contracts::BUILTIN_PROVIDER, at_ms)?;
    let stored = host
        .authority
        .conn
        .install_capability(&installation)
        .map_err(|error| match error {
            RepositoryError::Conflict => {
                CapabilityServerError::Conflict("capability contract changed")
            }
            other => CapabilityServerError::Storage(other),
        })?;
    Ok(CapabilityInstallResponse {
        capability_id: stored.capability_id,
        provider: stored.provider,
        permission: stored.permission,
        installed_at_ms: stored.installed_at_ms,
    })
}

/// Create the only authorization that permits execution: a durable
/// `(capability, project)` grant. Installation and an existing project are both
/// required, and `confirm` is mandatory because this is the consent step.
pub fn authorize(
    host: &mut AuthoritativeHost,
    capability_id: &str,
    project_id: &str,
    actor: &str,
    confirm: bool,
    at_ms: i64,
) -> Result<CapabilityGrantResponse, CapabilityServerError> {
    if ContractCatalog::builtin().contract(capability_id).is_none() {
        return Err(CapabilityServerError::BadRequest("unknown capability"));
    }
    if !confirm {
        return Err(CapabilityServerError::BadRequest(
            "capability authorization requires confirmation",
        ));
    }
    let grant = host
        .authority
        .conn
        .grant_capability(capability_id, project_id, actor, at_ms)
        .map_err(|error| match error {
            RepositoryError::Conflict => {
                CapabilityServerError::Conflict("capability authorization conflict")
            }
            other => CapabilityServerError::Storage(other),
        })?;
    Ok(CapabilityGrantResponse {
        capability_id: grant.capability_id,
        project_id: grant.project_id,
        granted_by: grant.granted_by,
        granted_at_ms: grant.granted_at_ms,
    })
}

/// Remove a project grant. Idempotent: revoking a grant that does not exist is
/// not an error, because the desired end state is already true.
pub fn revoke(
    host: &mut AuthoritativeHost,
    capability_id: &str,
    project_id: &str,
) -> Result<CapabilityRevokeResponse, CapabilityServerError> {
    let revoked = host
        .authority
        .conn
        .revoke_capability(capability_id, project_id)?;
    Ok(CapabilityRevokeResponse {
        capability_id: capability_id.to_owned(),
        project_id: project_id.to_owned(),
        revoked,
    })
}

fn require_announced(
    host: &AuthoritativeHost,
    device_id: &str,
    capability_id: &str,
) -> Result<(), CapabilityServerError> {
    if host
        .authority
        .conn
        .capability_announced(device_id, capability_id)?
    {
        Ok(())
    } else {
        Err(CapabilityServerError::Forbidden("capability not announced"))
    }
}

fn action_dto(
    envelope: &BlueprintActionEnvelope,
) -> Result<CapabilityActionDto, CapabilityServerError> {
    Ok(CapabilityActionDto {
        command_id: envelope.request.command_id.clone(),
        item_id: envelope.request.item_id.clone(),
        item_revision: envelope.request.item_revision,
        action_id: envelope.request.action_id.clone(),
        capability_id: envelope.request.capability.clone(),
        from: envelope.from_state.clone(),
        to: envelope.to_state.clone(),
        state: envelope.request.state.clone(),
        inputs: execution_plan(envelope)?.parameters,
        created_at_ms: envelope.created_at_ms,
        executor_device_id: envelope.executor_device_id.clone(),
        claim_token: envelope.claim_token.clone(),
    })
}

fn receipt_dto(
    receipt: &orbit_storage::capability_installs::CapabilityReceipt,
    envelope: &BlueprintActionEnvelope,
) -> CapabilityReceiptDto {
    // Who reported the outcome: an operator's explicit reconciliation, or the
    // executor device that held the claim. Both are durable identities.
    let reported_by = receipt
        .detail
        .get("reconciled_by")
        .or_else(|| receipt.detail.get("reported_by"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| envelope.executor_device_id.clone())
        .unwrap_or_default();
    CapabilityReceiptDto {
        outcome: receipt.outcome.clone(),
        detail: receipt.detail.clone(),
        reported_at_ms: receipt.recorded_at_ms,
        reported_by,
    }
}

/// A claim token is evidence of exactly one claim: it becomes part of a durable
/// uniqueness fence, so it must be a bounded, printable identifier.
fn valid_claim_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}

fn corrupt_plan(error: serde_json::Error) -> CapabilityServerError {
    CapabilityServerError::Storage(RepositoryError::CorruptData {
        detail: error.to_string(),
    })
}

fn storage_error(error: crate::AuthorityError) -> CapabilityServerError {
    CapabilityServerError::Storage(match error {
        crate::AuthorityError::Application(orbit_application::ApplicationError::Repository(
            repository,
        )) => repository,
        // Only used to classify a device-liveness read; the text never reaches a
        // client because the transport logs storage failures opaquely.
        other => RepositoryError::Other(format!("{other:?}")),
    })
}

/// A storage conflict on an enrollment write is the client's conflict, not an
/// internal failure; anything else stays opaque.
fn conflict_or_storage(error: RepositoryError, message: &'static str) -> CapabilityServerError {
    match error {
        RepositoryError::Conflict => CapabilityServerError::Conflict(message),
        other => CapabilityServerError::Storage(other),
    }
}

fn request_error(error: RepositoryError) -> CapabilityServerError {
    match error {
        // Any refusal from the storage planner is a fence problem for this
        // caller: a stale revision, a duplicate intent or an unauthorized edge.
        RepositoryError::Conflict => CapabilityServerError::Conflict("capability request conflict"),
        RepositoryError::Other(_) => CapabilityServerError::Conflict("capability request conflict"),
        other => CapabilityServerError::Storage(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability_composition::REFERENCE_CAPABILITY_ID;
    use orbit_application::ProjectRepository;
    use orbit_blueprint::Blueprint;
    use orbit_domain::{Project, ProjectId};
    use serde_json::json;

    const DEMO_MANIFEST: &str =
        include_str!("../../../blueprints/examples/capability-reference.json");
    const EXECUTOR_TOKEN: &str = "executor-secret-0123456789abcdefghij";
    const OTHER_TOKEN: &str = "other-secret-0123456789abcdefghij";
    const OPERATOR_TOKEN: &str = "operator-secret-0123456789abcdefghij";

    fn demo_manifest() -> Blueprint {
        Blueprint::from_json(
            DEMO_MANIFEST,
            &BTreeSet::from([REFERENCE_CAPABILITY_ID.to_owned()]),
        )
        .expect("demo manifest")
    }

    /// Install -> publish -> bind -> enroll, the only order publication allows.
    fn host() -> AuthoritativeHost {
        let mut host = AuthoritativeHost::open(":memory:").expect("host");
        let project =
            Project::new(ProjectId::new("project-1").unwrap(), "Reference").expect("project");
        ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
        insert_reference_install(&mut host);
        let installed = installed_capability_ids(&host).expect("installed set");
        host.publish_project_blueprint(&demo_manifest(), &installed)
            .expect("publish");
        host.activate_project_blueprint("project-1", "capability-reference", 1)
            .expect("bind");
        for item in ["item-1", "item-2"] {
            host.create_project_work_item(item, "project-1", "note-task", BTreeMap::new())
                .expect("item");
        }
        host.enroll_device("exec-1", EXECUTOR_TOKEN)
            .expect("device");
        host.enroll_device("exec-2", OTHER_TOKEN).expect("device");
        host.enroll_device("operator", OPERATOR_TOKEN)
            .expect("device");
        host
    }

    /// Install through the connected path; the local fixture keeps the same
    /// durable row by going through the very same contract type.
    fn insert_reference_install(host: &mut AuthoritativeHost) {
        let contract = ContractCatalog::builtin()
            .contract(REFERENCE_CAPABILITY_ID)
            .cloned()
            .expect("reference contract");
        host.authority
            .conn
            .install_capability(&contract.installation("orbit", 1).expect("installation"))
            .expect("install");
    }

    fn grant(host: &mut AuthoritativeHost) {
        host.authorize_trusted_capability(REFERENCE_CAPABILITY_ID, "project-1", "operator", 3)
            .expect("grant");
    }

    fn announce(host: &mut AuthoritativeHost, device_id: &str) {
        announce_executors(host, device_id, &[REFERENCE_CAPABILITY_ID.to_owned()], 2)
            .expect("announce");
    }

    fn body(command_id: &str, note: &str) -> ServerCapabilityRequest {
        ServerCapabilityRequest {
            command_id: command_id.to_owned(),
            item_id: "item-1".to_owned(),
            expected_revision: 0,
            action_id: "append_note".to_owned(),
            inputs: BTreeMap::from([("note".to_owned(), json!(note))]),
            confirm: true,
        }
    }

    fn unavailable_code(error: &CapabilityServerError) -> &str {
        match error {
            CapabilityServerError::Unavailable { code, .. } => code,
            other => panic!("expected an availability refusal, got {other:?}"),
        }
    }

    #[test]
    fn enrollment_is_idempotent_refreshes_and_refuses_unknown_capabilities() {
        let mut host = host();
        let first = announce_executors(
            &mut host,
            "exec-1",
            &[REFERENCE_CAPABILITY_ID.to_owned()],
            10,
        )
        .expect("announce");
        assert_eq!(first.device_id, "exec-1");
        assert_eq!(first.capabilities.len(), 1);
        assert_eq!(first.capabilities[0].announced_at_ms, 10);
        let second = announce_executors(
            &mut host,
            "exec-1",
            &[REFERENCE_CAPABILITY_ID.to_owned()],
            20,
        )
        .expect("re-announce");
        assert_eq!(
            second.capabilities[0].announced_at_ms, 20,
            "re-announcing refreshes the announcement moment"
        );
        assert_eq!(executor_roster(&host).expect("roster").executors.len(), 1);
        assert_eq!(
            announce_executors(&mut host, "exec-1", &["orbit.nope".to_owned()], 20),
            Err(CapabilityServerError::BadRequest("unknown capability"))
        );
        // A revoked device cannot enroll.
        host.revoke_device("exec-1").expect("revoke");
        assert!(
            announce_executors(
                &mut host,
                "exec-1",
                &[REFERENCE_CAPABILITY_ID.to_owned()],
                30
            )
            .is_err()
        );
        assert!(
            executor_roster(&host).expect("roster").executors.is_empty(),
            "a revoked device's announcement is not reported as an executor"
        );
    }

    #[test]
    fn offers_report_executor_missing_from_enrollment_not_from_a_registry() {
        let mut host = host();
        let document = offers(&host, "item-1").expect("offers");
        assert_eq!(document.item_revision, 0);
        assert_eq!(document.offers.len(), 1);
        let offer = &document.offers[0];
        assert_eq!(offer.action_id, "append_note");
        assert_eq!(offer.capability_id, REFERENCE_CAPABILITY_ID);
        assert_eq!(offer.from, "open");
        assert_eq!(offer.to, "closed");
        assert!(!offer.available);
        assert_eq!(offer.code.as_deref(), Some("executor_missing"));
        assert_eq!(
            offer.reason.as_deref(),
            Some("no active enrolled executor announced orbit.reference.work-item-note")
        );

        announce(&mut host, "exec-1");
        let offer = offers(&host, "item-1").expect("offers");
        assert_eq!(offer.offers[0].code.as_deref(), Some("not_authorized"));
        grant(&mut host);
        let offer = offers(&host, "item-1").expect("offers");
        assert!(offer.offers[0].available);
        assert!(offer.offers[0].code.is_none());
        assert!(offer.offers[0].reason.is_none());

        // Revocation of the grant reinstates the same durable refusal.
        host.revoke_trusted_capability(REFERENCE_CAPABILITY_ID, "project-1")
            .expect("revoke");
        let offer = offers(&host, "item-1").expect("offers");
        assert_eq!(offer.offers[0].code.as_deref(), Some("not_authorized"));
        assert_eq!(
            offers(&host, "missing"),
            Err(CapabilityServerError::NotFound("work item not found"))
        );
    }

    #[test]
    fn status_joins_contracts_installations_grants_and_enrollment() {
        let mut host = host();
        let snapshot = status(&host, Some("project-1")).expect("status");
        assert_eq!(snapshot.capabilities.len(), 2, "reference plus habit");
        let reference = snapshot
            .capabilities
            .iter()
            .find(|row| row.capability_id == REFERENCE_CAPABILITY_ID)
            .expect("reference row");
        assert_eq!(reference.name, "Reference notes");
        assert_eq!(reference.permission, "mutate");
        assert!(reference.installed);
        assert!(!reference.granted);
        assert!(reference.executors.is_empty());
        assert_eq!(reference.actions.len(), 1);
        assert_eq!(reference.actions[0].id, "append_note");
        assert_eq!(
            reference.actions[0].input_schema["required"],
            json!(["note"])
        );
        let habit = snapshot
            .capabilities
            .iter()
            .find(|row| row.capability_id == crate::habit_capability::CAPABILITY_ID)
            .expect("habit row");
        assert!(!habit.installed, "registration never installs");

        announce(&mut host, "exec-1");
        grant(&mut host);
        let snapshot = status(&host, Some("project-1")).expect("status");
        let reference = snapshot
            .capabilities
            .iter()
            .find(|row| row.capability_id == REFERENCE_CAPABILITY_ID)
            .expect("reference row");
        assert!(reference.granted);
        assert_eq!(reference.executors.len(), 1);
        assert_eq!(reference.executors[0].device_id, "exec-1");
        // A different project is not granted.
        let other = status(&host, Some("project-2")).expect("status");
        assert!(
            !other
                .capabilities
                .iter()
                .find(|row| row.capability_id == REFERENCE_CAPABILITY_ID)
                .expect("reference row")
                .granted
        );
    }

    #[test]
    fn request_refuses_each_gate_in_the_documented_order() {
        let mut host = host();
        // Uninstalled: the durable installation is the first gate after the
        // contract must exist at all.
        host.uninstall_trusted_capability(REFERENCE_CAPABILITY_ID)
            .expect("uninstall");
        let error = request(&mut host, body("a-0", "hi")).unwrap_err();
        assert_eq!(unavailable_code(&error), "capability_not_installed");
        insert_reference_install(&mut host);
        announce(&mut host, "exec-1");
        // Unauthorized.
        let error = request(&mut host, body("a-1", "hi")).unwrap_err();
        assert_eq!(unavailable_code(&error), "not_authorized");
        grant(&mut host);
        // Unknown action id keeps the request-conflict refusal.
        let mut unknown = body("a-2", "hi");
        unknown.action_id = "delete_everything".into();
        assert_eq!(
            request(&mut host, unknown),
            Err(CapabilityServerError::Conflict(
                "capability request conflict"
            ))
        );
        // Stale revision.
        let mut stale = body("a-3", "hi");
        stale.expected_revision = 9;
        assert_eq!(
            request(&mut host, stale),
            Err(CapabilityServerError::Conflict(
                "capability request conflict"
            ))
        );
        // An invented input is an input refusal, not a generic conflict.
        let mut injected = body("a-4", "hi");
        injected.inputs.insert("shell".into(), json!("rm -rf /"));
        assert_eq!(
            request(&mut host, injected),
            Err(CapabilityServerError::InvalidInput {
                reason: "undeclared input: shell".into()
            })
        );
        // The contract's own schema/length applies before anything is written.
        let mut blank = body("a-5", "");
        blank.inputs.insert("note".into(), json!(""));
        assert_eq!(
            request(&mut host, blank),
            Err(CapabilityServerError::InvalidInput {
                reason: "invalid value for note".into()
            })
        );
        assert!(
            list(&host, None, None).expect("list").requests.is_empty(),
            "no refused attempt is persisted"
        );

        // A valid request is durable, fenced and idempotent per command id.
        let created = request(&mut host, body("a-6", "hello")).expect("request");
        assert_eq!(created.request.state, "pending");
        assert_eq!(created.request.from, "open");
        assert_eq!(created.request.to, "closed");
        assert_eq!(created.request.capability_id, REFERENCE_CAPABILITY_ID);
        assert_eq!(created.request.inputs["note"], json!("hello"));
        assert!(created.request.executor_device_id.is_none());
        assert!(created.request.claim_token.is_none());
        assert!(created.request.created_at_ms >= 0);
        let replay = request(&mut host, body("a-6", "hello")).expect("replay");
        assert_eq!(replay.request.command_id, created.request.command_id);
        // The same (item, revision, action) with another command id conflicts.
        assert_eq!(
            request(&mut host, body("a-7", "hello")),
            Err(CapabilityServerError::Conflict(
                "capability request conflict"
            ))
        );
        // Reusing the command id with different inputs cannot rewrite it.
        assert_eq!(
            request(&mut host, body("a-6", "different")),
            Err(CapabilityServerError::Conflict(
                "capability request conflict"
            ))
        );
    }

    #[test]
    fn claim_is_single_and_fenced_by_device_and_token() {
        let mut host = host();
        announce(&mut host, "exec-1");
        grant(&mut host);
        request(&mut host, body("a-1", "hello")).expect("request");
        // A device that never announced cannot claim.
        assert_eq!(
            claim(&mut host, "operator", "a-1", "t-1"),
            Err(CapabilityServerError::Forbidden("capability not announced"))
        );
        let claimed = claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");
        assert_eq!(claimed.request.state, "claimed");
        assert_eq!(
            claimed.request.executor_device_id.as_deref(),
            Some("exec-1")
        );
        assert_eq!(claimed.request.claim_token.as_deref(), Some("t-1"));
        assert_eq!(claimed.execution_plan.capability, REFERENCE_CAPABILITY_ID);
        assert_eq!(claimed.execution_plan.action_id, "append_note");
        assert_eq!(claimed.execution_plan.parameters["note"], json!("hello"));
        assert_eq!(claimed.execution_plan.command_id, "a-1");

        // Same device and token replays the same claim.
        let replay = claim(&mut host, "exec-1", "a-1", "t-1").expect("replay");
        assert_eq!(replay.request, claimed.request);
        assert_eq!(replay.execution_plan, claimed.execution_plan);
        // Same device, different token: refused.
        assert_eq!(
            claim(&mut host, "exec-1", "a-1", "t-2"),
            Err(CapabilityServerError::Conflict(
                "capability action already claimed"
            ))
        );
        // A second enrolled executor: refused.
        announce(&mut host, "exec-2");
        assert_eq!(
            claim(&mut host, "exec-2", "a-1", "t-3"),
            Err(CapabilityServerError::Conflict(
                "capability action already claimed"
            ))
        );
        // A claim token belongs to exactly one intent.
        request(&mut host, {
            let mut second = body("a-2", "other");
            second.item_id = "item-2".into();
            second
        })
        .expect("second request");
        assert_eq!(
            claim(&mut host, "exec-2", "a-2", "t-1"),
            Err(CapabilityServerError::Conflict(
                "capability claim token already used"
            ))
        );
        // Nothing executed: the server has no provider to run.
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .expect("count"),
            0
        );
    }

    #[test]
    fn result_settles_once_and_is_never_retried_inside_the_server() {
        let mut host = host();
        announce(&mut host, "exec-1");
        grant(&mut host);
        request(&mut host, body("a-1", "hello")).expect("request");
        claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");
        // Wrong claim token or device cannot report.
        assert_eq!(
            result(
                &mut host,
                "exec-1",
                "a-1",
                "t-2",
                CapabilityOutcome::Succeeded,
                "",
                None,
                5
            ),
            Err(CapabilityServerError::Conflict(
                "capability action already claimed"
            ))
        );
        let settled = result(
            &mut host,
            "exec-1",
            "a-1",
            "t-1",
            CapabilityOutcome::Uncertain,
            "provider lost the response",
            None,
            6,
        )
        .expect("result");
        assert_eq!(settled.request.state, "uncertain");
        assert_eq!(settled.receipt.outcome, "uncertain");
        assert_eq!(settled.receipt.reported_by, "exec-1");
        assert_eq!(
            settled.receipt.detail["detail"],
            json!("provider lost the response")
        );
        // Replaying the same outcome returns the original receipt.
        let replay = result(
            &mut host,
            "exec-1",
            "a-1",
            "t-1",
            CapabilityOutcome::Uncertain,
            "provider lost the response",
            None,
            7,
        )
        .expect("replay");
        assert_eq!(replay.receipt.detail, settled.receipt.detail);
        // A different outcome is refused: only an explicit reconcile may decide.
        assert_eq!(
            result(
                &mut host,
                "exec-1",
                "a-1",
                "t-1",
                CapabilityOutcome::Succeeded,
                "",
                None,
                8
            ),
            Err(CapabilityServerError::Conflict(
                "capability action already settled"
            ))
        );
        // Nothing was re-dispatched and the reference effect never happened.
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .expect("count"),
            0
        );
        let shown = show(&host, "a-1").expect("show");
        assert_eq!(shown.request.state, "uncertain");
        assert_eq!(shown.receipt.expect("receipt").outcome, "uncertain");
    }

    /// A valid payload for the reference capability's declared effect.
    fn effect(item_id: &str, note: &str, tag: Option<&str>) -> Value {
        let mut payload = json!({
            "kind": "work_item_note",
            "item_id": item_id,
            "note": note,
        });
        if let Some(tag) = tag {
            payload["tag"] = json!(tag);
        }
        payload
    }

    /// An inline workflow that offers the habit capability, which declares no
    /// effect. It exists so "a capability that declares no effect cannot be
    /// handed one" is exercised against a real, runnable capability rather than a
    /// fabricated id.
    const HABIT_MANIFEST: &str = r#"{
        "id": "capability-habit",
        "version": 1,
        "name": "Habit reference",
        "description": "A capability with no declared effect",
        "required_capabilities": ["orbit.habit.recurrence"],
        "work_items": [{
            "id": "note-task",
            "name": "Habit task",
            "initial_state": "open",
            "states": ["open", "closed"],
            "fields": [],
            "transitions": [{
                "from": "open",
                "to": "closed",
                "actions": [{
                    "id": "evaluate_periods",
                    "capability": "orbit.habit.recurrence",
                    "description": "Evaluate the caller's periods",
                    "parameters": {}
                }]
            }]
        }]
    }"#;

    /// The same install -> publish -> bind -> enroll order, for an inline
    /// workflow and one capability.
    fn bound_host(manifest: &str, blueprint_id: &str, capability_id: &str) -> AuthoritativeHost {
        let mut host = AuthoritativeHost::open(":memory:").expect("host");
        let project =
            Project::new(ProjectId::new("project-1").unwrap(), "Reference").expect("project");
        ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
        let contract = ContractCatalog::builtin()
            .contract(capability_id)
            .cloned()
            .expect("contract");
        host.authority
            .conn
            .install_capability(&contract.installation("orbit", 1).expect("installation"))
            .expect("install");
        let installed = installed_capability_ids(&host).expect("installed set");
        let blueprint = Blueprint::from_json(manifest, &installed).expect("manifest");
        host.publish_project_blueprint(&blueprint, &installed)
            .expect("publish");
        host.activate_project_blueprint("project-1", blueprint_id, 1)
            .expect("bind");
        host.create_project_work_item("item-1", "project-1", "note-task", BTreeMap::new())
            .expect("item");
        host.enroll_device("exec-1", EXECUTOR_TOKEN)
            .expect("device");
        host.enroll_device("operator", OPERATOR_TOKEN)
            .expect("device");
        announce_executors(&mut host, "exec-1", &[capability_id.to_owned()], 2).expect("announce");
        host.authorize_trusted_capability(capability_id, "project-1", "operator", 3)
            .expect("grant");
        host
    }

    #[test]
    fn a_declared_effect_is_persisted_once_and_replayed_without_a_second_note() {
        let mut host = host();
        announce(&mut host, "exec-1");
        grant(&mut host);
        request(&mut host, body("a-1", "hello")).expect("request");
        claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");

        let mut payload = effect("item-1", "hello", Some("demo"));
        let settled = result(
            &mut host,
            "exec-1",
            "a-1",
            "t-1",
            CapabilityOutcome::Succeeded,
            "the provider wrote its note",
            Some(&payload),
            9,
        )
        .expect("result");
        assert_eq!(settled.request.state, "succeeded");
        // The receipt is the durable statement that the authority changed: the
        // kind and the persisted identity of the row, not a scratch claim.
        assert_eq!(settled.receipt.detail["effect"]["applied"], json!(true));
        assert_eq!(settled.receipt.detail["effect"]["kind"], "work_item_note");
        assert_eq!(settled.receipt.detail["effect"]["item_id"], "item-1");
        assert_eq!(settled.receipt.detail["effect"]["command_id"], "a-1");

        let read = effects(&host, "item-1").expect("effects");
        assert_eq!(read.item_id, "item-1");
        assert_eq!(read.notes.len(), 1, "{read:?}");
        assert_eq!(read.notes[0].command_id, "a-1");
        assert_eq!(read.notes[0].note, "hello");
        assert_eq!(read.notes[0].tag.as_deref(), Some("demo"));
        assert_eq!(read.notes[0].created_at_ms, 9);
        // The other item of the same project is untouched.
        assert!(effects(&host, "item-2").expect("effects").notes.is_empty());
        assert_eq!(
            effects(&host, "missing"),
            Err(CapabilityServerError::NotFound("work item not found"))
        );

        // Replaying the identical report returns the identical receipt and does
        // not append a second note.
        let replay = result(
            &mut host,
            "exec-1",
            "a-1",
            "t-1",
            CapabilityOutcome::Succeeded,
            "the provider wrote its note",
            Some(&payload),
            10,
        )
        .expect("replay");
        assert_eq!(replay.receipt, settled.receipt);
        assert_eq!(replay.request, settled.request);
        assert_eq!(effects(&host, "item-1").expect("effects").notes.len(), 1);
        // A different payload for a settled command id cannot be applied over it.
        payload["note"] = json!("a different note");
        assert_eq!(
            result(
                &mut host,
                "exec-1",
                "a-1",
                "t-1",
                CapabilityOutcome::Succeeded,
                "the provider wrote its note",
                Some(&payload),
                11
            ),
            Err(CapabilityServerError::Conflict(
                "capability action already settled"
            ))
        );
        assert_eq!(effects(&host, "item-1").expect("effects").notes.len(), 1);
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .expect("count"),
            1,
            "one command id owns exactly one effect row"
        );
    }

    #[test]
    fn result_refuses_an_effect_that_violates_the_declared_schema() {
        let mut host = host();
        announce(&mut host, "exec-1");
        grant(&mut host);
        request(&mut host, body("a-1", "hello")).expect("request");
        claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");

        let cases = [
            // Missing the required key.
            (
                json!({"kind": "work_item_note", "item_id": "item-1"}),
                "invalid capability effect",
            ),
            // Wrong type.
            (
                json!({"kind": "work_item_note", "item_id": "item-1", "note": 7}),
                "invalid capability effect",
            ),
            // The payload may not rename the effect.
            (
                json!({"kind": "path_write", "item_id": "item-1", "note": "x"}),
                "invalid capability effect",
            ),
            // The target must be this request's own item.
            (
                json!({"kind": "work_item_note", "item_id": "item-2", "note": "x"}),
                "invalid capability effect",
            ),
            // No undeclared key may ride along.
            (
                json!({"kind": "work_item_note", "item_id": "item-1", "note": "x", "table": "devices"}),
                "invalid capability effect",
            ),
            // Too long, against the declared bound.
            (
                json!({"kind": "work_item_note", "item_id": "item-1", "note": "x".repeat(2_001)}),
                "capability effect exceeds its declared length",
            ),
            (
                json!({"kind": "work_item_note", "item_id": "item-1", "note": "x", "tag": "t".repeat(65)}),
                "capability effect exceeds its declared length",
            ),
        ];
        for (payload, message) in cases {
            assert_eq!(
                result(
                    &mut host,
                    "exec-1",
                    "a-1",
                    "t-1",
                    CapabilityOutcome::Succeeded,
                    "",
                    Some(&payload),
                    9,
                ),
                Err(CapabilityServerError::BadRequest(message)),
                "{payload}"
            );
        }
        // Nothing was settled and no effect was written by a refusal.
        let shown = show(&host, "a-1").expect("show");
        assert_eq!(shown.request.state, "claimed");
        assert!(shown.receipt.is_none());
        assert_eq!(
            host.authority
                .conn
                .reference_work_item_note_count("item-1")
                .expect("count"),
            0
        );
        // The same claim can still be reported honestly.
        let settled = result(
            &mut host,
            "exec-1",
            "a-1",
            "t-1",
            CapabilityOutcome::Succeeded,
            "",
            Some(&effect("item-1", "hello", None)),
            12,
        )
        .expect("a valid report still settles");
        assert_eq!(settled.receipt.detail["effect"]["applied"], json!(true));
    }

    #[test]
    fn an_absent_effect_is_recorded_as_applied_false_with_a_reason() {
        let mut host = host();
        announce(&mut host, "exec-1");
        grant(&mut host);
        request(&mut host, body("a-1", "hello")).expect("request");
        claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");
        let settled = result(
            &mut host,
            "exec-1",
            "a-1",
            "t-1",
            CapabilityOutcome::Succeeded,
            "ran, reported no effect",
            None,
            9,
        )
        .expect("result");
        assert_eq!(
            settled.receipt.detail["effect"],
            json!({"applied": false, "reason": "no effect declared by the executor"})
        );
        assert!(
            effects(&host, "item-1").expect("effects").notes.is_empty(),
            "an absent effect never becomes a row"
        );
    }

    #[test]
    fn a_capability_that_declares_no_effect_cannot_be_handed_one() {
        let mut host = bound_host(
            HABIT_MANIFEST,
            "capability-habit",
            crate::habit_capability::CAPABILITY_ID,
        );
        let request_body = ServerCapabilityRequest {
            command_id: "h-1".to_owned(),
            item_id: "item-1".to_owned(),
            expected_revision: 0,
            action_id: "evaluate_periods".to_owned(),
            inputs: BTreeMap::from([
                ("cadence".to_owned(), json!({"kind": "daily"})),
                ("periods".to_owned(), json!([])),
                ("occurrences".to_owned(), json!([])),
            ]),
            confirm: true,
        };
        request(&mut host, request_body).expect("request");
        claim(&mut host, "exec-1", "h-1", "t-1").expect("claim");
        // The declared effect of the *reference* capability is not transferable:
        // this capability's contract declares none, so the payload is refused
        // before it is even read.
        assert_eq!(
            result(
                &mut host,
                "exec-1",
                "h-1",
                "t-1",
                CapabilityOutcome::Succeeded,
                "",
                Some(&effect("item-1", "smuggled", None)),
                9,
            ),
            Err(CapabilityServerError::BadRequest(
                "capability does not declare an effect"
            ))
        );
        assert!(
            effects(&host, "item-1").expect("effects").notes.is_empty(),
            "a non-declaring capability writes nothing"
        );
        // Reporting no effect is the honest report, and says so.
        let settled = result(
            &mut host,
            "exec-1",
            "h-1",
            "t-1",
            CapabilityOutcome::Succeeded,
            "",
            None,
            10,
        )
        .expect("result");
        assert_eq!(
            settled.receipt.detail["effect"],
            json!({"applied": false, "reason": "the capability's contract declares no effect"})
        );
    }

    #[test]
    fn a_persisted_effect_survives_a_restart_without_a_second_note() {
        let path = std::env::temp_dir().join(format!(
            "orbit-capability-effect-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));
        {
            let mut host = AuthoritativeHost::open(&path).expect("host");
            let project =
                Project::new(ProjectId::new("project-1").unwrap(), "Reference").expect("project");
            ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
            insert_reference_install(&mut host);
            let installed = installed_capability_ids(&host).expect("installed");
            host.publish_project_blueprint(&demo_manifest(), &installed)
                .expect("publish");
            host.activate_project_blueprint("project-1", "capability-reference", 1)
                .expect("bind");
            host.create_project_work_item("item-1", "project-1", "note-task", BTreeMap::new())
                .expect("item");
            host.enroll_device("exec-1", EXECUTOR_TOKEN)
                .expect("device");
            announce(&mut host, "exec-1");
            grant(&mut host);
            request(&mut host, body("a-1", "durable")).expect("request");
            claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");
            result(
                &mut host,
                "exec-1",
                "a-1",
                "t-1",
                CapabilityOutcome::Succeeded,
                "wrote it",
                Some(&effect("item-1", "durable", Some("kept"))),
                4_242,
            )
            .expect("result");
        }
        let mut host = AuthoritativeHost::open(&path).expect("reopen");
        let read = effects(&host, "item-1").expect("effects");
        assert_eq!(read.notes.len(), 1, "{read:?}");
        assert_eq!(read.notes[0].command_id, "a-1");
        assert_eq!(read.notes[0].note, "durable");
        assert_eq!(read.notes[0].tag.as_deref(), Some("kept"));
        assert_eq!(read.notes[0].created_at_ms, 4_242);
        // The receipt still states the durable change after the restart.
        let shown = show(&host, "a-1").expect("show");
        assert_eq!(shown.request.state, "succeeded");
        assert_eq!(
            shown.receipt.expect("receipt").detail["effect"]["applied"],
            json!(true)
        );
        // A replay after the restart is still idempotent for both the receipt and
        // the effect row.
        let replay = result(
            &mut host,
            "exec-1",
            "a-1",
            "t-1",
            CapabilityOutcome::Succeeded,
            "wrote it",
            Some(&effect("item-1", "durable", Some("kept"))),
            5_000,
        )
        .expect("replay");
        assert_eq!(replay.receipt.reported_at_ms, 4_242);
        assert_eq!(effects(&host, "item-1").expect("effects").notes.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reconcile_refuses_pending_and_settles_claimed_with_the_actor() {
        let mut host = host();
        announce(&mut host, "exec-1");
        grant(&mut host);
        request(&mut host, body("a-1", "hello")).expect("request");
        // Pending has not executed: an operator cannot invent an outcome.
        assert_eq!(
            reconcile(
                &mut host,
                "operator",
                "a-1",
                CapabilityReconcileDecision::ConfirmFailed,
                "",
                4
            ),
            Err(CapabilityServerError::Conflict(
                "capability action cannot be reconciled"
            ))
        );
        claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");
        let reconciled = reconcile(
            &mut host,
            "operator",
            "a-1",
            CapabilityReconcileDecision::ConfirmFailed,
            "the runner died before reporting",
            5,
        )
        .expect("reconcile");
        assert_eq!(reconciled.request.state, "failed");
        let receipt = reconciled.receipt.expect("receipt");
        assert_eq!(receipt.outcome, "failed");
        assert_eq!(receipt.reported_by, "operator");
        assert_eq!(
            receipt.detail["note"],
            json!("the runner died before reporting")
        );
        // Replaying the same decision is idempotent; a different one conflicts.
        assert!(
            reconcile(
                &mut host,
                "operator",
                "a-1",
                CapabilityReconcileDecision::ConfirmFailed,
                "the runner died before reporting",
                6
            )
            .is_ok()
        );
        assert_eq!(
            reconcile(
                &mut host,
                "operator",
                "a-1",
                CapabilityReconcileDecision::ConfirmSucceeded,
                "",
                7
            ),
            Err(CapabilityServerError::Conflict(
                "capability action cannot be reconciled"
            ))
        );
    }

    #[test]
    fn install_replays_and_a_changed_contract_conflicts() {
        let mut host = AuthoritativeHost::open(":memory:").expect("host");
        let installed = install(&mut host, REFERENCE_CAPABILITY_ID, 5).expect("install");
        assert_eq!(installed.permission, "mutate");
        assert_eq!(installed.provider, "orbit");
        assert_eq!(installed.installed_at_ms, 5);
        let replay = install(&mut host, REFERENCE_CAPABILITY_ID, 9).expect("replay");
        assert_eq!(
            replay.installed_at_ms, 5,
            "an identical re-install replays the immutable row"
        );
        assert_eq!(
            install(&mut host, "orbit.not.a.capability", 5),
            Err(CapabilityServerError::BadRequest("unknown capability"))
        );
        // A stored contract that no longer matches the catalog cannot be
        // silently upgraded.
        let mut changed = ContractCatalog::builtin()
            .contract(REFERENCE_CAPABILITY_ID)
            .cloned()
            .expect("contract")
            .installation("orbit", 5)
            .expect("installation");
        changed.actions_json = "[]".into();
        host.authority
            .conn
            .install_capability(&changed)
            .expect_err("a changed contract conflicts");
        let mut stored = changed.clone();
        stored.capability_id = "orbit.other".into();
        assert_eq!(
            host.authority.conn.install_capability(&stored).map(|_| ()),
            Ok(())
        );
    }

    #[test]
    fn grants_require_installation_confirmation_and_stay_scoped() {
        let mut host = host();
        let project =
            Project::new(ProjectId::new("project-2").unwrap(), "Second").expect("project");
        ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
        assert_eq!(
            authorize(
                &mut host,
                REFERENCE_CAPABILITY_ID,
                "project-1",
                "operator",
                false,
                4
            ),
            Err(CapabilityServerError::BadRequest(
                "capability authorization requires confirmation"
            ))
        );
        assert_eq!(
            authorize(&mut host, "orbit.nope", "project-1", "operator", true, 4),
            Err(CapabilityServerError::BadRequest("unknown capability"))
        );
        let grant = authorize(
            &mut host,
            REFERENCE_CAPABILITY_ID,
            "project-1",
            "operator",
            true,
            4,
        )
        .expect("grant");
        assert_eq!(grant.granted_by, "operator");
        // A device that is not an executor may still be the operator, but the
        // grant never widens to another project.
        let other = authorize(
            &mut host,
            REFERENCE_CAPABILITY_ID,
            "project-2",
            "operator",
            true,
            5,
        )
        .expect("grant");
        assert_eq!(other.project_id, "project-2");
        assert_eq!(
            authorize(
                &mut host,
                REFERENCE_CAPABILITY_ID,
                "missing-project",
                "operator",
                true,
                6
            ),
            Err(CapabilityServerError::Conflict(
                "capability authorization conflict"
            ))
        );
        assert!(
            revoke(&mut host, REFERENCE_CAPABILITY_ID, "project-1")
                .expect("revoke")
                .revoked
        );
        assert!(
            !revoke(&mut host, REFERENCE_CAPABILITY_ID, "project-1")
                .expect("revoke")
                .revoked
        );
    }

    #[test]
    fn claims_survive_reopening_and_lists_are_ordered() {
        let path = std::env::temp_dir().join(format!(
            "orbit-capability-server-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ));
        {
            let mut host = AuthoritativeHost::open(&path).expect("host");
            let project =
                Project::new(ProjectId::new("project-1").unwrap(), "Reference").expect("project");
            ProjectRepository::save(&mut host.authority.conn, project).expect("save project");
            insert_reference_install(&mut host);
            let installed = installed_capability_ids(&host).expect("installed");
            host.publish_project_blueprint(&demo_manifest(), &installed)
                .expect("publish");
            host.activate_project_blueprint("project-1", "capability-reference", 1)
                .expect("bind");
            host.create_project_work_item("item-1", "project-1", "note-task", BTreeMap::new())
                .expect("item");
            host.enroll_device("exec-1", EXECUTOR_TOKEN)
                .expect("device");
            announce(&mut host, "exec-1");
            grant(&mut host);
            request(&mut host, body("a-1", "hello")).expect("request");
            claim(&mut host, "exec-1", "a-1", "t-1").expect("claim");
        }
        // Restart: nothing resets a claim back to pending.
        let mut host = AuthoritativeHost::open(&path).expect("reopen");
        let shown = show(&host, "a-1").expect("show");
        assert_eq!(shown.request.state, "claimed");
        assert_eq!(shown.request.executor_device_id.as_deref(), Some("exec-1"));
        assert!(shown.receipt.is_none());
        // The same executor replays its claim after the restart.
        let replay = claim(&mut host, "exec-1", "a-1", "t-1").expect("replay");
        assert_eq!(replay.request.state, "claimed");
        // A second device still cannot take it.
        host.enroll_device("exec-2", OTHER_TOKEN).expect("device");
        announce(&mut host, "exec-2");
        assert_eq!(
            claim(&mut host, "exec-2", "a-1", "t-2"),
            Err(CapabilityServerError::Conflict(
                "capability action already claimed"
            ))
        );
        let listed = list(&host, Some("item-1"), Some("claimed")).expect("list");
        assert_eq!(listed.requests.len(), 1);
        assert_eq!(listed.requests[0].command_id, "a-1");
        assert!(
            list(&host, Some("item-1"), Some("pending"))
                .expect("list")
                .requests
                .is_empty()
        );
        let _ = std::fs::remove_file(&path);
    }
}
