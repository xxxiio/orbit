//! Host-owned blueprint registry operations. Transports must never open a
//! second authority database to install or bind a blueprint.
use crate::AuthoritativeHost;
use orbit_application::RepositoryError;
use orbit_blueprint::{Blueprint, BlueprintBinding};
use std::collections::BTreeSet;

impl AuthoritativeHost {
    /// Publish an immutable definition. Installed capabilities are supplied by
    /// trusted host composition, not by an untrusted manifest.
    pub fn publish_project_blueprint(
        &mut self,
        definition: &Blueprint,
        installed_capabilities: &BTreeSet<String>,
    ) -> Result<(), RepositoryError> {
        self.authority
            .conn
            .publish_blueprint(definition, installed_capabilities)
    }

    pub fn import_project_blueprint_json(
        &mut self,
        document: &str,
        installed_capabilities: &BTreeSet<String>,
    ) -> Result<Blueprint, RepositoryError> {
        self.authority
            .conn
            .import_blueprint_json(document, installed_capabilities)
    }

    pub fn project_blueprint(
        &self,
        id: &str,
        version: u32,
    ) -> Result<Option<Blueprint>, RepositoryError> {
        self.authority.conn.load_blueprint(id, version)
    }

    pub fn export_project_blueprint_json(
        &self,
        id: &str,
        version: u32,
    ) -> Result<Option<String>, RepositoryError> {
        self.authority.conn.export_blueprint_json(id, version)
    }

    pub fn available_blueprint_versions(
        &self,
        id: Option<&str>,
    ) -> Result<Vec<(String, u32)>, RepositoryError> {
        self.authority.conn.list_blueprint_versions(id)
    }

    /// Explicitly pin an existing project to a published revision. Never
    /// silently upgrade a binding when newer manifests are installed.
    pub fn activate_project_blueprint(
        &mut self,
        project_id: &str,
        id: &str,
        version: u32,
    ) -> Result<BlueprintBinding, RepositoryError> {
        self.authority
            .conn
            .bind_existing_project_blueprint(project_id, id, version)
    }

    pub fn active_project_blueprint(
        &self,
        project_id: &str,
    ) -> Result<Option<BlueprintBinding>, RepositoryError> {
        self.authority.conn.project_blueprint_binding(project_id)
    }
}

impl AuthoritativeHost {
    /// Create a typed blueprint work item under a project's pinned schema.
    pub fn create_project_work_item(
        &mut self,
        id: &str,
        project_id: &str,
        kind: &str,
        properties: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, RepositoryError> {
        self.authority
            .conn
            .create_blueprint_work_item(id, project_id, kind, properties)
    }

    pub fn project_work_item(
        &self,
        id: &str,
    ) -> Result<Option<orbit_blueprint::BlueprintWorkItem>, RepositoryError> {
        self.authority.conn.blueprint_work_item(id)
    }

    /// Only trusted application composition may supply capability grants.
    pub fn transition_project_work_item(
        &mut self,
        id: &str,
        to: &str,
        expected_revision: u64,
        granted_capabilities: &BTreeSet<String>,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, RepositoryError> {
        self.authority.conn.transition_blueprint_work_item(
            id,
            to,
            expected_revision,
            granted_capabilities,
        )
    }

    pub fn add_project_work_item_prerequisite(
        &mut self,
        item_id: &str,
        prerequisite_id: &str,
    ) -> Result<(), RepositoryError> {
        self.authority
            .conn
            .add_blueprint_work_item_prerequisite(item_id, prerequisite_id)
    }

    pub fn project_work_item_prerequisites(
        &self,
        item_id: &str,
    ) -> Result<Vec<String>, RepositoryError> {
        self.authority
            .conn
            .blueprint_work_item_prerequisites(item_id)
    }

    pub fn update_project_work_item_properties(
        &mut self,
        id: &str,
        expected_revision: u64,
        changes: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, RepositoryError> {
        self.authority
            .conn
            .edit_blueprint_work_item(id, expected_revision, changes)
    }
}

impl AuthoritativeHost {
    /// Explicit opt-in installation of the bundled development blueprint.
    /// Never binds existing projects or silently replaces published revisions.
    pub fn install_builtin_development_blueprint(&mut self) -> Result<Blueprint, RepositoryError> {
        self.import_project_blueprint_json(
            include_str!("../../../blueprints/development.json"),
            &BTreeSet::new(),
        )
    }
}

impl AuthoritativeHost {
    /// Atomically instantiate a named template under the project's pinned blueprint.
    /// This never starts an agent, invokes a plugin, or mutates unrelated projects.
    pub fn apply_project_blueprint_template(
        &mut self,
        project_id: &str,
        template_id: &str,
        namespace: &str,
    ) -> Result<orbit_blueprint::TemplatePlan, RepositoryError> {
        self.authority
            .conn
            .apply_blueprint_template(project_id, template_id, namespace)
    }
}

impl AuthoritativeHost {
    /// Explicit opt-in publication of a known bundled manifest. No existing
    /// project is bound or upgraded, and no capability is installed by this call.
    pub fn install_bundled_project_blueprint(
        &mut self,
        id: &str,
    ) -> Result<Blueprint, RepositoryError> {
        let blueprint = orbit_blueprint::bundled::bundled_blueprint(id)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        self.publish_project_blueprint(&blueprint, &BTreeSet::new())?;
        Ok(blueprint)
    }

    /// Metadata for explicit installation UI; does not mutate the registry.
    pub fn bundled_project_blueprints(
        &self,
    ) -> &'static [orbit_blueprint::bundled::BundledBlueprintDescriptor] {
        orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
    }
}

impl AuthoritativeHost {
    /// Records explicit authorized intent only. A separate trusted capability
    /// executor must claim the record before any external side effect.
    pub fn request_project_work_item_action(
        &mut self,
        intent: orbit_storage::blueprint_actions::BlueprintActionIntent<'_>,
        catalog: &orbit_blueprint::CapabilityCatalog,
        grants: &std::collections::BTreeSet<String>,
    ) -> Result<orbit_storage::blueprint_actions::BlueprintActionRequest, RepositoryError> {
        self.authority
            .conn
            .request_blueprint_action(intent, catalog, grants)
    }

    pub fn project_work_item_action(
        &self,
        command_id: &str,
    ) -> Result<Option<orbit_storage::blueprint_actions::BlueprintActionRequest>, RepositoryError>
    {
        self.authority.conn.blueprint_action_request(command_id)
    }

    /// Trusted executor boundary, not an HTTP route or blueprint permission grant.
    pub fn claim_project_work_item_action(
        &mut self,
        command_id: &str,
    ) -> Result<orbit_storage::blueprint_actions::BlueprintActionRequest, RepositoryError> {
        self.authority.conn.claim_blueprint_action(command_id)
    }

    pub fn record_project_work_item_action_result(
        &mut self,
        command_id: &str,
        state: &str,
    ) -> Result<orbit_storage::blueprint_actions::BlueprintActionRequest, RepositoryError> {
        self.authority
            .conn
            .record_blueprint_action_result(command_id, state)
    }
}

impl AuthoritativeHost {
    /// Server-authenticated device identity must be supplied by the transport,
    /// never trusted from the request payload.
    pub fn accept_blueprint_command(
        &mut self,
        command_id: &str,
        device_id: &str,
        route: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, RepositoryError> {
        self.authority
            .conn
            .apply_blueprint_command(command_id, device_id, route, body)
    }
}
