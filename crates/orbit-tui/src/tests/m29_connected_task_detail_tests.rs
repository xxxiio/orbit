use crate::app::{FormMode, Modal, Screen};
use crate::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// Minimal loopback authority: path-addressed JSON GET responses. Every
/// undeclared path is a 404, so a test cannot silently rely on a route it
/// never declared.
struct Authority {
    base: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Authority {
    fn start(routes: Vec<(&'static str, serde_json::Value)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !signal.load(Ordering::SeqCst) {
                let (mut socket, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => break,
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0_u8; 8192];
                let mut request = Vec::new();
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).unwrap_or(0);
                    if read == 0 || request.len() > 65536 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                }
                let head = String::from_utf8_lossy(&request);
                let target = head
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default();
                let path = target.split('?').next().unwrap_or_default();
                let body = routes
                    .iter()
                    .find(|(route, _)| *route == path)
                    .map(|(_, body)| body.to_string());
                let (status, body) = match body {
                    Some(body) => ("200 OK", body),
                    None => (
                        "404 Not Found",
                        serde_json::json!({"error":"route absent"}).to_string(),
                    ),
                };
                let raw = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(raw.as_bytes());
                let _ = socket.flush();
            }
        });
        Self {
            base,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for Authority {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn replica_path(kind: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "orbit-tui-m29-{kind}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn connected_app(server: &str, kind: &str) -> (App, std::path::PathBuf) {
    let path = replica_path(kind);
    let backend = crate::backend::Backend::open_connected_with_identity(
        &path,
        server,
        Some(("device-1", "token")),
    )
    .expect("connected backend");
    let app = App::open_with_backend(backend).expect("connected app");
    (app, path)
}

/// Connected replica with no credentials: every network call fails locally,
/// so queued/offline behaviour is exercised without a server.
fn offline_app(kind: &str) -> (App, std::path::PathBuf) {
    let path = replica_path(kind);
    let backend = crate::backend::Backend::open_connected_with_identity(
        &path,
        "https://example.invalid",
        None,
    )
    .expect("connected backend");
    let app = App::open_with_backend(backend).expect("connected app");
    (app, path)
}

fn rendered(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| crate::render::draw(frame, app))
        .unwrap();
    (0..terminal.backend().buffer().area.height)
        .map(|y| {
            (0..terminal.backend().buffer().area.width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn snapshot() -> serde_json::Value {
    serde_json::json!({
        "revision": 4_i64,
        "history_fingerprint": "a".repeat(64),
        "projects": [],
        "tasks": [{
            "id": "task-1",
            "title": "Ship it",
            "status": "inbox",
            "project_id": null,
            "milestone_id": null,
            "estimated_minutes": 45,
            "earliest_start_at_ms": null,
            "deadline_at_ms": null,
            "description": "Body from server",
            "priority": "high",
            "labels": ["api"],
        }],
        "task_dependencies": [], "habits": [], "habit_occurrences": [],
        "metrics": [], "metric_observations": [], "schedule": [],
        "routines": [], "milestones": [], "strategies": [],
    })
}

fn detail() -> serde_json::Value {
    serde_json::json!({
        "id": "task-1",
        "title": "Ship it",
        "status": "inbox",
        "project_id": null,
        "milestone_id": null,
        "estimated_minutes": 45,
        "earliest_start_at_ms": null,
        "deadline_at_ms": null,
        "description": "Body from server",
        "priority": "high",
        "labels": ["api", "ui"],
    })
}

fn criteria() -> serde_json::Value {
    serde_json::json!([
        {"id":"criterion-1","text":"Works offline","completed":true,
         "completed_at_ms":123_i64,"position":0},
        {"id":"criterion-2","text":"Syncs","completed":false,
         "completed_at_ms":null,"position":1},
    ])
}

fn activity() -> serde_json::Value {
    serde_json::json!([
        {"id":"activity-2","kind":"updated","origin":"human",
         "occurred_at_ms":200_i64,"summary":"Task task-1 description updated"},
        {"id":"activity-1","kind":"created","origin":"human",
         "occurred_at_ms":100_i64,"summary":"Task task-1 created"},
    ])
}

fn field<'a>(form: &'a app::FormState, key: &str) -> &'a str {
    form.fields
        .iter()
        .find(|candidate| candidate.key == key)
        .map(|candidate| candidate.value.as_str())
        .unwrap_or_default()
}

#[test]
fn connected_app_loads_task_details_from_the_authority_route() {
    // Keep the task out of the accepted snapshot so this test exercises
    // the live task-detail fallback rather than the snapshot-first path.
    let mut snapshot_without_task = snapshot();
    snapshot_without_task["tasks"] = serde_json::json!([]);
    let authority = Authority::start(vec![
        ("/v1/authoritative/snapshot", snapshot_without_task),
        ("/v1/authoritative/tasks/task-1", detail()),
    ]);
    let (mut app, path) = connected_app(&authority.base, "detail");
    assert!(app.client.is_connected());

    let task = app.client.task_detail("task-1").expect("connected detail");
    assert_eq!(task.description, "Body from server");
    assert_eq!(task.priority.as_deref(), Some("high"));
    assert_eq!(task.labels, vec!["api".to_string(), "ui".to_string()]);

    // The details editor is fed by the same read, so it no longer shows a
    // blank description/priority/labels from the thin accepted snapshot.
    app.start_form(FormMode::EditTaskDetails {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(form)) = app.modal.as_ref() else {
        panic!("details form missing");
    };
    assert_eq!(field(form, "description"), "Body from server");
    assert_eq!(field(form, "priority"), "high");
    assert_eq!(field(form, "labels"), "api, ui");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn connected_criteria_and_activity_actions_are_available() {
    let authority = Authority::start(vec![
        ("/v1/authoritative/snapshot", snapshot()),
        ("/v1/authoritative/tasks/task-1", detail()),
        (
            "/v1/authoritative/tasks/task-1/acceptance-criteria",
            criteria(),
        ),
        ("/v1/authoritative/tasks/task-1/activity", activity()),
    ]);
    let (mut app, path) = connected_app(&authority.base, "actions");
    app.go_screen(Screen::Inbox);
    assert_eq!(
        app.selected_item().map(|item| item.id.as_str()),
        Some("task-1")
    );

    let ids = app
        .available_actions()
        .into_iter()
        .map(|action| action.id)
        .collect::<Vec<_>>();
    for expected in [
        "edit_task_details",
        "manage_acceptance_criteria",
        "task_activity",
        "attach_file",
        "open_attachments",
    ] {
        assert!(
            ids.iter().any(|id| id == expected),
            "missing {expected}: {ids:?}"
        );
    }

    app.open_criteria("task-1".into(), "Ship it".into());
    let Some(Modal::Criteria(state)) = app.modal.as_ref() else {
        panic!("criteria modal missing: {}", app.status.text);
    };
    assert_eq!(state.items.len(), 2);
    assert_eq!(state.items[0].id, "criterion-1");
    assert!(state.items[0].completed);

    app.modal = None;
    app.open_activity("task-1".into(), "Ship it".into());
    let Some(Modal::Activity(state)) = app.modal.as_ref() else {
        panic!("activity modal missing: {}", app.status.text);
    };
    assert_eq!(state.items.len(), 2);
    assert_eq!(state.items[0].kind, "updated");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn connected_attachment_add_reports_pending_and_marks_the_row() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "pending-attach");
    app.go_screen(Screen::Inbox);
    let Some(item) = app.selected_item() else {
        panic!("task selection missing");
    };
    assert_eq!(item.id, "task-1");

    let source = std::env::temp_dir().join(format!(
        "orbit-tui-pending-attach-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&source, b"pending evidence").unwrap();

    app.start_form(FormMode::AttachFile {
        owner_kind: "task".into(),
        owner_id: "task-1".into(),
    });
    // A later list/upload attempt cannot reach the server route; the add
    // must therefore stay staged and report pending, never accepted.
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("attach form missing");
    };
    form.fields[0].value = source.to_string_lossy().into_owned();
    form.dirty = true;
    app.submit_form(form);
    assert_eq!(app.status.kind, StatusKind::Pending, "{}", app.status.text);
    assert!(
        app.status.text.to_lowercase().contains("pending"),
        "{}",
        app.status.text
    );
    assert_eq!(app.client.pending_attachment_count().unwrap(), 1);
    // The header must include staged uploads, not only outbox commands.
    assert_eq!(app.client.pending_count(), 1);

    app.open_attachments("task".into(), "task-1".into(), "Ship it".into());
    let Some(Modal::Attachments(state)) = app.modal.as_ref() else {
        panic!("attachments modal missing: {}", app.status.text);
    };
    assert!(state.items.is_empty(), "staged must not be accepted");
    assert_eq!(state.pending.len(), 1);
    assert_eq!(
        state.pending[0].attachment.filename,
        source.file_name().unwrap().to_string_lossy()
    );
    assert_eq!(state.pending[0].state, "pending");
    let pending_id = state.pending[0].attachment.id.clone();

    // A staged preview is labelled pending and reads staged bytes.
    app.preview_attachment(pending_id);
    let Some(Modal::Message { title, lines }) = app.modal.as_ref() else {
        panic!("preview modal missing: {}", app.status.text);
    };
    assert!(title.contains("[pending]"), "{title}");
    assert!(
        lines.iter().any(|line| line.contains("[pending]")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("pending evidence")),
        "staged content missing: {lines:?}"
    );

    app.open_attachments("task".into(), "task-1".into(), "Ship it".into());
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(120, 35)).unwrap();
    terminal
        .draw(|frame| render::draw(frame, &mut app))
        .unwrap();
    let rendered: String = (0..terminal.backend().buffer().area.height)
        .map(|y| {
            (0..terminal.backend().buffer().area.width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("[pending]"), "{rendered}");
    assert!(rendered.contains("orbit-tui-pending-attach"), "{rendered}");
    assert!(
        rendered.contains("1 pending"),
        "header must count the staged upload: {rendered}"
    );

    // Pending removal is local: the staged row is discarded without the
    // server, while accepted removal keeps requiring the online route.
    handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE).unwrap();
    let Some(Modal::Confirm { title, .. }) = app.modal.as_ref() else {
        panic!("pending removal confirmation missing");
    };
    assert_eq!(title, "Discard staged upload");
    handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE).unwrap();
    assert_eq!(app.client.pending_attachment_count().unwrap(), 0);
    assert!(app.status.text.contains("discarded"), "{}", app.status.text);
    let Some(Modal::Attachments(state)) = app.modal.as_ref() else {
        panic!(
            "discarding must refresh the attachments modal: {}",
            app.status.text
        );
    };
    assert!(state.pending.is_empty(), "{:?}", state.pending);
    assert_eq!(state.owner_id, "task-1");

    let _ = std::fs::remove_file(&source);
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn queued_task_details_save_reports_pending_not_saved() {
    let authority = Authority::start(vec![
        ("/v1/authoritative/snapshot", snapshot()),
        ("/v1/authoritative/tasks/task-1", detail()),
    ]);
    let (mut app, path) = connected_app(&authority.base, "pending-details");

    app.start_form(FormMode::EditTaskDetails {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("details form missing");
    };
    form.fields
        .iter_mut()
        .find(|candidate| candidate.key == "description")
        .expect("description field")
        .value = "Queued body".into();
    form.dirty = true;
    app.submit_form(form);

    assert_eq!(app.status.kind, StatusKind::Pending, "{}", app.status.text);
    assert!(
        app.status.text.to_lowercase().contains("pending"),
        "{}",
        app.status.text
    );
    assert_eq!(app.client.pending_count(), 1);
    // The queued value is outbox state only: the accepted read is unchanged.
    let accepted = app.client.task_detail("task-1").expect("accepted detail");
    assert_eq!(accepted.description, "Body from server");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn blocked_flush_and_per_command_conflict_are_visible() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "blocked-flush");

    app.client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Blocked".into(),
        })
        .expect("queued project");
    let id = app
        .client
        .last_queued_command_id()
        .expect("queued command identity");
    assert!(matches!(
        app.client.command_outcome(&id),
        crate::backend::MutationOutcome::Pending { .. }
    ));

    // The fake authority declares no command route, so flush receives a
    // permanent 404 rejection and must mark this command conflicted.
    let flush = orbit_ui::AvailableAction::enabled("flush_pending", "Flush", None);
    assert!(!app.execute_action(flush));
    assert_eq!(app.status.kind, StatusKind::Warning, "{}", app.status.text);
    assert!(
        app.status.text.contains("Flush blocked"),
        "{}",
        app.status.text
    );
    assert!(app.status.text.contains("rejected"), "{}", app.status.text);
    assert!(app.status.text.contains("conflict"), "{}", app.status.text);

    match app.client.command_outcome(&id) {
        crate::backend::MutationOutcome::Conflict { state, reason, .. } => {
            assert_eq!(state, "conflict");
            assert!(
                reason.contains("not found") || reason.contains("404"),
                "{reason}"
            );
        }
        other => panic!("expected a conflict for {id}: {other:?}"),
    }

    // Missing state fails closed as pending, never accepted.
    match app.client.command_outcome("never-queued") {
        crate::backend::MutationOutcome::Pending { state, .. } => {
            assert_eq!(state, "unavailable");
        }
        other => panic!("missing state must fail closed: {other:?}"),
    }

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn connected_attachment_actions_are_enabled() {
    let authority = Authority::start(vec![
        ("/v1/authoritative/snapshot", snapshot()),
        (
            "/v1/authoritative/attachments",
            serde_json::json!({"attachments": []}),
        ),
    ]);
    let (mut app, path) = connected_app(&authority.base, "attachments");
    app.go_screen(Screen::Inbox);

    let ids = app
        .available_actions()
        .into_iter()
        .map(|action| action.id)
        .collect::<Vec<_>>();
    for expected in ["attach_file", "open_attachments"] {
        assert!(
            ids.iter().any(|id| id == expected),
            "connected mode must advertise {expected}: {ids:?}"
        );
    }
    assert!(
        !ids.iter().any(|id| id == "purge_attachments"),
        "server-owned GC must stay unavailable: {ids:?}"
    );

    let attach = orbit_ui::AvailableAction::enabled("attach_file", "Attach file", None);
    assert!(!app.execute_action(attach));
    assert!(
        matches!(app.modal, Some(Modal::Form(ref form)) if matches!(form.mode, FormMode::AttachFile { .. })),
        "attach must open the file form: {}",
        app.status.text
    );
    app.modal = None;

    let open = orbit_ui::AvailableAction::enabled("open_attachments", "Attachments", None);
    assert!(!app.execute_action(open));
    let Some(Modal::Attachments(state)) = app.modal.as_ref() else {
        panic!("attachments modal missing: {}", app.status.text);
    };
    assert!(state.items.is_empty());
    assert!(state.pending.is_empty());

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn queued_connected_intent_is_pending_and_not_accepted() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "queue-inspect");
    assert_eq!(app.client.pending_count(), 0);
    assert!(app.client.list_projects().unwrap().is_empty());

    let intent = crate::connected_actions::parse_intent(
        "project.create queued-project Queued project",
        "generated",
    )
    .expect("intent");
    app.execute_pending(app::PendingAction::QueueIntent { intent });

    assert_eq!(app.client.pending_count(), 1);
    assert_eq!(app.status.kind, StatusKind::Pending, "{}", app.status.text);
    let status = app.status.text.to_lowercase();
    assert!(status.contains("pending"), "{}", app.status.text);
    assert!(status.contains("not accepted"), "{}", app.status.text);
    assert!(!app.status.text.contains("Accepted"));

    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "project");
    assert_eq!(rows[0].status, "pending");
    assert!(!rows[0].conflict);
    assert!(rows[0].summary.contains("queued-project"), "{:?}", rows[0]);
    assert!(rows[0].payload.contains("Queued project"));

    // The accepted projection and project list are unchanged.
    assert!(app.client.list_projects().unwrap().is_empty());
    assert!(
        app.client
            .workspace_view_data()
            .unwrap()
            .projects
            .is_empty()
    );

    let detail = app.client.pending_command_detail(&rows[0].id).unwrap();
    assert!(detail.contains("state: pending"), "{detail}");
    assert!(detail.contains("accepted receipt: none"), "{detail}");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn flush_without_server_keeps_the_command_pending_and_inspectable() {
    let (mut app, path) = offline_app("flush-offline");
    let intent = crate::connected_actions::parse_intent("task.create Offline task", "generated")
        .expect("intent");
    app.execute_pending(app::PendingAction::QueueIntent { intent });
    let id = app.client.last_queued_command_id().expect("queued id");
    assert_eq!(app.client.pending_count(), 1);

    let (rows, detail) = app.pending_flush_report(std::slice::from_ref(&id));
    assert_eq!(rows.len(), 1, "a failed flush must not drop the row");
    assert_eq!(rows[0].id, id);
    assert!(
        app.status.text.contains("Flush failed"),
        "{}",
        app.status.text
    );
    assert!(matches!(
        app.client.command_outcome(&id),
        crate::backend::MutationOutcome::Pending { .. }
    ));
    assert!(detail.contains("state: pending"), "{detail}");
    assert_eq!(app.client.pending_count(), 1);

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn staged_attachment_appears_as_its_own_pending_row() {
    let (mut app, path) = offline_app("pending-attachment-row");
    let source = std::env::temp_dir().join(format!(
        "orbit-tui-pending-row-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&source, b"staged bytes").unwrap();
    app.client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("staged upload");

    let rows = app.client.pending_commands().expect("pending rows");
    let row = rows
        .iter()
        .find(|row| row.kind == "attachment")
        .expect("attachment row");
    assert!(!row.conflict);
    assert_eq!(row.status, "pending");
    assert!(row.summary.contains("upload"), "{row:?}");
    let detail = app.client.pending_command_detail(&row.id).unwrap();
    assert!(detail.contains("never accepted by the server"), "{detail}");
    assert_eq!(app.client.pending_count(), 1);

    let _ = std::fs::remove_file(&source);
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn pending_modal_shows_pending_tag_and_never_claims_acceptance() {
    let (mut app, path) = offline_app("pending-render");
    let intent =
        crate::connected_actions::parse_intent("project.create p-render Queued project", "gen")
            .expect("intent");
    app.execute_pending(app::PendingAction::QueueIntent { intent });

    app.open_pending();
    assert!(
        matches!(app.modal, Some(Modal::Pending { .. })),
        "{}",
        app.status.text
    );
    for (width, height) in [(60_u16, 18_u16), (120, 35), (160, 45)] {
        let text = rendered(&mut app, width, height);
        assert!(text.contains("[pending]"), "{width}x{height}:\n{text}");
        assert!(text.contains("not accepted"), "{width}x{height}:\n{text}");
        // Only the modal surface is checked: the background shell may
        // legitimately describe a stale accepted view.
        let modal_lines: Vec<&str> = text
            .lines()
            .filter(|line| line.contains('║') || line.contains("Pending commands ·"))
            .collect();
        assert!(
            !modal_lines.iter().any(|line| line.contains("Accepted")),
            "{width}x{height}:\n{text}"
        );
    }

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_rejects_invalid_input_and_fails_closed_without_a_review() {
    let (mut app, path) = offline_app("console-validation");
    assert!(matches!(
        app.console_run("not.a.command"),
        crate::app::ConsoleOutcome::Error(_)
    ));
    assert_eq!(app.client.pending_count(), 0);

    assert!(matches!(
        app.console_run("batch task.create Good; schedule.resize blocked 0"),
        crate::app::ConsoleOutcome::Error(_)
    ));
    assert_eq!(app.client.pending_count(), 0);

    let outcome = app.console_run("resolve unknown-command task.create Replacement");
    let crate::app::ConsoleOutcome::Conflict(state) = outcome else {
        panic!("resolve must open a conflict review");
    };
    assert!(
        state.review.is_none(),
        "no review token can be established offline"
    );
    assert!(
        state
            .review_lines
            .join("\n")
            .contains("rejected, not accepted"),
        "{:?}",
        state.review_lines
    );
    let error = app
        .resolve_conflict(&state.command_id, state.review.as_ref(), &state.replacement)
        .unwrap_err();
    assert!(error.to_lowercase().contains("review"), "{error}");
    assert_eq!(app.client.pending_count(), 0);

    // A valid console command still only opens a confirmation; nothing is
    // queued before the user confirms.
    match app.console_run("task.create Valid task") {
        crate::app::ConsoleOutcome::Confirm { body, .. } => {
            let body = body.to_lowercase();
            assert!(body.contains("pending"), "{body}");
            assert!(body.contains("not accepted"), "{body}");
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }
    assert_eq!(app.client.pending_count(), 0);

    app.open_connected_console();
    assert!(matches!(app.modal, Some(Modal::Console { .. })));
    for (width, height) in [(60_u16, 18_u16), (160, 45)] {
        let text = rendered(&mut app, width, height);
        assert!(
            text.contains("Connected commands"),
            "{width}x{height}:\n{text}"
        );
        assert!(text.contains("PENDING"), "{width}x{height}:\n{text}");
    }
    app.modal = None;

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn conflict_review_is_explicit_and_preserves_the_rejected_row() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "conflict-review");
    let intent = crate::connected_actions::parse_intent(
        "project.create conflicted-project Rejected",
        "generated",
    )
    .expect("intent");
    app.execute_pending(app::PendingAction::QueueIntent { intent });
    let id = app.client.last_queued_command_id().expect("queued id");

    // The loopback authority declares no command route, so the flush gets a
    // permanent rejection and the command becomes conflicted.
    let flush = orbit_ui::AvailableAction::enabled("flush_pending", "Flush", None);
    assert!(!app.execute_action(flush));
    assert!(
        app.client
            .pending_commands()
            .unwrap()
            .iter()
            .any(|row| row.id == id && row.conflict),
        "the rejected row must remain visible: {}",
        app.status.text
    );

    app.open_conflict(&id);
    let Some(Modal::Conflict {
        review,
        review_lines,
        ..
    }) = app.modal.as_ref()
    else {
        panic!("conflict modal missing: {}", app.status.text);
    };
    assert!(review.is_some(), "{}", review_lines.join("\n"));
    let text = review_lines.join("\n");
    assert!(text.contains("REJECTED"), "{text}");
    assert!(text.contains("not accepted"), "{text}");
    assert!(text.contains("Accepted server revision"), "{text}");
    // No auto-rebase and no accepted claim: accepted projects stay empty
    // and the rejected row is still queued.
    assert_eq!(app.client.list_projects().unwrap().len(), 0);
    assert!(
        app.client
            .pending_commands()
            .unwrap()
            .iter()
            .any(|row| row.id == id)
    );

    let text = rendered(&mut app, 120, 35);
    assert!(text.contains("rejected, not accepted"), "{text}");
    let narrow = rendered(&mut app, 60, 18);
    assert!(narrow.contains("rejected, not accepted"), "{narrow}");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn local_deployment_has_no_pending_or_console_surface() {
    let path = replica_path("local-pending");
    let backend = crate::backend::Backend::open_local(&path).expect("local backend");
    let mut app = App::open_with_backend(backend).expect("local app");

    let labels: Vec<String> = app
        .palette_entries("")
        .into_iter()
        .map(|entry| entry.label)
        .collect();
    assert!(
        !labels.iter().any(|label| label == "Pending commands"),
        "{labels:?}"
    );
    assert!(
        !labels
            .iter()
            .any(|label| label.contains("Connected commands")),
        "{labels:?}"
    );

    let error = app.client.pending_commands().unwrap_err();
    assert!(error.contains("connected"), "{error}");
    let error = app.client.pending_command_detail("anything").unwrap_err();
    assert!(error.contains("connected"), "{error}");

    app.open_pending();
    assert!(app.modal.is_none());
    assert!(app.status.text.contains("connected"), "{}", app.status.text);

    let intent = crate::connected_actions::parse_intent("task.create Local", "generated").unwrap();
    let error = app
        .client
        .queue_connected_intent("task.create", intent.kind, &intent.action)
        .unwrap_err();
    assert!(error.contains("connected"), "{error}");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_planning_reads_fail_closed_without_accepted_state() {
    let (mut app, path) = offline_app("console-plan-readonly");
    for command in [
        "plan.portfolio",
        "plan.compare",
        "plan.review",
        "plan.focus task-1",
        "plan.group --project p",
    ] {
        match app.console_run(command) {
            crate::app::ConsoleOutcome::Error(error) => {
                assert!(error.contains("No accepted snapshot"), "{command}: {error}");
            }
            other => panic!("{command} must fail closed offline, got {other:?}"),
        }
    }
    assert_eq!(app.client.pending_count(), 0);
    assert!(
        app.pending_scenario.is_none(),
        "no scenario became approvable"
    );
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_approval_requires_an_explicit_review() {
    let (mut app, path) = offline_app("console-approve");
    match app.console_run("plan.approve --confirm") {
        crate::app::ConsoleOutcome::Error(error) => {
            assert!(error.contains("plan.review"), "{error}");
        }
        other => panic!("approval without a review must fail closed, got {other:?}"),
    }
    match app.console_run("plan.approve") {
        crate::app::ConsoleOutcome::Error(error) => {
            assert!(error.contains("--confirm"), "{error}");
        }
        other => panic!("implicit approval must be rejected, got {other:?}"),
    }
    assert_eq!(app.client.pending_count(), 0);
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_portfolio_compare_and_focus_read_only_the_accepted_snapshot() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "console-plan-reads");
    assert_eq!(app.client.accepted_revision().unwrap(), 4);

    for (command, heading) in [
        ("plan.portfolio", "PROJECT-AWARE NEXT ACTIONS"),
        ("plan.compare", "WHAT-IF PLANNING"),
    ] {
        match app.console_run(command) {
            crate::app::ConsoleOutcome::Lines(lines) => {
                let text = lines.join("\n");
                assert!(text.contains(heading), "{command}: {text}");
                assert!(
                    text.to_lowercase().contains("read-only"),
                    "{command}: {text}"
                );
            }
            other => panic!("{command} must be a read-only report, got {other:?}"),
        }
    }

    match app.console_run("plan.focus task-1") {
        crate::app::ConsoleOutcome::Lines(lines) => {
            let text = lines.join("\n");
            assert!(text.contains("TASK FOCUS"), "{text}");
            assert!(text.contains("task-1"), "{text}");
        }
        other => panic!("plan.focus must be read-only, got {other:?}"),
    }

    // Read-only analyses never queue and never become approvable.
    assert_eq!(app.client.pending_count(), 0);
    assert!(app.pending_scenario.is_none());
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_note_show_and_sync_observe_fail_closed_offline() {
    let (mut app, path) = offline_app("console-note-show");
    match app.console_run("note.show docs/plan.md") {
        crate::app::ConsoleOutcome::Error(error) => {
            assert!(error.contains("no cached copy"), "{error}");
        }
        other => panic!("offline note.show must not invent content, got {other:?}"),
    }
    match app.console_run("note.show ../escape.md") {
        crate::app::ConsoleOutcome::Error(error) => {
            assert!(error.contains("invalid vault-relative"), "{error}");
        }
        other => panic!("path traversal must be rejected, got {other:?}"),
    }
    assert!(matches!(
        app.console_run("note.show"),
        crate::app::ConsoleOutcome::Error(_)
    ));
    match app.console_run("sync.observe") {
        crate::app::ConsoleOutcome::Error(error) => {
            assert!(error.contains("Live sync observation"), "{error}");
        }
        other => panic!("offline sync.observe must fail closed, got {other:?}"),
    }
    assert!(
        matches!(
            app.console_run("sync.status"),
            crate::app::ConsoleOutcome::Lines(_)
        ),
        "the cached sync report stays available offline"
    );
    assert_eq!(app.client.pending_count(), 0);
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_batch_preview_queues_nothing_until_confirmed() {
    let (mut app, path) = offline_app("console-batch");
    let outcome = app.console_run(
        "batch task.create First; task.create Second; routine.create morning - Morning",
    );
    let crate::app::ConsoleOutcome::Confirm { pending, body, .. } = outcome else {
        panic!("a valid batch must be previewed, got {outcome:?}");
    };
    assert!(body.contains("PENDING"), "{body}");
    assert_eq!(app.client.pending_count(), 0, "preview stages nothing");

    app.execute_pending(pending);
    assert_eq!(app.client.pending_count(), 3);
    let rows = app.client.pending_commands().unwrap();
    assert_eq!(rows.len(), 3, "batch staging stays atomic");
    assert!(
        rows.iter()
            .all(|row| row.status == "pending" && !row.conflict)
    );
    assert!(app.client.list_projects().unwrap().is_empty());
    drop(app);
    let _ = std::fs::remove_file(&path);
}

fn snapshot_with_accepted_collections() -> serde_json::Value {
    let mut snapshot = snapshot();
    snapshot["routines"] = serde_json::json!([
        {"id": "morning", "name": "Morning reset", "habit_ids": ["h1", "h2"]},
        {"id": "evening", "name": "Evening review", "habit_ids": []}
    ]);
    snapshot["strategies"] = serde_json::json!([{
        "project_id": "p1",
        "mode": "standard",
        "objective": "Ship the beta",
        "progress_signals": ["tasks_completed"],
        "criteria": [
            {"key": "tests", "description": "Tests pass", "required": true, "status": "pending"}
        ]
    }]);
    snapshot["milestones"] = serde_json::json!([{
        "id": "m1", "project_id": "p1", "name": "Beta", "status": "planned"
    }]);
    snapshot["schedule"] = serde_json::json!([{
        "id": "b1", "title": "Deep work", "starts_at_ms": 0, "ends_at_ms": 1_800_000,
        "duration_minutes": 30, "flexibility": "movable",
        "target_kind": "task", "target_id": "task-1"
    }]);
    snapshot
}

#[test]
fn console_lists_accepted_routines_strategies_milestones_and_schedule() {
    let authority = Authority::start(vec![(
        "/v1/authoritative/snapshot",
        snapshot_with_accepted_collections(),
    )]);
    let (mut app, path) = connected_app(&authority.base, "console-accepted-lists");

    for (command, heading, marker) in [
        ("routine.list", "ACCEPTED ROUTINES", "morning"),
        ("strategy.list", "ACCEPTED STRATEGIES", "Ship the beta"),
        ("milestone.list", "ACCEPTED MILESTONES", "Beta"),
        ("schedule.list", "ACCEPTED SCHEDULE", "Deep work"),
    ] {
        match app.console_run(command) {
            crate::app::ConsoleOutcome::Lines(lines) => {
                let text = lines.join("\n");
                assert!(text.contains(heading), "{command}: {text}");
                assert!(
                    text.to_lowercase().contains("accepted"),
                    "{command} must label accepted state: {text}"
                );
                assert!(text.contains(marker), "{command}: {text}");
                assert!(
                    text.to_lowercase().contains("read-only"),
                    "{command}: {text}"
                );
                assert!(
                    text.contains("pending"),
                    "{command} must say pending intents are excluded: {text}"
                );
            }
            other => panic!("{command} must be a read-only listing, got {other:?}"),
        }
    }
    // The listings are reads: nothing queued, nothing staged.
    assert_eq!(app.client.pending_count(), 0);
    assert!(app.pending_scenario.is_none());

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_accepted_lists_state_their_empty_accepted_state() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "console-empty-lists");
    for (command, empty) in [
        ("routine.list", "No accepted routines"),
        ("strategy.list", "No accepted project strategies"),
        ("milestone.list", "No accepted milestones"),
        ("schedule.list", "No accepted schedule blocks"),
    ] {
        match app.console_run(command) {
            crate::app::ConsoleOutcome::Lines(lines) => {
                let text = lines.join("\n");
                assert!(text.contains(empty), "{command}: {text}");
                assert!(
                    text.contains("stays pending"),
                    "{command} must not imply the empty list is final: {text}"
                );
            }
            other => panic!("{command} must be a read-only listing, got {other:?}"),
        }
    }
    assert_eq!(app.client.pending_count(), 0);

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn console_suggestions_are_commands_not_help_prose() {
    let (app, path) = offline_app("console-suggestions");
    let suggestions = app.console_suggestions("");
    assert!(!suggestions.is_empty());
    for suggestion in &suggestions {
        let verb = suggestion.split_whitespace().next().unwrap_or_default();
        assert!(
            verb.contains('.') || matches!(verb, "batch" | "resolve"),
            "suggestion is not command-shaped: {suggestion:?}"
        );
        assert!(
            !suggestion.contains(" — "),
            "help prose leaked into suggestions: {suggestion:?}"
        );
        assert!(
            !suggestion.trim_end().ends_with('.'),
            "sentence leaked into suggestions: {suggestion:?}"
        );
    }
    for expected in [
        "routine.list",
        "strategy.list",
        "milestone.list",
        "schedule.list",
    ] {
        assert!(
            suggestions.iter().any(|line| line == expected),
            "missing read-only listing suggestion {expected}: {suggestions:?}"
        );
    }

    // Every concrete (placeholder-free) suggestion must be accepted by one of
    // the two grammars, so Tab never fills something the console rejects.
    for suggestion in &suggestions {
        if suggestion.contains('<') || suggestion.contains('[') {
            continue;
        }
        let parsed = crate::connected_actions::parse_intent(suggestion, "generated").is_ok()
            || crate::connected_ops::parse(suggestion, 0, Some(1), "generated").is_ok();
        assert!(parsed, "suggestion does not parse: {suggestion:?}");
    }

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_flush_that_leaves_staged_uploads_pending_is_not_reported_as_success() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "flush-staged-upload");
    let source = std::env::temp_dir().join(format!(
        "orbit-tui-flush-staged-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&source, b"staged bytes").unwrap();
    app.import_attachment(
        "task".into(),
        "task-1".into(),
        source.to_string_lossy().into_owned(),
        None,
    )
    .expect("staged upload");
    assert_eq!(app.client.pending_attachment_count().unwrap(), 1);
    assert_eq!(app.pending_count(), 1, "the cached count follows the stage");

    let (_rows, _detail) = app.pending_flush_report(&[]);
    assert_ne!(
        app.status.kind,
        StatusKind::Success,
        "a flush leaving staged uploads must not read as success: {}",
        app.status.text
    );
    assert!(
        matches!(app.status.kind, StatusKind::Pending | StatusKind::Warning),
        "{:?}: {}",
        app.status.kind,
        app.status.text
    );
    assert!(
        app.status.text.contains("staged attachment upload"),
        "{}",
        app.status.text
    );

    // The `f` action path must report the same pending truth.
    let flush = orbit_ui::AvailableAction::enabled("flush_pending", "Flush", None);
    assert!(!app.execute_action(flush));
    assert_ne!(
        app.status.kind,
        StatusKind::Success,
        "a flush action leaving staged uploads must not read as success: {}",
        app.status.text
    );
    assert!(
        matches!(app.status.kind, StatusKind::Pending | StatusKind::Warning),
        "{:?}: {}",
        app.status.kind,
        app.status.text
    );

    let _ = std::fs::remove_file(&source);
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn flush_refreshes_accepted_state_and_separates_accepted_from_pending() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "flush-refresh");
    app.go_screen(Screen::Inbox);
    // Build the docked detail cache for the selected task.
    let _ = app.detail_model();
    let builds = app.detail_cache_builds.get();
    let refreshes = app.workspace_refreshes.get();

    let intent = crate::connected_actions::parse_intent(
        "project.create refresh-project Queued refresh",
        "generated",
    )
    .expect("intent");
    app.execute_pending(app::PendingAction::QueueIntent { intent });
    assert_eq!(app.pending_count(), 1, "queue updates the cached count");
    let id = app.client.last_queued_command_id().expect("queued id");

    // The loopback authority rejects the command route, so the flush leaves the
    // command pending/rejected; accepted state is still refreshed.
    let _ = app.pending_flush_report(std::slice::from_ref(&id));

    assert!(
        app.workspace_refreshes.get() > refreshes,
        "a non-empty flush must re-read accepted state"
    );
    let _ = app.detail_model();
    assert!(
        app.detail_cache_builds.get() > builds,
        "a non-empty flush must invalidate the detail cache"
    );
    assert!(
        app.status.text.contains("accepted by the server"),
        "{}",
        app.status.text
    );
    assert!(
        app.status.text.contains("still pending/rejected"),
        "{}",
        app.status.text
    );
    assert!(
        app.status.text.contains("0 accepted by the server"),
        "{}",
        app.status.text
    );
    assert_ne!(app.status.kind, StatusKind::Success, "{}", app.status.text);

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_failed_flush_does_not_claim_to_have_refreshed_accepted_state() {
    let (mut app, path) = offline_app("flush-failed-refresh");
    let intent =
        crate::connected_actions::parse_intent("task.create Offline task", "generated").unwrap();
    app.execute_pending(app::PendingAction::QueueIntent { intent });
    let id = app.client.last_queued_command_id().expect("queued id");
    let refreshes = app.workspace_refreshes.get();
    let _ = app.pending_flush_report(std::slice::from_ref(&id));
    assert_eq!(
        app.workspace_refreshes.get(),
        refreshes,
        "an offline flush failure must not pretend to refresh accepted state"
    );
    assert_eq!(app.pending_count(), 1, "the command is still pending");
    assert_eq!(app.status.kind, StatusKind::Error, "{}", app.status.text);
    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn narrow_conflict_review_can_scroll_to_the_rejection_reason() {
    let authority = Authority::start(vec![("/v1/authoritative/snapshot", snapshot())]);
    let (mut app, path) = connected_app(&authority.base, "conflict-narrow");
    let intent = crate::connected_actions::parse_intent(
        "project.create narrow-conflict Rejected project",
        "generated",
    )
    .expect("intent");
    app.execute_pending(app::PendingAction::QueueIntent { intent });
    let id = app.client.last_queued_command_id().expect("queued id");
    let flush = orbit_ui::AvailableAction::enabled("flush_pending", "Flush", None);
    assert!(!app.execute_action(flush));

    app.open_conflict(&id);
    let Some(Modal::Conflict { review_lines, .. }) = app.modal.as_ref() else {
        panic!("conflict modal missing: {}", app.status.text);
    };
    let reason_line = review_lines
        .iter()
        .find(|line| line.starts_with("Rejection reason:"))
        .unwrap_or_else(|| panic!("no rejection reason in {review_lines:?}"))
        .clone();
    assert!(
        reason_line.contains("404") || reason_line.contains("not found"),
        "the server reason must be in the review: {reason_line}"
    );

    // A 60x18 terminal cannot show the whole review at once: the wrapped
    // reason continues below the fold, so scrolling must reach it.
    let mut seen_header = false;
    let mut seen_reason = false;
    for _ in 0..24 {
        let text = rendered(&mut app, 60, 18);
        if text.contains("Rejection reason:") {
            seen_header = true;
        }
        if text.contains("404") || text.contains("not found") {
            seen_reason = true;
            break;
        }
        // One line at a time so the window cannot jump past the reason.
        handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE).unwrap();
    }
    assert!(
        seen_header,
        "the review must label a rejection reason: {}",
        app.status.text
    );
    assert!(
        seen_reason,
        "scrolling must reach the server rejection reason at 60x18: {}",
        app.status.text
    );
    assert!(app.modal_scroll > 0, "scrolling must move the review");

    drop(app);
    let _ = std::fs::remove_file(&path);
}
