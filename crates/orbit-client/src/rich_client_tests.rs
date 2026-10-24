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
fn connected_attachments_stage_offline_while_accepted_reads_and_purge_stay_honest() {
    let dir = scratch_dir("connected-attachments");
    let source = dir.join("note.txt");
    fs::write(&source, b"staged evidence").expect("write source");
    let mut remote =
        AuthorityConnection::remote(":memory:", "https://example.org", None).expect("open remote");

    // Accepted server reads still require the server; the failure is
    // offline-specific, not a blanket "attachments unavailable".
    match remote.list_attachments("task", "task-1") {
        Err(super::unified::ClientConnectionError::Unavailable(message)) => {
            assert_eq!(message, super::unified::ATTACHMENTS_OFFLINE_MESSAGE);
            assert_eq!(
                message,
                "offline: connected attachment reads require a server connection; uploads staged on this replica can still be listed and discarded"
            );
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
    // Owner kinds are still validated locally before any network request.
    assert!(matches!(
        remote.list_attachments("sprint", "sprint-1"),
        Err(super::unified::ClientConnectionError::Local(error))
            if error.code == ClientErrorCode::InvalidInput
    ));

    // Import stages durable bytes without a server and never claims acceptance.
    let staged = remote
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            Some("Evidence".into()),
        )
        .expect("stage attachment offline");
    assert_eq!(staged.owner_kind, "task");
    assert_eq!(staged.filename, "note.txt");
    assert_eq!(staged.size_bytes, 15);
    assert!(
        remote
            .has_pending_attachment_uploads()
            .expect("pending flag")
    );
    assert_eq!(remote.pending_command_count().expect("pending count"), 1);
    let staged_rows = remote
        .staged_attachments("task", "task-1")
        .expect("staged list");
    assert_eq!(staged_rows.len(), 1);
    assert_eq!(staged_rows[0].attachment.id, staged.id);
    assert_eq!(staged_rows[0].state, "pending");
    assert_eq!(
        remote
            .pending_attachment_uploads()
            .expect("all staged")
            .len(),
        1
    );
    // Staged bytes are verifiable offline; accepted bytes are not fabricated.
    let (staged_meta, staged_bytes) = remote
        .attachment_content(&staged.id)
        .expect("read staged content offline");
    assert_eq!(staged_meta.id, staged.id);
    assert_eq!(staged_bytes, b"staged evidence");
    assert!(matches!(
        remote.attachment_content("attachment-unknown"),
        Err(super::unified::ClientConnectionError::Remote(
            crate::authority::AuthorityError::Unavailable(message)
        )) if message == crate::attachments_remote::ACCEPTED_CONTENT_REQUIRES_SERVER
    ));
    assert!(
        remote
            .export_attachment(
                &staged.id,
                dir.join("copy.txt").to_string_lossy().into_owned()
            )
            .is_ok()
    );
    assert!(matches!(
        remote.export_attachment(
            "attachment-unknown",
            dir.join("other.txt").to_string_lossy().into_owned()
        ),
        Err(super::unified::ClientConnectionError::Remote(
            crate::authority::AuthorityError::Unavailable(_)
        ))
    ));
    // Accepted removal is online-only; a staged upload is discarded locally.
    assert!(matches!(
        remote.remove_attachment("attachment-unknown".into()),
        Err(super::unified::ClientConnectionError::Remote(
            crate::authority::AuthorityError::Unavailable(message)
        )) if message.contains("offline: removing an accepted attachment requires a server connection")
    ));
    assert!(
        !remote
            .remove_attachment(staged.id.clone())
            .expect("discard staged upload")
    );
    assert!(
        !remote
            .has_pending_attachment_uploads()
            .expect("no pending upload")
    );
    assert_eq!(remote.pending_command_count().expect("settled"), 0);
    assert!(matches!(
        remote.purge_attachment_content(),
        Err(super::unified::ClientConnectionError::Unavailable(message))
            if message == super::unified::PURGE_ATTACHMENTS_UNAVAILABLE_MESSAGE
    ));

    // Rich task writes now queue a canonical command; without accepted state
    // the expected revision is unknowable, so the failure is offline-specific
    // rather than the old blanket "Unavailable".
    match remote.update_task_details(
        "task-1".into(),
        "Connected".into(),
        Some("high".into()),
        Vec::new(),
    ) {
        Err(super::unified::ClientConnectionError::Unsupported(message)) => {
            assert_eq!(message, super::unified::TASK_DETAILS_ACCEPTED_STATE_MESSAGE)
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
    assert!(matches!(
        remote.acceptance_criteria("task-1"),
        Err(super::unified::ClientConnectionError::Unsupported(message))
            if message == super::unified::TASK_READS_OFFLINE_MESSAGE
    ));
    assert!(matches!(
        remote.add_acceptance_criterion("task-1".into(), "Criteria".into()),
        Err(super::unified::ClientConnectionError::Unsupported(message))
            if message == super::unified::TASK_DETAILS_ACCEPTED_STATE_MESSAGE
    ));
    assert!(matches!(
        remote.task_activity("task-1"),
        Err(super::unified::ClientConnectionError::Unsupported(message))
            if message == super::unified::TASK_READS_OFFLINE_MESSAGE
    ));
    assert!(matches!(
        remote.task_detail("task-1"),
        Err(super::unified::ClientConnectionError::Unsupported(message))
            if message == super::unified::TASK_READS_OFFLINE_MESSAGE
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
    // Workflow reads are now live server reads; only binding *changes* remain
    // unavailable in this connected build.
    assert!(matches!(
        remote.project_workflow("project-1"),
        Err(super::unified::ClientConnectionError::Unsupported(_))
    ));
    assert!(matches!(
        remote.apply_workflow("project-1".into(), "development".into(), None),
        Err(super::unified::ClientConnectionError::Unavailable(message))
            if message == super::unified::PROJECT_WORKFLOW_UNAVAILABLE_MESSAGE
    ));
}

#[test]
fn local_task_detail_reads_one_task_and_reports_missing_ids() {
    let dir = scratch_dir("task-detail");
    let path = dir.join("workspace.db");
    let mut client = OrbitClient::open(&path).expect("open client");
    create_task(&mut client, "task-1");
    client
        .update_task_details(
            "task-1".into(),
            "Body".into(),
            Some("medium".into()),
            vec!["api".into()],
        )
        .expect("update details");

    let detail = client.task_detail("task-1").expect("read task detail");
    assert_eq!(detail.title, "Task task-1");
    assert_eq!(detail.description, "Body");
    assert_eq!(detail.priority.as_deref(), Some("medium"));
    assert_eq!(detail.labels, vec!["api".to_string()]);

    let missing = client
        .task_detail("task-missing")
        .expect_err("unknown task is an error");
    assert_eq!(missing.code, ClientErrorCode::NotFound);
    assert!(missing.message.contains("task-missing"));
    let invalid = client
        .task_detail(" ")
        .expect_err("a blank id is invalid input");
    assert_eq!(invalid.code, ClientErrorCode::InvalidInput);

    drop(client);
    let _ = fs::remove_dir_all(dir);
}

/// Connected reads and queued rich-task commands against a real HTTP socket.
mod connected_task_details {
    use super::*;
    use crate::authority::{AuthorityError, AuthorityHttp};
    use crate::unified::{ClientConnectionError, TASK_READS_OFFLINE_MESSAGE};
    use serde_json::{Value, json};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    struct Reply {
        path: &'static str,
        status: u16,
        body: String,
    }

    fn reply(path: &'static str, body: Value) -> Reply {
        Reply {
            path,
            status: 200,
            body: body.to_string(),
        }
    }

    fn missing(path: &'static str) -> Reply {
        Reply {
            path,
            status: 404,
            body: json!({"error": "task not found"}).to_string(),
        }
    }

    fn serve(replies: Vec<Reply>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for reply in replies {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0_u8; 16384];
                let mut request = Vec::new();
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let bytes = socket.read(&mut buffer).unwrap();
                    if bytes == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..bytes]);
                    assert!(request.len() < 65536, "oversized test request");
                }
                let start = String::from_utf8_lossy(&request);
                let lower = start.to_ascii_lowercase();
                let first = start.lines().next().unwrap_or("");
                assert!(
                    first.contains(reply.path),
                    "unexpected HTTP request {first}"
                );
                assert!(first.starts_with("GET "), "expected GET, got {first}");
                assert!(
                    lower.contains("authorization: bearer test-token"),
                    "missing bearer token in {start}"
                );
                assert!(
                    lower.contains("x-orbit-device-id: device-1"),
                    "missing device header in {start}"
                );
                let status = match reply.status {
                    200 => "200 OK",
                    404 => "404 Not Found",
                    _ => "500 Internal Server Error",
                };
                let raw = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.body.len(),
                    reply.body
                );
                socket.write_all(raw.as_bytes()).unwrap();
                socket.flush().unwrap();
            }
        });
        (address, handle)
    }

    fn remote(base: &str) -> AuthorityConnection {
        AuthorityConnection::remote(":memory:", base, Some(("device-1", "test-token")))
            .expect("open remote")
    }

    fn task(description: &str) -> Value {
        json!({
            "id": "task-1",
            "title": "Task 1",
            "status": "planned",
            "project_id": "project-1",
            "milestone_id": null,
            "estimated_minutes": 45,
            "earliest_start_at_ms": null,
            "deadline_at_ms": 1_700_000_000_000_i64,
            "description": description,
            "priority": "low",
            "labels": ["api"],
        })
    }

    fn snapshot(description: &str) -> Value {
        json!({
            "revision": 4_i64,
            "history_fingerprint": "a".repeat(64),
            "projects": [], "tasks": [task(description)],
            "task_dependencies": [], "habits": [], "habit_occurrences": [],
            "metrics": [], "metric_observations": [], "schedule": [],
            "routines": [], "milestones": [], "strategies": [],
        })
    }

    fn accepted(remote: &mut AuthorityConnection) {
        if let AuthorityConnection::Remote { replica, .. } = remote {
            replica
                .accept_snapshot_for_testing(&snapshot("old body"))
                .unwrap();
        }
    }

    fn criteria() -> Value {
        json!([
            {"id": "criterion-1", "text": "Works offline", "completed": true,
             "completed_at_ms": 123_i64, "position": 0},
            {"id": "criterion-2", "text": "Syncs", "completed": false,
             "completed_at_ms": null, "position": 1},
        ])
    }

    #[test]
    fn task_detail_route_maps_rich_fields() {
        let (base, handle) = serve(vec![reply(
            "/v1/authoritative/tasks/task-1",
            json!({
                "id": "task-1",
                "title": "Detailed",
                "status": "ready",
                "project_id": "project-1",
                "milestone_id": "milestone-1",
                "estimated_minutes": 90,
                "earliest_start_at_ms": 10_i64,
                "deadline_at_ms": 20_i64,
                "description": "Body from server",
                "priority": "high",
                "labels": ["api", "ui"],
            }),
        )]);
        let remote = remote(&base);
        let detail = remote.task_detail("task-1").expect("read detail");
        assert_eq!(detail.id, "task-1");
        assert_eq!(detail.title, "Detailed");
        assert_eq!(detail.status, "ready");
        assert_eq!(detail.project_id.as_deref(), Some("project-1"));
        assert_eq!(detail.milestone_id.as_deref(), Some("milestone-1"));
        assert_eq!(detail.estimated_minutes, Some(90));
        assert_eq!(detail.earliest_start_at_ms, Some(10));
        assert_eq!(detail.deadline_at_ms, Some(20));
        assert_eq!(detail.description, "Body from server");
        assert_eq!(detail.priority.as_deref(), Some("high"));
        assert_eq!(detail.labels, vec!["api".to_string(), "ui".to_string()]);
        handle.join().unwrap();
    }

    #[test]
    fn criteria_and_activity_routes_map_ordered_projections() {
        let (base, handle) = serve(vec![
            reply(
                "/v1/authoritative/tasks/task-1/acceptance-criteria",
                criteria(),
            ),
            reply(
                "/v1/authoritative/tasks/task-1/activity",
                json!([
                    {"id": "activity-2", "kind": "updated", "origin": "human",
                     "occurred_at_ms": 200_i64, "summary": "Task task-1 description updated"},
                    {"id": "activity-1", "kind": "created", "origin": "human",
                     "occurred_at_ms": 100_i64, "summary": "Task task-1 created"},
                ]),
            ),
        ]);
        let remote = remote(&base);

        let criteria = remote.acceptance_criteria("task-1").expect("criteria");
        assert_eq!(criteria.len(), 2);
        assert_eq!(criteria[0].id, "criterion-1");
        assert_eq!(criteria[0].text, "Works offline");
        assert!(criteria[0].completed);
        assert_eq!(criteria[0].completed_at_ms, Some(123));
        assert_eq!(criteria[0].position, 0);
        assert!(!criteria[1].completed);
        assert_eq!(criteria[1].position, 1);

        let activity = remote.task_activity("task-1").expect("activity");
        assert_eq!(
            activity
                .iter()
                .map(|item| (item.kind.as_str(), item.occurred_at_ms))
                .collect::<Vec<_>>(),
            vec![("updated", 200), ("created", 100)],
            "server order (newest first) is preserved"
        );
        assert_eq!(activity[0].origin, "human");
        handle.join().unwrap();
    }

    #[test]
    fn missing_task_detail_surfaces_a_typed_not_found_error() {
        let (base, handle) = serve(vec![missing("/v1/authoritative/tasks/task-missing")]);
        let remote = remote(&base);
        match remote.task_detail("task-missing") {
            Err(ClientConnectionError::Remote(AuthorityError::Invalid(message))) => {
                assert!(message.contains("/v1/authoritative/tasks/task-missing"));
                assert!(
                    message.contains("404"),
                    "message should mention 404: {message}"
                );
            }
            other => panic!("expected typed remote Invalid, got {other:?}"),
        }
        handle.join().unwrap();
    }

    #[test]
    fn details_update_queues_set_details_and_returns_the_accepted_value() {
        let mut remote = remote("https://example.org");
        accepted(&mut remote);

        let returned = remote
            .update_task_details(
                "task-1".into(),
                "new body".into(),
                Some("HIGH".into()),
                vec!["ui".into()],
            )
            .expect("queue details update");
        assert_eq!(
            returned.description, "old body",
            "the pending value must never be returned as accepted"
        );
        assert_eq!(returned.priority.as_deref(), Some("low"));
        assert_eq!(returned.labels, vec!["api".to_string()]);

        assert_eq!(remote.pending_command_count().unwrap(), 1);
        let pending = remote.unresolved_commands().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["kind"], "task");
        assert!(
            pending[0]["expected_revision"].is_null(),
            "the outbox fences the revision at flush time, not at queue time"
        );
        assert_eq!(pending[0]["action"]["type"], "set_details");
        assert_eq!(pending[0]["action"]["task_id"], "task-1");
        assert_eq!(pending[0]["action"]["description"], "new body");
        assert_eq!(
            pending[0]["action"]["priority"], "high",
            "priority is canonicalised before queueing"
        );
        assert_eq!(pending[0]["action"]["labels"][0], "ui");
    }

    #[test]
    fn pending_details_never_project_into_accepted_reads() {
        let mut remote = remote("https://example.org");
        accepted(&mut remote);
        remote
            .update_task_details(
                "task-1".into(),
                "pending body".into(),
                Some("urgent".into()),
                Vec::new(),
            )
            .expect("queue details update");

        let snapshot = remote.accepted_snapshot().unwrap().expect("snapshot");
        assert_eq!(snapshot["tasks"][0]["description"], "old body");
        assert_eq!(snapshot["tasks"][0]["priority"], "low");
        assert_eq!(remote.pending_command_count().unwrap(), 1);
        assert_eq!(remote.workspace().unwrap()["pending_count"], 1);
    }

    #[test]
    fn acceptance_mutations_queue_their_wire_operation_and_return_accepted_list() {
        let (base, handle) = serve(vec![
            reply(
                "/v1/authoritative/tasks/task-1/acceptance-criteria",
                criteria(),
            ),
            reply(
                "/v1/authoritative/tasks/task-1/acceptance-criteria",
                criteria(),
            ),
            reply(
                "/v1/authoritative/tasks/task-1/acceptance-criteria",
                criteria(),
            ),
            reply(
                "/v1/authoritative/tasks/task-1/acceptance-criteria",
                criteria(),
            ),
            reply(
                "/v1/authoritative/tasks/task-1/acceptance-criteria",
                criteria(),
            ),
        ]);
        let mut remote = remote(&base);
        accepted(&mut remote);

        let added = remote
            .add_acceptance_criterion("task-1".into(), "New criterion".into())
            .expect("add criterion");
        assert_eq!(
            added.len(),
            2,
            "the accepted list must not include the pending addition"
        );
        let text = remote
            .set_acceptance_criterion_text("task-1".into(), "criterion-2".into(), "Renamed".into())
            .expect("set text");
        assert_eq!(text[1].id, "criterion-2");
        remote
            .set_acceptance_criterion_completed("task-1".into(), "criterion-2".into(), true)
            .expect("set completed");
        remote
            .move_acceptance_criterion("task-1".into(), "criterion-2".into(), 0)
            .expect("reorder");
        remote
            .remove_acceptance_criterion("task-1".into(), "criterion-1".into())
            .expect("remove");

        let pending = remote.unresolved_commands().unwrap();
        assert_eq!(pending.len(), 5);
        let operations: Vec<&str> = pending
            .iter()
            .map(|command| {
                assert_eq!(command["action"]["type"], "acceptance_criteria");
                assert_eq!(command["action"]["task_id"], "task-1");
                command["action"]["change"]["operation"].as_str().unwrap()
            })
            .collect();
        assert_eq!(
            operations,
            vec!["add", "set_text", "set_completed", "reorder", "remove"]
        );
        assert!(pending[0]["action"]["change"]["criterion_id"].is_string());
        assert_eq!(
            pending[1]["action"]["change"]["criterion_id"],
            "criterion-2"
        );
        assert_eq!(pending[1]["action"]["change"]["text"], "Renamed");
        assert_eq!(pending[2]["action"]["change"]["completed"], true);
        assert_eq!(pending[3]["action"]["change"]["position"], 0);
        assert_eq!(
            pending[4]["action"]["change"]["criterion_id"],
            "criterion-1"
        );
        handle.join().unwrap();
    }

    #[test]
    fn offline_connected_reads_fail_with_a_clear_message() {
        let remote = AuthorityConnection::remote(":memory:", "https://example.org", None)
            .expect("open offline remote");
        match remote.task_detail("task-1") {
            Err(ClientConnectionError::Unsupported(message)) => {
                assert_eq!(message, TASK_READS_OFFLINE_MESSAGE)
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn project_workflow_route_maps_a_binding_and_none() {
        let (base, handle) = serve(vec![
            reply(
                "/v1/authoritative/blueprints/binding?project_id=project-1",
                json!({"project_id": "project-1", "blueprint_id": "development", "version": 2}),
            ),
            reply(
                "/v1/authoritative/blueprints/binding?project_id=project-empty",
                Value::Null,
            ),
        ]);
        let remote = remote(&base);
        let binding = remote
            .project_workflow("project-1")
            .expect("read workflow")
            .expect("binding present");
        assert_eq!(binding.project_id, "project-1");
        assert_eq!(binding.workflow_id, "development");
        assert_eq!(binding.version, 2);
        assert!(
            remote
                .project_workflow("project-empty")
                .expect("read workflow")
                .is_none()
        );
        handle.join().unwrap();
    }

    #[test]
    fn authenticated_get_bytes_returns_the_raw_body() {
        let (base, handle) = serve(vec![reply(
            "/v1/authoritative/tasks/task-1/attachment-content",
            json!({"raw": "bytes"}),
        )]);
        let transport = AuthorityHttp::new(&base, "test-token", "device-1").unwrap();
        let bytes = transport
            .get_bytes("/v1/authoritative/tasks/task-1/attachment-content")
            .expect("raw read");
        assert_eq!(bytes, br#"{"raw":"bytes"}"#);
        handle.join().unwrap();
    }
}
