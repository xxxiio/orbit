//! M29 readiness regressions: the three majors (queued-detail guard, detail
//! focus on hide, cache-only render) and the code-side minors.
use crate::app::{FocusPane, FormMode, Modal, PaletteAction, Screen, StatusKind};
use crate::*;
use ratatui::{Terminal, backend::TestBackend};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

fn local_app(stem: &str) -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-readiness-{stem}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

fn render(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn field<'a>(form: &'a app::FormState, key: &str) -> &'a str {
    form.fields
        .iter()
        .find(|candidate| candidate.key == key)
        .map(|candidate| candidate.value.as_str())
        .unwrap_or_default()
}

fn set_field(form: &mut app::FormState, key: &str, value: &str) {
    let Some(field) = form
        .fields
        .iter_mut()
        .find(|candidate| candidate.key == key)
    else {
        panic!("field {key} missing");
    };
    field.value = value.to_owned();
    field.cursor = value.chars().count();
    form.dirty = true;
}

// ---------------------------------------------------------------------------
// M1: a second details mutation while one is pending is refused, not merged.
// ---------------------------------------------------------------------------

/// Minimal loopback authority: path-addressed JSON routes for GET and POST.
/// Undeclared paths are 404, so a test cannot rely on a route it never declared.
struct Authority {
    base: String,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Authority {
    fn start(
        get_routes: Vec<(&'static str, serde_json::Value)>,
        post_routes: Vec<(&'static str, serde_json::Value)>,
    ) -> Self {
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
                let mut request_line = head.lines().next().unwrap_or_default().split_whitespace();
                let method = request_line.next().unwrap_or_default();
                let target = request_line.next().unwrap_or_default();
                let path = target.split('?').next().unwrap_or_default();
                let routes = if method == "POST" {
                    &post_routes
                } else {
                    &get_routes
                };
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

fn connected_app(server: &str, stem: &str) -> (App, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-readiness-{stem}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let backend = crate::backend::Backend::open_connected_with_identity(
        &path,
        server,
        Some(("device-1", "token")),
    )
    .expect("connected backend");
    let app = App::open_with_backend(backend).expect("connected app");
    (app, path)
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
        "labels": ["api"],
    })
}

/// Acceptance receipt for one queued task command at expected revision 4.
fn receipt() -> serde_json::Value {
    serde_json::json!({"revision": 5_i64, "duplicate": false, "task": {"id": "task-1"}})
}

fn queued_details(rows: &[crate::backend::PendingCommand]) -> usize {
    rows.iter()
        .filter(|row| row.payload.contains("set_details"))
        .count()
}

/// Queue one accepted `SetDetails` intent for `task-1` through the same form
/// the user would use, leaving no pending-form modal behind.
fn queue_details_edit(app: &mut App, description: &str) {
    app.start_form(FormMode::EditTaskDetails {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("details form missing");
    };
    set_field(&mut form, "description", description);
    app.submit_form(form);
    assert_eq!(app.status.kind, StatusKind::Pending, "{}", app.status.text);
    assert!(!matches!(app.modal, Some(Modal::Form(_))));
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(queued_details(&rows), 1, "{rows:?}");
}

/// Flush the connected outbox and close the modal surface the flush leaves.
fn flush_pending(app: &mut App) {
    let flush = orbit_ui::AvailableAction::enabled("flush_pending", "Flush", None);
    assert!(!app.execute_action(flush));
    assert_eq!(app.pending_count(), 0, "{}", app.status.text);
    app.modal = None;
}

#[test]
fn connected_pending_details_refuses_a_second_details_edit_until_flush() {
    let authority = Authority::start(
        vec![
            ("/v1/authoritative/snapshot", snapshot()),
            ("/v1/authoritative/tasks/task-1", detail()),
        ],
        vec![("/v1/authoritative/task-commands", receipt())],
    );
    let (mut app, path) = connected_app(&authority.base, "pending-details-guard");
    app.go_screen(Screen::Inbox);
    assert_eq!(
        app.selected_item().map(|item| item.id.as_str()),
        Some("task-1")
    );

    // First details edit: queues one canonical SetDetails command.
    app.start_form(FormMode::EditTaskDetails {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("details form missing");
    };
    set_field(&mut form, "description", "Queued description");
    app.submit_form(form);
    assert_eq!(app.status.kind, StatusKind::Pending, "{}", app.status.text);
    assert!(!matches!(app.modal, Some(Modal::Form(_))));
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(queued_details(&rows), 1, "{rows:?}");

    // Second details mutation (quick edit) is refused: the payload would be
    // rebuilt from accepted state and would silently revert the first edit.
    app.start_form(FormMode::QuickEditTask {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("quick edit form missing");
    };
    set_field(&mut form, "title", "Reverted title");
    app.submit_form(form);
    assert_eq!(app.status.kind, StatusKind::Warning, "{}", app.status.text);
    assert!(
        app.status
            .text
            .contains("A detail change for this task is already pending"),
        "{}",
        app.status.text
    );
    assert!(
        app.status
            .text
            .contains("close this form with Esc, then press f to flush"),
        "{}",
        app.status.text
    );
    let Some(Modal::Form(kept)) = app.modal.as_ref() else {
        panic!("a refused submit must keep the form open");
    };
    assert!(matches!(kept.mode, FormMode::QuickEditTask { .. }));
    assert_eq!(
        field(kept, "title"),
        "Reverted title",
        "the typed input is preserved"
    );
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(rows.len(), 1, "nothing else was queued: {rows:?}");
    assert_eq!(queued_details(&rows), 1, "{rows:?}");
    assert_eq!(
        app.client.task_detail("task-1").unwrap().description,
        "Body from server",
        "the first edit is still queued, never silently reverted"
    );

    // `f` flushes the queued command; the guard then releases.
    let flush = orbit_ui::AvailableAction::enabled("flush_pending", "Flush", None);
    assert!(!app.execute_action(flush));
    assert_eq!(app.pending_count(), 0, "{}", app.status.text);
    app.modal = None;

    app.start_form(FormMode::EditTaskDetails {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("details form missing after flush");
    };
    set_field(&mut form, "description", "Second accepted edit");
    app.submit_form(form);
    assert!(
        !matches!(app.modal, Some(Modal::Form(_))),
        "an edit after a flush is allowed: {}",
        app.status.text
    );
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(queued_details(&rows), 1, "{rows:?}");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// M1: the other two guarded writers refuse a second details change.
// ---------------------------------------------------------------------------

#[test]
fn connected_pending_details_refuses_the_details_form_until_flush() {
    let authority = Authority::start(
        vec![
            ("/v1/authoritative/snapshot", snapshot()),
            ("/v1/authoritative/tasks/task-1", detail()),
        ],
        vec![("/v1/authoritative/task-commands", receipt())],
    );
    let (mut app, path) = connected_app(&authority.base, "pending-details-form");
    app.go_screen(Screen::Inbox);
    queue_details_edit(&mut app, "Queued description");

    // The details editor rebuilds from accepted state too, so it is refused
    // while the first change is still queued.
    app.start_form(FormMode::EditTaskDetails {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("details form missing");
    };
    set_field(&mut form, "description", "Second details");
    app.submit_form(form);
    assert_eq!(app.status.kind, StatusKind::Warning, "{}", app.status.text);
    assert!(
        app.status
            .text
            .contains("close this form with Esc, then press f to flush"),
        "{}",
        app.status.text
    );
    let Some(Modal::Form(kept)) = app.modal.as_ref() else {
        panic!("a refused submit must keep the form open");
    };
    assert!(matches!(kept.mode, FormMode::EditTaskDetails { .. }));
    assert_eq!(
        field(kept, "description"),
        "Second details",
        "the typed input is preserved"
    );
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(rows.len(), 1, "nothing else was queued: {rows:?}");

    flush_pending(&mut app);

    // After the flush the same edit goes through.
    app.start_form(FormMode::EditTaskDetails {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("details form missing after flush");
    };
    set_field(&mut form, "description", "Second accepted details");
    app.submit_form(form);
    assert!(
        !matches!(app.modal, Some(Modal::Form(_))),
        "an edit after a flush is allowed: {}",
        app.status.text
    );
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(queued_details(&rows), 1, "{rows:?}");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn connected_pending_details_refuses_the_full_task_form_until_flush() {
    let authority = Authority::start(
        vec![
            ("/v1/authoritative/snapshot", snapshot()),
            ("/v1/authoritative/tasks/task-1", detail()),
        ],
        vec![("/v1/authoritative/task-commands", receipt())],
    );
    let (mut app, path) = connected_app(&authority.base, "pending-full-task-form");
    app.go_screen(Screen::Inbox);
    queue_details_edit(&mut app, "Queued description");

    // The full `e` form also rewrites rich details from accepted state.
    app.start_form(FormMode::EditTask {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("task form missing");
    };
    set_field(&mut form, "description", "Second details");
    app.submit_form(form);
    assert_eq!(app.status.kind, StatusKind::Warning, "{}", app.status.text);
    assert!(
        app.status
            .text
            .contains("close this form with Esc, then press f to flush"),
        "{}",
        app.status.text
    );
    let Some(Modal::Form(kept)) = app.modal.as_ref() else {
        panic!("a refused submit must keep the form open");
    };
    assert!(matches!(kept.mode, FormMode::EditTask { .. }));
    assert_eq!(
        field(kept, "description"),
        "Second details",
        "the typed input is preserved"
    );
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(rows.len(), 1, "nothing else was queued: {rows:?}");

    flush_pending(&mut app);

    app.start_form(FormMode::EditTask {
        task_id: "task-1".into(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("task form missing after flush");
    };
    set_field(&mut form, "description", "Second accepted details");
    app.submit_form(form);
    assert!(
        !matches!(app.modal, Some(Modal::Form(_))),
        "an edit after a flush is allowed: {}",
        app.status.text
    );
    let rows = app.client.pending_commands().expect("pending rows");
    assert_eq!(queued_details(&rows), 1, "{rows:?}");

    drop(app);
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// M2: hiding the detail pane while it owns focus must not strand focus.
// ---------------------------------------------------------------------------

#[test]
fn hiding_the_focused_detail_pane_returns_focus_to_the_workspace() {
    let mut app = local_app("detail-focus");
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Focus Project".into(),
        })
        .unwrap();
    app.client.install_bundled_blueprint("basic").unwrap();
    app.client
        .apply_workflow(project.id.clone(), "basic".to_owned(), None)
        .unwrap();
    for title in ["Alpha task", "Beta task"] {
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: title.into(),
                project_id: Some(project.id.clone()),
                milestone_id: None,
            })
            .unwrap();
        app.client
            .set_task_status(orbit_client::SetTaskStatusRequest {
                id: task.id.clone(),
                status: "ready".into(),
            })
            .unwrap();
    }
    app.refresh_workspace();
    app.open_project(&project.id);
    let board = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "board")
        .expect("board view");
    app.switch_view_by_index(board);
    app.active_component = 0;

    // The viewport must be wide before `D` toggles the docked pane.
    render(&mut app, 160, 45);
    assert_eq!(app.focus, FocusPane::Workspace);
    app.focus_next(false);
    assert_eq!(
        app.focus,
        FocusPane::Detail,
        "Tab reaches the detail pane at the wide breakpoint"
    );
    let focused_detail = render(&mut app, 160, 45);
    assert!(
        focused_detail
            .lines()
            .any(|line| line.contains('┏') && line.contains("Details")),
        "the detail pane should paint as focused: {focused_detail}"
    );

    handle_key(&mut app, KeyCode::Char('D'), KeyModifiers::NONE).unwrap();
    assert!(!app.show_detail);
    assert_eq!(
        app.focus,
        FocusPane::Workspace,
        "focus must not stay on a hidden pane"
    );

    let selected = app.selected;
    let detail_scroll = app.detail_scroll;
    handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::NONE).unwrap();
    assert_ne!(
        app.selected, selected,
        "j moves the workspace selection after D"
    );
    assert_eq!(
        app.detail_scroll, detail_scroll,
        "j must not scroll the hidden detail pane"
    );
    handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE).unwrap();
    assert_eq!(app.selected, selected, "k moves it back");

    let hidden = render(&mut app, 160, 45);
    assert!(
        hidden
            .lines()
            .any(|line| line.contains('┏') && line.contains("Task board")),
        "the workspace is the focused pane: {hidden}"
    );
    assert!(
        !hidden
            .lines()
            .any(|line| line.contains('┏') && line.contains("Details")),
        "the hidden pane cannot claim focus: {hidden}"
    );
}

// ---------------------------------------------------------------------------
// Minor: the palette has one "New task" row, carrying the contextual project.
// ---------------------------------------------------------------------------

#[test]
fn palette_offers_exactly_one_contextual_new_task_entry() {
    let mut app = local_app("palette-new-task");
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Palette Project".into(),
        })
        .unwrap();
    app.refresh_workspace();
    app.open_project(&project.id);

    let entries = app.palette_entries("");
    let new_task: Vec<_> = entries
        .iter()
        .filter(|entry| entry.label == "New task")
        .collect();
    assert_eq!(
        new_task.len(),
        1,
        "duplicate New task rows: {:?}",
        entries
            .iter()
            .map(|entry| entry.label.clone())
            .collect::<Vec<_>>()
    );
    match &new_task[0].action {
        PaletteAction::Form(FormMode::NewTask {
            project_id: Some(id),
        }) => assert_eq!(id, &project.id),
        other => panic!("the kept row must carry the open project: {other:?}"),
    }

    app.go_screen(Screen::Home);
    let entries = app.palette_entries("");
    let new_task: Vec<_> = entries
        .iter()
        .filter(|entry| entry.label == "New task")
        .collect();
    assert_eq!(new_task.len(), 1);
    match &new_task[0].action {
        PaletteAction::Form(FormMode::NewTask { project_id: None }) => {}
        other => panic!("outside a project there is no contextual project: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Minor: a corrected submit right after a validation error is not swallowed.
// ---------------------------------------------------------------------------

#[test]
fn corrected_submit_after_a_validation_error_is_not_treated_as_a_duplicate() {
    let mut app = local_app("duplicate-guard");

    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("new task form missing");
    };
    set_field(&mut form, "title", "");
    app.submit_form(form);
    let Some(Modal::Form(rejected)) = app.modal.as_ref() else {
        panic!("a validation error keeps the form open");
    };
    assert!(rejected.error.is_some());

    // Corrected immediately (well inside the 250 ms window).
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("form missing");
    };
    set_field(&mut form, "title", "Corrected task");
    app.submit_form(form);
    assert!(
        !app.status.text.contains("Duplicate form submission"),
        "{}",
        app.status.text
    );
    assert!(
        app.client
            .list_tasks()
            .unwrap()
            .iter()
            .any(|task| task.title == "Corrected task"),
        "the corrected submit must create the task"
    );

    // A genuine second submit of the same payload is still refused.
    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("new task form missing");
    };
    set_field(&mut form, "title", "Twice");
    app.submit_form(form);
    assert!(
        app.client
            .list_tasks()
            .unwrap()
            .iter()
            .any(|task| task.title == "Twice")
    );
    app.start_form(FormMode::NewTask { project_id: None });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("new task form missing");
    };
    set_field(&mut form, "title", "Twice");
    app.submit_form(form);
    assert!(
        app.status.text.contains("Duplicate form submission"),
        "{}",
        app.status.text
    );
    assert_eq!(
        app.client
            .list_tasks()
            .unwrap()
            .iter()
            .filter(|task| task.title == "Twice")
            .count(),
        1
    );
}

// ---------------------------------------------------------------------------
// Minor: help marks the keys the connected deployment refuses.
// ---------------------------------------------------------------------------

#[test]
fn help_marks_connected_inapplicable_keys() {
    let mut app = local_app("help-local");
    app.modal = Some(Modal::Help);
    let local = render(&mut app, 120, 40);
    assert!(local.contains("change the project workflow"), "{local}");
    assert!(
        !local.contains("unavailable in connected mode"),
        "the local authority supports these keys: {local}"
    );

    let path = std::env::temp_dir().join(format!(
        "orbit-tui-readiness-help-connected-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let backend = crate::backend::Backend::open_connected_with_identity(
        &path,
        "https://example.invalid",
        None,
    )
    .expect("connected backend");
    let mut connected = App::open_with_backend(backend).expect("connected app");
    connected.modal = Some(Modal::Help);
    let text = render(&mut connected, 120, 40);
    for line in text.lines() {
        if line.contains("change the project workflow") || line.contains("new knowledge note") {
            assert!(
                line.contains("local authority only") || line.contains("unavailable in connected"),
                "connected-inapplicable key not marked: {line:?}"
            );
        }
    }
    assert!(
        text.contains("local authority only"),
        "the connected help marks W/n: {text}"
    );

    drop(connected);
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// Minor: `refresh_after_task_title` only claims a fast path on project screens.
// ---------------------------------------------------------------------------

#[test]
fn quick_edit_on_a_non_project_screen_uses_the_documented_full_refresh() {
    let mut app = local_app("title-refresh-home");
    let task = app
        .client
        .create_task(orbit_client::CreateTaskRequest {
            id: None,
            title: "Triage me".into(),
            project_id: None,
            milestone_id: None,
        })
        .unwrap();
    app.refresh_workspace();
    app.go_screen(Screen::Inbox);
    assert_eq!(
        app.selected_item().map(|item| item.id.as_str()),
        Some(task.id.as_str())
    );

    app.start_form(FormMode::QuickEditTask {
        task_id: task.id.clone(),
    });
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("quick edit form missing");
    };
    set_field(&mut form, "title", "Triage later");
    let refreshes = app.workspace_refreshes.get();
    app.submit_form(form);
    assert!(
        app.status.text.contains("Quick edit saved"),
        "{}",
        app.status.text
    );
    assert_eq!(
        app.workspace_refreshes.get(),
        refreshes + 1,
        "Home/Inbox have no scoped read; the documented fallback is the full refresh"
    );
    assert!(
        app.workspace
            .as_ref()
            .unwrap()
            .inbox
            .iter()
            .any(|row| row.id == task.id && row.title == "Triage later"),
        "the full refresh shows the new title"
    );
}
