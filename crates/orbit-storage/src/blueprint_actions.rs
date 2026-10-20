//! Durable, explicitly authorized blueprint action intents. Execution belongs to
//! a separately registered capability adapter, never to blueprint evaluation.
use crate::{SqliteStore, repo_error};
use orbit_application::RepositoryError;
use orbit_blueprint::CapabilityCatalog;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
        let BlueprintActionIntent {
            item_id,
            expected_revision,
            from,
            action_id,
            command_id,
        } = intent;
        if command_id.trim().is_empty() || command_id.len() > 128 || item_id.trim().is_empty() {
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
        let plan = schema
            .plan_explicit_action(
                orbit_blueprint::ActionSelection {
                    item_kind: &item.kind,
                    from,
                    to: &item.state,
                    action_id,
                    command_id,
                },
                catalog,
                grants,
            )
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
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
}
