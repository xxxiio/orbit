use crate::app::{self, App, Modal, Screen, StatusKind};
use crate::input::handle_modal;
use crossterm::event::{KeyCode, KeyModifiers};

pub(crate) fn handle_key(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Result<bool, String> {
    if app.modal.is_none() {
        app.modal_scroll = 0;
    }
    if app.modal.is_some() {
        let quit = handle_modal(app, code, modifiers)?;
        // A follow-up modal launched from the inspector may have closed (saved,
        // cancelled or confirmed). Reopen the inspector only for its recorded
        // subject, never by guessing from the current selection.
        app.resume_pending_inspector();
        return Ok(quit);
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
        KeyCode::Char('!') if app.client.is_connected() => app.open_pending(),
        KeyCode::Char('r') => app.refresh_current(),
        KeyCode::Char('f') if app.client.is_connected() => {
            let action =
                orbit_ui::AvailableAction::enabled("flush_pending", "Flush pending commands", None);
            app.execute_action(action);
        }
        // `D` opens the inspector. While it is open, key dispatch routes to the
        // inspector handler, where `D` closes it again.
        KeyCode::Char('D') => {
            app.open_inspector();
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
        KeyCode::Enter => {
            // The navigation rail always wins while it owns the keyboard. The
            // Projects screen previously took Enter unconditionally, so moving
            // the sidebar selection to Home/Inbox/Agents/project and pressing
            // Enter appeared to do nothing and the workspace kept showing the
            // project list.
            if app.focus == app::FocusPane::Sidebar {
                app.activate_sidebar();
            } else if app.screen == Screen::Projects {
                app.activate_project_selection();
            } else {
                app.open_inspector();
            }
        }
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
        KeyCode::Char('E') => app.quick_edit_selected(),
        KeyCode::Char('S') => app.open_status_menu(),
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
