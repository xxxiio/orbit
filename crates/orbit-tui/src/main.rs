//! Orbit terminal client. One project-centric shell serves local and connected
//! deployments; blueprint views resolve through shared `orbit-ui` contracts.
mod app;
mod backend;
#[allow(dead_code)]
mod connected;
#[allow(dead_code)]
mod connected_actions;
mod planning_ui;
mod render;
mod terminal;
mod theme;
mod ui_state;
mod viewer;

use app::{App, FormFieldKind, Modal, Screen, StatusKind};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;

fn main() {
    if let Err(error) = run() {
        eprintln!("orbit-tui: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let deployment = orbit_client::deployment::from_env().map_err(|error| error.to_string())?;
    if let orbit_client::deployment::Deployment::Connected { replica, server } = &deployment
        && std::env::var("ORBIT_TUI_LEGACY_CONNECTED").ok().as_deref() == Some("1")
    {
        return connected::run(replica.to_string_lossy().into_owned(), server.to_string());
    }
    let mut app = App::open_deployment(deployment)?;
    ui_state::restore(&mut app);
    let mut session = terminal::TerminalSession::enter()?;
    let result = event_loop(session.terminal(), &mut app);
    let _ = ui_state::save(&app);
    session.restore();
    result
}

/// Compact navigation signature used to avoid writing session state on every
/// keypress while still persisting the moment navigation actually changes.
fn navigation_signature(app: &App) -> String {
    let project = app.project.as_ref();
    format!(
        "{:?}|{}|{}|{:?}|{}|{}|{}|{}|{}|{}|{}",
        app.screen,
        project.map(|project| project.id.as_str()).unwrap_or(""),
        project
            .and_then(|project| project.views.get(project.active_view))
            .map(|view| view.id.as_str())
            .unwrap_or(""),
        app.focus,
        app.sidebar_selection,
        app.projects_selection,
        app.active_component,
        app.selected,
        app.selected_item()
            .map(|item| item.id.as_str())
            .unwrap_or(""),
        app.scroll,
        app.detail_scroll,
    )
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    let mut last_signature = navigation_signature(app);
    loop {
        terminal
            .draw(|frame| render::draw(frame, app))
            .map_err(err)?;
        if !event::poll(Duration::from_millis(200)).map_err(err)? {
            continue;
        }
        let Event::Key(key) = event::read().map_err(err)? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q'))
        {
            if app.request_quit() {
                break;
            }
            continue;
        }
        if handle_key(app, key.code, key.modifiers)? {
            break;
        }
        let signature = navigation_signature(app);
        if signature != last_signature {
            last_signature = signature;
            if let Err(error) = ui_state::save(app) {
                app.notify(
                    format!("Session state not saved: {error}"),
                    StatusKind::Warning,
                );
            }
        }
    }
    Ok(())
}

/// Byte offset for a character index, clamped to the value length.
fn byte_offset(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(offset, _)| offset)
        .unwrap_or(value.len())
}

/// Insert a character at the caret.
fn text_insert(field: &mut app::FormField, character: char) {
    let offset = byte_offset(&field.value, field.cursor);
    field.value.insert(offset, character);
    field.cursor += 1;
}

/// Delete the character before the caret.
fn text_backspace(field: &mut app::FormField) {
    if field.cursor == 0 {
        return;
    }
    let end = byte_offset(&field.value, field.cursor);
    let start = byte_offset(&field.value, field.cursor - 1);
    field.value.replace_range(start..end, "");
    field.cursor -= 1;
}

/// Delete the character at the caret.
fn text_delete(field: &mut app::FormField) {
    let len = field.value.chars().count();
    if field.cursor >= len {
        return;
    }
    let start = byte_offset(&field.value, field.cursor);
    let end = byte_offset(&field.value, field.cursor + 1);
    field.value.replace_range(start..end, "");
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Result<bool, String> {
    if app.modal.is_none() {
        app.modal_scroll = 0;
    }
    if app.modal.is_some() {
        return handle_modal(app, code, modifiers);
    }
    if let Some(prefix) = app.prefix.take() {
        if prefix == 'g' {
            match code {
                KeyCode::Char('h') => {
                    app.go_screen(Screen::Home);
                    return Ok(false);
                }
                KeyCode::Char('i') => {
                    app.go_screen(Screen::Inbox);
                    return Ok(false);
                }
                KeyCode::Char('p') => {
                    app.go_screen(Screen::Projects);
                    return Ok(false);
                }
                KeyCode::Char('a') => {
                    app.go_screen(Screen::Agents);
                    return Ok(false);
                }
                _ => {}
            }
        }
        app.notify("Prefix cancelled", StatusKind::Info);
        return Ok(false);
    }
    if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('p')) {
        app.open_palette();
        return Ok(false);
    }
    match code {
        KeyCode::Char('g') => {
            app.prefix = Some('g');
            app.notify(
                "g… h Home · i Inbox · p Projects · a Agents",
                StatusKind::Info,
            );
        }
        KeyCode::Char('q') => {
            if app.request_quit() {
                return Ok(true);
            }
        }
        KeyCode::Esc => app.go_back(),
        KeyCode::Char('?') => app.modal = Some(Modal::Help),
        KeyCode::Char(':') => app.open_palette(),
        KeyCode::Char('/') => app.open_search(),
        KeyCode::Char('r') => app.refresh_current(),
        KeyCode::Char('f') if app.client.is_connected() => {
            let action =
                orbit_ui::AvailableAction::enabled("flush_pending", "Flush pending commands", None);
            app.execute_action(action);
        }
        KeyCode::Char('D') => {
            if app.viewport_width < theme::WIDE_BREAKPOINT {
                app.open_detail();
            } else {
                app.show_detail = !app.show_detail;
            }
        }
        KeyCode::Tab => app.focus_next(false),
        KeyCode::BackTab => app.focus_next(true),
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Right | KeyCode::Char('l') if app.focus == app::FocusPane::Workspace => {
            app.move_component(1)
        }
        KeyCode::Left | KeyCode::Char('h') if app.focus == app::FocusPane::Workspace => {
            app.move_component(-1)
        }
        KeyCode::Enter => match app.screen {
            Screen::Projects => app.activate_project_selection(),
            _ if app.focus == app::FocusPane::Sidebar => app.activate_sidebar(),
            _ => app.open_detail(),
        },
        KeyCode::Char('[') => match app.screen {
            Screen::Home => app.adjust_planning_horizon(false),
            Screen::Project => app.switch_view(-1),
            _ => app.move_component(-1),
        },
        KeyCode::Char(']') => match app.screen {
            Screen::Home => app.adjust_planning_horizon(true),
            Screen::Project => app.switch_view(1),
            _ => app.move_component(1),
        },
        KeyCode::Char('a') | KeyCode::Char('s') => app.open_action_menu(),
        KeyCode::Char('e') => app.edit_selected(),
        KeyCode::Char('K') => {
            if let Some(item) = app.selected_item()
                && item.kind == orbit_ui::model::ItemKind::Task
            {
                let task_id = item.id.clone();
                let title = item.title.clone();
                app.open_criteria(task_id, title);
            } else {
                app.notify(
                    "Select a task to manage acceptance criteria",
                    StatusKind::Warning,
                );
            }
        }
        KeyCode::Char('H') => {
            if let Some(item) = app.selected_item()
                && item.kind == orbit_ui::model::ItemKind::Task
            {
                let task_id = item.id.clone();
                let title = item.title.clone();
                app.open_activity(task_id, title);
            } else {
                app.notify("Select a task to inspect activity", StatusKind::Warning);
            }
        }
        KeyCode::Char('i') => {
            let action = orbit_ui::AvailableAction::enabled("attach_file", "Attach file", None);
            app.execute_action(action);
        }
        KeyCode::Char('F') => {
            let action =
                orbit_ui::AvailableAction::enabled("open_attachments", "Attachments", None);
            app.execute_action(action);
        }
        KeyCode::Char('W') => {
            if app.client.is_connected() {
                app.notify(
                    "Changing a workflow requires the local authoritative workspace.",
                    StatusKind::Warning,
                );
                return Ok(false);
            }
            let action =
                orbit_ui::AvailableAction::enabled("change_workflow", "Change workflow", None);
            app.execute_action(action);
        }
        KeyCode::Char('c') => {
            if app.screen == Screen::Projects {
                app.start_form(app::FormMode::NewProject);
            } else {
                app.start_form(app::FormMode::NewTask {
                    project_id: app
                        .project
                        .as_ref()
                        .filter(|_| app.screen == Screen::Project)
                        .map(|project| project.id.clone()),
                });
            }
        }
        KeyCode::Char('C') => {
            if let Some(project) = &app.project {
                app.start_form(app::FormMode::NewWorkItemKind {
                    project_id: project.id.clone(),
                });
            } else {
                app.notify(
                    "Open a project before creating a work item",
                    StatusKind::Warning,
                );
            }
        }
        KeyCode::Char('n') => app.start_form(app::FormMode::NewKnowledge),
        KeyCode::Char('p') if app.screen == Screen::Project => app.preview_plan(),
        KeyCode::Char('P') if app.screen == Screen::Project => {
            if app.pending_plan.is_some() {
                app.modal = Some(Modal::Confirm {
                    title: "Apply plan".into(),
                    body: "Apply the previewed schedule after authoritative revalidation?".into(),
                    pending: app::PendingAction::ApplyPlan,
                });
            } else {
                app.notify("No pending plan; press p first", StatusKind::Warning);
            }
        }
        KeyCode::Char('d') if matches!(app.screen, Screen::Home | Screen::Inbox) => {
            app.completion_impact()
        }
        KeyCode::Char('v') if matches!(app.screen, Screen::Home | Screen::Inbox) => {
            app.today_focus()
        }
        KeyCode::Char('A') if matches!(app.screen, Screen::Home | Screen::Inbox) => {
            app.today_actions()
        }
        KeyCode::Char('G') if matches!(app.screen, Screen::Home | Screen::Inbox) => {
            app.today_project_focus()
        }
        KeyCode::Char('b') => {
            if let Some(project) = &app.project {
                app.start_form(app::FormMode::BindEvidence {
                    project_id: project.id.clone(),
                    unbind: false,
                });
            } else {
                app.notify("Open a project first", StatusKind::Warning);
            }
        }
        KeyCode::Char('o') => {
            let metric_id = app
                .selected_item()
                .filter(|item| item.kind == orbit_ui::ItemKind::Metric)
                .map(|item| item.id.clone());
            app.start_form(app::FormMode::MetricObservation { metric_id });
        }
        KeyCode::Char('t') => {
            let metric_id = app
                .selected_item()
                .filter(|item| item.kind == orbit_ui::ItemKind::Metric)
                .map(|item| item.id.clone());
            app.start_form(app::FormMode::MetricTarget { metric_id });
        }
        KeyCode::Char(' ') => {
            if let Some(item) = app.selected_item()
                && item.kind == orbit_ui::ItemKind::Habit
            {
                let habit_id = item.id.clone();
                app.execute_pending(app::PendingAction::RecordHabit { habit_id });
            } else {
                app.notify(
                    "Select a habit to record an occurrence",
                    StatusKind::Warning,
                );
            }
        }
        KeyCode::Char(digit) if ('1'..='9').contains(&digit) && app.screen == Screen::Project => {
            app.switch_view_by_index((digit as u8 - b'1') as usize);
        }
        _ => {}
    }
    Ok(false)
}

fn handle_modal(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Result<bool, String> {
    let Some(mut modal) = app.modal.take() else {
        return Ok(false);
    };
    if matches!(code, KeyCode::Char('q'))
        && !matches!(
            modal,
            Modal::Palette { .. }
                | Modal::Search { .. }
                | Modal::Form(_)
                | Modal::QuitConfirm { .. }
                | Modal::Criteria(_)
                | Modal::Attachments(_)
        )
    {
        if matches!(modal, Modal::Help) {
            app.modal = None;
            return Ok(false);
        }
        return Ok(app.request_quit());
    }
    let mut keep = true;
    match &mut modal {
        Modal::QuitConfirm { resume } => match code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                return Ok(true);
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                keep = false;
                app.modal = resume.take().map(|modal| *modal);
                app.notify("Quit cancelled; input preserved", StatusKind::Info);
            }
            _ => {}
        },
        Modal::Form(form) => {
            // Ctrl+S saves from any field, including multi-line text where
            // Enter inserts a newline instead of submitting.
            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('s')) {
                let form = form.clone();
                app.submit_form(form);
                return Ok(false);
            }
            // Never type control chords into a field: only Ctrl+S is a form
            // command, everything else is ignored so shortcuts cannot corrupt
            // ordinary text entry.
            if modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(false);
            }
            match code {
                KeyCode::Esc => {
                    if form.dirty {
                        let form = form.clone();
                        keep = false;
                        app.modal = Some(Modal::Confirm {
                            title: "Discard unsaved changes?".into(),
                            body: "The form has unsaved edits.".into(),
                            pending: app::PendingAction::DiscardFormChanges {
                                form: Box::new(form),
                            },
                        });
                    } else {
                        keep = false;
                        app.notify("Form cancelled", StatusKind::Info);
                    }
                }
                KeyCode::Enter => {
                    let multiline = form
                        .fields
                        .get(form.focused)
                        .is_some_and(|field| matches!(field.kind, FormFieldKind::Multiline));
                    if multiline {
                        form.error = None;
                        if let Some(field) = form.fields.get_mut(form.focused) {
                            text_insert(field, '\n');
                        }
                        form.dirty = true;
                    } else {
                        let form = form.clone();
                        keep = false;
                        app.submit_form(form);
                    }
                }
                KeyCode::Tab => {
                    if !form.fields.is_empty() {
                        form.focused = (form.focused + 1) % form.fields.len();
                    }
                }
                KeyCode::BackTab => {
                    if !form.fields.is_empty() {
                        form.focused = (form.focused + form.fields.len() - 1) % form.fields.len();
                    }
                }
                KeyCode::Down => {
                    if !form.fields.is_empty() {
                        form.focused = (form.focused + 1).min(form.fields.len() - 1);
                    }
                }
                KeyCode::Up => {
                    form.focused = form.focused.saturating_sub(1);
                }
                KeyCode::Left | KeyCode::Right => {
                    form.error = None;
                    let delta = if matches!(code, KeyCode::Right) {
                        1
                    } else {
                        -1
                    };
                    if let Some(field) = form.fields.get_mut(form.focused) {
                        match &field.kind {
                            FormFieldKind::Choice(choices) if !choices.is_empty() => {
                                let current = choices
                                    .iter()
                                    .position(|choice| choice.value == field.value)
                                    .unwrap_or(0)
                                    as isize;
                                let next =
                                    (current + delta).clamp(0, choices.len() as isize - 1) as usize;
                                field.value = choices[next].value.clone();
                                form.dirty = true;
                            }
                            FormFieldKind::Boolean => {
                                field.value = if field.value == "true" {
                                    "false".into()
                                } else {
                                    "true".into()
                                };
                                form.dirty = true;
                            }
                            FormFieldKind::Text
                            | FormFieldKind::Multiline
                            | FormFieldKind::Integer => {
                                let len = field.value.chars().count();
                                let next = (field.cursor as isize + delta).clamp(0, len as isize);
                                field.cursor = next as usize;
                            }
                            FormFieldKind::Choice(_) => {}
                        }
                    }
                }
                KeyCode::Home | KeyCode::End => {
                    form.error = None;
                    let end = matches!(code, KeyCode::End);
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        field.cursor = if end { field.value.chars().count() } else { 0 };
                    }
                }
                KeyCode::Delete => {
                    form.error = None;
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        text_delete(field);
                        form.dirty = true;
                    }
                }
                KeyCode::Char(' ') => {
                    form.error = None;
                    if let Some(field) = form.fields.get_mut(form.focused) {
                        match field.kind {
                            FormFieldKind::Boolean => {
                                field.value = if field.value == "true" {
                                    "false".into()
                                } else {
                                    "true".into()
                                };
                                form.dirty = true;
                            }
                            FormFieldKind::Text
                            | FormFieldKind::Multiline
                            | FormFieldKind::Integer => {
                                text_insert(field, ' ');
                                form.dirty = true;
                            }
                            FormFieldKind::Choice(_) => {}
                        }
                    }
                }
                KeyCode::Backspace => {
                    form.error = None;
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        text_backspace(field);
                        form.dirty = true;
                    }
                }
                KeyCode::Char(character) => {
                    form.error = None;
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        text_insert(field, character);
                        form.dirty = true;
                    }
                }
                _ => {}
            }
        }
        Modal::Confirm {
            title,
            body,
            pending,
        } => match code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let pending = pending.clone();
                let title = title.clone();
                keep = false;
                app.notify(format!("Submitting {title}"), StatusKind::Pending);
                app.execute_pending(pending);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                keep = false;
                if let app::PendingAction::DiscardFormChanges { form } = pending.clone() {
                    app.modal = Some(Modal::Form(*form));
                    app.notify("Form restored; unsaved changes preserved", StatusKind::Info);
                } else {
                    app.notify("Cancelled; no change was submitted", StatusKind::Info);
                }
            }
            _ => {
                let _ = body;
            }
        },
        Modal::ActionMenu {
            actions, selected, ..
        } => match code {
            KeyCode::Esc => {
                keep = false;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(actions.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                *selected = selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let action = actions.get(*selected).cloned();
                keep = false;
                if let Some(action) = action {
                    app.execute_action(action);
                }
            }
            _ => {}
        },
        Modal::Palette {
            query,
            entries,
            selected,
        } => match code {
            KeyCode::Esc => keep = false,
            KeyCode::Down => *selected = (*selected + 1).min(entries.len().saturating_sub(1)),
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Backspace => {
                query.pop();
                *entries = app.palette_entries(query);
                *selected = 0;
            }
            KeyCode::Char(character) => {
                query.push(character);
                *entries = app.palette_entries(query);
                *selected = 0;
            }
            KeyCode::Enter => {
                let action = entries.get(*selected).map(|entry| entry.action.clone());
                keep = false;
                if let Some(action) = action {
                    if matches!(action, app::PaletteAction::Quit) {
                        app.modal = Some(Modal::Palette {
                            query: query.clone(),
                            entries: entries.clone(),
                            selected: *selected,
                        });
                        return Ok(app.request_quit());
                    }
                    if app.execute_palette_action(action) {
                        return Ok(true);
                    }
                }
            }
            _ => {}
        },
        Modal::Help => match code {
            KeyCode::Esc | KeyCode::Char('?') => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal_scroll = app.modal_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal_scroll = app.modal_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => app.modal_scroll = app.modal_scroll.saturating_add(10),
            KeyCode::PageUp => app.modal_scroll = app.modal_scroll.saturating_sub(10),
            _ => {}
        },
        Modal::Criteria(state) => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                state.selected = (state.selected + 1).min(state.items.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
            }
            KeyCode::Char(' ') => {
                if let Some(item) = state.items.get(state.selected).cloned() {
                    match app.client.set_acceptance_criterion_completed(
                        state.task_id.clone(),
                        item.id,
                        !item.completed,
                    ) {
                        Ok(items) => {
                            state.items = items;
                            state.selected =
                                state.selected.min(state.items.len().saturating_sub(1));
                            let label = if item.completed {
                                "Acceptance criterion reopened"
                            } else {
                                "Acceptance criterion checked"
                            };
                            let (message, kind) = app.mutation_status(label);
                            app.notify(message, kind);
                        }
                        Err(error) => app.notify(
                            format!("Updating the criterion failed: {}", error.message),
                            StatusKind::Error,
                        ),
                    }
                }
            }
            KeyCode::Char('c') => {
                let task_id = state.task_id.clone();
                keep = false;
                app.start_form(app::FormMode::AddCriterion { task_id });
            }
            KeyCode::Char('r') => {
                if let Some(item) = state.items.get(state.selected) {
                    let task_id = state.task_id.clone();
                    let criterion_id = item.id.clone();
                    keep = false;
                    app.start_form(app::FormMode::EditCriterion {
                        task_id,
                        criterion_id,
                    });
                }
            }
            KeyCode::Char('J') => {
                if let Some(item) = state.items.get(state.selected).cloned() {
                    let position = (state.selected + 1).min(state.items.len().saturating_sub(1));
                    match app.client.move_acceptance_criterion(
                        state.task_id.clone(),
                        item.id,
                        position,
                    ) {
                        Ok(items) => {
                            state.items = items;
                            state.selected = position;
                            let (message, kind) =
                                app.mutation_status("Acceptance criterion reordered");
                            app.notify(message, kind);
                        }
                        Err(error) => app.notify(
                            format!("Reordering the criterion failed: {}", error.message),
                            StatusKind::Error,
                        ),
                    }
                }
            }
            KeyCode::Char('K') => {
                if let Some(item) = state.items.get(state.selected).cloned() {
                    let position = state.selected.saturating_sub(1);
                    match app.client.move_acceptance_criterion(
                        state.task_id.clone(),
                        item.id,
                        position,
                    ) {
                        Ok(items) => {
                            state.items = items;
                            state.selected = position;
                            let (message, kind) =
                                app.mutation_status("Acceptance criterion reordered");
                            app.notify(message, kind);
                        }
                        Err(error) => app.notify(
                            format!("Reordering the criterion failed: {}", error.message),
                            StatusKind::Error,
                        ),
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Some(item) = state.items.get(state.selected) {
                    let task_id = state.task_id.clone();
                    let criterion_id = item.id.clone();
                    let text = item.text.clone();
                    keep = false;
                    app.modal = Some(Modal::Confirm {
                        title: "Remove acceptance criterion".into(),
                        body: format!("Remove '{text}'?"),
                        pending: app::PendingAction::RemoveCriterion {
                            task_id,
                            criterion_id,
                            task_title: state.task_title.clone(),
                        },
                    });
                }
            }
            _ => {}
        },
        Modal::Attachments(state) => match code {
            KeyCode::Esc => keep = false,
            KeyCode::Enter => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    keep = false;
                    app.preview_attachment(attachment_id);
                } else {
                    keep = false;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.selected = (state.selected + 1).min(state.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
            }
            KeyCode::Char('A') => {
                let owner_kind = state.owner_kind.clone();
                let owner_id = state.owner_id.clone();
                keep = false;
                app.start_form(app::FormMode::AttachFile {
                    owner_kind,
                    owner_id,
                });
            }
            KeyCode::Char('o') => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    let filename = item.filename.clone();
                    keep = false;
                    app.modal = Some(Modal::Confirm {
                        title: "Open attachment".into(),
                        body: format!(
                            "Export '{}' and hand it to your configured external viewer? Orbit never executes file content.",
                            item.filename
                        ),
                        pending: app::PendingAction::OpenAttachment {
                            attachment_id,
                            filename,
                        },
                    });
                }
            }
            KeyCode::Char('E') => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    keep = false;
                    app.start_form(app::FormMode::ExportAttachment { attachment_id });
                }
            }
            KeyCode::Char('x') => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    let filename = item.filename.clone();
                    let pending = state.selected_is_pending();
                    keep = false;
                    app.modal = Some(Modal::Confirm {
                        title: if pending {
                            "Discard staged upload".into()
                        } else {
                            "Remove attachment".into()
                        },
                        body: if pending {
                            format!(
                                "Discard the locally staged upload for '{filename}'? Nothing was accepted on the server."
                            )
                        } else {
                            format!(
                                "Remove the association for '{filename}'? This requires the server and deletes content only when nothing else references it."
                            )
                        },
                        pending: app::PendingAction::RemoveAttachment {
                            attachment_id,
                            owner_kind: state.owner_kind.clone(),
                            owner_id: state.owner_id.clone(),
                            title: state.title.clone(),
                        },
                    });
                }
            }
            _ => {}
        },
        Modal::Activity(state) => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => state.scroll = state.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => state.scroll = state.scroll.saturating_sub(1),
            KeyCode::PageDown => state.scroll = state.scroll.saturating_add(10),
            KeyCode::PageUp => state.scroll = state.scroll.saturating_sub(10),
            _ => {}
        },
        Modal::Detail(_) => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal_scroll = app.modal_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal_scroll = app.modal_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => app.modal_scroll = app.modal_scroll.saturating_add(10),
            KeyCode::PageUp => app.modal_scroll = app.modal_scroll.saturating_sub(10),
            _ => {}
        },
        Modal::TodayActions { menu, selected } => match code {
            KeyCode::Esc => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(menu.actions.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Enter => {
                let index = *selected;
                keep = false;
                app.today_select_action(index);
            }
            _ => {}
        },
        Modal::Search {
            query,
            hits,
            selected,
        } => match code {
            KeyCode::Esc => keep = false,
            KeyCode::Down => *selected = (*selected + 1).min(hits.len().saturating_sub(1)),
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Backspace => {
                query.pop();
                *hits = app.search_hits(query);
                *selected = 0;
            }
            KeyCode::Char(character) => {
                query.push(character);
                *hits = app.search_hits(query);
                *selected = 0;
            }
            KeyCode::Enter => {
                let hit = hits.get(*selected).cloned();
                keep = false;
                if let Some(hit) = hit {
                    app.execute_palette_action(hit.action);
                }
            }
            _ => {}
        },
        Modal::Message { .. } => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal_scroll = app.modal_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal_scroll = app.modal_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => app.modal_scroll = app.modal_scroll.saturating_add(10),
            KeyCode::PageUp => app.modal_scroll = app.modal_scroll.saturating_sub(10),
            _ => {}
        },
    }
    if keep {
        app.modal = Some(modal);
    }
    Ok(false)
}

fn err<E: std::fmt::Debug>(error: E) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use app::{FocusPane, FormMode, Modal, PaletteAction, PendingAction};
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-m28-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    fn submit_project(app: &mut App, name: &str) {
        app.start_form(FormMode::NewProject);
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("expected project form");
        };
        form.fields[0].value = name.into();
        form.dirty = true;
        app.submit_form(form);
    }

    fn buffer_text(app: &mut App, width: u16, height: u16) -> String {
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

    #[test]
    fn fresh_workspace_renders_home_without_querying_storage_in_renderer() {
        let mut app = app();
        assert_eq!(app.screen, Screen::Home);
        assert!(!app.home.components.is_empty());
        let text = buffer_text(&mut app, 120, 30);
        assert!(text.contains("ORBIT"), "{text}");
        assert!(text.contains("Today / Next"), "{text}");
        assert!(text.contains("Details"), "{text}");
    }

    #[test]
    fn project_creation_pins_the_basic_workflow_by_default() {
        let mut app = app();
        submit_project(&mut app, "Apollo");
        assert_eq!(app.screen, Screen::Project);
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.name, "Apollo");
        assert!(
            !project.generated,
            "a new project should carry the pinned Basic workflow, not generated defaults"
        );
        assert!(
            project.views.iter().any(|view| view.id == "overview"),
            "Basic declares an overview view"
        );
        assert!(
            project.blueprint_label.to_lowercase().contains("basic"),
            "label was {}",
            project.blueprint_label
        );
        let text = buffer_text(&mut app, 120, 30);
        assert!(text.contains("Basic"), "{text}");
    }

    #[test]
    fn unbound_project_falls_back_to_generated_views() {
        let mut app = app();
        submit_project(&mut app, "Legacy");
        let project_id = app.project.as_ref().unwrap().id.clone();
        // Simulate a legacy workspace: the binding is removed while the project
        // stays fully usable through generated default views.
        app.client
            .remove_project_workflow(project_id.clone())
            .unwrap();
        app.refresh_project();
        let project = app.project.as_ref().unwrap();
        assert!(project.generated);
        assert!(project.views.iter().any(|view| view.id == "overview"));
        let text = buffer_text(&mut app, 120, 30);
        assert!(text.contains("generated default"), "{text}");
    }

    #[test]
    fn research_blueprint_work_item_offers_authorized_transition() {
        let mut app = app();
        submit_project(&mut app, "Research");
        let project_id = app.project.as_ref().unwrap().id.clone();
        app.client.install_bundled_blueprint("research").unwrap();
        app.client
            .apply_workflow(project_id.clone(), "research".to_owned(), None)
            .unwrap();
        app.refresh_project();
        let item = app
            .client
            .create_blueprint_work_item(
                &project_id,
                "hypothesis",
                std::collections::BTreeMap::from([(
                    "question".into(),
                    serde_json::json!("Does M28 resolve?"),
                )]),
            )
            .unwrap();
        app.refresh_project();
        let board = app
            .project
            .as_ref()
            .unwrap()
            .views
            .iter()
            .position(|view| view.id == "workflow")
            .unwrap();
        app.switch_view_by_index(board);
        let component = app.project.as_ref().unwrap().views[board]
            .components
            .iter()
            .position(|component| component.id == "board")
            .unwrap();
        app.active_component = component;
        app.selected = app
            .current_component()
            .unwrap()
            .selection
            .iter()
            .position(|row| row.id == item.id)
            .unwrap();
        let action = app
            .available_actions()
            .into_iter()
            .find(|action| {
                action.id == "blueprint_transition"
                    && action.payload.as_deref() == Some("investigating")
            })
            .expect("transition offer");
        app.execute_action(action);
        let updated = app.client.list_blueprint_work_items(&project_id).unwrap();
        assert_eq!(updated[0].state, "investigating");
    }

    #[test]
    fn narrow_terminals_render_a_single_readable_pane() {
        let mut app = app();
        app.focus = FocusPane::Workspace;
        let text = buffer_text(&mut app, 60, 18);
        assert!(
            text.contains("Today / Next") || text.contains("Home"),
            "{text}"
        );
        assert!(
            !text.contains("Details"),
            "narrow layout should not reserve a detail pane: {text}"
        );
    }

    #[test]
    fn missing_component_keeps_the_rest_of_the_view() {
        let mut app = app();
        app.home.components.insert(
            0,
            orbit_ui::ResolvedComponent {
                id: "unavailable".into(),
                title: "Unavailable".into(),
                component_type: "unknown_extension".into(),
                status: orbit_ui::LoadStatus::Missing("not installed".into()),
                body: orbit_ui::ComponentBody::Missing {
                    component_type: "unknown_extension".into(),
                    reason: "Install the extension that provides it.".into(),
                },
                selection: Vec::new(),
            },
        );
        let text = buffer_text(&mut app, 100, 26);
        assert!(text.contains("Missing component"), "{text}");
        assert!(text.contains("Today / Next"), "{text}");
    }

    #[test]
    fn palette_and_search_filter_without_losing_modal_cancel() {
        let mut app = app();
        app.open_palette();
        assert!(matches!(app.modal, Some(Modal::Palette { .. })));
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert!(app.modal.is_none());
        app.open_search();
        handle_key(&mut app, KeyCode::Char('i'), KeyModifiers::NONE).unwrap();
        assert!(matches!(app.modal, Some(Modal::Search { .. })));
    }

    #[test]
    fn project_selection_enter_opens_project() {
        let mut app = app();
        submit_project(&mut app, "Apollo");
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        handle_key(&mut app, KeyCode::Char('g'), KeyModifiers::NONE).unwrap();
        handle_key(&mut app, KeyCode::Char('p'), KeyModifiers::NONE).unwrap();
        assert_eq!(app.screen, Screen::Projects);
        app.focus = FocusPane::Workspace;
        app.projects_selection = 0;
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(app.screen, Screen::Project);
        assert_eq!(app.project.as_ref().unwrap().name, "Apollo");
    }

    #[test]
    fn duplicate_record_submission_is_ignored_until_refresh() {
        let mut app = app();
        submit_project(&mut app, "Habits");
        let project_id = app.project.as_ref().unwrap().id.clone();
        let habit = app
            .client
            .create_habit(orbit_client::CreateHabitRequest {
                id: None,
                name: "Stretch".into(),
                cadence: "daily".into(),
            })
            .unwrap();
        app.refresh_project();
        let pending = PendingAction::RecordHabit {
            habit_id: habit.id.clone(),
        };
        app.execute_pending(pending.clone());
        app.execute_pending(pending);
        let occurrences = app.client.habit_history(habit.id.clone()).unwrap();
        assert_eq!(occurrences.len(), 1);
        assert!(app.status.text.contains("Duplicate"), "{}", app.status.text);
        let _ = project_id;
    }

    #[test]
    fn focus_cycles_between_three_panes_on_wide_terminals() {
        let mut app = app();
        app.set_viewport(120);
        app.focus = FocusPane::Workspace;
        app.focus_next(false);
        assert_eq!(app.focus, FocusPane::Detail);
        app.focus_next(false);
        assert_eq!(app.focus, FocusPane::Sidebar);
    }

    #[test]
    fn q_quits_from_any_non_text_screen() {
        let mut app = app();
        submit_project(&mut app, "Apollo");
        assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
        assert_eq!(app.screen, Screen::Project);
        let mut home = self::app();
        assert!(handle_key(&mut home, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
    }

    #[test]
    fn ctrl_quit_keeps_unsaved_prompt_until_confirmed() {
        let mut app = app();
        app.start_form(FormMode::NewTask { project_id: None });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = "unsaved capture".into();
        form.dirty = true;
        app.modal = Some(Modal::Form(form));
        assert!(!app.request_quit());
        assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("form restored");
        };
        assert_eq!(form.fields[0].value, "unsaved capture");
    }

    #[test]
    fn palette_actions_share_the_contextual_catalogue() {
        let mut app = app();
        submit_project(&mut app, "Apollo");
        app.open_palette();
        let Some(Modal::Palette { entries, .. }) = &app.modal else {
            panic!("palette");
        };
        assert!(entries.iter().any(|entry| entry.label.contains("New task")));
        assert!(
            entries
                .iter()
                .any(|entry| matches!(entry.action, PaletteAction::Go(Screen::Home)))
        );
    }
}

#[cfg(test)]
mod m28_render_tests {
    use super::*;
    use app::FocusPane;
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-m28-render-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    fn text(app: &mut App, width: u16, height: u16) -> String {
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

    fn project_with_development(app: &mut App) -> String {
        let project = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Apollo".into(),
            })
            .unwrap();
        app.client.install_bundled_blueprint("development").unwrap();
        app.client
            .apply_workflow(project.id.clone(), "development".to_owned(), None)
            .unwrap();
        app.refresh_workspace();
        app.open_project(&project.id);
        project.id
    }

    #[test]
    fn overview_renders_live_metric_cards_and_task_rows() {
        let mut app = app();
        let project_id = project_with_development(&mut app);
        app.client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "Ship the shell".into(),
                project_id: Some(project_id.clone()),
                milestone_id: None,
            })
            .unwrap();
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "Ready work".into(),
                project_id: Some(project_id),
                milestone_id: None,
            })
            .unwrap();
        app.client
            .set_task_status(orbit_client::SetTaskStatusRequest {
                id: task.id,
                status: "ready".into(),
            })
            .unwrap();
        app.refresh_project();
        let rendered = text(&mut app, 140, 40);
        assert!(rendered.contains("Ready tasks"), "{rendered}");
        assert!(rendered.contains("Ready work"), "{rendered}");
        assert!(rendered.contains("Ship the shell"), "{rendered}");
    }

    #[test]
    fn board_renders_workflow_columns_and_selection() {
        let mut app = app();
        project_with_development(&mut app);
        let board = app
            .project
            .as_ref()
            .unwrap()
            .views
            .iter()
            .position(|view| view.id == "board")
            .unwrap();
        app.switch_view_by_index(board);
        let rendered = text(&mut app, 140, 40);
        for column in ["Inbox", "Planned", "Ready", "In progress"] {
            assert!(rendered.contains(column), "missing {column}: {rendered}");
        }
    }

    #[test]
    fn stale_and_error_states_are_visible() {
        let mut app = app();
        let project_id = project_with_development(&mut app);
        app.project.as_mut().unwrap().stale_error = Some("server unavailable".into());
        let rendered = text(&mut app, 120, 30);
        assert!(rendered.contains("[stale]"), "{rendered}");
        app.home.components.insert(
            0,
            orbit_ui::ResolvedComponent {
                id: "broken".into(),
                title: "Broken".into(),
                component_type: "list".into(),
                status: orbit_ui::LoadStatus::Error("bad binding".into()),
                body: orbit_ui::ComponentBody::Text {
                    heading: "Broken".into(),
                    lines: vec!["Invalid configuration".into()],
                },
                selection: Vec::new(),
            },
        );
        app.go_screen(Screen::Home);
        let rendered = text(&mut app, 120, 30);
        assert!(rendered.contains("error: bad binding"), "{rendered}");
        let _ = project_id;
    }

    #[test]
    fn loading_component_keeps_context_and_never_looks_authoritative() {
        let mut app = app();
        app.home.components.insert(
            0,
            orbit_ui::ResolvedComponent {
                id: "loading".into(),
                title: "Loading".into(),
                component_type: "list".into(),
                status: orbit_ui::LoadStatus::Loading,
                body: orbit_ui::ComponentBody::Text {
                    heading: "Loading".into(),
                    lines: Vec::new(),
                },
                selection: Vec::new(),
            },
        );
        let rendered = text(&mut app, 120, 30);
        assert!(rendered.contains("Loading accepted data"), "{rendered}");
        assert!(rendered.contains("previous view remains"), "{rendered}");
        assert!(rendered.contains("resolution"), "{rendered}");
    }

    #[test]
    fn table_renderer_handles_narrow_and_wide_terminals() {
        let mut app = app();
        app.home.components = vec![orbit_ui::ResolvedComponent {
            id: "table".into(),
            title: "Table".into(),
            component_type: "table".into(),
            status: orbit_ui::LoadStatus::Ready,
            body: orbit_ui::ComponentBody::Table {
                columns: vec!["title".into(), "status".into(), "estimate".into()],
                rows: vec![orbit_ui::model::TableRowModel {
                    id: "t1".into(),
                    kind: orbit_ui::ItemKind::Task,
                    status: "done".into(),
                    cells: vec![
                        "A long task title that should be truncated".into(),
                        "done".into(),
                        "30m".into(),
                    ],
                }],
            },
            selection: Vec::new(),
        }];
        app.focus = FocusPane::Workspace;
        let wide = text(&mut app, 120, 24);
        assert!(wide.contains("TITLE"), "{wide}");
        assert!(wide.contains("STATUS"), "{wide}");
        assert!(wide.contains("ESTIMATE"), "{wide}");
        assert!(wide.contains("A long task title"), "{wide}");
        assert!(wide.contains("✓"), "{wide}");
        let narrow = text(&mut app, 60, 16);
        assert!(narrow.contains("TITLE"), "{narrow}");
        assert!(narrow.contains("STATUS"), "{narrow}");
    }

    #[test]
    fn tab_focus_and_key_prefix_do_not_collide() {
        let mut app = app();
        app.set_viewport(120);
        app.focus = FocusPane::Workspace;
        handle_key(&mut app, KeyCode::Char('g'), KeyModifiers::NONE).unwrap();
        assert_eq!(app.prefix, Some('g'));
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE).unwrap();
        assert_eq!(app.screen, Screen::Agents);
        assert!(app.prefix.is_none());
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(app.focus, FocusPane::Detail);
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE).unwrap();
        assert_eq!(app.focus, FocusPane::Sidebar);
        handle_key(&mut app, KeyCode::BackTab, KeyModifiers::NONE).unwrap();
        assert_eq!(app.focus, FocusPane::Detail);
    }

    #[test]
    fn project_switch_reloads_views_without_reusing_previous_project_data() {
        let mut app = app();
        let first = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "First".into(),
            })
            .unwrap();
        let second = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Second".into(),
            })
            .unwrap();
        app.refresh_workspace();
        app.go_screen(Screen::Projects);
        app.projects_selection = app
            .projects
            .iter()
            .position(|project| project.id == first.id)
            .unwrap();
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(app.project.as_ref().unwrap().id, first.id);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        app.go_screen(Screen::Projects);
        app.projects_selection = app
            .projects
            .iter()
            .position(|project| project.id == second.id)
            .unwrap();
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(app.project.as_ref().unwrap().id, second.id);
    }

    #[test]
    fn tui_sources_do_not_open_storage_or_spawn_commands() {
        for source in [
            include_str!("app.rs"),
            include_str!("render.rs"),
            include_str!("theme.rs"),
        ] {
            assert!(!source.contains("rusqlite"));
            assert!(!source.contains("Command::new"));
            assert!(!source.contains("std::process::Command"));
            assert!(!source.contains("SELECT "));
            assert!(!source.contains("INSERT INTO"));
        }
    }
}

#[cfg(test)]
mod m28_planning_tests {
    use super::*;
    use app::{Modal, Screen};

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-m28-planning-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    #[test]
    fn today_tasks_are_selectable_and_offer_revision_checked_destinations() {
        let mut app = app();
        let project = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Planning".into(),
            })
            .unwrap();
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "Accepted task".into(),
                project_id: Some(project.id),
                milestone_id: None,
            })
            .unwrap();
        app.client
            .set_task_status(orbit_client::SetTaskStatusRequest {
                id: task.id.clone(),
                status: "ready".into(),
            })
            .unwrap();
        app.refresh_planning();
        app.refresh_workspace();
        assert_eq!(app.screen, Screen::Home);
        let component = app
            .home
            .components
            .iter()
            .position(|component| component.id == "today_tasks")
            .expect("today task component");
        app.active_component = component;
        app.selected = app
            .current_component()
            .unwrap()
            .selection
            .iter()
            .position(|row| row.id == task.id)
            .expect("planning task selection");
        app.today_actions();
        assert!(
            matches!(app.modal, Some(Modal::TodayActions { .. })),
            "expected accepted planning action menu"
        );
        app.today_select_action(0);
        assert!(
            matches!(app.modal, Some(Modal::Message { .. })),
            "expected read-only destination"
        );
        let _ = task;
    }
}

#[cfg(test)]
mod m28_navigation_tests {
    use super::*;
    use app::{FocusPane, FormMode};
    use ratatui::backend::TestBackend;

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-m28-nav-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    fn add_project(app: &mut App, name: &str) -> String {
        let project = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: name.into(),
            })
            .unwrap();
        app.refresh_workspace();
        project.id
    }

    fn add_task(app: &mut App, project_id: &str, title: &str) -> String {
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: title.into(),
                project_id: Some(project_id.into()),
                milestone_id: None,
            })
            .unwrap();
        app.refresh_workspace();
        task.id
    }

    #[test]
    fn back_navigation_restores_the_previous_screen_and_selection() {
        let mut app = app();
        add_project(&mut app, "First");
        add_project(&mut app, "Second");
        app.go_screen(Screen::Projects);
        app.focus = FocusPane::Workspace;
        app.projects_selection = 1;
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert_eq!(app.screen, Screen::Project);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert_eq!(app.screen, Screen::Projects);
        assert_eq!(app.projects_selection, 1);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn switching_views_restores_each_views_component_and_selection() {
        let mut app = app();
        let project = add_project(&mut app, "Work");
        app.client.install_bundled_blueprint("development").unwrap();
        app.client
            .apply_workflow(project.clone(), "development".to_owned(), None)
            .unwrap();
        for title in ["One", "Two", "Three"] {
            add_task(&mut app, &project, title);
        }
        app.open_project(&project);
        let overview = app
            .project
            .as_ref()
            .unwrap()
            .views
            .iter()
            .position(|view| view.id == "overview")
            .unwrap();
        let board = app
            .project
            .as_ref()
            .unwrap()
            .views
            .iter()
            .position(|view| view.id == "board")
            .unwrap();
        app.switch_view_by_index(overview);
        let component = app.project.as_ref().unwrap().views[overview]
            .components
            .iter()
            .position(|component| component.selection.len() >= 3)
            .unwrap();
        app.active_component = component;
        app.selected = 2;
        app.scroll = 1;
        app.switch_view_by_index(board);
        let board_component = app.project.as_ref().unwrap().views[board]
            .components
            .iter()
            .position(|component| component.selection.len() >= 3)
            .unwrap();
        app.active_component = board_component;
        app.selected = 1;
        app.switch_view_by_index(overview);
        assert_eq!(app.active_component, component);
        assert_eq!(app.selected, 2);
        assert_eq!(app.scroll, 1);
        app.switch_view_by_index(board);
        assert_eq!(app.active_component, board_component);
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn back_restores_the_same_project_view_by_stable_id() {
        let mut app = app();
        let project = add_project(&mut app, "View restore");
        app.client.install_bundled_blueprint("development").unwrap();
        app.client
            .apply_workflow(project.clone(), "development".to_owned(), None)
            .unwrap();
        app.open_project(&project);
        let board = app
            .project
            .as_ref()
            .unwrap()
            .views
            .iter()
            .position(|view| view.id == "board")
            .unwrap();
        app.switch_view_by_index(board);
        app.go_screen(Screen::Projects);
        app.go_back();
        assert_eq!(app.screen, Screen::Project);
        assert_eq!(
            app.project.as_ref().unwrap().views[app.project.as_ref().unwrap().active_view].id,
            "board"
        );
    }

    #[test]
    fn switching_components_restores_each_components_selection() {
        let mut app = app();
        let project = add_project(&mut app, "Work");
        app.client.install_bundled_blueprint("research").unwrap();
        app.client
            .apply_workflow(project.clone(), "research".to_owned(), None)
            .unwrap();
        for question in ["One?", "Two?", "Three?"] {
            app.client
                .create_blueprint_work_item(
                    &project,
                    "hypothesis",
                    std::collections::BTreeMap::from([(
                        "question".into(),
                        serde_json::json!(question),
                    )]),
                )
                .unwrap();
        }
        app.open_project(&project);
        let workflow = app
            .project
            .as_ref()
            .unwrap()
            .views
            .iter()
            .position(|view| view.id == "workflow")
            .unwrap();
        app.switch_view_by_index(workflow);
        let view = app.project.as_ref().unwrap().active_view;
        let components: Vec<usize> = app.project.as_ref().unwrap().views[view]
            .components
            .iter()
            .enumerate()
            .filter(|(_, component)| component.selection.len() >= 3)
            .map(|(index, _)| index)
            .collect();
        assert!(components.len() >= 2, "need two selectable components");
        let (first, second) = (components[0], components[1]);
        app.active_component = first;
        app.selected = 2;
        app.scroll = 1;
        app.move_component((second as isize) - (first as isize));
        assert_eq!(app.active_component, second);
        assert_eq!(app.selected, 0);
        app.selected = 1;
        app.move_component((first as isize) - (second as isize));
        assert_eq!(app.active_component, first);
        assert_eq!(app.selected, 2);
        assert_eq!(app.scroll, 1);
    }

    #[test]
    fn quit_confirmation_renders_and_can_be_cancelled() {
        let mut app = app();
        app.start_form(FormMode::NewTask { project_id: None });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = "draft".into();
        form.dirty = true;
        app.modal = Some(Modal::Form(form));
        assert!(!app.request_quit());
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render::draw(frame, &mut app))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Quit Orbit?"), "{text}");
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert!(matches!(app.modal, Some(Modal::Form(_))));
    }
}

#[cfg(test)]
mod m28_quit_tests {
    use super::*;
    use app::{FocusPane, FormMode};
    use ratatui::backend::TestBackend;

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-m28-quit-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    fn selectable(app: &mut App) -> bool {
        app.current_component()
            .map(|component| !component.selection.is_empty())
            .unwrap_or(false)
            || matches!(app.screen, Screen::Projects | Screen::Home)
    }

    #[test]
    fn q_quits_from_every_global_screen() {
        for screen in [
            Screen::Home,
            Screen::Inbox,
            Screen::Projects,
            Screen::Agents,
        ] {
            let mut app = app();
            app.go_screen(screen);
            assert!(
                handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap(),
                "{screen:?}"
            );
        }
        let mut app = app();
        app.client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Quit".into(),
            })
            .unwrap();
        app.refresh_workspace();
        app.open_project(&app.projects[0].id.clone());
        assert_eq!(app.screen, Screen::Project);
        assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
    }

    #[test]
    fn q_quits_non_text_modals_and_enters_text_in_text_fields() {
        let mut app = app();
        app.open_palette();
        for _ in 0..3 {
            handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap();
        }
        let Some(Modal::Palette { query, .. }) = &app.modal else {
            panic!("palette");
        };
        assert_eq!(query, "qqq");

        app.modal = Some(Modal::Help);
        // `q` closes help instead of quitting so the user cannot lose context.
        assert!(!handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
        assert!(app.modal.is_none());

        app.modal = Some(Modal::Message {
            title: "Notice".into(),
            lines: vec!["text".into()],
        });
        assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());

        app.modal = Some(Modal::Confirm {
            title: "Change".into(),
            body: "body".into(),
            pending: app::PendingAction::ApplyPlan,
        });
        assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
    }

    #[test]
    fn quit_confirmation_appears_for_unsaved_text_and_exposes_both_choices() {
        let mut app = app();
        app.start_form(FormMode::NewTask { project_id: None });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = "draft".into();
        form.dirty = true;
        app.modal = Some(Modal::Form(form));
        assert!(!app.request_quit());
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
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
        assert!(rendered.contains("Quit Orbit?"), "{rendered}");
        assert!(rendered.contains("Ctrl+Q"), "{rendered}");
        assert!(rendered.contains("Esc or n"), "{rendered}");
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE).unwrap();
        assert!(matches!(app.modal, Some(Modal::Form(_))));
        assert!(!app.request_quit());
        assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
        assert!(handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE).unwrap());
    }

    #[test]
    fn quit_is_independent_of_selection_and_focus() {
        let mut app = app();
        app.focus = FocusPane::Detail;
        app.selected = 99;
        assert!(app.request_quit());
        app.focus = FocusPane::Sidebar;
        app.selected = 0;
        assert!(app.request_quit());
        let _ = selectable(&mut app);
    }
}

#[cfg(test)]
mod m28_form_tests {
    use super::*;
    use app::{FormField, FormMode, Modal};

    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-tui-m28-form-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn app_at(path: &std::path::Path) -> App {
        App::open(path.to_string_lossy().into_owned()).unwrap()
    }

    fn fill(form: &mut app::FormState, key: &str, value: &str) {
        let field = form
            .fields
            .iter_mut()
            .find(|field: &&mut FormField| field.key == key)
            .unwrap_or_else(|| panic!("missing field {key}"));
        field.value = value.into();
        form.dirty = true;
    }

    fn current_form(app: &mut App) -> app::FormState {
        let Some(Modal::Form(form)) = app.modal.take() else {
            panic!("expected form");
        };
        form
    }

    fn create_project(app: &mut App, name: &str) -> String {
        app.start_form(FormMode::NewProject);
        let mut form = current_form(app);
        fill(&mut form, "name", name);
        app.submit_form(form);
        app.project.as_ref().unwrap().id.clone()
    }

    #[test]
    fn project_and_task_forms_persist_across_restart() {
        let path = path();
        let mut app = app_at(&path);
        let project = create_project(&mut app, "Form Project");
        app.start_form(FormMode::NewTask {
            project_id: Some(project.clone()),
        });
        let mut form = current_form(&mut app);
        fill(&mut form, "title", "Form task");
        fill(&mut form, "estimate", "45");
        fill(&mut form, "deadline", "1893456000000");
        app.submit_form(form);
        assert!(
            app.modal.is_none(),
            "task form should close: {:?}",
            app.modal
        );
        assert!(
            app.client
                .list_tasks()
                .unwrap()
                .iter()
                .any(|task| task.title == "Form task" && task.estimated_minutes == Some(45))
        );
        drop(app);

        let app = app_at(&path);
        assert!(app.projects.iter().any(|row| row.name == "Form Project"));
        let task = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|task| task.title == "Form task")
            .expect("persisted task");
        assert_eq!(task.estimated_minutes, Some(45));
        assert_eq!(task.deadline_at_ms, Some(1_893_456_000_000));
    }

    #[test]
    fn task_form_closes_through_the_key_handler() {
        let mut app = app_at(&path());
        create_project(&mut app, "Key handler");
        app.start_form(FormMode::NewTask {
            project_id: app.project.as_ref().map(|project| project.id.clone()),
        });
        for character in "First task".chars() {
            handle_key(&mut app, KeyCode::Char(character), KeyModifiers::NONE).unwrap();
        }
        assert!(!handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap());
        assert!(app.modal.is_none(), "modal after Enter: {:?}", app.modal);
    }

    #[test]
    fn edit_task_form_renames_and_updates_planning_fields() {
        let mut app = app_at(&path());
        let project = create_project(&mut app, "Edit Project");
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "Old title".into(),
                project_id: Some(project),
                milestone_id: None,
            })
            .unwrap();
        app.start_form(FormMode::EditTask {
            task_id: task.id.clone(),
        });
        let mut form = current_form(&mut app);
        fill(&mut form, "title", "New title");
        fill(&mut form, "estimate", "30");
        app.submit_form(form);
        let updated = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|row| row.id == task.id)
            .unwrap();
        assert_eq!(updated.title, "New title");
        assert_eq!(updated.estimated_minutes, Some(30));
    }

    #[test]
    fn edit_project_form_preserves_criteria_and_updates_supported_settings() {
        let mut app = app_at(&path());
        let project = create_project(&mut app, "Settings");
        app.start_form(FormMode::EditProject {
            project_id: project.clone(),
        });
        let mut form = current_form(&mut app);
        fill(&mut form, "status", "active");
        fill(&mut form, "mode", "learning");
        fill(&mut form, "objective", "Learn the workflow");
        app.submit_form(form);
        let updated = app
            .client
            .list_projects()
            .unwrap()
            .into_iter()
            .find(|row| row.id == project)
            .unwrap();
        assert_eq!(updated.status, "active");
        assert_eq!(updated.mode, "learning");
        assert_eq!(updated.objective.as_deref(), Some("Learn the workflow"));
    }

    #[test]
    fn work_item_forms_use_declared_fields_and_persist_edits() {
        let mut app = app_at(&path());
        let project = create_project(&mut app, "Research");
        app.client.install_bundled_blueprint("research").unwrap();
        app.client
            .apply_workflow(project.clone(), "research".to_owned(), None)
            .unwrap();
        app.refresh_project();
        app.start_form(FormMode::NewWorkItemKind {
            project_id: project.clone(),
        });
        let mut kind_form = current_form(&mut app);
        fill(&mut kind_form, "kind", "hypothesis");
        app.submit_form(kind_form);
        let mut fields = current_form(&mut app);
        fill(&mut fields, "question", "Does the form work?");
        fill(&mut fields, "confidence", "high");
        app.submit_form(fields);
        app.refresh_project();
        let item = app
            .client
            .list_blueprint_work_items(&project)
            .unwrap()
            .into_iter()
            .find(|item| item.kind == "hypothesis")
            .expect("created item");
        assert_eq!(item.properties["question"], "Does the form work?");
        app.start_form(FormMode::EditWorkItem {
            item_id: item.id.clone(),
            revision: item.revision,
            blueprint: app
                .client
                .project_blueprint_definition(&project)
                .unwrap()
                .unwrap(),
            kind: item.kind.clone(),
        });
        let mut edit = current_form(&mut app);
        fill(&mut edit, "question", "Edited question");
        app.submit_form(edit);
        let edited = app
            .client
            .list_blueprint_work_items(&project)
            .unwrap()
            .into_iter()
            .find(|row| row.id == item.id)
            .unwrap();
        assert_eq!(edited.properties["question"], "Edited question");
    }

    #[test]
    fn dependency_forms_add_and_remove_prerequisites() {
        let mut app = app_at(&path());
        let project = create_project(&mut app, "Dependencies");
        let first = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "First".into(),
                project_id: Some(project.clone()),
                milestone_id: None,
            })
            .unwrap();
        let second = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "Second".into(),
                project_id: Some(project),
                milestone_id: None,
            })
            .unwrap();
        app.refresh_project();
        app.start_form(FormMode::AddDependency {
            task_id: second.id.clone(),
        });
        let mut add = current_form(&mut app);
        fill(&mut add, "prerequisite", &first.id);
        app.submit_form(add);
        let data = app
            .client
            .project_view_data(&app.project.as_ref().unwrap().id, crate::app::now_ms())
            .unwrap();
        assert!(
            data.tasks
                .iter()
                .find(|task| task.id == second.id)
                .unwrap()
                .prerequisite_ids
                .contains(&first.id)
        );

        app.refresh_project();
        app.start_form(FormMode::RemoveDependency {
            task_id: second.id.clone(),
        });
        let mut remove = current_form(&mut app);
        fill(&mut remove, "prerequisite", &first.id);
        app.submit_form(remove);
        let data = app
            .client
            .project_view_data(&app.project.as_ref().unwrap().id, crate::app::now_ms())
            .unwrap();
        assert!(
            !data
                .tasks
                .iter()
                .find(|task| task.id == second.id)
                .unwrap()
                .prerequisite_ids
                .contains(&first.id)
        );
    }

    #[test]
    fn invalid_form_preserves_values_and_reports_the_field() {
        let mut app = app_at(&path());
        app.start_form(FormMode::NewTask { project_id: None });
        let mut form = current_form(&mut app);
        fill(&mut form, "title", "Valid title");
        fill(&mut form, "estimate", "not-a-number");
        app.submit_form(form);
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("form should stay open");
        };
        assert!(form.error.as_deref().unwrap_or("").contains("integer"));
        assert_eq!(
            form.fields
                .iter()
                .find(|field| field.key == "estimate")
                .unwrap()
                .value,
            "not-a-number"
        );
    }
}

#[cfg(test)]
mod m28_post_gate_tests {
    use super::*;
    use app::{FormMode, Modal};

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-m28-gate-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    #[test]
    fn q_is_text_inside_forms_and_never_quits_or_discards() {
        let mut app = app();
        app.start_form(FormMode::NewTask { project_id: None });
        assert!(!handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE).unwrap());
        assert!(!handle_key(&mut app, KeyCode::Char('b'), KeyModifiers::NONE).unwrap());
        assert!(!handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("form should remain open");
        };
        assert_eq!(form.fields[0].value, "abq");
        assert!(form.dirty);
        assert!(!app.request_quit());
        assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
        handle_key(&mut app, KeyCode::Char('n'), KeyModifiers::NONE).unwrap();
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("form should be restored");
        };
        assert_eq!(form.fields[0].value, "abq");
    }
}

#[cfg(test)]
mod m28_scroll_quit_edges {
    use super::*;
    use app::{FocusPane, Modal};

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-m28-scroll-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    #[test]
    fn text_component_scroll_survives_navigation_and_refresh() {
        let mut app = app();
        let component = app
            .home
            .components
            .iter()
            .position(|component| component.selection.is_empty())
            .expect("text component");
        app.screen = Screen::Home;
        app.active_component = component;
        app.selected = 0;
        app.scroll = 4;
        app.go_screen(Screen::Inbox);
        app.go_back();
        assert_eq!(app.active_component, component);
        assert_eq!(app.scroll, 4);
        app.refresh_workspace();
        assert_eq!(app.scroll, 4);
    }

    #[test]
    fn palette_quit_with_query_uses_the_single_confirmation_path() {
        let mut app = app();
        app.open_palette();
        for character in "quit".chars() {
            handle_key(&mut app, KeyCode::Char(character), KeyModifiers::NONE).unwrap();
        }
        let Some(Modal::Palette {
            entries, selected, ..
        }) = &mut app.modal
        else {
            panic!("palette");
        };
        *selected = entries
            .iter()
            .position(|entry| matches!(entry.action, app::PaletteAction::Quit))
            .expect("quit entry");
        let result = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert!(!result);
        assert!(matches!(app.modal, Some(Modal::QuitConfirm { .. })));
        let _ = FocusPane::Workspace;
    }
}

#[cfg(test)]
mod m28_oracle_remediation_tests {
    use super::*;
    use app::{FormFieldKind, FormMode, Modal};

    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-tui-m28-fix-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn app() -> App {
        App::open(path().to_string_lossy().into_owned()).unwrap()
    }
    fn fill(form: &mut app::FormState, key: &str, value: &str) {
        let field = form
            .fields
            .iter_mut()
            .find(|field| field.key == key)
            .unwrap();
        field.value = value.into();
        form.dirty = true;
    }
    fn take_form(app: &mut App) -> app::FormState {
        let Some(Modal::Form(form)) = app.modal.take() else {
            panic!("form");
        };
        form
    }
    fn project(app: &mut App, name: &str) -> String {
        app.start_form(FormMode::NewProject);
        let mut form = take_form(app);
        fill(&mut form, "name", name);
        app.submit_form(form);
        app.project.as_ref().unwrap().id.clone()
    }

    #[test]
    fn project_objective_uses_default_signals_and_opens() {
        let mut app = app();
        app.start_form(FormMode::NewProject);
        let mut form = take_form(&mut app);
        fill(&mut form, "name", "Objective");
        fill(&mut form, "objective", "Ship it");
        app.submit_form(form);
        assert!(app.modal.is_none(), "{:?}", app.modal);
        let project = app
            .client
            .list_projects()
            .unwrap()
            .into_iter()
            .find(|project| project.name == "Objective")
            .unwrap();
        assert_eq!(project.objective.as_deref(), Some("Ship it"));
        assert!(project.progress_signals.len() >= 4);
        assert_eq!(app.screen, Screen::Project);
    }

    #[test]
    fn project_edit_preserves_satisfied_criterion_status() {
        let mut app = app();
        let project_id = project(&mut app, "Criteria");
        app.client
            .configure_project_strategy(orbit_client::ConfigureProjectStrategyRequest {
                id: project_id.clone(),
                mode: "standard".into(),
                objective: None,
                progress_signals: vec![
                    "milestones".into(),
                    "tasks".into(),
                    "blockers".into(),
                    "forecast".into(),
                ],
                success_criteria: vec![orbit_client::ProjectCriterionInputDto {
                    key: "ship".into(),
                    description: "Ship".into(),
                    required: true,
                    status: "satisfied".into(),
                }],
            })
            .unwrap();
        app.refresh_workspace();
        app.open_project(&project_id);
        app.start_form(FormMode::EditProject {
            project_id: project_id.clone(),
        });
        let mut form = take_form(&mut app);
        fill(&mut form, "objective", "Still ship");
        app.submit_form(form);
        let project = app
            .client
            .list_projects()
            .unwrap()
            .into_iter()
            .find(|project| project.id == project_id)
            .unwrap();
        assert_eq!(project.success_criteria[0].status, "satisfied");
    }

    #[test]
    fn zero_estimate_never_creates_or_renames() {
        let mut app = app();
        let project_id = project(&mut app, "Estimate");
        app.start_form(FormMode::NewTask {
            project_id: Some(project_id.clone()),
        });
        let mut form = take_form(&mut app);
        fill(&mut form, "title", "Zero");
        fill(&mut form, "estimate", "0");
        app.submit_form(form);
        assert!(matches!(app.modal, Some(Modal::Form(_))));
        assert!(app.client.list_tasks().unwrap().is_empty());

        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "Original".into(),
                project_id: Some(project_id),
                milestone_id: None,
            })
            .unwrap();
        app.start_form(FormMode::EditTask {
            task_id: task.id.clone(),
        });
        let mut form = take_form(&mut app);
        fill(&mut form, "title", "Renamed");
        fill(&mut form, "estimate", "0");
        app.submit_form(form);
        let unchanged = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|row| row.id == task.id)
            .unwrap();
        assert_eq!(unchanged.title, "Original");
        assert_eq!(unchanged.estimated_minutes, None);
    }

    #[test]
    fn work_item_edit_only_patches_changed_values() {
        let mut app = app();
        let project_id = project(&mut app, "Patch");
        app.client.install_bundled_blueprint("research").unwrap();
        app.client
            .apply_workflow(project_id.clone(), "research".to_owned(), None)
            .unwrap();
        let item = app
            .client
            .create_blueprint_work_item(
                &project_id,
                "hypothesis",
                std::collections::BTreeMap::from([(
                    "question".into(),
                    serde_json::json!("Original question"),
                )]),
            )
            .unwrap();
        app.open_project(&project_id);
        app.start_form(FormMode::EditWorkItem {
            item_id: item.id.clone(),
            revision: item.revision,
            blueprint: app
                .client
                .project_blueprint_definition(&project_id)
                .unwrap()
                .unwrap(),
            kind: item.kind.clone(),
        });
        let mut form = take_form(&mut app);
        fill(&mut form, "question", "Changed question");
        app.submit_form(form);
        let updated = app
            .client
            .list_blueprint_work_items(&project_id)
            .unwrap()
            .into_iter()
            .find(|row| row.id == item.id)
            .unwrap();
        assert_eq!(updated.properties["question"], "Changed question");
        // The blueprint normalizer may materialize declared defaults; the UI must
        // not invent fields that are not declared. Only the changed key was sent.
    }

    #[test]
    fn metric_form_allows_first_metric_target_by_id() {
        let mut app = app();
        app.start_form(FormMode::MetricTarget { metric_id: None });
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("metric form");
        };
        assert!(matches!(
            form.fields[0].kind,
            FormFieldKind::Text | FormFieldKind::Choice(_)
        ));
        assert!(form.fields[0].required);
    }
}

#[cfg(test)]
mod m28_gate2_fix_tests {
    use super::*;
    use app::{FormMode, Modal};
    use ratatui::backend::TestBackend;

    fn app() -> App {
        let path = std::env::temp_dir().join(format!(
            "orbit-tui-m28-g2-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        App::open(path.to_string_lossy().into_owned()).unwrap()
    }

    #[test]
    fn form_errors_are_rendered_above_the_fields() {
        let mut app = app();
        app.start_form(FormMode::NewTask { project_id: None });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.error = Some("title is required".into());
        app.modal = Some(Modal::Form(form));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
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
        assert!(rendered.contains("error: title is required"), "{rendered}");
    }

    #[test]
    fn duplicate_enter_keeps_the_invalid_form_open() {
        let mut app = app();
        app.start_form(FormMode::NewTask { project_id: None });
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert!(matches!(app.modal, Some(Modal::Form(_))));
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert!(matches!(app.modal, Some(Modal::Form(_))));
    }

    #[test]
    fn estimate_upper_bound_is_rejected_before_creation() {
        let mut app = app();
        app.start_form(FormMode::NewTask { project_id: None });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = "Too long".into();
        form.fields
            .iter_mut()
            .find(|field| field.key == "estimate")
            .unwrap()
            .value = "525601".into();
        form.dirty = true;
        app.submit_form(form);
        assert!(matches!(app.modal, Some(Modal::Form(_))));
        assert!(app.client.list_tasks().unwrap().is_empty());
    }

    #[test]
    fn unchanged_work_item_edit_does_not_bump_revision() {
        let mut app = app();
        app.client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Noop".into(),
            })
            .unwrap();
        app.refresh_workspace();
        let project = app.projects[0].id.clone();
        app.client.install_bundled_blueprint("research").unwrap();
        app.client
            .apply_workflow(project.clone(), "research".to_owned(), None)
            .unwrap();
        let item = app
            .client
            .create_blueprint_work_item(
                &project,
                "hypothesis",
                std::collections::BTreeMap::from([(
                    "question".into(),
                    serde_json::json!("No change"),
                )]),
            )
            .unwrap();
        app.open_project(&project);
        app.start_form(FormMode::EditWorkItem {
            item_id: item.id.clone(),
            revision: item.revision,
            blueprint: app
                .client
                .project_blueprint_definition(&project)
                .unwrap()
                .unwrap(),
            kind: item.kind.clone(),
        });
        let Some(Modal::Form(form)) = app.modal.take() else {
            panic!("form");
        };
        app.submit_form(form);
        assert!(app.modal.is_none());
        let after = app
            .client
            .list_blueprint_work_items(&project)
            .unwrap()
            .into_iter()
            .find(|row| row.id == item.id)
            .unwrap();
        assert_eq!(after.revision, item.revision);
    }
}

#[cfg(test)]
mod m28_task_scope_tests {
    use super::*;
    use app::{FormMode, Modal};

    fn app() -> App {
        let path = std::env::temp_dir().join(format!(
            "orbit-tui-m28-scope-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        App::open(path.to_string_lossy().into_owned()).unwrap()
    }

    fn fill(form: &mut app::FormState, key: &str, value: &str) {
        form.fields
            .iter_mut()
            .find(|field| field.key == key)
            .unwrap()
            .value = value.into();
        form.dirty = true;
    }

    #[test]
    fn task_edit_moves_project_without_renaming_on_invalid_milestone() {
        let mut app = app();
        let first = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "First".into(),
            })
            .unwrap();
        let second = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Second".into(),
            })
            .unwrap();
        app.refresh_workspace();
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: "Move me".into(),
                project_id: Some(first.id.clone()),
                milestone_id: None,
            })
            .unwrap();
        app.open_project(&first.id);
        app.start_form(FormMode::EditTask {
            task_id: task.id.clone(),
        });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        fill(&mut form, "project", &second.id);
        app.submit_form(form);
        let moved = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|row| row.id == task.id)
            .unwrap();
        assert_eq!(moved.project_id.as_deref(), Some(second.id.as_str()));

        app.start_form(FormMode::EditTask {
            task_id: task.id.clone(),
        });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        fill(&mut form, "title", "Should not apply");
        fill(&mut form, "milestone", "missing");
        app.submit_form(form);
        assert!(matches!(app.modal, Some(Modal::Form(_))));
        let unchanged = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|row| row.id == task.id)
            .unwrap();
        assert_eq!(unchanged.title, "Move me");
    }
}

#[cfg(test)]
mod m28_unified_connected_tests {
    use super::*;
    use app::{FormMode, Modal};

    #[test]
    fn connected_project_objective_is_rejected_before_queueing_anything() {
        let path = std::env::temp_dir().join(format!(
            "orbit-tui-m28-connected-objective-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let backend = crate::backend::Backend::open_connected_with_identity(
            &path,
            "https://example.org",
            None,
        )
        .unwrap();
        let mut app = App::open_with_backend(backend).unwrap();
        app.start_form(FormMode::NewProject);
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = "No orphan".into();
        form.fields[1].value = "Unsupported objective".into();
        form.dirty = true;
        app.submit_form(form);
        assert!(matches!(app.modal, Some(Modal::Form(_))));
        assert_eq!(app.client.pending_count(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn connected_mode_uses_the_shared_shell_and_never_claims_pending_acceptance() {
        let path = std::env::temp_dir().join(format!(
            "orbit-tui-m28-connected-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let backend = crate::backend::Backend::open_connected_with_identity(
            &path,
            "https://example.org",
            None,
        )
        .unwrap();
        let mut app = App::open_with_backend(backend).unwrap();
        assert_eq!(app.screen, Screen::Home);
        assert!(app.client.is_connected());
        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
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
        assert!(rendered.contains("CONNECTED"), "{rendered}");

        app.screen = Screen::Agents;
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(120, 35)).unwrap();
        terminal
            .draw(|frame| render::draw(frame, &mut app))
            .unwrap();
        let agents: String = (0..terminal.backend().buffer().area.height)
            .map(|y| {
                (0..terminal.backend().buffer().area.width)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            agents.contains("not") && agents.contains("projected"),
            "{agents}"
        );
        assert!(!agents.contains("No agent jobs"), "{agents}");
        app.screen = Screen::Home;

        app.start_form(FormMode::NewProject);
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = "Queued project".into();
        form.dirty = true;
        app.submit_form(form);
        assert!(
            app.projects.is_empty(),
            "pending project must not appear accepted"
        );
        assert!(
            app.status.text.to_lowercase().contains("pending"),
            "{}",
            app.status.text
        );
        assert!(app.request_quit());
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod m29_connected_task_detail_tests {
    use super::*;
    use app::{FormMode, Modal};
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
}

#[cfg(test)]
mod m28_layout_tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        let path = std::env::temp_dir().join(format!(
            "orbit-tui-m28-layout-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let mut app = App::open(path.to_string_lossy().into_owned()).unwrap();
        let project = app
            .client
            .create_project(orbit_client::CreateProjectRequest {
                id: None,
                name: "Layout".into(),
            })
            .unwrap();
        app.client.install_bundled_blueprint("development").unwrap();
        app.client
            .apply_workflow(project.id.clone(), "development".to_owned(), None)
            .unwrap();
        for (title, statuses) in [
            ("Inbox task", vec![]),
            ("Ready task", vec!["ready"]),
            ("Progress task", vec!["ready", "in_progress"]),
            ("Review task", vec!["ready", "in_progress", "review"]),
            ("Done task", vec!["ready", "in_progress", "done"]),
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
        let board = app
            .project
            .as_ref()
            .unwrap()
            .views
            .iter()
            .position(|view| view.id == "board")
            .unwrap();
        app.switch_view_by_index(board);
        app
    }

    fn text(app: &mut App, width: u16, height: u16) -> String {
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

    #[test]
    fn common_terminal_sizes_remain_readable_and_keep_quit_visible() {
        let mut app = app();
        for (width, height) in [(60, 18), (80, 24), (100, 30), (120, 35), (160, 45)] {
            let rendered = text(&mut app, width, height);
            assert!(
                rendered.contains("Ctrl+Q quit"),
                "{width}x{height}: {rendered}"
            );
            assert!(rendered.contains("Task board"), "{width}x{height}");
            assert!(rendered.contains("Inbox"), "{width}x{height}");
            if width >= 120 {
                assert!(rendered.contains("Details"), "{width}x{height}");
            }
        }
    }

    #[test]
    fn kanban_uses_a_readable_window_instead_of_nine_slivers() {
        let mut app = app();
        let rendered = text(&mut app, 120, 35);
        assert!(rendered.contains("Planned"), "{rendered}");
        assert!(rendered.contains("Ready"), "{rendered}");
        assert!(
            rendered.contains("Inbox task") || rendered.contains("Ready task"),
            "{rendered}"
        );
        assert!(!rendered.contains("╭ Inb╮"), "{rendered}");
        let wide = text(&mut app, 160, 45);
        assert!(wide.contains("columns 1-"), "{wide}");
    }
}

#[cfg(test)]
mod m28_scroll_tests {
    use super::*;
    use app::FocusPane;
    use ratatui::{Terminal, backend::TestBackend};

    fn text(app: &mut App, width: u16, height: u16) -> String {
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

    fn app() -> App {
        let path = std::env::temp_dir().join(format!(
            "orbit-tui-m28-scroll2-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        App::open(path.to_string_lossy().into_owned()).unwrap()
    }

    #[test]
    fn palette_selection_stays_visible_at_60x18() {
        let mut app = app();
        for index in 0..8 {
            app.client
                .create_project(orbit_client::CreateProjectRequest {
                    id: None,
                    name: format!("Palette project {index}"),
                })
                .unwrap();
        }
        app.refresh_workspace();
        app.open_palette();
        for _ in 0..100 {
            handle_key(&mut app, KeyCode::Down, KeyModifiers::NONE).unwrap();
        }
        let rendered = text(&mut app, 60, 18);
        assert!(rendered.contains("Quit"), "{rendered}");
        assert!(rendered.contains("▸ Quit"), "{rendered}");
    }

    #[test]
    fn sidebar_selection_scrolls_into_view() {
        let mut app = app();
        for index in 0..30 {
            app.client
                .create_project(orbit_client::CreateProjectRequest {
                    id: None,
                    name: format!("Project {index}"),
                })
                .unwrap();
        }
        app.refresh_workspace();
        app.screen = Screen::Home;
        app.focus = FocusPane::Sidebar;
        app.sidebar_selection = app.sidebar_len() - 1;
        let rendered = text(&mut app, 60, 18);
        assert!(rendered.contains("Project 29"), "{rendered}");
        assert!(
            rendered
                .lines()
                .any(|line| line.contains("▸") && line.contains("Project 29")),
            "{rendered}"
        );
    }

    #[test]
    fn help_modal_scrolls_to_later_lines() {
        let mut app = app();
        app.modal = Some(Modal::Help);
        let before = text(&mut app, 60, 18);
        assert!(!before.contains("Ctrl+Q / Ctrl+C"), "{before}");
        for _ in 0..3 {
            handle_key(&mut app, KeyCode::PageDown, KeyModifiers::NONE).unwrap();
        }
        let after = text(&mut app, 60, 18);
        assert!(after.contains("Ctrl+Q / Ctrl+C"), "{after}");
    }

    #[test]
    fn detail_focus_resets_its_own_scroll() {
        let mut app = app();
        app.focus = FocusPane::Workspace;
        app.detail_scroll = 7;
        app.focus_next(false);
        assert_eq!(app.focus, FocusPane::Detail);
        assert_eq!(app.detail_scroll, 0);
    }
}

#[cfg(test)]
mod m28_focus_scroll_tests {
    use super::*;
    use app::FocusPane;
    use orbit_ui::model::{ItemKind, TimelineEventModel};
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        let path = std::env::temp_dir().join(format!(
            "orbit-tui-m28-focus-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        App::open(path.to_string_lossy().into_owned()).unwrap()
    }
    fn text(app: &mut App, width: u16, height: u16) -> String {
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

    #[test]
    fn tab_cycles_sidebar_and_workspace_below_wide_breakpoint() {
        let mut app = app();
        app.set_viewport(100);
        app.focus = FocusPane::Workspace;
        app.focus_next(false);
        assert_eq!(app.focus, FocusPane::Sidebar);
        app.focus_next(false);
        assert_eq!(app.focus, FocusPane::Workspace);
    }

    #[test]
    fn timeline_selection_stays_visible_at_60x18() {
        let mut app = app();
        let events = (0..12)
            .map(|index| TimelineEventModel {
                id: format!("event-{index}"),
                kind: ItemKind::Schedule,
                title: format!("Timeline event {index}"),
                when: format!("2026-10-22 {index:02}:00Z"),
                sort_key: index,
                status: "planned".into(),
                detail: "detail".into(),
            })
            .collect::<Vec<_>>();
        app.home.components = vec![orbit_ui::ResolvedComponent {
            id: "timeline".into(),
            title: "Timeline".into(),
            component_type: "timeline".into(),
            status: orbit_ui::LoadStatus::Ready,
            body: orbit_ui::ComponentBody::Timeline { events },
            selection: (0..12)
                .map(|index| orbit_ui::SelectionItem {
                    id: format!("event-{index}"),
                    kind: ItemKind::Schedule,
                    title: format!("Timeline event {index}"),
                    subtitle: String::new(),
                    status: "planned".into(),
                    fields: Vec::new(),
                    allowed_statuses: Vec::new(),
                    allowed_transitions: Vec::new(),
                    revision: None,
                })
                .collect(),
        }];
        app.screen = Screen::Home;
        app.active_component = 0;
        app.selected = 11;
        app.focus = FocusPane::Workspace;
        let rendered = text(&mut app, 60, 18);
        assert!(rendered.contains("Timeline event 11"), "{rendered}");
        assert!(
            rendered
                .lines()
                .any(|line| line.contains("▸") && line.contains("event 11")),
            "{rendered}"
        );
    }
}

#[cfg(test)]
mod m28_usability_recovery_tests {
    use super::*;
    use app::{FocusPane, FormMode, Modal};
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-usability-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&filename);
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }

    fn submit_project(app: &mut App, name: &str) -> String {
        app.start_form(FormMode::NewProject);
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("expected project form");
        };
        form.fields[0].value = name.into();
        form.dirty = true;
        app.submit_form(form);
        app.project.as_ref().expect("project opened").id.clone()
    }

    fn submit_task(app: &mut App, project_id: &str, title: &str) -> String {
        app.start_form(FormMode::NewTask {
            project_id: Some(project_id.to_owned()),
        });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("expected task form");
        };
        form.fields[0].value = title.into();
        form.dirty = true;
        app.submit_form(form);
        app.client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|task| task.title == title)
            .expect("task created")
            .id
    }

    fn buffer_text(app: &mut App, width: u16, height: u16) -> String {
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

    #[test]
    fn new_project_form_defaults_to_the_basic_workflow() {
        let mut app = app();
        app.start_form(FormMode::NewProject);
        let Some(Modal::Form(form)) = &app.modal else {
            panic!("form");
        };
        let workflow = form
            .fields
            .iter()
            .find(|field| field.key == "workflow")
            .expect("workflow picker");
        assert_eq!(workflow.value, "basic");
        let FormFieldKind::Choice(choices) = &workflow.kind else {
            panic!("workflow must be a choice");
        };
        assert!(choices.iter().any(|choice| choice.value == "basic"));
    }

    #[test]
    fn task_details_form_edits_multiline_text_and_saves_with_ctrl_s() {
        let mut app = app();
        let project_id = submit_project(&mut app, "Details");
        let task_id = submit_task(&mut app, &project_id, "Write docs");
        app.start_form(FormMode::EditTaskDetails {
            task_id: task_id.clone(),
        });
        for code in [
            KeyCode::Char('L'),
            KeyCode::Char('i'),
            KeyCode::Char('n'),
            KeyCode::Char('e'),
        ] {
            handle_key(&mut app, code, KeyModifiers::NONE).unwrap();
        }
        // Enter inserts a newline in a multi-line field instead of submitting.
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE).unwrap();
        assert!(matches!(app.modal, Some(Modal::Form(_))));
        handle_key(&mut app, KeyCode::Char('2'), KeyModifiers::NONE).unwrap();
        handle_key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE).unwrap();
        handle_key(&mut app, KeyCode::Char('!'), KeyModifiers::NONE).unwrap();
        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL).unwrap();

        let task = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|task| task.id == task_id)
            .expect("task");
        assert_eq!(task.description, "Line\n2 !");
        assert!(app.modal.is_none());
    }

    #[test]
    fn editing_details_marks_priority_and_labels_without_touching_status() {
        let mut app = app();
        let project_id = submit_project(&mut app, "Rich");
        let task_id = submit_task(&mut app, &project_id, "Ship it");
        app.start_form(FormMode::EditTaskDetails {
            task_id: task_id.clone(),
        });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[1].value = "high".into();
        form.fields[2].value = "api, docs".into();
        app.submit_form(form);
        let task = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|task| task.id == task_id)
            .expect("task");
        assert_eq!(task.priority.as_deref(), Some("high"));
        assert_eq!(task.labels, vec!["api".to_owned(), "docs".to_owned()]);
        assert_eq!(task.status, "inbox");
    }

    #[test]
    fn acceptance_criteria_are_checkable_without_completing_the_task() {
        let mut app = app();
        let project_id = submit_project(&mut app, "Criteria");
        let task_id = submit_task(&mut app, &project_id, "Acceptance");
        app.start_form(FormMode::AddCriterion {
            task_id: task_id.clone(),
        });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = "Works offline".into();
        form.dirty = true;
        app.submit_form(form);
        assert!(matches!(app.modal, Some(Modal::Criteria(_))));
        handle_key(&mut app, KeyCode::Char(' '), KeyModifiers::NONE).unwrap();
        let criteria = app.client.acceptance_criteria(&task_id).unwrap();
        assert_eq!(criteria.len(), 1);
        assert!(criteria[0].completed);
        let task = app
            .client
            .list_tasks()
            .unwrap()
            .into_iter()
            .find(|task| task.id == task_id)
            .expect("task");
        assert_eq!(
            task.status, "inbox",
            "checking every criterion must not complete the task"
        );
    }

    #[test]
    fn attachments_can_be_added_listed_and_removed_from_the_tui() {
        let mut app = app();
        let project_id = submit_project(&mut app, "Files");
        let task_id = submit_task(&mut app, &project_id, "Attach");
        let source = std::env::temp_dir().join(format!(
            "orbit-attach-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&source, b"evidence").unwrap();

        app.start_form(FormMode::AttachFile {
            owner_kind: "task".into(),
            owner_id: task_id.clone(),
        });
        let Some(Modal::Form(mut form)) = app.modal.take() else {
            panic!("form");
        };
        form.fields[0].value = source.to_string_lossy().into_owned();
        form.dirty = true;
        app.submit_form(form);
        assert!(matches!(app.modal, Some(Modal::Attachments(_))));
        let attachments = app.client.list_attachments("task", &task_id).unwrap();
        assert_eq!(attachments.len(), 1);

        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE).unwrap();
        assert!(matches!(app.modal, Some(Modal::Confirm { .. })));
        handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE).unwrap();
        app.open_attachments("task".into(), task_id.clone(), "Attach".into());
        let Some(Modal::Attachments(state)) = &app.modal else {
            panic!("attachments modal");
        };
        assert!(state.items.is_empty());
        let _ = std::fs::remove_file(&source);
    }

    #[test]
    fn first_run_banner_and_action_bar_are_visible() {
        let mut app = app();
        let text = buffer_text(&mut app, 120, 30);
        assert!(text.contains("Getting started"), "{text}");
        assert!(text.contains("Basic is the default workflow"), "{text}");
        submit_project(&mut app, "Guided");
        let text = buffer_text(&mut app, 120, 30);
        assert!(text.contains("Actions:"), "{text}");
        assert!(text.contains("more actions"), "{text}");
    }

    /// Session-state tests mutate a process-global environment variable, so they
    /// must not run concurrently with each other.
    static STATE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn state_env_lock() -> std::sync::MutexGuard<'static, ()> {
        STATE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn temp_path(stem: &str, extension: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-tui-{stem}-{}-{}.{extension}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn session_app(database: &std::path::Path) -> App {
        App::open(database.to_string_lossy().into_owned()).unwrap()
    }

    /// Create a task directly (the form path deduplicates rapid submissions)
    /// and refresh the active project view.
    fn create_task(app: &mut App, project_id: &str, title: &str) -> String {
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: title.into(),
                project_id: Some(project_id.to_owned()),
                milestone_id: None,
            })
            .unwrap();
        app.refresh_current();
        task.id
    }

    /// Position of a stable id inside one view, as `(component, row)`.
    fn locate_selection(view: &orbit_ui::ResolvedView, id: &str) -> Option<(usize, usize)> {
        view.components
            .iter()
            .enumerate()
            .find_map(|(component, item)| {
                item.selection
                    .iter()
                    .position(|row| row.id == id)
                    .map(|row| (component, row))
            })
    }

    /// Position of a stable id across a project's views, as
    /// `(view, component, row)`.
    fn locate_project_selection(app: &App, id: &str) -> Option<(usize, usize, usize)> {
        let project = app.project.as_ref()?;
        project
            .views
            .iter()
            .enumerate()
            .find_map(|(view, resolved)| {
                locate_selection(resolved, id).map(|(component, row)| (view, component, row))
            })
    }

    #[test]
    fn session_state_round_trips_active_project_and_view() {
        let _guard = state_env_lock();
        let state_file = temp_path("state", "json");
        let database = temp_path("session", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let mut app = session_app(&database);
        let project_id = submit_project(&mut app, "Session");
        create_task(&mut app, &project_id, "First");
        let target = create_task(&mut app, &project_id, "Target");
        let (view_index, component_index, row) =
            locate_project_selection(&app, &target).expect("selectable task row");
        app.switch_view_by_index(view_index);
        app.active_component = component_index;
        app.selected = row;
        app.scroll = row;
        app.detail_scroll = 3;
        app.focus = FocusPane::Detail;
        let expected_view = app.current_view().map(|view| view.id.clone());
        assert!(expected_view.is_some(), "project view resolved");

        ui_state::save(&app).unwrap();
        let loaded = ui_state::load().expect("state file");
        assert_eq!(loaded.project_id.as_deref(), Some(project_id.as_str()));
        assert_eq!(loaded.view_id, expected_view);
        assert_eq!(loaded.selected_id.as_deref(), Some(target.as_str()));
        assert_eq!(loaded.selected_index, Some(row));
        assert_eq!(loaded.active_component, Some(component_index));
        assert_eq!(loaded.scroll, Some(row));
        assert_eq!(loaded.detail_scroll, Some(3));
        assert_eq!(loaded.focus.as_deref(), Some("detail"));

        let mut restored = session_app(&database);
        ui_state::restore(&mut restored);
        assert_eq!(restored.screen, Screen::Project);
        assert_eq!(
            restored.project.as_ref().map(|project| project.id.clone()),
            Some(project_id)
        );
        assert_eq!(
            restored.current_view().map(|view| view.id.clone()),
            expected_view
        );
        assert_eq!(
            restored.focus,
            FocusPane::Detail,
            "focus must survive project restore"
        );
        assert_eq!(restored.active_component, component_index);
        assert_eq!(
            restored.current_selected_id().as_deref(),
            Some(target.as_str())
        );
        assert_eq!(restored.selected, row);
        assert_eq!(restored.scroll, row);
        assert_eq!(restored.detail_scroll, 3);

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    /// Regression: opening the project resets focus, so restore must apply the
    /// saved focus afterwards.
    #[test]
    fn session_state_focus_survives_project_restore() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-focus", "json");
        let database = temp_path("session-focus", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let mut app = session_app(&database);
        submit_project(&mut app, "Focus");
        app.focus = FocusPane::Sidebar;
        ui_state::save(&app).unwrap();

        let mut restored = session_app(&database);
        ui_state::restore(&mut restored);
        assert_eq!(restored.screen, Screen::Project);
        assert_eq!(restored.focus, FocusPane::Sidebar);

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn session_state_restores_selection_by_id_over_a_stale_index() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-reorder", "json");
        let database = temp_path("session-reorder", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let mut app = session_app(&database);
        let project_id = submit_project(&mut app, "Reorder");
        create_task(&mut app, &project_id, "First");
        create_task(&mut app, &project_id, "Second");
        let target = create_task(&mut app, &project_id, "Third");
        let (view_index, component_index, row) =
            locate_project_selection(&app, &target).expect("selectable task row");
        assert!(row > 0, "target is not the first row: {row}");
        let view_id = app
            .project
            .as_ref()
            .and_then(|project| project.views.get(view_index))
            .map(|view| view.id.clone())
            .expect("view id");

        // A stale index (rows reordered since the save) must not win over the id.
        std::fs::write(
            &state_file,
            format!(
                r#"{{"project_id":"{project_id}","view_id":"{view_id}","active_component":{component_index},"selected_id":"{target}","selected_index":0}}"#
            ),
        )
        .unwrap();

        let mut restored = session_app(&database);
        ui_state::restore(&mut restored);
        assert_eq!(
            restored.current_view().map(|view| view.id.clone()),
            Some(view_id)
        );
        assert_eq!(restored.active_component, component_index);
        assert_eq!(
            restored.current_selected_id().as_deref(),
            Some(target.as_str())
        );
        assert_eq!(restored.selected, row, "the id resolves to its new index");

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn session_state_clamps_when_the_selected_item_is_gone() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-gone", "json");
        let database = temp_path("session-gone", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let mut app = session_app(&database);
        let project_id = submit_project(&mut app, "Gone");
        create_task(&mut app, &project_id, "First");
        let target = create_task(&mut app, &project_id, "Second");
        let (view_index, component_index, _) =
            locate_project_selection(&app, &target).expect("selectable task row");
        let view_id = app
            .project
            .as_ref()
            .and_then(|project| project.views.get(view_index))
            .map(|view| view.id.clone())
            .expect("view id");
        std::fs::write(
            &state_file,
            format!(
                r#"{{"project_id":"{project_id}","view_id":"{view_id}","active_component":{component_index},"selected_id":"deleted-item","selected_index":99}}"#
            ),
        )
        .unwrap();

        let mut restored = session_app(&database);
        ui_state::restore(&mut restored);
        let len = restored
            .current_component()
            .map(|component| component.selection.len())
            .unwrap_or(0);
        assert!(len > 0, "the view still has rows");
        assert_eq!(
            restored.selected,
            len - 1,
            "a missing id falls back to the clamped index"
        );
        let selected = restored.selected_item().expect("a valid row is selected");
        assert_ne!(selected.id, "deleted-item");

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn session_state_falls_back_to_view_zero_when_the_view_is_gone() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-view", "json");
        let database = temp_path("session-view", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let mut app = session_app(&database);
        let project_id = submit_project(&mut app, "Views");
        let target = create_task(&mut app, &project_id, "First");
        let first_view = app
            .project
            .as_ref()
            .and_then(|project| project.views.first())
            .map(|view| view.id.clone())
            .expect("first view");
        std::fs::write(
            &state_file,
            format!(
                r#"{{"project_id":"{project_id}","view_id":"retired-view","active_component":0,"selected_id":"{target}","selected_index":0}}"#
            ),
        )
        .unwrap();

        let mut restored = session_app(&database);
        ui_state::restore(&mut restored);
        assert_eq!(
            restored.current_view().map(|view| view.id.clone()),
            Some(first_view),
            "a missing view id lands on view 0"
        );
        let len = restored
            .current_component()
            .map(|component| component.selection.len())
            .unwrap_or(0);
        assert!(restored.selected < len.max(1), "selection stays valid");

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn session_state_restores_non_project_screen_and_selection() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-home", "json");
        let database = temp_path("session-home", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let mut app = session_app(&database);
        let project_id = submit_project(&mut app, "Home");
        create_task(&mut app, &project_id, "One");
        let target = create_task(&mut app, &project_id, "Two");
        app.go_screen(Screen::Home);
        app.refresh_workspace();
        let (component_index, row) =
            locate_selection(app.current_view().expect("home view"), &target).expect("home row");
        app.active_component = component_index;
        app.selected = row;
        app.focus = FocusPane::Sidebar;
        ui_state::save(&app).unwrap();

        let mut restored = session_app(&database);
        ui_state::restore(&mut restored);
        assert_eq!(restored.screen, Screen::Home);
        assert_eq!(restored.focus, FocusPane::Sidebar);
        assert_eq!(restored.active_component, component_index);
        assert_eq!(
            restored.current_selected_id().as_deref(),
            Some(target.as_str())
        );

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn session_state_ignores_corrupt_and_partial_files() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-corrupt", "json");
        let database = temp_path("session-corrupt", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        std::fs::write(&state_file, "{not json").unwrap();
        assert!(ui_state::load().is_none(), "corrupt state is ignored");
        let mut app = session_app(&database);
        ui_state::restore(&mut app);
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.selected, 0);

        std::fs::write(&state_file, r#"{"project_id":"missing-project"}"#).unwrap();
        let partial = ui_state::load().expect("partial state loads");
        assert_eq!(partial.project_id.as_deref(), Some("missing-project"));
        assert_eq!(partial.screen, None);
        assert_eq!(partial.selected_id, None);
        ui_state::restore(&mut app);
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.selected, 0);

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn session_state_ignores_unknown_screen_and_focus_names() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-unknown", "json");
        let database = temp_path("session-unknown", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let project_id = {
            let mut app = session_app(&database);
            submit_project(&mut app, "Unknown")
        };
        std::fs::write(
            &state_file,
            format!(r#"{{"screen":"nowhere","project_id":"{project_id}","focus":"ceiling"}}"#),
        )
        .unwrap();
        let mut app = session_app(&database);
        ui_state::restore(&mut app);
        // The unknown screen name is ignored; the saved project still restores
        // and the unknown focus name falls back to the default pane.
        assert_eq!(app.screen, Screen::Project);
        assert_eq!(
            app.project.as_ref().map(|project| project.id.clone()),
            Some(project_id)
        );
        assert_eq!(app.focus, FocusPane::Workspace);

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn session_state_clamps_sidebar_and_projects_selections() {
        let _guard = state_env_lock();
        let state_file = temp_path("state-clamp", "json");
        let database = temp_path("session-clamp", "sqlite");
        let _ = std::fs::remove_file(&database);
        // SAFETY: the module lock keeps this variable private to the test.
        unsafe { std::env::set_var("ORBIT_TUI_STATE", &state_file) };

        let mut app = session_app(&database);
        submit_project(&mut app, "Clamp");
        std::fs::write(
            &state_file,
            r#"{"screen":"projects","focus":"sidebar","sidebar_selection":99,"projects_selection":99}"#,
        )
        .unwrap();

        let mut restored = session_app(&database);
        ui_state::restore(&mut restored);
        assert_eq!(restored.screen, Screen::Projects);
        assert_eq!(restored.focus, FocusPane::Sidebar);
        assert_eq!(restored.sidebar_selection, restored.sidebar_len() - 1);
        assert_eq!(restored.projects_selection, restored.projects.len() - 1);

        unsafe { std::env::remove_var("ORBIT_TUI_STATE") };
        let _ = std::fs::remove_file(&state_file);
    }

    #[test]
    fn selection_restore_tolerates_empty_views_and_missing_components() {
        let mut app = app();
        app.screen = Screen::Home;
        app.home.components.clear();
        app.restore_selection(Some("missing"), Some(5), Some(5), Some(3), Some(2));
        assert_eq!(app.active_component, 0);
        assert_eq!(app.selected, 0);
        assert_eq!(app.detail_scroll, 2);

        app.home.components = vec![orbit_ui::ResolvedComponent {
            id: "empty".into(),
            title: "Empty".into(),
            component_type: "list".into(),
            status: orbit_ui::LoadStatus::Empty,
            body: orbit_ui::ComponentBody::List { items: Vec::new() },
            selection: Vec::new(),
        }];
        app.restore_selection(Some("missing"), Some(5), Some(2), Some(1), None);
        assert_eq!(app.active_component, 0);
        assert_eq!(app.selected, 0);
        assert_eq!(app.detail_scroll, 2);
        assert_eq!(app.scroll, 1);
    }
}
