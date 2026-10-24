//! Orbit terminal client. One project-centric shell serves local and connected
//! deployments; blueprint views resolve through shared `orbit-ui` contracts.
mod app;
mod backend;
mod connected_actions;
mod connected_ops;
mod input;
mod planning_ui;
mod render;
mod terminal;
mod theme;
mod ui_state;
mod viewer;

use app::{App, StatusKind};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use input::handle_key;
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

/// Flush pending navigation state once. A failed write stays pending until the
/// next navigation change so a broken state directory cannot warn on every tick.
fn flush_navigation(app: &mut App, saved_signature: &mut String, dirty: &mut bool) {
    if !*dirty {
        return;
    }
    let signature = navigation_signature(app);
    match ui_state::save(app) {
        Ok(()) => {
            *dirty = false;
            *saved_signature = signature;
        }
        Err(error) => {
            *dirty = false;
            app.notify(
                format!("Session state not saved: {error}"),
                StatusKind::Warning,
            );
        }
    }
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    // Writes are debounced: a signature change only marks the session dirty and
    // the idle poll timeout (or exit) flushes it, so held keys and bursts of
    // navigation cost one write instead of one per keypress.
    let mut saved_signature = navigation_signature(app);
    let mut dirty = false;
    loop {
        terminal
            .draw(|frame| render::draw(frame, app))
            .map_err(err)?;
        if !event::poll(Duration::from_millis(200)).map_err(err)? {
            flush_navigation(app, &mut saved_signature, &mut dirty);
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
        if navigation_signature(app) != saved_signature {
            dirty = true;
        }
    }
    flush_navigation(app, &mut saved_signature, &mut dirty);
    Ok(())
}

fn err<E: std::fmt::Debug>(error: E) -> String {
    format!("{error:?}")
}

#[cfg(test)]
#[path = "tests/m28_focus_scroll_tests.rs"]
mod m28_focus_scroll_tests;
#[cfg(test)]
#[path = "tests/m28_form_tests.rs"]
mod m28_form_tests;
#[cfg(test)]
#[path = "tests/m28_gate2_fix_tests.rs"]
mod m28_gate2_fix_tests;
#[cfg(test)]
#[path = "tests/m28_layout_tests.rs"]
mod m28_layout_tests;
#[cfg(test)]
#[path = "tests/m28_navigation_tests.rs"]
mod m28_navigation_tests;
#[cfg(test)]
#[path = "tests/m28_oracle_remediation_tests.rs"]
mod m28_oracle_remediation_tests;
#[cfg(test)]
#[path = "tests/m28_planning_tests.rs"]
mod m28_planning_tests;
#[cfg(test)]
#[path = "tests/m28_post_gate_tests.rs"]
mod m28_post_gate_tests;
#[cfg(test)]
#[path = "tests/m28_quit_tests.rs"]
mod m28_quit_tests;
#[cfg(test)]
#[path = "tests/m28_render_tests.rs"]
mod m28_render_tests;
#[cfg(test)]
#[path = "tests/m28_scroll_quit_edges.rs"]
mod m28_scroll_quit_edges;
#[cfg(test)]
#[path = "tests/m28_scroll_tests.rs"]
mod m28_scroll_tests;
#[cfg(test)]
#[path = "tests/m28_task_scope_tests.rs"]
mod m28_task_scope_tests;
#[cfg(test)]
#[path = "tests/m28_unified_connected_tests.rs"]
mod m28_unified_connected_tests;
#[cfg(test)]
#[path = "tests/m28_usability_recovery_tests.rs"]
mod m28_usability_recovery_tests;
#[cfg(test)]
#[path = "tests/m29_connected_task_detail_tests.rs"]
mod m29_connected_task_detail_tests;
#[cfg(test)]
#[path = "tests/m29_layout_presentation_tests.rs"]
mod m29_layout_presentation_tests;
#[cfg(test)]
#[path = "tests/m29_perf_render_tests.rs"]
mod m29_perf_render_tests;
#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;
