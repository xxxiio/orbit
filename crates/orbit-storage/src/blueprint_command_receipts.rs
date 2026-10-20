//! Idempotent authoritative blueprint mutations. The receipt is committed with
//! the mutation in the same SQLite savepoint, including under outer transactions.
use crate::{SqliteStore, repo_error};
use orbit_application::RepositoryError;
use orbit_blueprint::Blueprint;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::BTreeSet;

impl SqliteStore {
    pub(crate) fn ensure_blueprint_receipts_schema(&self) -> Result<(), RepositoryError> {
        self.execute_batch("CREATE TABLE IF NOT EXISTS orbit_blueprint_command_receipts (            command_id TEXT PRIMARY KEY, device_id TEXT NOT NULL,            route TEXT NOT NULL, request_json TEXT NOT NULL CHECK(json_valid(request_json)),            result_json TEXT NOT NULL CHECK(json_valid(result_json)));")
            .map_err(repo_error)
    }

    /// An accepted command is immutable. Matching retries return the original
    /// response; command-id reuse with different requests or devices conflicts.
    pub fn apply_blueprint_command(
        &mut self,
        command_id: &str,
        device_id: &str,
        route: &str,
        body: &Value,
    ) -> Result<Value, RepositoryError> {
        if command_id.is_empty()
            || command_id.len() > 256
            || !command_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b))
            || device_id.is_empty()
            || device_id.len() > 256
            || !body.is_object()
        {
            return Err(RepositoryError::Conflict);
        }
        let request =
            serde_json::to_string(body).map_err(|e| RepositoryError::Other(e.to_string()))?;
        self.execute_batch("SAVEPOINT orbit_blueprint_command_receipt")
            .map_err(repo_error)?;
        let result = (|| -> Result<Value, RepositoryError> {
            let existing: Option<(String,String,String,String)> = self.query_row(
                "SELECT device_id,route,request_json,result_json FROM orbit_blueprint_command_receipts WHERE command_id=?1",
                [command_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))
                .optional().map_err(repo_error)?;
            if let Some((owner, prior_route, prior_request, prior_result)) = existing {
                if owner != device_id || prior_route != route || prior_request != request {
                    return Err(RepositoryError::Conflict);
                }
                return serde_json::from_str(&prior_result).map_err(|e| {
                    RepositoryError::CorruptData {
                        detail: e.to_string(),
                    }
                });
            }
            let field = |key: &str| -> Result<&str, RepositoryError> {
                body.get(key)
                    .and_then(Value::as_str)
                    .ok_or(RepositoryError::Conflict)
            };
            let revision = |key: &str| -> Result<u64, RepositoryError> {
                body.get(key)
                    .and_then(Value::as_u64)
                    .ok_or(RepositoryError::Conflict)
            };
            let result = match route {
                "blueprints" => {
                    let blueprint: Blueprint = serde_json::from_value(
                        body.get("definition")
                            .cloned()
                            .ok_or(RepositoryError::Conflict)?,
                    )
                    .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    self.publish_blueprint(&blueprint, &BTreeSet::new())?;
                    serde_json::to_value(blueprint)
                        .map_err(|e| RepositoryError::Other(e.to_string()))?
                }
                "blueprints/install" => {
                    let blueprint = orbit_blueprint::bundled::bundled_blueprint(field("id")?)
                        .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    self.publish_blueprint(&blueprint, &BTreeSet::new())?;
                    serde_json::to_value(blueprint)
                        .map_err(|e| RepositoryError::Other(e.to_string()))?
                }
                "blueprints/bind" => json!(self.bind_existing_project_blueprint(
                    field("project_id")?,
                    field("id")?,
                    u32::try_from(revision("version")?).map_err(|_| RepositoryError::Conflict)?
                )?),
                "blueprints/items" => {
                    let props: std::collections::BTreeMap<String, Value> = serde_json::from_value(
                        body.get("properties").cloned().unwrap_or(json!({})),
                    )
                    .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    json!(self.create_blueprint_work_item(
                        field("id")?,
                        field("project_id")?,
                        field("kind")?,
                        props
                    )?)
                }
                "blueprints/transition" => json!(self.transition_blueprint_work_item(
                    field("id")?,
                    field("to")?,
                    revision("expected_revision")?,
                    &BTreeSet::new()
                )?),
                "blueprints/properties" => {
                    let changes = serde_json::from_value(
                        body.get("changes")
                            .cloned()
                            .ok_or(RepositoryError::Conflict)?,
                    )
                    .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    json!(self.edit_blueprint_work_item(
                        field("id")?,
                        revision("expected_revision")?,
                        changes
                    )?)
                }
                "blueprints/template" => json!(self.apply_blueprint_template(
                    field("project_id")?,
                    field("template_id")?,
                    field("namespace")?
                )?),
                _ => return Err(RepositoryError::Conflict),
            };
            self.execute("INSERT INTO orbit_blueprint_command_receipts (command_id,device_id,route,request_json,result_json) VALUES (?1,?2,?3,?4,?5)",
                params![command_id,device_id,route,request,result.to_string()]).map_err(repo_error)?;
            Ok(result)
        })();
        match result {
            Ok(value) => {
                self.execute_batch("RELEASE SAVEPOINT orbit_blueprint_command_receipt")
                    .map_err(repo_error)?;
                Ok(value)
            }
            Err(error) => {
                self.execute_batch("ROLLBACK TO SAVEPOINT orbit_blueprint_command_receipt; RELEASE SAVEPOINT orbit_blueprint_command_receipt").map_err(repo_error)?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_install_replays_original_result_and_binds_device() {
        let mut store = SqliteStore::in_memory().unwrap();
        let payload = json!({"id":"development"});
        let first = store
            .apply_blueprint_command("command-1", "device-a", "blueprints/install", &payload)
            .unwrap();
        assert_eq!(
            first,
            store
                .apply_blueprint_command("command-1", "device-a", "blueprints/install", &payload)
                .unwrap()
        );
        assert!(
            store
                .apply_blueprint_command("command-1", "device-b", "blueprints/install", &payload)
                .is_err()
        );
        assert!(
            store
                .apply_blueprint_command(
                    "command-1",
                    "device-a",
                    "blueprints/install",
                    &json!({"id":"learning"})
                )
                .is_err()
        );
        assert!(store.load_blueprint("development", 1).unwrap().is_some());
    }
    #[test]
    fn invalid_mutation_leaves_no_receipt_or_partial_blueprint() {
        let mut store = SqliteStore::in_memory().unwrap();
        assert!(
            store
                .apply_blueprint_command(
                    "command-2",
                    "device-a",
                    "blueprints/bind",
                    &json!({"project_id":"missing","id":"missing","version":1})
                )
                .is_err()
        );
        let count: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM orbit_blueprint_command_receipts",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert!(
            store
                .apply_blueprint_command(
                    "command-2",
                    "device-a",
                    "blueprints/install",
                    &json!({"id":"development"})
                )
                .is_ok()
        );
    }
}
