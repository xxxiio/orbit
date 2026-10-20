//! Blueprint-owned work items: persistent pinned schema and optimistic updates.
//! Existing domain tasks are deliberately unchanged.
use crate::{SqliteStore, repo_error};
use orbit_application::RepositoryError;
use orbit_blueprint::{Blueprint, BlueprintWorkItem, TransitionGuard};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

impl SqliteStore {
    pub(crate) fn ensure_blueprint_work_item_schema(&self) -> Result<(), RepositoryError> {
        self.execute_batch("CREATE TABLE IF NOT EXISTS orbit_blueprint_work_items (
            id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            kind TEXT NOT NULL, state TEXT NOT NULL, properties_json TEXT NOT NULL,
            revision INTEGER NOT NULL CHECK(revision >= 0),
            blueprint_id TEXT NOT NULL, blueprint_version INTEGER NOT NULL,
            FOREIGN KEY(blueprint_id, blueprint_version) REFERENCES orbit_blueprints(id, version));
            CREATE INDEX IF NOT EXISTS orbit_blueprint_items_project ON orbit_blueprint_work_items(project_id,id);
            CREATE TABLE IF NOT EXISTS orbit_blueprint_item_dependencies (
              project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
              item_id TEXT NOT NULL REFERENCES orbit_blueprint_work_items(id) ON DELETE CASCADE,
              prerequisite_id TEXT NOT NULL REFERENCES orbit_blueprint_work_items(id) ON DELETE CASCADE,
              PRIMARY KEY(item_id, prerequisite_id), CHECK(item_id <> prerequisite_id));
            CREATE INDEX IF NOT EXISTS orbit_blueprint_dependency_reverse ON orbit_blueprint_item_dependencies(prerequisite_id);")
            .map_err(repo_error)
    }

    fn pinned_schema(&self, project_id: &str) -> Result<Blueprint, RepositoryError> {
        let binding = self
            .project_blueprint_binding(project_id)?
            .ok_or(RepositoryError::Conflict)?;
        self.load_blueprint(&binding.blueprint_id, binding.version)?
            .ok_or(RepositoryError::Conflict)
    }

    pub fn create_blueprint_work_item(
        &mut self,
        id: &str,
        project_id: &str,
        kind: &str,
        properties: BTreeMap<String, Value>,
    ) -> Result<BlueprintWorkItem, RepositoryError> {
        let schema = self.pinned_schema(project_id)?;
        let item = schema
            .instantiate(id, project_id, kind, properties)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        let value = serde_json::to_string(&item.properties)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        self.execute("INSERT INTO orbit_blueprint_work_items(id,project_id,kind,state,properties_json,revision,blueprint_id,blueprint_version)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![item.id,item.project_id,item.kind,item.state,value,item.revision,schema.id,schema.version])
            .map_err(repo_error)?;
        Ok(item)
    }

    /// Apply a published template to a project in one atomic authority operation.
    /// The caller chooses an isolated namespace. Any collision fails the entire
    /// application; partial templates must never become visible to other readers.
    /// A savepoint permits this operation inside an outer accepted-command transaction.
    pub fn apply_blueprint_template(
        &mut self,
        project_id: &str,
        template_id: &str,
        namespace: &str,
    ) -> Result<orbit_blueprint::TemplatePlan, RepositoryError> {
        // Build and validate the complete plan before opening the write boundary.
        let schema = self.pinned_schema(project_id)?;
        let plan = schema
            .plan_template(template_id, project_id, namespace)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        self.execute_batch("SAVEPOINT orbit_blueprint_template_apply")
            .map_err(repo_error)?;
        let result = (|| -> Result<(), RepositoryError> {
            for item in &plan.items {
                // Do not replay or overwrite existing IDs: every template
                // application creates a distinct, immutable instance.
                let created = self.create_blueprint_work_item(
                    &item.id,
                    project_id,
                    &item.kind,
                    item.properties.clone(),
                )?;
                if created != *item {
                    return Err(RepositoryError::Conflict);
                }
            }
            for (item_id, prerequisite_id) in &plan.prerequisites {
                self.add_blueprint_work_item_prerequisite(item_id, prerequisite_id)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Err(e) =
                    self.execute_batch("RELEASE SAVEPOINT orbit_blueprint_template_apply")
                {
                    let _ = self.execute_batch("ROLLBACK TO SAVEPOINT orbit_blueprint_template_apply; RELEASE SAVEPOINT orbit_blueprint_template_apply");
                    return Err(repo_error(e));
                }
                Ok(plan)
            }
            Err(e) => {
                self.execute_batch("ROLLBACK TO SAVEPOINT orbit_blueprint_template_apply; RELEASE SAVEPOINT orbit_blueprint_template_apply")
                    .map_err(repo_error)?;
                Err(e)
            }
        }
    }

    pub fn blueprint_work_item(
        &self,
        id: &str,
    ) -> Result<Option<BlueprintWorkItem>, RepositoryError> {
        self.query_row("SELECT id,project_id,kind,state,properties_json,revision FROM orbit_blueprint_work_items WHERE id=?1", [id],
            |row| {
                let serialized: String = row.get(4)?;
                let properties: BTreeMap<String, Value> = serde_json::from_str(&serialized)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
                Ok(BlueprintWorkItem {id:row.get(0)?,project_id:row.get(1)?,kind:row.get(2)?,state:row.get(3)?,properties,revision:row.get(5)?})
            }).optional().map_err(repo_error)
    }

    pub fn transition_blueprint_work_item(
        &mut self,
        id: &str,
        to: &str,
        revision: u64,
        capabilities: &BTreeSet<String>,
    ) -> Result<BlueprintWorkItem, RepositoryError> {
        let old = self
            .blueprint_work_item(id)?
            .ok_or(RepositoryError::Conflict)?;
        let schema = self.pinned_schema(&old.project_id)?;
        let next = schema
            .transition(&old, to, revision, capabilities)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        // Evaluate relationship predicates at the authority, not from a client-supplied snapshot.
        let edge = schema
            .item_kind(&old.kind)
            .map_err(|e| RepositoryError::Other(e.to_string()))?
            .transitions
            .iter()
            .find(|t| t.from == old.state && t.to == to)
            .ok_or(RepositoryError::Conflict)?;
        for guard in &edge.guards {
            if let TransitionGuard::AllPrerequisitesInState { state } = guard {
                let count: i64 = self
                    .query_row(
                        "SELECT COUNT(*) FROM orbit_blueprint_item_dependencies d \
                     JOIN orbit_blueprint_work_items p ON p.id=d.prerequisite_id \
                     WHERE d.item_id=?1 AND p.state<>?2",
                        params![id, state],
                        |row| row.get(0),
                    )
                    .map_err(repo_error)?;
                if count > 0 {
                    return Err(RepositoryError::Conflict);
                }
            }
        }
        self.save_blueprint_work_item(&old, &next)?;
        Ok(next)
    }

    /// Add a prerequisite within the same project. Cycles and cross-project edges are rejected.
    pub fn add_blueprint_work_item_prerequisite(
        &mut self,
        item_id: &str,
        prerequisite_id: &str,
    ) -> Result<(), RepositoryError> {
        if item_id == prerequisite_id {
            return Err(RepositoryError::Conflict);
        }
        let item = self
            .blueprint_work_item(item_id)?
            .ok_or(RepositoryError::Conflict)?;
        let prerequisite = self
            .blueprint_work_item(prerequisite_id)?
            .ok_or(RepositoryError::Conflict)?;
        if item.project_id != prerequisite.project_id {
            return Err(RepositoryError::Conflict);
        }
        let cycle: i64 = self
            .query_row(
                "WITH RECURSIVE ancestors(id) AS ( \
              SELECT prerequisite_id FROM orbit_blueprint_item_dependencies WHERE item_id=?1 \
              UNION SELECT d.prerequisite_id FROM orbit_blueprint_item_dependencies d \
              JOIN ancestors a ON d.item_id=a.id) \
             SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)",
                params![prerequisite_id, item_id],
                |row| row.get(0),
            )
            .map_err(repo_error)?;
        if cycle != 0 {
            return Err(RepositoryError::Conflict);
        }
        self.execute("INSERT OR IGNORE INTO orbit_blueprint_item_dependencies(project_id,item_id,prerequisite_id) VALUES (?1,?2,?3)",
            params![item.project_id,item_id,prerequisite_id]).map_err(repo_error)?;
        Ok(())
    }

    pub fn blueprint_work_item_prerequisites(
        &self,
        item_id: &str,
    ) -> Result<Vec<String>, RepositoryError> {
        let mut statement = self.prepare("SELECT prerequisite_id FROM orbit_blueprint_item_dependencies WHERE item_id=?1 ORDER BY prerequisite_id").map_err(repo_error)?;
        let result = statement
            .query_map([item_id], |row| row.get(0))
            .map_err(repo_error)?;
        result
            .collect::<Result<Vec<String>, _>>()
            .map_err(repo_error)
    }

    pub fn edit_blueprint_work_item(
        &mut self,
        id: &str,
        revision: u64,
        patch: BTreeMap<String, Value>,
    ) -> Result<BlueprintWorkItem, RepositoryError> {
        let old = self
            .blueprint_work_item(id)?
            .ok_or(RepositoryError::Conflict)?;
        let schema = self.pinned_schema(&old.project_id)?;
        let next = schema
            .update_properties(&old, revision, patch)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        self.save_blueprint_work_item(&old, &next)?;
        Ok(next)
    }

    fn save_blueprint_work_item(
        &mut self,
        old: &BlueprintWorkItem,
        next: &BlueprintWorkItem,
    ) -> Result<(), RepositoryError> {
        let serialized = serde_json::to_string(&next.properties)
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        let count = self
            .execute(
                "UPDATE orbit_blueprint_work_items SET state=?1,properties_json=?2,revision=?3
            WHERE id=?4 AND project_id=?5 AND revision=?6",
                params![
                    next.state,
                    serialized,
                    next.revision,
                    old.id,
                    old.project_id,
                    old.revision
                ],
            )
            .map_err(repo_error)?;
        if count != 1 {
            return Err(RepositoryError::Conflict);
        }
        Ok(())
    }
}
