//! Rich task, acceptance-criteria, attachment and workflow client coverage.
use super::*;
use crate::unified::AuthorityConnection;
use std::fs;
use std::path::PathBuf;

fn scratch_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "orbit-client-rich-{label}-{}-{}",
        process::id(),
        CLIENT_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create scratch directory");
    path
}

fn create_task(client: &mut OrbitClient, id: &str) -> TaskDto {
    client
        .create_task(CreateTaskRequest {
            id: Some(id.to_owned()),
            title: format!("Task {id}"),
            project_id: None,
            milestone_id: None,
        })
        .expect("create task")
}

#[test]
fn rich_details_roundtrip_trims_labels_and_rejects_unknown_priority() {
    let dir = scratch_dir("details");
    let path = dir.join("workspace.db");
    let mut client = OrbitClient::open(&path).expect("open client");
    create_task(&mut client, "task-1");

    let updated = client
        .update_task_details(
            "task-1".into(),
            "Detailed **markdown** body".into(),
            Some("HIGH".into()),
            vec![" backend ".into(), "backend".into(), "ui".into()],
        )
        .expect("update details");
    assert_eq!(updated.description, "Detailed **markdown** body");
    assert_eq!(updated.priority.as_deref(), Some("high"));
    assert_eq!(
        updated.labels,
        vec!["backend".to_string(), "ui".to_string()]
    );

    let error = client
        .update_task_details(
            "task-1".into(),
            "".into(),
            Some("critical".into()),
            Vec::new(),
        )
        .expect_err("unknown priority must be rejected");
    assert_eq!(error.code, ClientErrorCode::InvalidInput);
    assert!(error.message.contains("low, medium, high, urgent"));

    let cleared = client
        .update_task_details("task-1".into(), "".into(), Some("".into()), Vec::new())
        .expect("blank priority clears it");
    assert_eq!(cleared.priority, None);
    assert!(cleared.labels.is_empty());

    let reopened = OrbitClient::open(&path).expect("reopen client");
    let listed = reopened
        .list_tasks()
        .expect("list tasks")
        .into_iter()
        .find(|task| task.id == "task-1")
        .expect("task present");
    assert_eq!(listed.description, "");
    assert_eq!(listed.priority, None);
    drop(reopened);
    drop(client);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn acceptance_criteria_check_reorder_and_remove_persist_in_order() {
    let dir = scratch_dir("criteria");
    let path = dir.join("workspace.db");
    let mut client = OrbitClient::open(&path).expect("open client");
    create_task(&mut client, "task-1");

    let first = client
        .add_acceptance_criterion("task-1".into(), "  Works offline  ".into())
        .expect("add first");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].text, "Works offline");
    assert_eq!(first[0].position, 0);
    assert!(!first[0].completed);

    let second = client
        .add_acceptance_criterion("task-1".into(), "Syncs".into())
        .expect("add second");
    let third = client
        .add_acceptance_criterion("task-1".into(), "Accessible".into())
        .expect("add third");
    assert_eq!(third.len(), 3);
    let ids: Vec<String> = third.iter().map(|item| item.id.clone()).collect();
    assert_eq!(
        third
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Works offline", "Syncs", "Accessible"]
    );
    assert_eq!(second.len(), 2);

    let checked = client
        .set_acceptance_criterion_completed("task-1".into(), ids[1].clone(), true)
        .expect("check second");
    assert!(checked[1].completed);
    assert!(checked[1].completed_at_ms.is_some());
    assert_eq!(checked[0].position, 0);
    assert_eq!(checked[2].position, 2);

    let reordered = client
        .move_acceptance_criterion("task-1".into(), ids[2].clone(), 0)
        .expect("move third to front");
    assert_eq!(
        reordered
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Accessible", "Works offline", "Syncs"]
    );
    assert_eq!(
        reordered
            .iter()
            .map(|item| item.position)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(reordered[2].id, ids[1], "completion follows its criterion");
    assert!(reordered[2].completed);

    let renamed = client
        .set_acceptance_criterion_text("task-1".into(), ids[0].clone(), "Offline".into())
        .expect("rename first criterion");
    assert_eq!(renamed[1].text, "Offline");

    let removed = client
        .remove_acceptance_criterion("task-1".into(), ids[1].clone())
        .expect("remove checked criterion");
    assert_eq!(
        removed
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Accessible", "Offline"]
    );

    let reopened = OrbitClient::open(&path).expect("reopen client");
    let persisted = reopened
        .acceptance_criteria("task-1")
        .expect("read criteria");
    assert_eq!(
        persisted
            .iter()
            .map(|item| (item.text.as_str(), item.position))
            .collect::<Vec<_>>(),
        vec![("Accessible", 0), ("Offline", 1)]
    );
    assert_eq!(persisted[0].id, ids[2]);
    drop(reopened);
    drop(client);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn attachments_import_read_export_refuse_overwrite_and_share_content() {
    let dir = scratch_dir("attachments");
    let path = dir.join("workspace.db");
    let source = dir.join("evidence.txt");
    fs::write(&source, b"shared evidence").expect("write source");
    let mut client = OrbitClient::open(&path).expect("open client");
    create_task(&mut client, "task-1");
    create_task(&mut client, "task-2");

    let imported = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            Some("Evidence".into()),
        )
        .expect("import attachment");
    assert_eq!(imported.owner_kind, "task");
    assert_eq!(imported.owner_id, "task-1");
    assert_eq!(imported.filename, "evidence.txt");
    assert_eq!(imported.content_type, "text/plain");
    assert_eq!(imported.size_bytes, 15);
    assert_eq!(imported.description.as_deref(), Some("Evidence"));
    assert!(!imported.is_image);

    let listed = client
        .list_attachments("task", "task-1")
        .expect("list attachments");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, imported.id);

    let (metadata, content) = client
        .attachment_content(&imported.id)
        .expect("read attachment content");
    assert_eq!(content, b"shared evidence");
    assert_eq!(metadata.checksum, imported.checksum);

    let second = client
        .import_attachment(
            "task".into(),
            "task-2".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("import same bytes for another task");
    assert_ne!(second.id, imported.id);
    assert_eq!(
        second.checksum, imported.checksum,
        "identical bytes share one content object"
    );

    let export = dir.join("copy.txt");
    let exported = client
        .export_attachment(&imported.id, export.to_string_lossy().into_owned())
        .expect("export attachment");
    assert_eq!(exported.id, imported.id);
    assert_eq!(fs::read(&export).expect("read export"), b"shared evidence");
    let refused = client
        .export_attachment(&imported.id, export.to_string_lossy().into_owned())
        .expect_err("existing destination must not be overwritten");
    assert_eq!(refused.code, ClientErrorCode::Conflict);
    assert_eq!(
        fs::read(&export).expect("export intact"),
        b"shared evidence"
    );

    let removed_first = client
        .remove_attachment(imported.id.clone())
        .expect("remove first association");
    assert!(
        !removed_first,
        "the shared object survives while the second reference exists"
    );
    assert_eq!(
        client
            .attachment_content(&second.id)
            .expect("content survives")
            .1,
        b"shared evidence"
    );
    assert_eq!(
        client.purge_attachment_content().expect("purge"),
        0,
        "a referenced object is never purged"
    );

    let removed_second = client
        .remove_attachment(second.id)
        .expect("remove second association");
    assert!(removed_second, "the last reference removes the object");
    assert!(client.attachment_content(&imported.id).is_err());
    assert_eq!(client.purge_attachment_content().expect("purge"), 0);
    assert!(
        client
            .list_attachments("task", "task-1")
            .expect("list empty")
            .is_empty()
    );

    let unknown = client
        .list_attachments("sprint", "sprint-1")
        .expect_err("unknown owner kind is rejected");
    assert_eq!(unknown.code, ClientErrorCode::InvalidInput);
    assert!(unknown.message.contains("task, project, work_item"));

    let directory = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            dir.to_string_lossy().into_owned(),
            None,
        )
        .expect_err("directories are rejected");
    assert_eq!(directory.code, ClientErrorCode::InvalidInput);
    assert!(directory.message.contains("not a regular file"));

    drop(client);
    let _ = fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn attachment_import_rejects_symbolic_links() {
    let dir = scratch_dir("symlink");
    let path = dir.join("workspace.db");
    let source = dir.join("real.txt");
    fs::write(&source, b"payload").expect("write source");
    let link = dir.join("link.txt");
    std::os::unix::fs::symlink(&source, &link).expect("create symlink");
    let mut client = OrbitClient::open(&path).expect("open client");
    create_task(&mut client, "task-1");

    let error = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            link.to_string_lossy().into_owned(),
            None,
        )
        .expect_err("symlinks are rejected");
    assert_eq!(error.code, ClientErrorCode::InvalidInput);
    assert!(error.message.contains("symbolic link"));
    assert!(
        client
            .list_attachments("task", "task-1")
            .expect("list")
            .is_empty()
    );

    drop(client);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn task_activity_records_meaningful_changes_only() {
    let dir = scratch_dir("activity");
    let path = dir.join("workspace.db");
    let source = dir.join("trace.log");
    fs::write(&source, b"trace").expect("write source");
    let mut client = OrbitClient::open(&path).expect("open client");
    create_task(&mut client, "task-1");

    let before = client.task_activity("task-1").expect("initial activity");
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].kind, "created");
    assert_eq!(before[0].origin, "human");

    client
        .update_task_details(
            "task-1".into(),
            "Describe".into(),
            Some("high".into()),
            vec!["api".into()],
        )
        .expect("update details");
    client
        .add_acceptance_criterion("task-1".into(), "Works offline".into())
        .expect("add criterion");
    let attachment = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("import attachment");

    let activity = client.task_activity("task-1").expect("task activity");
    assert_eq!(activity.len(), 4);
    let kinds: Vec<&str> = activity.iter().map(|item| item.kind.as_str()).collect();
    assert_eq!(
        kinds,
        vec!["attachment_added", "updated", "updated", "created"],
        "newest first"
    );
    assert!(activity[0].summary.contains("trace.log"));
    assert!(activity[1].summary.contains("Acceptance criterion"));
    assert!(activity[2].summary.contains("description"));

    // Re-applying identical details is a no-op and must not add activity noise.
    client
        .update_task_details(
            "task-1".into(),
            "Describe".into(),
            Some("high".into()),
            vec!["api".into()],
        )
        .expect("no-op details update");
    assert_eq!(
        client.task_activity("task-1").expect("activity").len(),
        4,
        "a no-op update writes no activity"
    );

    client.remove_attachment(attachment.id).expect("remove");
    let after_removal = client.task_activity("task-1").expect("activity");
    assert_eq!(after_removal.len(), 5);
    assert_eq!(after_removal[0].kind, "attachment_removed");

    assert!(
        client
            .task_activity("task-missing")
            .expect("missing task has no activity")
            .is_empty()
    );

    drop(client);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn bundled_workflows_include_basic_as_default_offline() {
    let dir = scratch_dir("workflows");
    let path = dir.join("workspace.db");
    let client = OrbitClient::open(&path).expect("open client");
    let workflows = client.bundled_workflows();
    assert!(workflows.iter().all(|workflow| workflow.bundled));
    let basic = workflows
        .iter()
        .find(|workflow| workflow.id == "basic")
        .expect("basic workflow is bundled");
    assert_eq!(basic.name, "Basic");
    assert_eq!(basic.version, 1);
    assert!(basic.is_default);
    assert_eq!(
        workflows
            .iter()
            .filter(|workflow| workflow.is_default)
            .count(),
        1,
        "exactly one default workflow"
    );
    assert!(
        workflows
            .iter()
            .any(|workflow| workflow.id == "development" && !workflow.is_default)
    );
    drop(client);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn project_workflow_binding_roundtrips_through_the_client() {
    let dir = scratch_dir("workflow-binding");
    let path = dir.join("workspace.db");
    let mut client = OrbitClient::open(&path).expect("open client");
    let project = client
        .create_project(CreateProjectRequest {
            id: Some("project-1".into()),
            name: "Orbit".into(),
        })
        .expect("create project");
    let initial = client
        .project_workflow(&project.id)
        .expect("read workflow")
        .expect("new projects start on the bundled basic workflow");
    assert_eq!(initial.workflow_id, "basic");
    assert_eq!(initial.version, 1);

    client
        .apply_workflow("project-1".into(), "development".into(), None)
        .expect("apply bundled workflow");
    let binding = client
        .project_workflow("project-1")
        .expect("read workflow")
        .expect("workflow bound");
    assert_eq!(binding.project_id, "project-1");
    assert_eq!(binding.workflow_id, "development");
    assert_eq!(binding.version, 2);

    drop(client);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn connected_mode_fails_closed_for_attachments_and_rich_task_commands() {
    let mut remote =
        AuthorityConnection::remote(":memory:", "https://example.org", None).expect("open remote");

    let attachment_error = remote
        .list_attachments("task", "task-1")
        .expect_err("attachments are unavailable in connected mode");
    match attachment_error {
        super::unified::ClientConnectionError::Unavailable(message) => {
            assert_eq!(message, super::unified::ATTACHMENTS_UNAVAILABLE_MESSAGE);
            assert_eq!(
                message,
                "attachments are not available in connected mode in this build; the server does not yet accept attachment uploads"
            );
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }

    assert!(matches!(
        remote.import_attachment("task".into(), "task-1".into(), "/tmp/x".into(), None),
        Err(super::unified::ClientConnectionError::Unavailable(message))
            if message == super::unified::ATTACHMENTS_UNAVAILABLE_MESSAGE
    ));
    assert!(matches!(
        remote.attachment_content("attachment-1"),
        Err(super::unified::ClientConnectionError::Unavailable(_))
    ));
    assert!(matches!(
        remote.export_attachment("attachment-1", "/tmp/x".into()),
        Err(super::unified::ClientConnectionError::Unavailable(_))
    ));
    assert!(matches!(
        remote.remove_attachment("attachment-1".into()),
        Err(super::unified::ClientConnectionError::Unavailable(_))
    ));
    assert!(matches!(
        remote.purge_attachment_content(),
        Err(super::unified::ClientConnectionError::Unavailable(_))
    ));

    match remote.update_task_details(
        "task-1".into(),
        "Connected".into(),
        Some("high".into()),
        Vec::new(),
    ) {
        Err(super::unified::ClientConnectionError::Unavailable(message)) => {
            assert_eq!(message, super::unified::TASK_DETAILS_UNAVAILABLE_MESSAGE)
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
    assert!(matches!(
        remote.acceptance_criteria("task-1"),
        Err(super::unified::ClientConnectionError::Unavailable(message))
            if message == super::unified::TASK_DETAILS_UNAVAILABLE_MESSAGE
    ));
    assert!(matches!(
        remote.add_acceptance_criterion("task-1".into(), "Criteria".into()),
        Err(super::unified::ClientConnectionError::Unavailable(_))
    ));
    assert!(matches!(
        remote.task_activity("task-1"),
        Err(super::unified::ClientConnectionError::Unavailable(_))
    ));
    assert_eq!(
        remote
            .bundled_workflows()
            .iter()
            .filter(|w| w.is_default)
            .count(),
        1,
        "static workflow descriptors remain readable offline"
    );
    assert!(matches!(
        remote.project_workflow("project-1"),
        Err(super::unified::ClientConnectionError::Unavailable(_))
    ));
}
