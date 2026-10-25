//! Durable, explicitly authorized blueprint action intents. Execution belongs to
//! a separately registered capability adapter, never to blueprint evaluation.
use crate::{SqliteStore, repo_error};
use orbit_application::RepositoryError;
use orbit_blueprint::CapabilityCatalog;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlueprintActionRequest {
    pub command_id: String,
    pub item_id: String,
    pub item_revision: u64,
    pub action_id: String,
    pub capability: String,
    pub plan_json: String,
    pub state: String,
}

/// Caller-supplied fields identifying one explicit action invocation.
#[derive(Debug, Clone, Copy)]
pub struct BlueprintActionIntent<'a> {
    pub item_id: &'a str,
    pub expected_revision: u64,
    pub from: &'a str,
    pub action_id: &'a str,
    pub command_id: &'a str,
}

impl SqliteStore {
    pub(crate) fn ensure_blueprint_actions_schema(&self) -> Result<(), RepositoryError> {
        self.execute_batch("CREATE TABLE IF NOT EXISTS orbit_blueprint_action_requests (            command_id TEXT PRIMARY KEY,            item_id TEXT NOT NULL REFERENCES orbit_blueprint_work_items(id) ON DELETE CASCADE,            item_revision INTEGER NOT NULL CHECK(item_revision >= 0),            action_id TEXT NOT NULL, capability TEXT NOT NULL,            plan_json TEXT NOT NULL CHECK(json_valid(plan_json)),            state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','claimed','succeeded','failed','uncertain')),            UNIQUE(item_id, item_revision, action_id));            CREATE INDEX IF NOT EXISTS orbit_blueprint_actions_pending ON orbit_blueprint_action_requests(state,command_id);")
            .map_err(repo_error)
    }

    /// Plans and records a user's explicit invocation. No external effect occurs.
    /// Catalog and grants must come from trusted host composition, not a manifest.
    pub fn request_blueprint_action(
        &mut self,
        intent: BlueprintActionIntent<'_>,
        catalog: &CapabilityCatalog,
        grants: &BTreeSet<String>,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        self.request_blueprint_action_with_inputs(intent, &BTreeMap::new(), catalog, grants)
    }

    /// Same contract as [`Self::request_blueprint_action`], additionally merging
    /// explicit user-supplied values over the manifest-declared defaults.
    ///
    /// A supplied key must be declared by the blueprint action; a caller can
    /// never invent a parameter. Validation happens before persistence, and the
    /// persisted `plan_json` is the canonical merged invocation.
    pub fn request_blueprint_action_with_inputs(
        &mut self,
        intent: BlueprintActionIntent<'_>,
        inputs: &BTreeMap<String, Value>,
        catalog: &CapabilityCatalog,
        grants: &BTreeSet<String>,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        self.request_blueprint_action_with_inputs_allowing(
            intent,
            inputs,
            &BTreeSet::new(),
            catalog,
            grants,
        )
    }

    /// Input variant for the trusted capability composition: `allowed_inputs`
    /// names inputs declared by the capability's own input schema, so optional
    /// inputs do not have to be materialised as manifest defaults. Passing an
    /// empty set reproduces [`Self::request_blueprint_action_with_inputs`].
    pub fn request_blueprint_action_with_inputs_allowing(
        &mut self,
        intent: BlueprintActionIntent<'_>,
        inputs: &BTreeMap<String, Value>,
        allowed_inputs: &BTreeSet<String>,
        catalog: &CapabilityCatalog,
        grants: &BTreeSet<String>,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        let BlueprintActionIntent {
            item_id,
            expected_revision,
            from,
            action_id,
            command_id,
        } = intent;
        // Preserved contract: the selected action belongs to the transition that
        // produced the item's current state, so the edge target is that state.
        let to = self
            .blueprint_work_item(item_id)?
            .ok_or(RepositoryError::Conflict)?
            .state;
        self.store_blueprint_action_plan(
            item_id,
            expected_revision,
            from,
            &to,
            action_id,
            command_id,
            inputs,
            allowed_inputs,
            catalog,
            grants,
        )
    }

    /// Record an explicit invocation of an action offered by a transition whose
    /// `from` is the item's current state. Used by the trusted capability
    /// composition, which offers actions before the move rather than after it.
    pub fn request_blueprint_action_on_transition(
        &mut self,
        intent: BlueprintActionIntent<'_>,
        to: &str,
        inputs: &BTreeMap<String, Value>,
        catalog: &CapabilityCatalog,
        grants: &BTreeSet<String>,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        self.request_blueprint_action_on_transition_allowing(
            intent,
            to,
            inputs,
            &BTreeSet::new(),
            catalog,
            grants,
        )
    }

    /// See [`Self::request_blueprint_action_on_transition`] and
    /// [`Self::request_blueprint_action_with_inputs_allowing`].
    pub fn request_blueprint_action_on_transition_allowing(
        &mut self,
        intent: BlueprintActionIntent<'_>,
        to: &str,
        inputs: &BTreeMap<String, Value>,
        allowed_inputs: &BTreeSet<String>,
        catalog: &CapabilityCatalog,
        grants: &BTreeSet<String>,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        let BlueprintActionIntent {
            item_id,
            expected_revision,
            from,
            action_id,
            command_id,
        } = intent;
        self.store_blueprint_action_plan(
            item_id,
            expected_revision,
            from,
            to,
            action_id,
            command_id,
            inputs,
            allowed_inputs,
            catalog,
            grants,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn store_blueprint_action_plan(
        &mut self,
        item_id: &str,
        expected_revision: u64,
        from: &str,
        to: &str,
        action_id: &str,
        command_id: &str,
        inputs: &BTreeMap<String, Value>,
        allowed_inputs: &BTreeSet<String>,
        catalog: &CapabilityCatalog,
        grants: &BTreeSet<String>,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        if !valid_command_id(command_id) || item_id.trim().is_empty() {
            return Err(RepositoryError::Conflict);
        }
        let item = self
            .blueprint_work_item(item_id)?
            .ok_or(RepositoryError::Conflict)?;
        if item.revision != expected_revision {
            return Err(RepositoryError::Conflict);
        }
        let binding = self
            .project_blueprint_binding(&item.project_id)?
            .ok_or(RepositoryError::Conflict)?;
        let schema = self
            .load_blueprint(&binding.blueprint_id, binding.version)?
            .ok_or(RepositoryError::Conflict)?;
        let offer = schema
            .offered_actions(&item.kind, from, to)
            .map_err(|e| RepositoryError::Other(e.to_string()))?
            .into_iter()
            .find(|offer| offer.action_id == action_id)
            .ok_or(RepositoryError::Conflict)?;
        // Reject invented parameters before anything is persisted. A key is
        // legitimate when the manifest declares it (default) or when the trusted
        // capability's input schema declares it (optional input).
        let mut parameters = offer.parameters.clone();
        for (key, value) in inputs {
            if !offer.parameters.contains_key(key) && !allowed_inputs.contains(key) {
                return Err(RepositoryError::Conflict);
            }
            parameters.insert(key.clone(), value.clone());
        }
        let mut plan = schema
            .plan_explicit_action(
                orbit_blueprint::ActionSelection {
                    item_kind: &item.kind,
                    from,
                    to,
                    action_id,
                    command_id,
                },
                catalog,
                grants,
            )
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        plan.invocation.parameters = parameters;
        let payload = serde_json::to_string(&plan.invocation)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        self.execute("INSERT OR IGNORE INTO orbit_blueprint_action_requests             (command_id,item_id,item_revision,action_id,capability,plan_json)             VALUES (?1,?2,?3,?4,?5,?6)",
            params![command_id,item_id,expected_revision,plan.invocation.action_id,
                plan.invocation.capability,payload]).map_err(repo_error)?;
        let stored = self
            .blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)?;
        if stored.item_id != item_id
            || stored.item_revision != expected_revision
            || stored.action_id != action_id
            || stored.plan_json != payload
        {
            return Err(RepositoryError::Conflict);
        }
        Ok(stored)
    }

    pub fn blueprint_action_request(
        &self,
        id: &str,
    ) -> Result<Option<BlueprintActionRequest>, RepositoryError> {
        self.query_row("SELECT command_id,item_id,item_revision,action_id,capability,plan_json,state             FROM orbit_blueprint_action_requests WHERE command_id=?1", [id], |row| Ok(BlueprintActionRequest {
                command_id:row.get(0)?, item_id:row.get(1)?, item_revision:row.get(2)?,
                action_id:row.get(3)?, capability:row.get(4)?, plan_json:row.get(5)?, state:row.get(6)?,
            })).optional().map_err(repo_error)
    }

    /// Strict single-claim fencing: uncertain outcomes MUST NOT be automatically retried.
    pub fn claim_blueprint_action(
        &mut self,
        command_id: &str,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        let affected = self.execute("UPDATE orbit_blueprint_action_requests SET state='claimed'             WHERE command_id=?1 AND state='pending'", [command_id]).map_err(repo_error)?;
        if affected != 1 {
            return Err(RepositoryError::Conflict);
        }
        self.blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)
    }

    pub fn record_blueprint_action_result(
        &mut self,
        command_id: &str,
        result: &str,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        if !matches!(result, "succeeded" | "failed" | "uncertain") {
            return Err(RepositoryError::Conflict);
        }
        let changed = self.execute("UPDATE orbit_blueprint_action_requests SET state=?2             WHERE command_id=?1 AND state='claimed'", params![command_id,result]).map_err(repo_error)?;
        if changed == 0 {
            let current = self
                .blueprint_action_request(command_id)?
                .ok_or(RepositoryError::Conflict)?;
            return if current.state == result {
                Ok(current)
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        self.blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)
    }

    /// Explicitly resolve an `uncertain` outcome. This never re-claims the
    /// request and never re-executes the capability.
    pub fn reconcile_blueprint_action_result(
        &mut self,
        command_id: &str,
        result: &str,
    ) -> Result<BlueprintActionRequest, RepositoryError> {
        if !matches!(result, "succeeded" | "failed") {
            return Err(RepositoryError::Conflict);
        }
        let changed = self
            .execute(
                "UPDATE orbit_blueprint_action_requests SET state=?2
                 WHERE command_id=?1 AND state='uncertain'",
                params![command_id, result],
            )
            .map_err(repo_error)?;
        if changed == 0 {
            let current = self
                .blueprint_action_request(command_id)?
                .ok_or(RepositoryError::Conflict)?;
            return if current.state == result {
                Ok(current)
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        self.blueprint_action_request(command_id)?
            .ok_or(RepositoryError::Conflict)
    }
}

fn valid_command_id(command_id: &str) -> bool {
    !command_id.is_empty()
        && command_id.len() <= 128
        && command_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_application::ProjectRepository;
    use orbit_blueprint::{CapabilityDescriptor, CapabilityPermission};
    use orbit_domain::{Project, ProjectId};
    use serde_json::json;

    fn setup() -> (SqliteStore, CapabilityCatalog) {
        let mut store = SqliteStore::in_memory().unwrap();
        let project =
            Project::new(ProjectId::new("project-1").unwrap(), "Reference").expect("project");
        ProjectRepository::save(&mut store, project).expect("save project");
        let blueprint: orbit_blueprint::Blueprint = serde_json::from_value(json!({
            "id": "reference", "version": 1, "name": "Reference",
            "required_capabilities": ["orbit.reference.work-item-note"],
            "work_items": [{
                "id": "note-task", "name": "Note task", "initial_state": "open",
                "states": ["open", "closed"],
                "transitions": [{
                    "from": "closed", "to": "open",
                    "actions": [{
                        "id": "append_note",
                        "capability": "orbit.reference.work-item-note",
                        "description": "Append a note",
                        "parameters": {"note": "", "tag": "default"}
                    }]
                }]
            }]
        }))
        .expect("blueprint");
        let mut catalog = CapabilityCatalog::default();
        catalog
            .register(CapabilityDescriptor {
                id: "orbit.reference.work-item-note".into(),
                provider: "orbit".into(),
                permission: CapabilityPermission::Mutate,
                enabled: true,
            })
            .unwrap();
        store
            .publish_blueprint(&blueprint, &catalog.available())
            .unwrap();
        store
            .bind_existing_project_blueprint("project-1", "reference", 1)
            .unwrap();
        store
            .create_blueprint_work_item("item-1", "project-1", "note-task", BTreeMap::new())
            .unwrap();
        (store, catalog)
    }

    fn grants() -> BTreeSet<String> {
        BTreeSet::from(["orbit.reference.work-item-note".to_owned()])
    }

    #[test]
    fn request_merges_declared_defaults_with_user_inputs() {
        let (mut store, catalog) = setup();
        let mut inputs = BTreeMap::new();
        inputs.insert("note".to_owned(), json!("hello"));
        let request = store
            .request_blueprint_action_with_inputs(
                BlueprintActionIntent {
                    item_id: "item-1",
                    expected_revision: 0,
                    from: "closed",
                    action_id: "append_note",
                    command_id: "action-1",
                },
                &inputs,
                &catalog,
                &grants(),
            )
            .unwrap();
        assert_eq!(
            request.plan_json,
            r#"{"action_id":"append_note","capability":"orbit.reference.work-item-note","parameters":{"note":"hello","tag":"default"},"command_id":"action-1"}"#
        );
        // Replay with the same command ID and inputs is idempotent.
        assert_eq!(
            store
                .request_blueprint_action_with_inputs(
                    BlueprintActionIntent {
                        item_id: "item-1",
                        expected_revision: 0,
                        from: "closed",
                        action_id: "append_note",
                        command_id: "action-1",
                    },
                    &inputs,
                    &catalog,
                    &grants(),
                )
                .unwrap(),
            request
        );
    }

    #[test]
    fn request_rejects_invented_parameters_and_missing_grants() {
        let (mut store, catalog) = setup();
        let mut invented = BTreeMap::new();
        invented.insert("shell".to_owned(), json!("rm -rf /"));
        assert_eq!(
            store.request_blueprint_action_with_inputs(
                BlueprintActionIntent {
                    item_id: "item-1",
                    expected_revision: 0,
                    from: "closed",
                    action_id: "append_note",
                    command_id: "action-2",
                },
                &invented,
                &catalog,
                &grants(),
            ),
            Err(RepositoryError::Conflict)
        );
        assert!(
            store
                .capability_action_requests(None, None, 10)
                .unwrap()
                .is_empty()
        );
        let mut inputs = BTreeMap::new();
        inputs.insert("note".to_owned(), json!("hello"));
        assert!(
            store
                .request_blueprint_action_with_inputs(
                    BlueprintActionIntent {
                        item_id: "item-1",
                        expected_revision: 0,
                        from: "closed",
                        action_id: "append_note",
                        command_id: "action-2",
                    },
                    &inputs,
                    &catalog,
                    &BTreeSet::new(),
                )
                .is_err()
        );
        // A revision mismatch and an undeclared edge also fail closed.
        assert!(
            store
                .request_blueprint_action_with_inputs(
                    BlueprintActionIntent {
                        item_id: "item-1",
                        expected_revision: 7,
                        from: "closed",
                        action_id: "append_note",
                        command_id: "action-3",
                    },
                    &inputs,
                    &catalog,
                    &grants(),
                )
                .is_err()
        );
        assert!(
            store
                .request_blueprint_action_with_inputs(
                    BlueprintActionIntent {
                        item_id: "item-1",
                        expected_revision: 0,
                        from: "open",
                        action_id: "append_note",
                        command_id: "action-4",
                    },
                    &inputs,
                    &catalog,
                    &grants(),
                )
                .is_err()
        );
    }

    #[test]
    fn different_command_for_same_action_conflicts() {
        let (mut store, catalog) = setup();
        let intent = |command_id: &'static str| BlueprintActionIntent {
            item_id: "item-1",
            expected_revision: 0,
            from: "closed",
            action_id: "append_note",
            command_id,
        };
        store
            .request_blueprint_action(intent("action-1"), &catalog, &grants())
            .unwrap();
        assert_eq!(
            store.request_blueprint_action(intent("action-2"), &catalog, &grants()),
            Err(RepositoryError::Conflict)
        );
    }
}
