//! Authority-owned default workflow binding.
//!
//! New projects are ordinary task projects, so the authority installs the
//! bundled `basic` manifest and binds it at creation time. The binding is the
//! project's identity for its whole lifetime; nothing here replaces an
//! existing binding.
use crate::store::AuthorityError;
use orbit_blueprint::Blueprint;
use orbit_blueprint::bundled;
use rusqlite::{Connection, params};
use std::collections::BTreeSet;

/// Version declared by a trusted bundled descriptor, if the id is bundled.
pub(crate) fn bundled_version(id: &str) -> Option<u32> {
    bundled::BUNDLED_BLUEPRINTS
        .iter()
        .find(|descriptor| descriptor.id == id)
        .map(|descriptor| descriptor.version)
}

/// Install one bundled manifest without binding any project. `INSERT OR IGNORE`
/// keeps published revisions immutable: an already-present identical (or even
/// conflicting) row is never rewritten by this convenience path.
pub(crate) fn install_bundled_workflow(
    conn: &Connection,
    id: &str,
) -> Result<Blueprint, AuthorityError> {
    let blueprint =
        bundled::bundled_blueprint(id).map_err(|_| AuthorityError::Invalid("unknown workflow"))?;
    blueprint
        .validate(&BTreeSet::new())
        .map_err(|_| AuthorityError::Invalid("invalid bundled workflow"))?;
    let serialized = blueprint
        .to_json()
        .map_err(|_| AuthorityError::Invalid("invalid bundled workflow"))?;
    conn.execute(
        "INSERT OR IGNORE INTO orbit_blueprints(id,version,manifest_json) VALUES(?1,?2,?3)",
        params![blueprint.id, blueprint.version, serialized],
    )?;
    Ok(blueprint)
}

/// Ensure a newly-created project is bound to the bundled `basic` workflow.
///
/// Both project-creation authority paths call this inside their enclosing SQL
/// transaction, so either the project row, its activity/receipt, the manifest
/// and the binding all commit together, or none of them do. `INSERT OR IGNORE`
/// on the project binding makes the helper idempotent; a project already
/// bound to *any* workflow is deliberately left untouched rather than being
/// silently re-bound.
pub(crate) fn ensure_basic_workflow(
    conn: &Connection,
    project_id: &str,
) -> Result<(), AuthorityError> {
    let blueprint = install_bundled_workflow(conn, "basic")?;
    conn.execute(
        "INSERT OR IGNORE INTO orbit_project_blueprints(project_id,blueprint_id,blueprint_version) VALUES(?1,?2,?3)",
        params![project_id, blueprint.id, blueprint.version],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthoritativeHost;
    use crate::domain_commands::ProjectCommand;
    use orbit_application::{CreateProjectCommand, CreateProjectWithActivityCommand};
    use orbit_domain::{ActivityId, ActivityOrigin, ProjectId};

    fn rich_project(id: &str, activity: &str) -> CreateProjectWithActivityCommand {
        CreateProjectWithActivityCommand {
            project: CreateProjectCommand {
                id: ProjectId::new(id).unwrap(),
                name: format!("Project {id}"),
            },
            activity_id: ActivityId::new(activity).unwrap(),
            origin: ActivityOrigin::Human,
            summary: format!("Created project {id}"),
        }
    }

    fn binding(host: &AuthoritativeHost, project_id: &str) -> (String, u32) {
        let binding = host
            .active_project_blueprint(project_id)
            .unwrap()
            .expect("project binding");
        (binding.blueprint_id, binding.version)
    }

    #[test]
    fn rich_creation_binds_basic_workflow_and_views_resolve() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let (revision, duplicate, _) = host
            .create_project_accepted("user", "create", 0, rich_project("p", "activity-p"))
            .unwrap();
        assert_eq!((revision, duplicate), (1, false));
        assert_eq!(binding(&host, "p"), ("basic".to_owned(), 1));
        // A second, independent read sees the same durable binding.
        assert_eq!(binding(&host, "p"), ("basic".to_owned(), 1));
        let blueprint = host.project_blueprint("basic", 1).unwrap().unwrap();
        assert!(blueprint.views.iter().any(|view| view.id == "tasks"));
        assert_eq!(host.snapshot().unwrap().0, 1);
    }

    #[test]
    fn minimal_creation_binds_basic_workflow_and_duplicate_is_idempotent() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let command = ProjectCommand {
            workflow_id: None,
            command_id: "minimal-create".into(),
            expected_revision: 0,
            project_id: "p".into(),
            name: "Minimal project".into(),
        };
        let result = host.project_command("device", command.clone()).unwrap();
        assert_eq!(result.revision, 1);
        assert_eq!(binding(&host, "p"), ("basic".to_owned(), 1));
        let retry = host.project_command("device", command).unwrap();
        assert!(retry.duplicate);
        let bindings: i64 = host
            .authority
            .conn
            .query_row("SELECT COUNT(*) FROM orbit_project_blueprints", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(bindings, 1);
        let manifests: i64 = host
            .authority
            .conn
            .query_row(
                "SELECT COUNT(*) FROM orbit_blueprints WHERE id='basic' AND version=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(manifests, 1);
    }

    #[test]
    fn minimal_creation_pins_an_explicit_bundled_workflow() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let command = ProjectCommand {
            command_id: "explicit-workflow".into(),
            expected_revision: 0,
            project_id: "explicit".into(),
            name: "Explicit workflow".into(),
            workflow_id: Some("development".into()),
        };
        let result = host.project_command("device", command).unwrap();
        assert_eq!(result.revision, 1);
        assert_eq!(binding(&host, "explicit"), ("development".to_owned(), 2));
        let basic_bindings: i64 = host
            .authority
            .conn
            .query_row(
                "SELECT COUNT(*) FROM orbit_project_blueprints WHERE blueprint_id='basic'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            basic_bindings, 0,
            "explicit selection must not also bind Basic"
        );
    }

    #[test]
    fn minimal_creation_rejects_an_unknown_workflow() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let result = host.project_command(
            "device",
            ProjectCommand {
                command_id: "unknown-workflow".into(),
                expected_revision: 0,
                project_id: "unknown".into(),
                name: "Unknown workflow".into(),
                workflow_id: Some("does-not-exist".into()),
            },
        );
        assert!(result.is_err());
        let projects: i64 = host
            .authority
            .conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            projects, 0,
            "a rejected workflow must roll back the project"
        );
    }

    #[test]
    fn default_binding_never_replaces_an_existing_binding() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "create", 0, rich_project("p", "activity-p"))
            .unwrap();
        host.install_bundled_project_blueprint("development")
            .unwrap();
        host.authority
            .conn
            .replace_project_blueprint_binding(&ProjectId::new("p").unwrap(), "development", 2)
            .unwrap();
        ensure_basic_workflow(&host.authority.conn, "p").unwrap();
        assert_eq!(binding(&host, "p"), ("development".to_owned(), 2));
    }

    #[test]
    fn explicit_workflow_change_is_retriable_and_rejects_unknowns() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.create_project_accepted("user", "create", 0, rich_project("p", "activity-p"))
            .unwrap();
        let project_id = ProjectId::new("p").unwrap();
        let (revision, duplicate, result) = host
            .set_project_workflow_accepted(
                "user",
                "switch",
                1,
                project_id.clone(),
                "development".into(),
                None,
            )
            .unwrap();
        assert_eq!((revision, duplicate, result.as_str()), (2, false, "p"));
        assert_eq!(binding(&host, "p"), ("development".to_owned(), 2));
        let (revision, duplicate, _) = host
            .set_project_workflow_accepted(
                "user",
                "switch",
                1,
                project_id.clone(),
                "development".into(),
                None,
            )
            .unwrap();
        assert_eq!(revision, 2);
        assert!(duplicate);
        assert_eq!(binding(&host, "p"), ("development".to_owned(), 2));
        assert!(matches!(
            host.set_project_workflow_accepted(
                "user",
                "unknown-workflow",
                2,
                project_id.clone(),
                "not-bundled".into(),
                None,
            ),
            Err(AuthorityError::Invalid("unknown workflow"))
        ));
        assert!(matches!(
            host.set_project_workflow_accepted(
                "user",
                "unknown-project",
                2,
                ProjectId::new("absent").unwrap(),
                "development".into(),
                Some(2),
            ),
            Err(AuthorityError::Invalid("unknown project"))
        ));
        assert_eq!(host.snapshot().unwrap().0, 2);
    }
}
