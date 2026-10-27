//! Durable, explicitly authorized blueprint action intents. Execution belongs to
//! a separately registered capability adapter, never to blueprint evaluation.
use crate::{SqliteStore, is_unique_violation, repo_error};
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

/// The durable envelope of one explicit action intent: the request, the
/// transition it was fenced to when it was recorded, and the executor claim (if
/// any). The envelope is what a connected runner needs; it never contains a
/// provider, a path or code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlueprintActionEnvelope {
    pub request: BlueprintActionRequest,
    pub from_state: String,
    pub to_state: String,
    pub created_at_ms: i64,
    pub executor_device_id: Option<String>,
    pub claim_token: Option<String>,
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
        // `from_state`/`to_state`/`created_at_ms` record the envelope a
        // connected executor needs in order to replay exactly what was shown to
        // the caller. `executor_device_id`/`claim_token` are the durable claim
        // fence: once written they are never reset, so a restarted server cannot
        // return a claimed intent to the pending pool.
        self.execute_batch("CREATE TABLE IF NOT EXISTS orbit_blueprint_action_requests (            command_id TEXT PRIMARY KEY,            item_id TEXT NOT NULL REFERENCES orbit_blueprint_work_items(id) ON DELETE CASCADE,            item_revision INTEGER NOT NULL CHECK(item_revision >= 0),            action_id TEXT NOT NULL, capability TEXT NOT NULL,            plan_json TEXT NOT NULL CHECK(json_valid(plan_json)),            state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','claimed','succeeded','failed','uncertain')),            from_state TEXT NOT NULL DEFAULT '',            to_state TEXT NOT NULL DEFAULT '',            created_at_ms INTEGER NOT NULL DEFAULT 0,            executor_device_id TEXT,            claim_token TEXT,            UNIQUE(item_id, item_revision, action_id));            CREATE INDEX IF NOT EXISTS orbit_blueprint_actions_pending ON orbit_blueprint_action_requests(state,command_id);")
            .map_err(repo_error)?;
        self.upgrade_blueprint_action_columns()?;
        // A claim token identifies exactly one intent. The partial index allows
        // the historical `NULL` (never claimed) state for every other row.
        self.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS orbit_blueprint_action_claim_tokens
                 ON orbit_blueprint_action_requests(claim_token) WHERE claim_token IS NOT NULL;",
        )
        .map_err(repo_error)
    }

    /// Additive upgrade for databases created before the connected claim fence
    /// existed. `CREATE TABLE IF NOT EXISTS` cannot add columns, and SQLite has
    /// no `ADD COLUMN IF NOT EXISTS`, so the current columns are inspected
    /// first. Names and definitions are compile-time constants.
    ///
    /// Rows written before I4 read back with an empty transition envelope and a
    /// creation moment of `0`: the server reports what is durable rather than
    /// reconstructing a `from`/`to` it never recorded.
    fn upgrade_blueprint_action_columns(&self) -> Result<(), RepositoryError> {
        let mut statement = self
            .prepare("PRAGMA table_info('orbit_blueprint_action_requests')")
            .map_err(repo_error)?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(repo_error)?
            .collect::<Result<std::collections::BTreeSet<_>, _>>()
            .map_err(repo_error)?;
        for (name, definition) in [
            ("from_state", "TEXT NOT NULL DEFAULT ''"),
            ("to_state", "TEXT NOT NULL DEFAULT ''"),
            ("created_at_ms", "INTEGER NOT NULL DEFAULT 0"),
            ("executor_device_id", "TEXT"),
            ("claim_token", "TEXT"),
        ] {
            if columns.contains(name) {
                continue;
            }
            self.execute_batch(&format!(
                "ALTER TABLE orbit_blueprint_action_requests ADD COLUMN {name} {definition}"
            ))
            .map_err(repo_error)?;
        }
        Ok(())
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
        // The envelope is recorded with the intent so a connected executor can
        // be handed the exact transition and creation moment later, without the
        // server inventing either from the item's current state.
        self.execute("INSERT OR IGNORE INTO orbit_blueprint_action_requests             (command_id,item_id,item_revision,action_id,capability,plan_json,from_state,to_state,created_at_ms)             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![command_id,item_id,expected_revision,plan.invocation.action_id,
                plan.invocation.capability,payload,from,to,now_ms()]).map_err(repo_error)?;
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

    /// The durable envelope of one intent, including the transition it was
    /// fenced to, its creation moment and the executor claim.
    pub fn blueprint_action_envelope(
        &self,
        command_id: &str,
    ) -> Result<Option<BlueprintActionEnvelope>, RepositoryError> {
        self.query_row(
            "SELECT command_id,item_id,item_revision,action_id,capability,plan_json,state,
                    from_state,to_state,created_at_ms,executor_device_id,claim_token
             FROM orbit_blueprint_action_requests WHERE command_id=?1",
            [command_id],
            blueprint_action_envelope_from_row,
        )
        .optional()
        .map_err(repo_error)
    }

    /// List intents newest first by their recorded creation moment. `state` is
    /// matched exactly; an unknown state simply matches nothing.
    pub fn blueprint_action_envelopes(
        &self,
        item_id: Option<&str>,
        state: Option<&str>,
        limit: usize,
    ) -> Result<Vec<BlueprintActionEnvelope>, RepositoryError> {
        let mut statement = self
            .prepare(
                "SELECT command_id,item_id,item_revision,action_id,capability,plan_json,state,
                        from_state,to_state,created_at_ms,executor_device_id,claim_token
                 FROM orbit_blueprint_action_requests
                 WHERE (?1 IS NULL OR item_id=?1) AND (?2 IS NULL OR state=?2)
                 ORDER BY created_at_ms, command_id LIMIT ?3",
            )
            .map_err(repo_error)?;
        let rows = statement
            .query_map(
                params![item_id, state, clamp_limit(limit)],
                blueprint_action_envelope_from_row,
            )
            .map_err(repo_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(repo_error)
    }

    /// Whether some *other* intent already owns this claim token. Tokens are
    /// per-claim evidence, so reusing one across intents is refused rather than
    /// silently aliasing two claims.
    pub fn claim_token_in_use(
        &self,
        claim_token: &str,
        except_command_id: &str,
    ) -> Result<bool, RepositoryError> {
        self.query_row(
            "SELECT EXISTS(SELECT 1 FROM orbit_blueprint_action_requests
                 WHERE claim_token=?1 AND command_id<>?2)",
            params![claim_token, except_command_id],
            |row| row.get(0),
        )
        .map_err(repo_error)
    }

    /// Atomically claim one pending intent for an enrolled executor.
    ///
    /// The single `pending -> claimed` statement is the whole fence: a second
    /// executor cannot win the same intent, and replaying the identical
    /// device+token returns the same claim instead of claiming twice. A claim is
    /// never reset; recovery is an explicit reconcile, not a state rollback.
    pub fn claim_blueprint_action_for_executor(
        &mut self,
        device_id: &str,
        command_id: &str,
        claim_token: &str,
    ) -> Result<BlueprintActionEnvelope, RepositoryError> {
        if !valid_command_id(command_id)
            || device_id.trim().is_empty()
            || device_id.len() > 128
            || !valid_claim_token(claim_token)
        {
            return Err(RepositoryError::Conflict);
        }
        let changed = self
            .execute(
                "UPDATE orbit_blueprint_action_requests
                 SET state='claimed', executor_device_id=?2, claim_token=?3
                 WHERE command_id=?1 AND state='pending'",
                params![command_id, device_id, claim_token],
            )
            .map_err(|error| {
                // The unique claim-token index makes concurrent reuse a storage
                // refusal, never two claims sharing one token.
                if is_unique_violation(&error) {
                    RepositoryError::Conflict
                } else {
                    repo_error(error)
                }
            })?;
        if changed != 1 {
            // Same device + same token replays; anything else conflicts.
            let existing = self
                .blueprint_action_envelope(command_id)?
                .ok_or(RepositoryError::Conflict)?;
            return if existing.request.state == "claimed"
                && existing.executor_device_id.as_deref() == Some(device_id)
                && existing.claim_token.as_deref() == Some(claim_token)
            {
                Ok(existing)
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        self.blueprint_action_envelope(command_id)?
            .ok_or(RepositoryError::Conflict)
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

fn valid_claim_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}

fn clamp_limit(value: usize) -> i64 {
    i64::try_from(value.clamp(1, 1_000)).unwrap_or(1_000)
}

/// Wall-clock creation moment of a newly recorded intent. Callers cannot supply
/// it: a client could otherwise reorder or backdate the durable journal.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

fn blueprint_action_envelope_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<BlueprintActionEnvelope> {
    Ok(BlueprintActionEnvelope {
        request: BlueprintActionRequest {
            command_id: row.get(0)?,
            item_id: row.get(1)?,
            item_revision: row.get(2)?,
            action_id: row.get(3)?,
            capability: row.get(4)?,
            plan_json: row.get(5)?,
            state: row.get(6)?,
        },
        from_state: row.get(7)?,
        to_state: row.get(8)?,
        created_at_ms: row.get(9)?,
        executor_device_id: row.get(10)?,
        claim_token: row.get(11)?,
    })
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
    fn a_pre_claim_database_gains_the_envelope_and_claim_columns() {
        let store = SqliteStore::in_memory().unwrap();
        // The shape this table had before the connected claim fence existed.
        store
            .execute_batch(
                "DROP TABLE orbit_blueprint_action_requests;
                 CREATE TABLE orbit_blueprint_action_requests (
                     command_id TEXT PRIMARY KEY,
                     item_id TEXT NOT NULL,
                     item_revision INTEGER NOT NULL,
                     action_id TEXT NOT NULL,
                     capability TEXT NOT NULL,
                     plan_json TEXT NOT NULL,
                     state TEXT NOT NULL DEFAULT 'pending',
                     UNIQUE(item_id,item_revision,action_id));",
            )
            .unwrap();
        store.ensure_blueprint_actions_schema().unwrap();
        // The envelope, claim and creation columns exist and read as absent for
        // rows written before them, rather than failing the whole journal.
        store
            .query_row(
                "SELECT from_state,to_state,created_at_ms,executor_device_id,claim_token
                 FROM orbit_blueprint_action_requests LIMIT 1",
                [],
                |_| Ok(()),
            )
            .optional()
            .expect("envelope columns are queryable");
        // Re-running the schema upgrade is idempotent.
        store.ensure_blueprint_actions_schema().unwrap();
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
