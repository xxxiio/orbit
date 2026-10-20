//! Explicit action offers and authorization-only invocation planning.
//! This module never invokes plugins, Herdr, or an external process.
use crate::capabilities::CapabilityCatalog;
use crate::{Blueprint, BlueprintError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionOffer {
    pub action_id: String,
    pub capability: String,
    pub description: String,
    pub parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionInvocation {
    pub action_id: String,
    pub capability: String,
    pub parameters: BTreeMap<String, Value>,
    /// The command ID is supplied by the authoritative caller for replay fencing.
    pub command_id: String,
}

/// A validated proposal, NOT a dispatch permission or execution receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPlan {
    pub blueprint_id: String,
    pub blueprint_version: u32,
    pub item_kind: String,
    pub from: String,
    pub to: String,
    pub invocation: ActionInvocation,
}

/// The exact user-selected transition action to authorize. Grouping the
/// identity fields keeps the authorization call below the lint threshold and
/// prevents callers from accidentally reordering same-typed string arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSelection<'a> {
    pub item_kind: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    pub action_id: &'a str,
    pub command_id: &'a str,
}

impl Blueprint {
    pub fn offered_actions(
        &self,
        item_kind: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<ActionOffer>, BlueprintError> {
        let item = self.item_kind(item_kind)?;
        let edge = item
            .transitions
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .ok_or_else(|| BlueprintError::Invalid("action transition not defined".into()))?;
        Ok(edge
            .actions
            .iter()
            .map(|action| ActionOffer {
                action_id: action.id.clone(),
                capability: action.capability.clone(),
                description: action.description.clone(),
                parameters: action.parameters.clone(),
            })
            .collect())
    }

    /// Called only after the authority has checked and committed the transition.
    /// A host must independently verify item identity, revision and command ownership
    /// before dispatch. An action plan is never executable by itself.
    pub fn plan_explicit_action(
        &self,
        selection: ActionSelection<'_>,
        catalog: &CapabilityCatalog,
        host_grants: &BTreeSet<String>,
    ) -> Result<ActionPlan, BlueprintError> {
        if selection.command_id.trim().is_empty() || selection.command_id.len() > 128 {
            return Err(BlueprintError::Invalid("invalid action command ID".into()));
        }
        let offer = self
            .offered_actions(selection.item_kind, selection.from, selection.to)?
            .into_iter()
            .find(|offer| offer.action_id == selection.action_id)
            .ok_or_else(|| BlueprintError::NotFound("action not declared on transition".into()))?;
        catalog.authorize(&offer.capability, host_grants)?;
        Ok(ActionPlan {
            blueprint_id: self.id.clone(),
            blueprint_version: self.version,
            item_kind: selection.item_kind.into(),
            from: selection.from.into(),
            to: selection.to.into(),
            invocation: ActionInvocation {
                action_id: offer.action_id,
                capability: offer.capability,
                parameters: offer.parameters,
                command_id: selection.command_id.into(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityDescriptor, CapabilityPermission, WorkflowAction};

    #[test]
    fn action_declaration_never_grants_execution() {
        let mut blueprint: Blueprint =
            serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap();
        blueprint.required_capabilities.push("agent.launch".into());
        let edge = &mut blueprint.work_items[0].transitions[0];
        edge.actions.push(WorkflowAction {
            id: "implement".into(),
            capability: "agent.launch".into(),
            description: "Offer agent launch".into(),
            parameters: BTreeMap::new(),
        });
        let mut catalog = CapabilityCatalog::default();
        catalog
            .register(CapabilityDescriptor {
                id: "agent.launch".into(),
                provider: "herdr".into(),
                permission: CapabilityPermission::ExternalEffect,
                enabled: true,
            })
            .unwrap();
        let from = edge.from.clone();
        let to = edge.to.clone();
        blueprint.validate(&catalog.available()).unwrap();
        assert_eq!(
            blueprint
                .offered_actions("feature", &from, &to)
                .unwrap()
                .len(),
            1
        );
        assert!(
            blueprint
                .plan_explicit_action(
                    ActionSelection {
                        item_kind: "feature",
                        from: &from,
                        to: &to,
                        action_id: "implement",
                        command_id: "cmd-1",
                    },
                    &catalog,
                    &BTreeSet::new()
                )
                .is_err()
        );
        assert!(
            blueprint
                .plan_explicit_action(
                    ActionSelection {
                        item_kind: "feature",
                        from: &from,
                        to: &to,
                        action_id: "implement",
                        command_id: "cmd-1",
                    },
                    &catalog,
                    &BTreeSet::from(["agent.launch".into()])
                )
                .is_ok()
        );
    }

    #[test]
    fn undeclared_capability_fails_publication() {
        let mut blueprint: Blueprint =
            serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap();
        blueprint.work_items[0].transitions[0]
            .actions
            .push(WorkflowAction {
                id: "start".into(),
                capability: "agent.launch".into(),
                description: String::new(),
                parameters: BTreeMap::new(),
            });
        assert!(blueprint.validate(&BTreeSet::new()).is_err());
    }
}
