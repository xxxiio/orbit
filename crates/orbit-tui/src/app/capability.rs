//! Cached capability-action state and the contextual actions it produces.
//!
//! Capability offers are resolved against durable installation/authorization
//! state by the backend. This module only caches that result on [`App`] from
//! input and mutation paths, and turns it into the same contextual
//! [`AvailableAction`] shape every other action uses. Nothing here executes.
use super::*;
use orbit_client::{OrbitCapabilityAvailability, OrbitCapabilityReceipt};

impl App {
    /// Re-resolve the offers for the selected work item.
    ///
    /// Called from selection/navigation changes, project/workspace refreshes and
    /// after every capability request, execution or reconciliation. Never from
    /// the renderer: painting reads [`App::capability_offers`] only.
    pub fn refresh_capability_actions(&mut self) {
        self.capability_offers.clear();
        self.capability_history.clear();
        self.capability_error = None;
        // Connected mode has no capability-action projection (M25 EC-004).
        if self.client.is_connected() {
            return;
        }
        let Some((item_id, revision)) = self.selected_item().and_then(|item| {
            (item.kind == ItemKind::WorkItem)
                .then(|| (item.id.clone(), item.revision.unwrap_or_default()))
        }) else {
            return;
        };
        if self.project.is_none() {
            return;
        }
        match self.client.capability_action_offers(&item_id) {
            Ok(offers) => {
                let history = match self.client.capability_action_history(&item_id, 20) {
                    Ok(history) => history,
                    Err(error) => {
                        self.capability_error = Some(error.message);
                        Vec::new()
                    }
                };
                self.capability_offers = offers
                    .into_iter()
                    .map(|offer| {
                        let command_id = history
                            .iter()
                            .find(|row| {
                                row.action_id == offer.action_id
                                    && matches!(
                                        row.state.as_str(),
                                        "pending" | "claimed" | "uncertain"
                                    )
                            })
                            .map(|row| row.command_id.clone());
                        CapabilityActionView {
                            item_id: item_id.clone(),
                            revision,
                            command_id,
                            offer,
                        }
                    })
                    .collect();
                self.capability_history = history;
            }
            Err(error) => self.capability_error = Some(error.message),
        }
    }

    /// Contextual actions for one work item, exactly as the action menu,
    /// palette and inspector use them.
    pub(crate) fn capability_actions_for(&self, item_id: &str) -> Vec<AvailableAction> {
        self.capability_offers
            .iter()
            .filter(|view| view.item_id == item_id)
            .map(Self::capability_action)
            .collect()
    }

    fn capability_action(view: &CapabilityActionView) -> AvailableAction {
        let offer = &view.offer;
        let id = view.action_key();
        let label = format!("Action {}", offer.description);
        let transition = format!("{} · {} → {}", offer.capability, offer.from, offer.to);
        let mut action = match &offer.availability {
            OrbitCapabilityAvailability::Available => {
                AvailableAction::enabled(&id, label, Some(view.item_id.clone()))
            }
            OrbitCapabilityAvailability::Unavailable { code, reason } => {
                let mut action = AvailableAction::disabled(&id, label, format!("{code}: {reason}"));
                // Presentation-only: a disabled row never executes, but the
                // payload keeps the identity visible to the inspector footer.
                action.payload = Some(view.item_id.clone());
                action.description = format!("{transition} · {code}: {reason}");
                action.destructive = offer.external_effect;
                return action;
            }
        };
        action.description = match capability_schema_problem(&offer.input_schema) {
            Ok(()) => format!("{transition} · available"),
            Err(problem) => {
                action.enabled = false;
                action.disabled_reason = Some(format!("unsupported_input_schema: {problem}"));
                format!("{transition} · unsupported_input_schema: {problem}")
            }
        };
        action.destructive = offer.external_effect;
        action
    }

    /// Find the cached offer behind an action id (`capability:<cap>:<action>`).
    pub(crate) fn capability_view_for_action(
        &self,
        action: &AvailableAction,
    ) -> Option<&CapabilityActionView> {
        self.capability_offers.iter().find(|view| {
            view.action_key() == action.id
                && action.payload.as_deref() == Some(view.item_id.as_str())
        })
    }

    /// Open the input form for one offered action.
    ///
    /// The command id is minted here, once per form. Re-submitting the same form
    /// after a transient failure reuses it; a new invocation mints a new one.
    pub(crate) fn start_capability_action(&mut self, view: &CapabilityActionView) {
        if !view.offer.availability.is_available() {
            let reason = view
                .offer
                .availability
                .code()
                .unwrap_or("unavailable")
                .to_owned();
            self.notify(
                format!(
                    "Capability action {} is unavailable ({reason})",
                    view.offer.action_id
                ),
                StatusKind::Warning,
            );
            return;
        }
        if let Err(problem) = capability_schema_problem(&view.offer.input_schema) {
            self.notify(
                format!("Capability action input is unsupported: {problem}"),
                StatusKind::Error,
            );
            return;
        }
        self.start_form(FormMode::CapabilityAction {
            item_id: view.item_id.clone(),
            revision: view.revision,
            capability: view.offer.capability.clone(),
            action_id: view.offer.action_id.clone(),
            command_id: crate::backend::next_command_id(),
            external_effect: view.offer.external_effect,
        });
    }

    /// Result lines shared by the result modal and its tests.
    pub fn capability_result_lines(receipt: &OrbitCapabilityReceipt) -> Vec<String> {
        let mut lines = vec![
            format!("outcome: {}", receipt.outcome),
            format!("provider: {}", receipt.provider),
            format!("action: {}", receipt.action_id),
            format!("capability: {}", receipt.capability),
            format!("item: {}", receipt.item_id),
            format!("command: {}", receipt.command_id),
            format!("detail: {}", receipt.detail),
        ];
        match receipt.outcome.as_str() {
            "succeeded" => lines.push(
                "The trusted executor recorded this outcome. Re-running the same command id returns this receipt and never appends a second effect."
                    .into(),
            ),
            "failed" => lines.push(
                "The action failed. It is never retried automatically; a new attempt needs a new explicit request."
                    .into(),
            ),
            "uncertain" => {
                lines.push(String::new());
                lines.push(
                    "Outcome uncertain: the provider did not confirm a result. This is not completed and must be reconciled explicitly."
                        .into(),
                );
            }
            _ => {}
        }
        lines
    }

    /// Show the execution result. Only an `uncertain` outcome carries the two
    /// explicit reconciliation decisions.
    pub(crate) fn show_capability_result(&mut self, receipt: OrbitCapabilityReceipt) {
        let lines = Self::capability_result_lines(&receipt);
        let reconcile_command =
            (receipt.outcome == "uncertain").then(|| receipt.command_id.clone());
        self.modal = Some(Modal::CapabilityResult {
            title: format!("Capability action · {}", receipt.outcome),
            lines,
            reconcile_command,
            selected: 0,
        });
    }
}

/// Validate the JSON-schema subset the form builder can render. An unknown type
/// disables the action with this reason instead of silently dropping a field.
pub(crate) fn capability_schema_problem(schema: &serde_json::Value) -> Result<(), String> {
    let Some(object) = schema.as_object() else {
        return Err("input schema is not an object".into());
    };
    let Some(properties) = object.get("properties") else {
        return Ok(());
    };
    let Some(properties) = properties.as_object() else {
        return Err("input schema properties are not an object".into());
    };
    for (key, property) in properties {
        let Some(kind) = property.get("type").and_then(serde_json::Value::as_str) else {
            return Err(format!("property '{key}' has no declared type"));
        };
        if !matches!(kind, "string" | "integer" | "boolean") {
            return Err(format!("property '{key}' uses unsupported type '{kind}'"));
        }
    }
    Ok(())
}
