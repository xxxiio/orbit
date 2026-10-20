//! Version-pinned blueprint storage. Never rewrites published versions or project bindings.
use crate::{SqliteStore, repo_error};
use orbit_application::RepositoryError;
use orbit_blueprint::{Blueprint, BlueprintBinding};
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeSet;

impl SqliteStore {
    pub(crate) fn ensure_blueprint_schema(&self) -> Result<(), RepositoryError> {
        self.execute_batch("\
            CREATE TABLE IF NOT EXISTS orbit_blueprints (\
              id TEXT NOT NULL, version INTEGER NOT NULL CHECK(version > 0),\
              manifest_json TEXT NOT NULL, PRIMARY KEY(id, version));\
            CREATE TABLE IF NOT EXISTS orbit_project_blueprints (\
              project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,\
              blueprint_id TEXT NOT NULL, blueprint_version INTEGER NOT NULL,\
              FOREIGN KEY(blueprint_id, blueprint_version) REFERENCES orbit_blueprints(id, version));\
        ").map_err(repo_error)
    }

    pub fn publish_blueprint(
        &mut self,
        blueprint: &Blueprint,
        capabilities: &BTreeSet<String>,
    ) -> Result<(), RepositoryError> {
        blueprint
            .validate(capabilities)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        let serialized = blueprint
            .to_json()
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        let existing: Option<String> = self
            .query_row(
                "SELECT manifest_json FROM orbit_blueprints WHERE id=?1 AND version=?2",
                params![blueprint.id, blueprint.version],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        if let Some(value) = existing {
            // Compare logical manifests, not whitespace/order of imported JSON.
            let existing: Blueprint =
                serde_json::from_str(&value).map_err(|e| RepositoryError::CorruptData {
                    detail: e.to_string(),
                })?;
            return if existing == *blueprint {
                Ok(())
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        self.execute(
            "INSERT INTO orbit_blueprints (id,version,manifest_json) VALUES (?1,?2,?3)",
            params![blueprint.id, blueprint.version, serialized],
        )
        .map_err(repo_error)?;
        Ok(())
    }

    pub fn load_blueprint(
        &self,
        id: &str,
        version: u32,
    ) -> Result<Option<Blueprint>, RepositoryError> {
        let raw: Option<String> = self
            .query_row(
                "SELECT manifest_json FROM orbit_blueprints WHERE id=?1 AND version=?2",
                params![id, version],
                |r| r.get(0),
            )
            .optional()
            .map_err(repo_error)?;
        raw.map(|s| {
            serde_json::from_str(&s).map_err(|e| RepositoryError::CorruptData {
                detail: e.to_string(),
            })
        })
        .transpose()
    }

    /// Enumerate all published immutable revisions in stable order. Consumers can
    /// discover versions without scanning or interpreting SQLite internals.
    pub fn list_blueprint_versions(
        &self,
        id: Option<&str>,
    ) -> Result<Vec<(String, u32)>, RepositoryError> {
        let mut result = Vec::new();
        if let Some(id) = id {
            let mut statement = self
                .connection
                .prepare("SELECT id,version FROM orbit_blueprints WHERE id=?1 ORDER BY version")
                .map_err(repo_error)?;
            let rows = statement
                .query_map([id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
                })
                .map_err(repo_error)?;
            for row in rows {
                result.push(row.map_err(repo_error)?);
            }
        } else {
            let mut statement = self
                .connection
                .prepare("SELECT id,version FROM orbit_blueprints ORDER BY id,version")
                .map_err(repo_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
                })
                .map_err(repo_error)?;
            for row in rows {
                result.push(row.map_err(repo_error)?);
            }
        }
        Ok(result)
    }

    /// Publish a manifest from an external JSON document. Validation happens
    /// before persistence; published versions cannot be silently overwritten.
    pub fn import_blueprint_json(
        &mut self,
        json: &str,
        capabilities: &BTreeSet<String>,
    ) -> Result<Blueprint, RepositoryError> {
        let parsed = Blueprint::from_json(json, capabilities)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        self.publish_blueprint(&parsed, capabilities)?;
        Ok(parsed)
    }

    pub fn export_blueprint_json(
        &self,
        id: &str,
        version: u32,
    ) -> Result<Option<String>, RepositoryError> {
        self.load_blueprint(id, version)?
            .map(|manifest| {
                manifest
                    .to_json()
                    .map_err(|e| RepositoryError::Other(e.to_string()))
            })
            .transpose()
    }

    /// Bind an existing project only if it has no blueprint. A missing binding
    /// is explicit legacy state, not an implicit upgrade to the latest version.
    /// This command is safe to replay and refuses to rewrite another binding.
    pub fn bind_existing_project_blueprint(
        &mut self,
        project_id: &str,
        blueprint_id: &str,
        version: u32,
    ) -> Result<BlueprintBinding, RepositoryError> {
        let binding = BlueprintBinding {
            project_id: project_id.to_owned(),
            blueprint_id: blueprint_id.to_owned(),
            version,
        };
        self.bind_project_blueprint(&binding)?;
        Ok(binding)
    }

    /// First binding wins; blueprint upgrades require a separate explicit operation.
    pub fn bind_project_blueprint(
        &mut self,
        binding: &BlueprintBinding,
    ) -> Result<(), RepositoryError> {
        if binding.project_id.trim().is_empty() || binding.version == 0 {
            return Err(RepositoryError::Conflict);
        }
        let existing = self.project_blueprint_binding(&binding.project_id)?;
        if let Some(old) = existing {
            return if old == *binding {
                Ok(())
            } else {
                Err(RepositoryError::Conflict)
            };
        }
        let exists: bool = self
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [&binding.project_id],
                |r| r.get(0),
            )
            .map_err(repo_error)?;
        if !exists
            || self
                .load_blueprint(&binding.blueprint_id, binding.version)?
                .is_none()
        {
            return Err(RepositoryError::Conflict);
        }
        self.execute("INSERT INTO orbit_project_blueprints(project_id,blueprint_id,blueprint_version) VALUES (?1,?2,?3)",
            params![binding.project_id, binding.blueprint_id, binding.version]).map_err(repo_error)?;
        Ok(())
    }

    pub fn project_blueprint_binding(
        &self,
        project_id: &str,
    ) -> Result<Option<BlueprintBinding>, RepositoryError> {
        self.query_row("SELECT project_id,blueprint_id,blueprint_version FROM orbit_project_blueprints WHERE project_id=?1", [project_id],
            |r| Ok(BlueprintBinding {project_id:r.get(0)?,blueprint_id:r.get(1)?,version:r.get(2)?}))
            .optional().map_err(repo_error)
    }
}
