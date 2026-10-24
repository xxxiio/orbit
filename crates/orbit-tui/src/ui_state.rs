//! Durable TUI session state.
//!
//! Only navigation intent is persisted: the active project, view, screen,
//! focus, component and row selections plus scroll offsets. Domain state always
//! comes from the authority. A corrupt or stale file is ignored rather than
//! blocking startup, and a project or view that no longer exists is skipped.
use crate::app::{App, FocusPane, Screen};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Schema version for the session file. Older files default to `0` and are read
/// with every field optional; unknown future fields are ignored.
pub const UI_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiState {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub screen: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub view_id: Option<String>,
    #[serde(default)]
    pub focus: Option<String>,
    #[serde(default)]
    pub sidebar_selection: Option<usize>,
    #[serde(default)]
    pub projects_selection: Option<usize>,
    #[serde(default)]
    pub active_component: Option<usize>,
    #[serde(default)]
    pub selected_id: Option<String>,
    #[serde(default)]
    pub selected_index: Option<usize>,
    #[serde(default)]
    pub scroll: Option<usize>,
    #[serde(default)]
    pub detail_scroll: Option<usize>,
}

/// State file location. `ORBIT_TUI_STATE` overrides it for tests and isolated
/// sessions; otherwise it lives beside the deployment state directory.
pub fn state_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("ORBIT_TUI_STATE")
        && !explicit.trim().is_empty()
    {
        return Some(PathBuf::from(explicit));
    }
    let paths = orbit_client::config::OrbitPaths::discover(None).ok()?;
    Some(paths.state_dir.join("tui-state.json"))
}

fn screen_name(screen: Screen) -> &'static str {
    match screen {
        Screen::Home => "home",
        Screen::Inbox => "inbox",
        Screen::Projects => "projects",
        Screen::Agents => "agents",
        Screen::Project => "project",
    }
}

fn screen_from_name(name: &str) -> Option<Screen> {
    match name {
        "home" => Some(Screen::Home),
        "inbox" => Some(Screen::Inbox),
        "projects" => Some(Screen::Projects),
        "agents" => Some(Screen::Agents),
        "project" => Some(Screen::Project),
        _ => None,
    }
}

fn focus_name(focus: FocusPane) -> &'static str {
    match focus {
        FocusPane::Sidebar => "sidebar",
        FocusPane::Workspace => "workspace",
        FocusPane::Detail => "detail",
    }
}

fn focus_from_name(name: &str) -> Option<FocusPane> {
    match name {
        "sidebar" => Some(FocusPane::Sidebar),
        "workspace" => Some(FocusPane::Workspace),
        "detail" => Some(FocusPane::Detail),
        _ => None,
    }
}

/// Best-effort load. Never fails the application.
pub fn load() -> Option<UiState> {
    let path = state_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Persist the current navigation intent. Errors are reported to the caller so
/// the shell can surface a warning without losing the session.
pub fn save(app: &App) -> Result<(), String> {
    let Some(path) = state_path() else {
        return Ok(());
    };
    let state = UiState {
        version: UI_STATE_VERSION,
        screen: Some(screen_name(app.screen).to_owned()),
        project_id: app.project.as_ref().map(|project| project.id.clone()),
        view_id: app
            .project
            .as_ref()
            .and_then(|project| project.views.get(project.active_view))
            .map(|view| view.id.clone()),
        focus: Some(focus_name(app.focus).to_owned()),
        sidebar_selection: Some(app.sidebar_selection),
        projects_selection: Some(app.projects_selection),
        active_component: Some(app.active_component),
        selected_id: app.current_selected_id(),
        selected_index: Some(app.selected),
        scroll: Some(app.scroll),
        detail_scroll: Some(app.detail_scroll),
    };
    let serialized = serde_json::to_string_pretty(&state).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&path, serialized).map_err(|error| error.to_string())
}

/// Restore navigation intent after the workspace has loaded. Missing projects
/// or views are ignored so an external deletion never traps the user.
///
/// Ordering matters: the project and view are restored first because opening a
/// project resets focus and selection, then the saved focus, component,
/// selection and scroll offsets are applied.
pub fn restore(app: &mut App) {
    let Some(state) = load() else {
        return;
    };
    // Unknown screen names are ignored: the project (if any) still restores and
    // the shell otherwise starts normally.
    let screen = state.screen.as_deref().and_then(screen_from_name);
    let focus = state.focus.as_deref().and_then(focus_from_name);
    let project_id = state.project_id.as_deref();
    let wants_project = matches!(screen, None | Some(Screen::Project));
    if wants_project
        && let Some(project_id) = project_id
        && app.project_exists(project_id)
    {
        app.open_project_restored(project_id, state.view_id.as_deref());
        if let Some(focus) = focus {
            app.focus = focus;
        }
    } else if let Some(screen) = screen {
        // A saved project whose authority record is gone falls back to the list.
        let screen = if screen == Screen::Project {
            Screen::Projects
        } else {
            screen
        };
        app.go_screen(screen);
        if let Some(focus) = focus {
            app.focus = focus;
        }
    }
    if let Some(index) = state.sidebar_selection {
        app.sidebar_selection = index.min(app.sidebar_len().saturating_sub(1));
    }
    if let Some(index) = state.projects_selection {
        app.projects_selection = index.min(app.projects.len().saturating_sub(1));
    }
    app.restore_selection(
        state.selected_id.as_deref(),
        state.selected_index,
        state.active_component,
        state.scroll,
        state.detail_scroll,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_and_focus_names_round_trip() {
        for screen in [
            Screen::Home,
            Screen::Inbox,
            Screen::Projects,
            Screen::Agents,
            Screen::Project,
        ] {
            assert_eq!(screen_from_name(screen_name(screen)), Some(screen));
        }
        for focus in [FocusPane::Sidebar, FocusPane::Workspace, FocusPane::Detail] {
            assert_eq!(focus_from_name(focus_name(focus)), Some(focus));
        }
        assert_eq!(screen_from_name("unknown"), None);
    }

    #[test]
    fn partial_state_uses_defaults_and_ignores_unknown_fields() {
        let state: UiState = serde_json::from_str(
            r#"{"project_id":"p1","selected_id":"t1","screen":"nope","focus":"nope","future":1}"#,
        )
        .unwrap();
        assert_eq!(state.version, 0);
        assert_eq!(state.project_id.as_deref(), Some("p1"));
        assert_eq!(state.selected_id.as_deref(), Some("t1"));
        assert_eq!(screen_from_name(state.screen.as_deref().unwrap()), None);
        assert_eq!(focus_from_name(state.focus.as_deref().unwrap()), None);
        assert_eq!(state.active_component, None);
        assert_eq!(state.selected_index, None);
        assert_eq!(state.scroll, None);
        assert_eq!(state.detail_scroll, None);
    }

    #[test]
    fn state_round_trips_every_navigation_field() {
        let state = UiState {
            version: UI_STATE_VERSION,
            screen: Some("project".into()),
            project_id: Some("p1".into()),
            view_id: Some("work".into()),
            focus: Some("detail".into()),
            sidebar_selection: Some(5),
            projects_selection: Some(2),
            active_component: Some(3),
            selected_id: Some("t9".into()),
            selected_index: Some(7),
            scroll: Some(4),
            detail_scroll: Some(11),
        };
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: UiState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.version, UI_STATE_VERSION);
        assert_eq!(decoded.project_id, state.project_id);
        assert_eq!(decoded.view_id, state.view_id);
        assert_eq!(decoded.active_component, state.active_component);
        assert_eq!(decoded.selected_id, state.selected_id);
        assert_eq!(decoded.selected_index, state.selected_index);
        assert_eq!(decoded.scroll, state.scroll);
        assert_eq!(decoded.detail_scroll, state.detail_scroll);
    }
}
