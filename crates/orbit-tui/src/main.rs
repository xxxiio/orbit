//! Orbit terminal client. The shell is project-centric; feature surfaces are
//! blueprint-defined views resolved through shared `orbit-ui` contracts.
mod app;
mod connected;
mod connected_actions;
mod planning_ui;
mod render;
mod theme;

use app::{App, Modal, Screen, StatusKind};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;

fn main() {
    if let Err(error) = run() {
        eprintln!("orbit-tui: {error}");
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
    enable_raw_mode().map_err(err)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(err)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(err)?;
    let result = event_loop(&mut terminal, &mut app);
    disable_raw_mode().map_err(err)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(err)?;
    terminal.show_cursor().map_err(err)?;
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
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            break;
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
            if app.screen == Screen::Home {
                return Ok(true);
            }
            app.go_screen(Screen::Home);
        }
        KeyCode::Esc => {
            if app.screen != Screen::Home {
                app.go_screen(Screen::Home);
            } else {
                app.notify("Already Home", StatusKind::Info);
            }
        }
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
        KeyCode::Right | KeyCode::Char('l') => app.move_component(1),
        KeyCode::Left | KeyCode::Char('h') => app.move_component(-1),
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
        KeyCode::Char('c') => {
            if app.screen == Screen::Projects {
                app.start_prompt(app::PromptKind::NewProject);
            } else {
                app.start_prompt(app::PromptKind::NewTask {
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
                app.start_prompt(app::PromptKind::NewWorkItem {
                    project_id: project.id.clone(),
                });
            } else {
                app.notify(
                    "Open a project before creating a work item",
                    StatusKind::Warning,
                );
            }
        }
        KeyCode::Char('n') => app.start_prompt(app::PromptKind::NewKnowledge),
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
                app.start_prompt(app::PromptKind::BindEvidence {
                    project_id: project.id.clone(),
                    unbind: false,
                });
            } else {
                app.notify("Open a project first", StatusKind::Warning);
            }
        }
        KeyCode::Char('o') => {
            let metric_id = app.selected_item().map(|item| item.id.clone());
            app.start_prompt(app::PromptKind::MetricObservation { metric_id });
        }
        KeyCode::Char('t') => {
            let metric_id = app.selected_item().map(|item| item.id.clone());
            app.start_prompt(app::PromptKind::MetricTarget { metric_id });
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
    let mut keep = true;
    match &mut modal {
        Modal::Prompt(prompt) => match code {
            KeyCode::Esc => {
                keep = false;
                app.notify("Prompt cancelled", StatusKind::Info);
            }
            KeyCode::Enter => {
                let prompt = prompt.clone();
                keep = false;
                app.submit_prompt(prompt);
            }
            KeyCode::Backspace => {
                prompt.value.pop();
            }
            KeyCode::Char(character) => prompt.value.push(character),
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
                if let Some(action) = action
                    && app.execute_palette_action(action)
                {
                    return Ok(true);
                }
            }
            _ => {}
        },
        Modal::Help => {
            if matches!(code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
                keep = false;
            }
        }
        Modal::Detail(_) => {
            if matches!(code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
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
            if matches!(code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
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
    use app::{FocusPane, Modal, PaletteAction, PendingAction, PromptKind};
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

    fn submit(app: &mut App, kind: PromptKind, value: &str) {
        app.start_prompt(kind);
        let Some(Modal::Prompt(mut prompt)) = app.modal.take() else {
            panic!("expected prompt");
        };
        prompt.value = value.into();
        app.submit_prompt(prompt);
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
        submit(&mut app, PromptKind::NewProject, "Apollo");
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
        submit(&mut app, PromptKind::NewProject, "Research");
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
        submit(&mut app, PromptKind::NewProject, "Apollo");
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap();
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
        submit(&mut app, PromptKind::NewProject, "Habits");
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
    fn quit_returns_home_before_leaving() {
        let mut app = app();
        submit(&mut app, PromptKind::NewProject, "Apollo");
        assert!(!handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
        assert_eq!(app.screen, Screen::Home);
        assert!(handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap());
    }

    #[test]
    fn palette_actions_share_the_contextual_catalogue() {
        let mut app = app();
        submit(&mut app, PromptKind::NewProject, "Apollo");
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
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::NONE).unwrap();
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
