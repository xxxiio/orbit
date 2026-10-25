//! M30 workstream A: the explicit item inspector.
//!
//! These tests pin the contract that replaced the permanent docked Details
//! pane: two regions by default, an overlay inspector opened on demand, a
//! full-screen inspector on terminals too small for a readable overlay, one
//! rebuild per open (never per frame), and an explicit fallback when the
//! inspected entity disappears.
use crate::app::{FocusPane, FormMode, InspectorState, Modal, Screen};
use crate::*;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn base_app(stem: &str) -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-{stem}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

/// A project on the Basic workflow with three tasks and the Board view active.
fn project_app(stem: &str) -> App {
    let mut app = base_app(stem);
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Inspector Project".into(),
        })
        .unwrap();
    app.client.install_bundled_blueprint("basic").unwrap();
    app.client
        .apply_workflow(project.id.clone(), "basic".to_owned(), None)
        .unwrap();
    for (title, statuses) in [
        ("Alpha task for inspection", vec!["ready"]),
        ("Beta task for inspection", vec!["ready"]),
        ("Gamma task for inspection", vec!["ready", "in_progress"]),
    ] {
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: title.into(),
                project_id: Some(project.id.clone()),
                milestone_id: None,
            })
            .unwrap();
        for status in statuses {
            app.client
                .set_task_status(orbit_client::SetTaskStatusRequest {
                    id: task.id.clone(),
                    status: status.into(),
                })
                .unwrap();
        }
    }
    app.refresh_workspace();
    app.open_project(&project.id);
    switch_to(&mut app, "board");
    app
}

fn switch_to(app: &mut App, view_id: &str) {
    let index = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == view_id)
        .unwrap_or_else(|| panic!("view {view_id} missing"));
    app.switch_view_by_index(index);
    app.active_component = 0;
    app.selected = 0;
}

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render(app: &mut App, width: u16, height: u16) -> String {
    text(&buffer(app, width, height))
}

fn inspector(app: &App) -> &InspectorState {
    match &app.modal {
        Some(Modal::Inspector(state)) => state,
        other => panic!("inspector modal expected, found {other:?}"),
    }
}

fn press(app: &mut App, code: KeyCode) {
    handle_key(app, code, KeyModifiers::NONE).unwrap();
}

// ---------------------------------------------------------------------------
// 1. The default layout is exactly two regions at every size.
// ---------------------------------------------------------------------------
#[test]
fn default_layout_is_sidebar_plus_workspace_at_every_size() {
    let mut app = project_app("m30-two-regions");
    for (width, height) in [(80, 24), (120, 35), (160, 45)] {
        let label = format!("{width}x{height}");
        let rendered = render(&mut app, width, height);
        assert!(
            !rendered.contains("Details"),
            "{label}: no Details block may be painted:\n{rendered}"
        );
        // Exactly two bordered panes: the sidebar and the workspace. The
        // project view also paints its own component panels, so count only the
        // chrome titles instead of every border.
        assert!(
            rendered.contains("Orbit"),
            "{label}: sidebar title missing:\n{rendered}"
        );
        assert!(
            rendered.contains("Task board"),
            "{label}: workspace body missing:\n{rendered}"
        );
        // The sidebar owns columns 0..=21 at these widths, so the workspace
        // inner width is the frame width beyond the sidebar border.
        let first_workspace_border = rendered
            .lines()
            .map(|line| line.chars().position(|c| c == '┏' || c == '╭'))
            .find(|position| position.is_some())
            .flatten()
            .unwrap_or_else(|| panic!("{label}: no pane border\n{rendered}"));
        assert!(
            first_workspace_border < 24,
            "{label}: sidebar must be the leftmost pane, found {first_workspace_border}\n{rendered}"
        );
        // Nothing is squeezed out: the board legend and the footer survive.
        assert!(rendered.contains("Actions:"), "{label}:\n{rendered}");
        assert!(
            rendered.contains("Ctrl+Q quit"),
            "{label}: quit hint missing:\n{rendered}"
        );
    }
}

#[test]
fn workspace_owns_all_width_beyond_the_sidebar() {
    let mut app = project_app("m30-workspace-width");
    for (width, height) in [(80u16, 24u16), (120, 35), (160, 45)] {
        let rendered = render(&mut app, width, height);
        for line in rendered.lines() {
            assert_eq!(
                line.chars().count(),
                usize::from(width),
                "{width}x{height}: row width changed: {line:?}"
            );
        }
        // The workspace right border sits in the last column.
        let body = rendered
            .lines()
            .find(|line| line.contains("Task board"))
            .unwrap_or_else(|| panic!("{width}x{height}: board row missing\n{rendered}"));
        assert!(
            body.trim_end().ends_with('┓') || body.trim_end().ends_with('╮'),
            "{width}x{height}: workspace border is not at the frame edge: {body:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Enter opens the inspector; Esc closes it without moving anything.
// ---------------------------------------------------------------------------
#[test]
fn enter_opens_an_overlay_and_esc_restores_the_workspace_unchanged() {
    let mut app = project_app("m30-open-close");
    app.selected = 1;
    app.scroll = 1;
    let component = app.active_component;
    let selected = app.selected;
    let selected_id = app.current_selected_id();
    let scroll = app.scroll;
    let focus = app.focus;

    press(&mut app, KeyCode::Enter);
    assert!(
        matches!(app.modal, Some(Modal::Inspector(_))),
        "Enter must open the inspector: {}",
        app.status.text
    );
    assert_eq!(inspector(&app).return_focus, focus);
    assert!(!inspector(&app).unavailable);
    let rendered = render(&mut app, 120, 35);
    assert!(
        rendered.contains("Inspector · Beta task for inspection"),
        "the overlay must name its subject:\n{rendered}"
    );
    assert!(
        rendered.contains("Esc close · j/k scroll · Enter close"),
        "the inspector footer is missing:\n{rendered}"
    );
    assert!(rendered.contains("PROPERTIES"), "{rendered}");

    // Scrolling the inspector never touches the workspace list.
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.selected, selected, "the list selection must not move");
    assert_eq!(app.scroll, scroll, "the list scroll must not move");

    let closed_at = inspector(&app).scroll;
    press(&mut app, KeyCode::Esc);
    assert!(app.modal.is_none(), "Esc must close the inspector");
    assert_eq!(app.active_component, component);
    assert_eq!(app.selected, selected);
    assert_eq!(app.current_selected_id(), selected_id);
    assert_eq!(app.scroll, scroll);
    assert_eq!(app.focus, focus, "focus returns to the same pane");

    // Reopening the same subject keeps the inspector's own scroll offset.
    press(&mut app, KeyCode::Enter);
    assert_eq!(inspector(&app).scroll, closed_at);
    press(&mut app, KeyCode::Esc);
}

#[test]
fn d_opens_and_closes_the_inspector() {
    let mut app = project_app("m30-toggle");
    press(&mut app, KeyCode::Char('D'));
    assert!(
        matches!(app.modal, Some(Modal::Inspector(_))),
        "{}",
        app.status.text
    );
    press(&mut app, KeyCode::Char('D'));
    assert!(app.modal.is_none(), "D closes an open inspector");
    press(&mut app, KeyCode::Char('D'));
    assert!(matches!(app.modal, Some(Modal::Inspector(_))));
}

#[test]
fn tab_cycles_only_the_painted_panes() {
    let mut app = project_app("m30-tab");
    app.focus = FocusPane::Workspace;
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut app, KeyCode::Tab);
        seen.push(app.focus);
    }
    assert_eq!(
        seen,
        vec![
            FocusPane::Sidebar,
            FocusPane::Workspace,
            FocusPane::Sidebar,
            FocusPane::Workspace
        ]
    );
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.focus, FocusPane::Sidebar);
}

#[test]
fn closing_returns_focus_to_the_sidebar_when_it_owned_it() {
    let mut app = project_app("m30-sidebar-return");
    app.focus = FocusPane::Sidebar;
    press(&mut app, KeyCode::Char('D'));
    assert_eq!(inspector(&app).return_focus, FocusPane::Sidebar);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.focus, FocusPane::Sidebar);
}

// ---------------------------------------------------------------------------
// 3. Overlay vs full-screen decision.
// ---------------------------------------------------------------------------
fn inspector_popup_origin(app: &mut App, width: u16, height: u16) -> (u16, u16) {
    let buffer = buffer(app, width, height);
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let symbol = buffer[(x, y)].symbol();
            if symbol == "╔" {
                return (x, y);
            }
        }
    }
    panic!("no inspector border found at {width}x{height}");
}

#[test]
fn small_terminals_open_the_inspector_full_screen() {
    let mut app = project_app("m30-fullscreen");
    for (width, height) in [(60, 18), (79, 19)] {
        app.modal = None;
        press(&mut app, KeyCode::Enter);
        let rendered = render(&mut app, width, height);
        assert!(
            rendered.contains("Inspector ·"),
            "{width}x{height}: {rendered}"
        );
        assert_eq!(
            inspector_popup_origin(&mut app, width, height),
            (0, 0),
            "{width}x{height}: the inspector must use the whole frame"
        );
        press(&mut app, KeyCode::Esc);
        assert!(app.modal.is_none());
        assert!(app.status.kind != StatusKind::Error);
    }
}

#[test]
fn overlay_decision_boundary_is_readability_based() {
    let mut app = project_app("m30-boundary");
    // 80x20 is the smallest readable overlay.
    press(&mut app, KeyCode::Enter);
    let origin = inspector_popup_origin(&mut app, 80, 20);
    assert!(
        origin.0 > 0 && origin.1 > 0,
        "80x20 must be an inset overlay, found origin {origin:?}"
    );
    press(&mut app, KeyCode::Esc);
    // One column narrower falls back to the whole frame.
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        inspector_popup_origin(&mut app, 79, 20),
        (0, 0),
        "79x20 must open full screen"
    );
    app.modal = None;
    // ... and so does one row shorter at the same width.
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        inspector_popup_origin(&mut app, 80, 19),
        (0, 0),
        "80x19 must open full screen"
    );
}

// ---------------------------------------------------------------------------
// 4. The inspector never opens itself.
// ---------------------------------------------------------------------------
#[test]
fn the_inspector_stays_closed_across_views_screens_and_refreshes() {
    let mut app = project_app("m30-no-auto-open");
    assert!(app.modal.is_none());
    app.switch_view(1);
    assert!(app.modal.is_none(), "a view switch must not open it");
    app.switch_view(-1);
    assert!(app.modal.is_none());
    app.refresh_project();
    assert!(app.modal.is_none(), "a refresh must not open it");
    app.go_screen(Screen::Home);
    assert!(app.modal.is_none());
    app.refresh_workspace();
    assert!(app.modal.is_none());
    press(&mut app, KeyCode::Char('r'));
    assert!(app.modal.is_none(), "refresh must not open it");
}

// ---------------------------------------------------------------------------
// 5. Empty views report "No selection" instead of an empty panel.
// ---------------------------------------------------------------------------
#[test]
fn an_empty_view_reports_no_selection() {
    let mut app = base_app("m30-empty");
    app.home.components = vec![orbit_ui::ResolvedComponent {
        id: "empty".into(),
        title: "Empty".into(),
        component_type: "list".into(),
        status: orbit_ui::LoadStatus::Empty,
        body: orbit_ui::ComponentBody::List { items: Vec::new() },
        selection: Vec::new(),
    }];
    app.active_component = 0;
    app.selected = 0;
    press(&mut app, KeyCode::Enter);
    assert!(app.modal.is_none(), "no empty inspector panel");
    assert!(
        app.status.text.contains("No selection"),
        "{}",
        app.status.text
    );
    press(&mut app, KeyCode::Char('D'));
    assert!(app.modal.is_none());
    assert!(app.status.text.contains("No selection"));
}

// ---------------------------------------------------------------------------
// 6. A vanished subject paints the explicit fallback.
// ---------------------------------------------------------------------------
#[test]
fn a_deleted_entity_shows_the_fallback_and_esc_still_closes() {
    let mut app = project_app("m30-deleted");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();
    let title = app.selected_item().unwrap().title.clone();
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.modal, Some(Modal::Inspector(_))));

    // An external authority mutation moves the task out of this project, so
    // the row disappears from the resolved view underneath the overlay.
    app.client
        .assign_task_project(task_id.clone(), None)
        .unwrap();
    app.refresh_workspace();
    app.refresh_project();

    let rendered = render(&mut app, 120, 35);
    assert!(
        rendered.contains("This item is no longer available"),
        "the fallback must be explicit:\n{rendered}"
    );
    assert!(
        rendered.contains(&title),
        "the fallback keeps the last known title:\n{rendered}"
    );
    assert!(
        rendered.contains("kind: task"),
        "the fallback keeps the last known kind:\n{rendered}"
    );
    assert!(inspector(&app).unavailable);

    press(&mut app, KeyCode::Esc);
    assert!(app.modal.is_none(), "Esc still closes the fallback");
}

#[test]
fn a_project_that_leaves_the_workspace_shows_the_fallback() {
    let mut app = base_app("m30-project-closed");
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Closing Project".into(),
        })
        .unwrap();
    app.refresh_workspace();
    app.go_screen(Screen::Projects);
    app.projects_selection = app
        .projects
        .iter()
        .position(|row| row.id == project.id)
        .expect("project row");

    press(&mut app, KeyCode::Char('D'));
    let state = inspector(&app);
    assert_eq!(state.detail.title, "Closing Project");
    assert!(rendered_has(&mut app, "Inspector · Closing Project"));

    // The project disappears from accepted state (closed or archived).
    app.projects.retain(|row| row.id != project.id);
    app.refresh_inspector();
    let fallback = render(&mut app, 120, 35);
    assert!(
        fallback.contains("This item is no longer available"),
        "{fallback}"
    );
    assert!(
        fallback.contains("Closing Project"),
        "the fallback keeps the last known title: {fallback}"
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.modal.is_none());
}

fn rendered_has(app: &mut App, needle: &str) -> bool {
    render(app, 120, 35).contains(needle)
}

// ---------------------------------------------------------------------------
// 7. Edit round trip: save reopens the inspector for the same entity.
// ---------------------------------------------------------------------------
#[test]
fn quick_edit_from_the_inspector_reopens_it_with_the_new_title() {
    let mut app = project_app("m30-edit-round-trip");
    app.selected = 1;
    let task_id = app.selected_item().unwrap().id.clone();
    let selected = app.selected;
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.modal, Some(Modal::Inspector(_))));

    press(&mut app, KeyCode::Char('E'));
    assert!(
        matches!(app.modal, Some(Modal::Form(_))),
        "quick edit form expected: {}",
        app.status.text
    );
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("quick edit form");
    };
    form.fields[0].value = "Renamed in the inspector".into();
    form.fields[0].cursor = form.fields[0].value.chars().count();
    form.dirty = true;
    app.submit_form(form);

    let state = inspector(&app);
    assert_eq!(state.detail.title, "Renamed in the inspector");
    assert!(!state.unavailable);
    assert_eq!(app.selected, selected, "workspace selection changed");
    assert_eq!(app.current_selected_id().as_deref(), Some(task_id.as_str()));
    let rendered = render(&mut app, 120, 35);
    assert!(
        rendered.contains("Inspector · Renamed in the inspector"),
        "{rendered}"
    );
}

#[test]
fn full_edit_from_the_inspector_reopens_it_with_the_saved_detail() {
    let mut app = project_app("m30-full-edit");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('e'));
    let Some(Modal::Form(mut form)) = app.modal.take() else {
        panic!("edit form: {}", app.status.text);
    };
    assert!(matches!(form.mode, FormMode::EditTask { .. }));
    let description = form
        .fields
        .iter()
        .position(|field| field.key == "description")
        .expect("description field");
    form.fields[description].value = "Edited from the inspector".into();
    form.dirty = true;
    app.submit_form(form);

    let state = inspector(&app);
    assert!(
        state
            .detail
            .fields
            .iter()
            .any(|(key, value)| key == "Description" && value == "Edited from the inspector"),
        "the inspector must reopen with the saved detail: {:?}",
        state.detail.fields
    );
    assert_eq!(app.current_selected_id().as_deref(), Some(task_id.as_str()));

    // Cancelling an edit launched from the inspector also returns to it.
    press(&mut app, KeyCode::Esc);
    assert!(app.modal.is_none());
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    press(&mut app, KeyCode::Esc);
    assert!(
        matches!(app.modal, Some(Modal::Inspector(_))),
        "a cancelled edit returns to the inspector: {}",
        app.status.text
    );
}

#[test]
fn status_change_launched_from_the_inspector_returns_to_it() {
    let mut app = project_app("m30-status-round-trip");
    app.selected = 0;
    let task_id = app.selected_item().unwrap().id.clone();
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('a'));
    let Some(Modal::ActionMenu {
        actions, selected, ..
    }) = &mut app.modal
    else {
        panic!("action menu: {}", app.status.text);
    };
    *selected = actions
        .iter()
        .position(|action| action.id == "set_task_status" && action.enabled)
        .expect("enabled status action");
    let status = actions[*selected].payload.clone().unwrap();
    press(&mut app, KeyCode::Enter);
    if matches!(app.modal, Some(Modal::Confirm { .. })) {
        press(&mut app, KeyCode::Char('y'));
    }
    let state = inspector(&app);
    assert_eq!(
        state.detail.status, status,
        "the inspector must be rebuilt after the status change"
    );
    assert_eq!(app.current_selected_id().as_deref(), Some(task_id.as_str()));
}

// ---------------------------------------------------------------------------
// 8. Render purity: zero reads and zero rebuilds per frame.
// ---------------------------------------------------------------------------
#[test]
fn render_performs_no_reads_and_no_rebuilds() {
    let mut app = project_app("m30-render-purity");
    app.selected = 0;
    crate::backend::diagnostics::reset_detail_reads();
    crate::backend::diagnostics::reset_pending_count_reads();
    let builds = app.inspector_builds.get();
    for _ in 0..4 {
        render(&mut app, 120, 35);
    }
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "rendering must not read task detail, criteria or attachments"
    );
    assert_eq!(
        crate::backend::diagnostics::pending_count_reads(),
        0,
        "rendering must not re-read the pending count"
    );
    assert_eq!(
        app.inspector_builds.get(),
        builds,
        "rendering must not rebuild the inspector model"
    );

    crate::backend::diagnostics::reset_detail_reads();
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.inspector_builds.get(),
        builds + 1,
        "opening the inspector builds exactly one model"
    );
    let opened = crate::backend::diagnostics::detail_reads();
    assert!(opened > 0, "opening is an explicit request and may read");
    crate::backend::diagnostics::reset_detail_reads();
    for _ in 0..4 {
        render(&mut app, 120, 35);
    }
    assert_eq!(
        crate::backend::diagnostics::detail_reads(),
        0,
        "painting an open inspector stays read-free"
    );
    assert_eq!(
        app.inspector_builds.get(),
        builds + 1,
        "painting an open inspector never rebuilds it"
    );
}

// ---------------------------------------------------------------------------
// 9. Session restore tolerates the removed pane focus.
// ---------------------------------------------------------------------------
#[test]
fn session_restore_maps_a_saved_detail_focus_to_the_workspace() {
    // The legacy name used to mean the docked pane; it now resolves to the
    // workspace, so an old session file restores instead of failing.
    assert_eq!(
        crate::ui_state::focus_from_name("detail"),
        Some(FocusPane::Workspace)
    );
    assert_eq!(
        crate::ui_state::focus_from_name("sidebar"),
        Some(FocusPane::Sidebar)
    );

    let mut app = base_app("m30-state-restore");
    app.focus = FocusPane::Sidebar;
    app.restore_selection(None, Some(0), Some(0), None, Some(0));
    assert_eq!(app.selected, 0);
}
