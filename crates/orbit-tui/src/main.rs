//! Orbit terminal client. The shell is project-centric; feature surfaces are
//! blueprint-defined views resolved through shared `orbit-ui` contracts.
mod app;
mod connected;
mod connected_actions;
mod planning_ui;
mod render;
mod terminal;
mod theme;

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
    let path = match orbit_client::deployment::from_env().map_err(|error| error.to_string())? {
        orbit_client::deployment::Deployment::Connected { replica, server } => {
            return connected::run(replica.to_string_lossy().into_owned(), server);
        }
        orbit_client::deployment::Deployment::Local { database } => database,
    };
    let mut app = App::open(path.to_string_lossy().into_owned())?;
    let mut session = terminal::TerminalSession::enter()?;
    let result = event_loop(session.terminal(), &mut app);
    session.restore();
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
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
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Result<bool, String> {
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
            let metric_id = app.selected_item().map(|item| item.id.clone());
            app.start_form(app::FormMode::MetricObservation { metric_id });
        }
        KeyCode::Char('t') => {
            let metric_id = app.selected_item().map(|item| item.id.clone());
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

fn handle_modal(app: &mut App, code: KeyCode, _modifiers: KeyModifiers) -> Result<bool, String> {
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
        )
    {
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
        Modal::Form(form) => match code {
            KeyCode::Esc => {
                keep = false;
                app.notify("Form cancelled", StatusKind::Info);
            }
            KeyCode::Enter => {
                let form = form.clone();
                keep = false;
                app.submit_form(form);
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
                                .unwrap_or(0) as isize;
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
                        _ => {}
                    }
                }
            }
            KeyCode::Char(' ') => {
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
                        FormFieldKind::Text => {
                            field.value.push(' ');
                            form.dirty = true;
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(field) = form.fields.get_mut(form.focused)
                    && matches!(field.kind, FormFieldKind::Text | FormFieldKind::Integer)
                {
                    field.value.pop();
                    form.dirty = true;
                }
            }
            KeyCode::Char(character) => {
                if let Some(field) = form.fields.get_mut(form.focused)
                    && matches!(field.kind, FormFieldKind::Text | FormFieldKind::Integer)
                {
                    field.value.push(character);
                    form.dirty = true;
                }
            }
            _ => {}
        },
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
                app.notify("Cancelled; no change was submitted", StatusKind::Info);
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
        Modal::Help => {
            if matches!(code, KeyCode::Esc | KeyCode::Char('?')) {
                keep = false;
            }
        }
        Modal::Detail(_) => {
            if matches!(code, KeyCode::Esc | KeyCode::Enter) {
                keep = false;
            }
        }
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
        Modal::Message { .. } => {
            if matches!(code, KeyCode::Esc | KeyCode::Enter) {
                keep = false;
            }
        }
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
    fn project_creation_opens_generated_legacy_views() {
        let mut app = app();
        submit_project(&mut app, "Apollo");
        assert_eq!(app.screen, Screen::Project);
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.name, "Apollo");
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
            .bind_project_blueprint(&project_id, "research", 2)
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
            .bind_project_blueprint(&project.id, "development", 2)
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
        for column in ["Inbox", "Ready", "Review", "Done"] {
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
        assert!(
            rendered.contains("previous view remains visible"),
            "{rendered}"
        );
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
                columns: vec!["title".into(), "status".into()],
                rows: vec![orbit_ui::model::TableRowModel {
                    id: "t1".into(),
                    kind: orbit_ui::ItemKind::Task,
                    status: "ready".into(),
                    cells: vec![
                        "A long task title that should be truncated".into(),
                        "ready".into(),
                    ],
                }],
            },
            selection: Vec::new(),
        }];
        app.focus = FocusPane::Workspace;
        let wide = text(&mut app, 120, 24);
        assert!(wide.contains("TITLE"), "{wide}");
        assert!(wide.contains("A long task title"), "{wide}");
        let narrow = text(&mut app, 60, 16);
        assert!(narrow.contains("TITLE"), "{narrow}");
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
            .bind_project_blueprint(&project, "development", 2)
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
            .bind_project_blueprint(&project, "development", 2)
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
            .bind_project_blueprint(&project, "research", 2)
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
        assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());

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
            .bind_project_blueprint(&project, "research", 2)
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
        assert_eq!(form.fields[3].value, "not-a-number");
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
