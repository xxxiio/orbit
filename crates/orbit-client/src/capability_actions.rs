//! Trusted capability actions through the embedded client facade.
//!
//! These wrappers expose the host-owned composition path without leaking
//! storage internals to UI consumers. Every offer, availability decision,
//! request, claim and receipt stays owned by
//! [`orbit_runtime::capability_composition::TrustedComposition`]; the client
//! only constructs the composition and maps the durable records into DTOs.
//!
//! Nothing here grants execution authority: `request` persists an explicit
//! pending intent and `execute` is a separate, explicit claim of exactly that
//! command id.

use crate::{ClientError, OrbitClient};
use orbit_blueprint::Blueprint;
use orbit_runtime::capability_composition::{
    CapabilityActionRequest, CapabilityReconcileDecision, TrustedComposition,
};
use orbit_storage::capability_installs::CapabilityReceipt;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Actor recorded on explicit reconciliations driven by the terminal shell.
pub const CAPABILITY_ACTION_ACTOR: &str = "orbit-tui";

pub use orbit_runtime::capability_composition::{
    CapabilityStatusRow as OrbitCapabilityStatus, OfferedCapabilityAction as OrbitCapabilityOffer,
};

/// Why an offered capability action cannot be requested or executed right now.
pub use orbit_runtime::capability_composition::CapabilityActionAvailability as OrbitCapabilityAvailability;

/// One durable capability action request, without the storage envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrbitCapabilityAction {
    pub command_id: String,
    pub item_id: String,
    pub item_revision: u64,
    pub action_id: String,
    pub capability: String,
    /// Durable state: `pending`, `claimed`, `succeeded`, `failed`, `uncertain`.
    pub state: String,
}

/// A durable request plus its recorded receipt, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrbitCapabilityActionState {
    pub request: OrbitCapabilityAction,
    pub receipt: Option<OrbitCapabilityReceipt>,
}

/// A user's explicit request to invoke one offered capability action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrbitCapabilityRequest {
    pub item_id: String,
    pub expected_revision: u64,
    pub action_id: String,
    pub command_id: String,
    pub inputs: BTreeMap<String, Value>,
}

/// The authority's record of one capability execution attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrbitCapabilityReceipt {
    pub command_id: String,
    pub item_id: String,
    pub capability: String,
    pub action_id: String,
    pub provider: String,
    /// `succeeded`, `failed` or `uncertain`. Never inferred from request state.
    pub outcome: String,
    pub detail: Value,
    pub recorded_at_ms: i64,
}

impl From<CapabilityReceipt> for OrbitCapabilityReceipt {
    fn from(receipt: CapabilityReceipt) -> Self {
        Self {
            command_id: receipt.command_id,
            item_id: receipt.item_id,
            capability: receipt.capability,
            action_id: receipt.action_id,
            provider: receipt.provider,
            outcome: receipt.outcome,
            detail: receipt.detail,
            recorded_at_ms: receipt.recorded_at_ms,
        }
    }
}

impl From<orbit_storage::blueprint_actions::BlueprintActionRequest> for OrbitCapabilityAction {
    fn from(request: orbit_storage::blueprint_actions::BlueprintActionRequest) -> Self {
        Self {
            command_id: request.command_id,
            item_id: request.item_id,
            item_revision: request.item_revision,
            action_id: request.action_id,
            capability: request.capability,
            state: request.state,
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn composition() -> TrustedComposition {
    TrustedComposition::with_builtin_reference()
}

impl OrbitClient {
    /// Install one capability the trusted composition knows. Durable
    /// installation is a prerequisite for publication and authorization.
    pub fn install_trusted_capability(&mut self, capability_id: &str) -> Result<(), ClientError> {
        let composition = composition();
        self.runtime
            .install_trusted_capability(&composition, capability_id, now_ms())
            .map(|_| ())
            .map_err(ClientError::from)
    }

    /// Grant one capability to exactly one project. Explicit; never implied by
    /// a blueprint manifest or an installation row.
    pub fn authorize_trusted_capability(
        &mut self,
        capability_id: &str,
        project_id: &str,
    ) -> Result<(), ClientError> {
        self.runtime
            .authorize_trusted_capability(
                capability_id,
                project_id,
                CAPABILITY_ACTION_ACTOR,
                now_ms(),
            )
            .map(|_| ())
            .map_err(ClientError::from)
    }

    /// Publish a blueprint manifest against the installed capabilities. A
    /// manifest that needs an uninstalled capability is refused here.
    pub fn publish_blueprint_manifest(
        &mut self,
        manifest_json: &str,
    ) -> Result<Blueprint, ClientError> {
        let composition = composition();
        let installed = self
            .runtime
            .installed_capability_ids(&composition)
            .map_err(ClientError::from)?;
        self.runtime
            .import_project_blueprint_json(manifest_json, &installed)
            .map_err(ClientError::from)
    }

    /// Every action the item's current state offers, with availability computed
    /// from durable installation, authorization and composition state.
    pub fn capability_action_offers(
        &self,
        item_id: &str,
    ) -> Result<Vec<OrbitCapabilityOffer>, ClientError> {
        let composition = composition();
        self.runtime
            .capability_action_offers(&composition, item_id)
            .map_err(ClientError::from)
    }

    /// Installation/authorization view for one project (or all projects).
    pub fn capability_status(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<OrbitCapabilityStatus>, ClientError> {
        let composition = composition();
        self.runtime
            .capability_status(&composition, project_id)
            .map_err(ClientError::from)
    }

    /// Persist an explicit request. It is `pending`; nothing executes here.
    pub fn request_capability_action(
        &mut self,
        request: OrbitCapabilityRequest,
    ) -> Result<OrbitCapabilityAction, ClientError> {
        let composition = composition();
        self.runtime
            .request_capability_action(
                &composition,
                CapabilityActionRequest {
                    item_id: request.item_id,
                    expected_revision: request.expected_revision,
                    action_id: request.action_id,
                    command_id: request.command_id,
                    inputs: request.inputs,
                },
            )
            .map(OrbitCapabilityAction::from)
            .map_err(ClientError::from)
    }

    /// Claim and execute one recorded request exactly once. Replaying after a
    /// receipt returns that receipt instead of dispatching again.
    pub fn execute_capability_action(
        &mut self,
        command_id: &str,
    ) -> Result<OrbitCapabilityReceipt, ClientError> {
        let mut composition = composition();
        self.runtime
            .execute_capability_action(&mut composition, command_id, now_ms())
            .map(OrbitCapabilityReceipt::from)
            .map_err(ClientError::from)
    }

    /// Durable request plus receipt for one command id.
    pub fn capability_action_state(
        &self,
        command_id: &str,
    ) -> Result<Option<OrbitCapabilityActionState>, ClientError> {
        let state = self
            .runtime
            .capability_action_receipt(command_id)
            .map_err(ClientError::from)?;
        Ok(state.map(|state| OrbitCapabilityActionState {
            request: state.request.into(),
            receipt: state.receipt.map(OrbitCapabilityReceipt::from),
        }))
    }

    /// Recent durable requests for one work item, newest first as returned by
    /// the authority. `state` is the durable lifecycle state, never a claim of
    /// acceptance and never an executor result.
    pub fn capability_action_history(
        &self,
        item_id: &str,
        limit: usize,
    ) -> Result<Vec<OrbitCapabilityAction>, ClientError> {
        self.runtime
            .list_capability_actions(Some(item_id), None, limit)
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(ClientError::from)
    }

    /// Durable effects of the built-in reference capability for one work item.
    /// A count is enough for UI/acceptance assertions and avoids exporting the
    /// storage note record through the facade.
    pub fn reference_work_item_note_count(&self, item_id: &str) -> Result<usize, ClientError> {
        self.runtime
            .reference_work_item_notes(item_id)
            .map(|notes| notes.len())
            .map_err(ClientError::from)
    }

    /// Settle an `uncertain` (or lost-acknowledgement `claimed`) outcome with an
    /// explicit operator decision. Never dispatches and never rewrites a
    /// recorded `succeeded`/`failed`.
    pub fn reconcile_capability_action(
        &mut self,
        command_id: &str,
        succeed: bool,
    ) -> Result<OrbitCapabilityReceipt, ClientError> {
        let decision = if succeed {
            CapabilityReconcileDecision::ConfirmSucceeded
        } else {
            CapabilityReconcileDecision::ConfirmFailed
        };
        self.runtime
            .reconcile_capability_action(command_id, decision, CAPABILITY_ACTION_ACTOR, now_ms())
            .map(OrbitCapabilityReceipt::from)
            .map_err(ClientError::from)
    }
}
