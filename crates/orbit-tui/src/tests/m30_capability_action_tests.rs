//! M30 workstream C: trusted capability actions through the project-centric TUI.
//!
//! These tests pin the shell contract: offers are resolved once per selection
//! change (never while painting), unavailable offers stay visible with their
//! reason, the form is built from the trusted input schema, a request is
//! durable `pending` intent only, and execution/reconciliation are separate
//! explicit confirmations.
use crate::app::{
    App, CapabilityActionView, FocusPane, FormField, FormFieldKind, FormMode, FormState,
    InspectorState, Modal, PendingAction, Screen, StatusKind,
};
use crate::backend::{CONNECTED_CAPABILITY_REFUSED, diagnostics};
use crate::*;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use std::collections::BTreeMap;

const CAPABILITY: &str = "orbit.reference.work-item-note";
const MANIFEST: &str = include_str!("../../../../blueprints/examples/capability-reference.json");

fn base_app(stem: &str) -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m30-cap-{stem}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
}

/// Install -> publish -> bind -> create work item, without authorizing.
fn unauthorized_app(stem: &str) -> (App, String, String) {
    let mut app = base_app(stem);
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Capability UI Project".into(),
        })
        .unwrap();
    app.client.install_trusted_capability(CAPABILITY).unwrap();
    app.client.publish_blueprint_manifest(MANIFEST).unwrap();
    app.client
        .apply_workflow(
            project.id.clone(),
            "capability-reference".to_owned(),
            Some(1),
        )
        .unwrap();
    let item = app
        .client
        .create_blueprint_work_item(&project.id, "note-task", BTreeMap::new())
        .unwrap();
    app.refresh_workspace();
    app.open_project(&project.id);
    select_item(&mut app, &item.id);
    (app, project.id, item.id)
}

fn authorized_app(stem: &str) -> (App, String, String) {
    let (mut app, project_id, item_id) = unauthorized_app(stem);
    app.client
        .authorize_trusted_capability(CAPABILITY, &project_id)
        .unwrap();
    app.refresh_capability_actions();
    (app, project_id, item_id)
}

fn select_item(app: &mut App, item_id: &str) {
    let view = app
        .current_view()
        .unwrap_or_else(|| panic!("no view for {item_id}"));
    let mut found = None;
    for (component_index, component) in view.components.iter().enumerate() {
        if let Some(row) = component
            .selection
            .iter()
            .position(|item| item.id == item_id)
        {
            found = Some((component_index, row));
            break;
        }
    }
    let (component, row) = found.unwrap_or_else(|| panic!("item {item_id} not in the active view"));
    app.active_component = component;
    app.selected = row;
    app.focus = FocusPane::Workspace;
    app.refresh_capability_actions();
}

fn capability_action_id() -> String {
    format!("capability:{CAPABILITY}:append_note")
}

fn offers<'a>(app: &'a App, item_id: &str) -> Vec<&'a CapabilityActionView> {
    app.capability_offers
        .iter()
        .filter(|view| view.item_id == item_id)
        .collect()
}

fn action(app: &App, id: &str) -> orbit_ui::AvailableAction {
    app.available_actions()
        .into_iter()
        .find(|action| action.id == id)
        .unwrap_or_else(|| panic!("action {id} missing"))
}

fn render(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, app)).unwrap();
    let buffer: Buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inspector(app: &App) -> &InspectorState {
    match &app.modal {
        Some(Modal::Inspector(state)) => state,
        other => panic!("inspector modal expected, found {other:?}"),
    }
}

/// The inspector overlay has a bounded height; scroll its own offset so the
/// capability block (below the action list) is painted in the buffer.
fn scroll_inspector(app: &mut App, offset: usize) {
    match &mut app.modal {
        Some(Modal::Inspector(state)) => state.scroll = offset,
        other => panic!("inspector modal expected, found {other:?}"),
    }
}

fn form(app: &App) -> &FormState {
    match &app.modal {
        Some(Modal::Form(form)) => form,
        other => panic!("form expected, found {other:?}"),
    }
}

fn take_form(app: &mut App) -> FormState {
    match app.modal.take() {
        Some(Modal::Form(form)) => form,
        other => panic!("form expected, found {other:?}"),
    }
}

fn field<'a>(form: &'a FormState, key: &str) -> &'a FormField {
    form.fields
        .iter()
        .find(|field| field.key == key)
        .unwrap_or_else(|| panic!("field {key} missing: {:?}", form.fields))
}

fn press(app: &mut App, code: KeyCode) {
    handle_key(app, code, KeyModifiers::NONE).unwrap();
}

// ---------------------------------------------------------------------------
// 1. Availability: visible while unauthorized, enabled after a grant.
// ---------------------------------------------------------------------------
#[test]
fn unauthorized_offer_is_listed_with_its_reason_not_hidden() {
    let (mut app, project_id, item_id) = unauthorized_app("m30-cap-availability");
    let offered = offers(&app, &item_id);
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(
        offered[0].offer.availability.code(),
        Some("not_authorized"),
        "{:?}",
        offered[0].offer.availability
    );

    let id = capability_action_id();
    let disabled = action(&app, &id);
    assert!(!disabled.enabled, "an unauthorized offer is not runnable");
    assert_eq!(disabled.payload.as_deref(), Some(item_id.as_str()));
    assert!(
        disabled
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("not_authorized")),
        "{:?}",
        disabled.disabled_reason
    );
    assert!(
        disabled.description.contains(CAPABILITY)
            && disabled.description.contains("open → closed")
            && disabled.description.contains("not_authorized"),
        "{}",
        disabled.description
    );

    // The palette mirrors the menu entry, disabled with the same reason.
    let entry = app
        .palette_entries("")
        .into_iter()
        .find(|entry| entry.label == disabled.label)
        .unwrap_or_else(|| panic!("palette entry missing for {}", disabled.label));
    assert!(!entry.enabled);
    assert!(
        entry.description.contains("not_authorized"),
        "{}",
        entry.description
    );
    assert!(matches!(entry.action, app::PaletteAction::Execute(_)));

    // Executing a disabled offer refuses and explains; it never opens a form.
    let before = app.capability_history.len();
    app.execute_action(disabled.clone());
    assert!(app.modal.is_none());
    assert!(app.status.kind == StatusKind::Warning);
    assert!(app.status.text.contains("not_authorized") || app.status.text.contains("unavailable"));
    assert_eq!(app.capability_history.len(), before);

    // Authorization is durable state; the next resolution enables the offer.
    app.client
        .authorize_trusted_capability(CAPABILITY, &project_id)
        .unwrap();
    app.refresh_capability_actions();
    let enabled = action(&app, &id);
    assert!(enabled.enabled, "{}", app.status.text);
    assert!(enabled.disabled_reason.is_none());
    assert!(
        !enabled.destructive,
        "the reference action has no external effect"
    );
    assert!(enabled.description.contains("available"));
    let entry = app
        .palette_entries("")
        .into_iter()
        .find(|entry| entry.label == enabled.label)
        .unwrap();
    assert!(entry.enabled);
}

// ---------------------------------------------------------------------------
// 2. Inspector: the action and its availability wording are painted.
// ---------------------------------------------------------------------------
#[test]
fn inspector_lists_the_capability_action_and_availability() {
    let (mut app, _, _item_id) = unauthorized_app("m30-cap-inspector");
    app.open_inspector();
    assert!(matches!(app.modal, Some(Modal::Inspector(_))));

    let state = inspector(&app);
    assert!(
        state
            .detail
            .actions
            .iter()
            .any(|action| action.id == capability_action_id()),
        "capability action must flow through DetailModel.actions: {:?}",
        state.detail.actions
    );
    let rendered = {
        scroll_inspector(&mut app, 3);
        render(&mut app, 120, 40)
    };
    assert!(rendered.contains("CAPABILITY ACTIONS"), "{rendered}");
    assert!(
        rendered.contains("Action Append a durable note"),
        "{rendered}"
    );
    assert!(rendered.contains("not_authorized"), "{rendered}");
    assert!(
        rendered.contains("open → closed"),
        "the transition must be named: {rendered}"
    );

    // An unavailable offer is not advertised as runnable through the menu.
    press(&mut app, KeyCode::Char('a'));
    let Some(Modal::ActionMenu { actions, .. }) = &app.modal else {
        panic!("action menu expected: {}", app.status.text);
    };
    let row = actions
        .iter()
        .find(|action| action.id == capability_action_id())
        .expect("capability action in the menu");
    assert!(!row.enabled);
}

#[test]
fn inspector_history_marks_uncertain_as_requiring_reconciliation() {
    let (mut app, _, item_id) = authorized_app("m30-cap-history");
    // Simulate the durable history the authority would return for an item whose
    // outcome is uncertain. The TUI must not render it as completed.
    app.open_inspector();
    // Inject the durable history after the open resolved live offers: the
    // renderer reads this cache directly and must label it honestly.
    app.capability_history = vec![orbit_client::OrbitCapabilityAction {
        command_id: "cmd-uncertain".into(),
        item_id: item_id.clone(),
        item_revision: 0,
        action_id: "append_note".into(),
        capability: CAPABILITY.into(),
        state: "uncertain".into(),
    }];
    scroll_inspector(&mut app, 6);
    let rendered = render(&mut app, 120, 40);
    assert!(rendered.contains("RECENT CAPABILITY ACTIONS"), "{rendered}");
    assert!(rendered.contains("uncertain"), "{rendered}");
    assert!(
        rendered.contains("requires reconciliation (never completed)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("orbit blueprint action reconcile cmd-uncertain"),
        "{rendered}"
    );
}

// ---------------------------------------------------------------------------
// 3. Form: fields come from the trusted schema, with declared defaults.
// ---------------------------------------------------------------------------
#[test]
fn form_fields_match_the_capability_schema() {
    let (mut app, _, _item_id) = authorized_app("m30-cap-schema");
    let action = action(&app, &capability_action_id());
    app.execute_action(action);
    let form = form(&app).clone();
    assert!(
        matches!(form.mode, FormMode::CapabilityAction { revision: 0, .. }),
        "{:?}",
        form.mode
    );
    assert!(
        form.title.contains("Append a durable note"),
        "{}",
        form.title
    );
    assert!(
        form.hint.contains(CAPABILITY) && form.hint.contains("open → closed"),
        "{}",
        form.hint
    );

    let note = field(&form, "note");
    assert!(matches!(note.kind, FormFieldKind::Text));
    assert!(note.required, "note is in the schema's required list");
    assert_eq!(note.label, "Note body (1..2000 characters)");
    assert_eq!(note.value, "", "declared_parameters supplies the default");

    let tag = field(&form, "tag");
    assert!(matches!(tag.kind, FormFieldKind::Text));
    assert!(!tag.required);
    assert_eq!(tag.label, "Optional label for the note");
    assert!(field(&form, "note").hint.contains("at least 1"));
    assert_eq!(form.fields.len(), 2);

    // The stable command id is minted once in the form state.
    let FormMode::CapabilityAction { command_id, .. } = &form.mode else {
        unreachable!()
    };
    assert!(!command_id.is_empty());
}

// ---------------------------------------------------------------------------
// 4. Request is pending intent only; confirmation executes exactly once.
// ---------------------------------------------------------------------------
#[test]
fn request_is_pending_and_confirmation_executes_once() {
    let (mut app, _, item_id) = authorized_app("m30-cap-request");
    let action = action(&app, &capability_action_id());
    app.execute_action(action);
    let mut form = take_form(&mut app);
    let FormMode::CapabilityAction { command_id, .. } = form.mode.clone() else {
        unreachable!()
    };

    // An empty required input never reaches the backend.
    form.fields
        .iter_mut()
        .find(|field| field.key == "note")
        .unwrap()
        .value
        .clear();
    app.submit_form(form.clone());
    assert!(matches!(app.modal, Some(Modal::Form(_))));
    assert!(
        form_error(&app).contains("required"),
        "{:?}",
        form_error(&app)
    );
    assert!(
        app.capability_history.is_empty(),
        "no request may be created"
    );
    assert_eq!(
        app.client.reference_work_item_note_count(&item_id).unwrap(),
        0
    );

    // A valid submission records one pending request and does not execute.
    form.fields
        .iter_mut()
        .find(|field| field.key == "note")
        .unwrap()
        .value = "hello from the TUI".into();
    form.dirty = true;
    app.submit_form(form);
    let Some(Modal::Confirm { body, pending, .. }) = &app.modal else {
        panic!("confirmation expected: {}", app.status.text);
    };
    assert!(body.contains("Request recorded (pending"), "{body}");
    assert!(body.contains(&command_id), "{body}");
    assert!(body.contains("orbit blueprint action execute"), "{body}");
    assert!(matches!(
        pending,
        PendingAction::ExecuteCapabilityAction { command_id: id, .. } if id == &command_id
    ));

    assert_eq!(app.capability_history.len(), 1);
    assert_eq!(app.capability_history[0].state, "pending");
    let state = app
        .client
        .capability_action_state(&command_id)
        .unwrap()
        .expect("durable request");
    assert!(state.receipt.is_none(), "a pending request has no receipt");
    assert_eq!(
        app.client.reference_work_item_note_count(&item_id).unwrap(),
        0,
        "nothing may execute before the explicit confirmation"
    );

    // Enter on the confirmation runs the trusted executor exactly once.
    press(&mut app, KeyCode::Enter);
    let Some(Modal::CapabilityResult {
        title,
        lines,
        reconcile_command,
        ..
    }) = &app.modal
    else {
        panic!("result modal expected: {}", app.status.text);
    };
    assert!(title.contains("succeeded"), "{title}");
    assert!(
        lines.iter().any(|line| line.contains("outcome: succeeded")),
        "{lines:?}"
    );
    assert!(
        reconcile_command.is_none(),
        "succeeded needs no reconciliation"
    );
    assert_eq!(app.capability_history[0].state, "succeeded");
    assert_eq!(
        app.client.reference_work_item_note_count(&item_id).unwrap(),
        1
    );

    // Replaying the command id returns the receipt and never appends a second
    // note: execution is not a retry loop.
    let replay = app.client.execute_capability_action(&command_id).unwrap();
    assert_eq!(replay.outcome, "succeeded");
    assert_eq!(
        app.client.reference_work_item_note_count(&item_id).unwrap(),
        1
    );

    // The inspector reflects the durable result.
    press(&mut app, KeyCode::Enter);
    assert!(app.modal.is_none());
    app.open_inspector();
    scroll_inspector(&mut app, 6);
    let rendered = render(&mut app, 120, 40);
    assert!(rendered.contains("RECENT CAPABILITY ACTIONS"), "{rendered}");
    assert!(rendered.contains("succeeded"), "{rendered}");
    assert!(rendered.contains(&command_id), "{rendered}");
}

fn form_error(app: &App) -> String {
    match &app.modal {
        Some(Modal::Form(form)) => form.error.clone().unwrap_or_default(),
        other => panic!("form expected, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 5. Uncertain outcomes are never presented as completed.
// ---------------------------------------------------------------------------
#[test]
fn uncertain_result_offers_an_explicit_reconciliation_confirmation() {
    let receipt = orbit_client::OrbitCapabilityReceipt {
        command_id: "cmd-uncertain".into(),
        item_id: "item-1".into(),
        capability: CAPABILITY.into(),
        action_id: "append_note".into(),
        provider: "orbit".into(),
        outcome: "uncertain".into(),
        detail: serde_json::json!({"state": "uncertain"}),
        recorded_at_ms: 1,
    };
    let lines = App::capability_result_lines(&receipt);
    assert!(
        lines.iter().any(|line| line.contains("Outcome uncertain")),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("not completed")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("outcome: succeeded")),
        "{lines:?}"
    );

    let mut app = base_app("m30-cap-uncertain");
    app.show_capability_result(receipt);
    let rendered = render(&mut app, 120, 40);
    assert!(
        rendered.contains("Capability action · uncertain"),
        "{rendered}"
    );
    assert!(
        rendered.contains("Reconcile as succeeded (--succeed)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("Reconcile as failed (--fail)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("Esc leaves it uncertain (not completed)"),
        "{rendered}"
    );

    // The chosen decision goes through a second explicit confirmation.
    press(&mut app, KeyCode::Enter);
    let Some(Modal::Confirm { pending, body, .. }) = &app.modal else {
        panic!("reconcile confirmation expected: {}", app.status.text);
    };
    assert!(
        matches!(
            pending,
            PendingAction::ReconcileCapabilityAction {
                command_id,
                succeed: true
            } if command_id == "cmd-uncertain"
        ),
        "{pending:?}"
    );
    assert!(body.contains("does not re-run"), "{body}");
    press(&mut app, KeyCode::Esc);
    assert!(app.modal.is_none());
}

// ---------------------------------------------------------------------------
// 6. Connected mode refuses honestly and queues nothing.
// ---------------------------------------------------------------------------
#[test]
fn connected_mode_refuses_capability_actions_without_queueing() {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m30-cap-connected-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let backend =
        crate::backend::Backend::open_connected_with_identity(&path, "https://example.org", None)
            .unwrap();
    let mut app = App::open_with_backend(backend).unwrap();
    assert!(app.client.is_connected());
    app.refresh_capability_actions();
    assert!(app.capability_offers.is_empty());
    assert!(app.capability_history.is_empty());

    let error = app.client.capability_action_offers("item-1").unwrap_err();
    assert!(matches!(
        error.code,
        orbit_client::ClientErrorCode::Unavailable
    ));
    assert_eq!(error.message, CONNECTED_CAPABILITY_REFUSED);
    assert!(error.message.contains("M25 EC-004"));
    for error in [
        app.client.capability_status(None).unwrap_err(),
        app.client.capability_action_state("cmd-1").unwrap_err(),
        app.client
            .capability_action_history("item-1", 10)
            .unwrap_err(),
    ] {
        assert_eq!(error.message, CONNECTED_CAPABILITY_REFUSED);
    }
    let request = app
        .client
        .request_capability_action(orbit_client::OrbitCapabilityRequest {
            item_id: "item-1".into(),
            expected_revision: 0,
            action_id: "append_note".into(),
            command_id: "cmd-1".into(),
            inputs: BTreeMap::new(),
        })
        .unwrap_err();
    assert_eq!(request.message, CONNECTED_CAPABILITY_REFUSED);
    assert_eq!(app.client.pending_count(), 0, "no outbox write");
    assert!(
        app.available_actions()
            .iter()
            .all(|action| { !action.id.starts_with("capability:") })
    );
    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// 7. Render purity: cached offers, no backend reads, no model rebuilds.
// ---------------------------------------------------------------------------
#[test]
fn render_reads_no_capability_state_and_rebuilds_nothing() {
    let (mut app, _, _) = authorized_app("m30-cap-render-purity");
    app.open_inspector();
    assert!(matches!(app.modal, Some(Modal::Inspector(_))));
    let builds = app.inspector_builds.get();
    diagnostics::reset_capability_reads();
    diagnostics::reset_detail_reads();
    diagnostics::reset_pending_count_reads();
    for _ in 0..5 {
        render(&mut app, 120, 40);
    }
    assert_eq!(
        diagnostics::capability_reads(),
        0,
        "painting must read the cached offers/history only"
    );
    assert_eq!(diagnostics::detail_reads(), 0);
    assert_eq!(diagnostics::pending_count_reads(), 0);
    assert_eq!(
        app.inspector_builds.get(),
        builds,
        "painting must not rebuild the inspector model"
    );

    // A selection change is an input path and does resolve offers again.
    diagnostics::reset_capability_reads();
    app.move_selection(1);
    assert!(
        diagnostics::capability_reads() > 0,
        "selection changes refresh the cache"
    );
}

#[test]
fn connected_capability_actions_are_never_rendered() {
    // Guard the closed allowlist even if a stale cache existed.
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m30-cap-connected-render-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let backend =
        crate::backend::Backend::open_connected_with_identity(&path, "https://example.org", None)
            .unwrap();
    let mut app = App::open_with_backend(backend).unwrap();
    app.screen = Screen::Home;
    let rendered = render(&mut app, 120, 30);
    assert!(!rendered.contains("CAPABILITY ACTIONS"), "{rendered}");
    let _ = std::fs::remove_file(&path);
}
