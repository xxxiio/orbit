//! Durable TUI session state.
//!
//! Only navigation intent is persisted: the active project, view, screen and
//! selections. Domain state always comes from the authority. A corrupt or
//! stale file is ignored rather than blocking startup, and a project or view
//! that no longer exists is skipped.
use crate::app::{App, FocusPane, Screen};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiState {
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
    pub selected_id: Option<String>,
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
        selected_id: None,
    };
    let serialized = serde_json::to_string_pretty(&state).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&path, serialized).map_err(|error| error.to_string())
}

/// Restore navigation intent after the workspace has loaded. Missing projects
/// or views are ignored so an external deletion never traps the user.
pub fn restore(app: &mut App) {
    let Some(state) = load() else {
        return;
    };
    if let Some(focus) = state.focus.as_deref().and_then(focus_from_name) {
        app.focus = focus;
    }
    if let Some(index) = state.sidebar_selection {
        app.sidebar_selection = index;
    }
    if let Some(index) = state.projects_selection {
        app.projects_selection = index;
    }
    let project_id = state.project_id.clone();
    let view_id = state.view_id.clone();
    let screen = state.screen.as_deref().and_then(screen_from_name);
    if let Some(project_id) = project_id
        && app.project_exists(&project_id)
    {
        app.open_project_restored(&project_id, view_id.as_deref());
        return;
    }
    if let Some(screen) = screen {
        app.go_screen(screen);
    }
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
}
