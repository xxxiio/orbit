//! Frame chrome: header, body regions, sidebar, workspace panes, action bar,
//! status line and the bordered blocks the rest of the renderer draws into.
use super::components::{draw_components, draw_projects_screen, render_selectable_list};
use super::layout::shell_plan;
use crate::app::{App, FocusPane, Screen, StatusKind};
use crate::theme::MEDIUM_BREAKPOINT;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, ListItem, Paragraph, Wrap},
};

pub(super) fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let segment = match app.screen {
        Screen::Home => "Home / Today",
        Screen::Inbox => "Inbox",
        Screen::Projects => "Projects",
        Screen::Agents => "Agents",
        Screen::Project => app
            .project
            .as_ref()
            .map(|project| project.name.as_str())
            .unwrap_or("Project"),
    };
    let mut spans = vec![
        Span::styled(" ORBIT ", app.theme.accent()),
        Span::styled("› ", app.theme.muted()),
        Span::styled(segment.to_owned(), app.theme.title()),
    ];
    if app.screen == Screen::Project
        && let Some(project) = &app.project
    {
        spans.push(Span::styled("  ", app.theme.muted()));
        spans.push(Span::styled(
            project.blueprint_label.clone(),
            app.theme.muted(),
        ));
        if project.stale_error.is_some() {
            spans.push(Span::styled("  [stale]", app.theme.warning()));
        }
    }
    // The status summary is the first thing to go: it must never push the
    // breadcrumb off the row.
    let summary = if app.client.is_connected() {
        Some((
            format!(
                "   CONNECTED{}{} · {} projects · {} pending · ! inspect",
                if app.workspace_error.is_some() {
                    " [offline/stale]"
                } else {
                    ""
                },
                "",
                app.projects.len(),
                // The count is cached on `App` and refreshed by queue, flush,
                // refresh and inspector events; render never calls the backend.
                app.pending_count()
            ),
            app.theme.warning(),
        ))
    } else {
        app.workspace.as_ref().map(|workspace| {
            (
                format!(
                    "   {} projects · {} inbox{}",
                    workspace.projects.len(),
                    workspace.inbox.len(),
                    if workspace.pending_commands > 0 {
                        format!(" · {} pending", workspace.pending_commands)
                    } else {
                        String::new()
                    }
                ),
                app.theme.muted(),
            )
        })
    };
    if let Some((summary, style)) = summary {
        let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
        if used + summary.chars().count() <= usize::from(area.width) {
            spans.push(Span::styled(summary, style));
        }
    }
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), area);
}
pub(super) fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    // First-run guidance: never present an unexplained empty dashboard.
    if app.projects.is_empty() {
        let banner = Line::from(vec![
            Span::styled(" Getting started: ", app.theme.title()),
            Span::styled(
                "press g p, then c, to create your first project. Basic is the default workflow; every project ships with Overview, Tasks, Board and Timeline views.",
                app.theme.text(),
            ),
        ]);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(3)])
            .split(area);
        frame.render_widget(Paragraph::new(banner).wrap(Wrap { trim: false }), rows[0]);
        draw_body_regions(frame, rows[1], app);
        return;
    }
    draw_body_regions(frame, area, app);
}

fn draw_body_regions(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < MEDIUM_BREAKPOINT {
        draw_compact_body(frame, area, app);
        return;
    }
    let plan = shell_plan(area.width, app);
    // Exactly two regions: sidebar + workspace. The workspace takes every
    // column the sidebar does not claim, at every terminal width.
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(plan.sidebar),
            Constraint::Min(plan.workspace_floor.max(1)),
        ])
        .split(area);
    draw_sidebar(frame, chunks[0], app);
    draw_main(frame, chunks[1], app);
}
/// Narrow terminals keep one pane at a time, but never hide the way back: the
/// pane bar names both panes, marks the focused one and names the key that
/// switches. Focus itself is untouched, so `Tab` round-trips exactly.
fn draw_compact_body(frame: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(2)])
        .split(area);
    draw_pane_bar(frame, rows[0], app);
    if app.focus == FocusPane::Sidebar {
        draw_sidebar(frame, rows[1], app);
    } else {
        draw_main(frame, rows[1], app);
    }
}

fn draw_pane_bar(frame: &mut Frame, area: Rect, app: &App) {
    let sidebar_focused = app.focus == FocusPane::Sidebar;
    let mut spans = Vec::new();
    for (label, focused) in [
        ("Workspace", !sidebar_focused),
        ("Sidebar", sidebar_focused),
    ] {
        spans.push(Span::styled(
            format!("{} {label} ", if focused { "▸" } else { " " }),
            if focused {
                app.theme.accent()
            } else {
                app.theme.muted()
            },
        ));
        spans.push(Span::styled("│", app.theme.border()));
    }
    spans.push(Span::styled(
        " Tab switch · Enter inspect",
        app.theme.muted(),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == FocusPane::Sidebar;
    let block = panel_block("Orbit", focused, app, area.width);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 6 || inner.height == 0 {
        return;
    }
    // `▸ ` selection rail + `» ` current-destination marker. `●` is reserved
    // for the project's `active` status glyph, so "you are here" never depends
    // on colour and never reuses a status symbol.
    let label_width = usize::from(inner.width).saturating_sub(4);
    let mut items = Vec::new();
    for index in 0..app.sidebar_len() {
        let selected = index == app.sidebar_selection;
        let current = match app.screen {
            Screen::Home => index == 0,
            Screen::Inbox => index == 1,
            Screen::Projects => index == 2,
            Screen::Agents => index == 3,
            Screen::Project => index
                .checked_sub(4)
                .and_then(|position| app.projects.get(position))
                .is_some_and(|row| {
                    app.project
                        .as_ref()
                        .is_some_and(|project| project.id == row.id)
                }),
        };
        let marker = if selected { "▸ " } else { "  " };
        let current_marker = if current { "» " } else { "  " };
        let style = if selected {
            app.theme.selected()
        } else {
            app.theme.text()
        };
        let project = index
            .checked_sub(4)
            .and_then(|position| app.projects.get(position));
        let (label, glyph) = match project {
            Some(project) => (
                project.name.clone(),
                Some(crate::theme::Theme::status_glyph(&project.status)),
            ),
            None => (app.sidebar_label(index), None),
        };
        let room = label_width.saturating_sub(if glyph.is_some() { 2 } else { 0 });
        let mut spans = vec![
            Span::styled(
                marker,
                if selected {
                    app.theme.accent()
                } else {
                    app.theme.muted()
                },
            ),
            Span::styled(current_marker, app.theme.muted()),
            Span::styled(truncate_ellipsis(&label, room), style),
        ];
        if let (Some(glyph), Some(project)) = (glyph, project) {
            spans.push(Span::styled(
                format!(" {glyph}"),
                if selected {
                    app.theme.selected()
                } else {
                    app.theme.status(&project.status)
                },
            ));
        }
        items.push(ListItem::new(Line::from(spans)));
    }
    render_selectable_list(frame, inner, items, app.sidebar_selection);
}

fn draw_main(frame: &mut Frame, area: Rect, app: &App) {
    if app.screen == Screen::Projects {
        draw_projects_screen(frame, area, app);
        return;
    }
    let Some(view) = app.current_view() else {
        frame.render_widget(
            Paragraph::new("No view is loaded. Press r to refresh.").block(panel_block(
                "Workspace",
                app.focus == FocusPane::Workspace,
                app,
                area.width,
            )),
            area,
        );
        return;
    };
    let mut constraints = Vec::new();
    let show_tabs = app.screen == Screen::Project && !view.components.is_empty();
    if show_tabs {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(3));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let components_area = if show_tabs {
        draw_tabs(frame, chunks[0], app);
        chunks[1]
    } else {
        chunks[0]
    };
    draw_components(frame, components_area, app, view);
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let Some(project) = &app.project else {
        return;
    };
    if project.views.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let active = project.active_view.min(project.views.len() - 1);
    let labels: Vec<String> = project
        .views
        .iter()
        .enumerate()
        .map(|(index, view)| {
            format!(
                " {} {} ",
                if index == active { "▸" } else { " " },
                view.title
            )
        })
        .collect();
    let widths: Vec<usize> = labels.iter().map(|label| label.chars().count()).collect();
    let hint = if project.generated {
        " generated default · [ ] switch view "
    } else {
        " [ ] switch view "
    };
    let hint_width = hint.chars().count();
    let budget = usize::from(area.width).saturating_sub(hint_width);
    let (start, end) = tab_window(&widths, active, budget);
    let mut spans = Vec::new();
    if start > 0 {
        spans.push(Span::styled("‹", app.theme.muted()));
    }
    for (offset, index) in (start..end).enumerate() {
        if offset > 0 {
            spans.push(Span::styled("│", app.theme.muted()));
        }
        let style = if index == active {
            app.theme.accent().add_modifier(Modifier::UNDERLINED)
        } else {
            app.theme.muted()
        };
        spans.push(Span::styled(labels[index].clone(), style));
    }
    if end < labels.len() {
        spans.push(Span::styled("›", app.theme.muted()));
    }
    let used: usize = widths[start..end].iter().sum::<usize>() + end.saturating_sub(start + 1);
    if used + hint_width <= usize::from(area.width) {
        spans.push(Span::styled(hint, app.theme.muted()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Pick a contiguous window of tabs that contains the active view.
///
/// Terminals with many project views would otherwise clip the active tab off
/// the right edge, which hides where the user is.
fn tab_window(widths: &[usize], active: usize, budget: usize) -> (usize, usize) {
    if widths.is_empty() {
        return (0, 0);
    }
    let active = active.min(widths.len() - 1);
    let mut start = active;
    let mut end = active + 1;
    let mut used = widths[active];
    loop {
        if end < widths.len() && used + widths[end] <= budget {
            used += widths[end];
            end += 1;
        } else if start > 0 && used + widths[start - 1] <= budget {
            start -= 1;
            used += widths[start];
        } else {
            break;
        }
    }
    (start, end)
}
/// Bottom action bar: at most three high-value operations for the current
/// selection, each with its real key, then the stable `a more actions` and
/// `q quit` entries. Disabled actions never appear here; the menu explains them.
fn draw_action_bar(frame: &mut Frame, area: Rect, app: &App) {
    let width = usize::from(area.width);
    let actions = app.compact_actions();
    // Narrow terminals need the space for a third real action; the suffix
    // stays honest (`a` still opens the full menu).
    let suffix = if width < 70 {
        " · a more · q quit"
    } else {
        " · a more actions · q quit"
    };
    let mut spans = vec![Span::styled(" Actions: ", app.theme.title())];
    let mut used = " Actions: ".chars().count();
    for (shown, entry) in actions.iter().enumerate() {
        let rendered = format!("{} {}", entry.key, entry.label);
        let separator = if shown > 0 { 3 } else { 0 };
        if used + separator + rendered.chars().count() + suffix.chars().count() > width {
            break;
        }
        if shown > 0 {
            spans.push(Span::styled(" · ", app.theme.muted()));
        }
        spans.push(Span::styled(format!("{} ", entry.key), app.theme.accent()));
        spans.push(Span::styled(entry.label.clone(), app.theme.text()));
        used += separator + rendered.chars().count();
    }
    spans.push(Span::styled(suffix, app.theme.muted()));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Short, honest keyboard reminder for the current screen. Keys that the shell
/// does not bind are never advertised here.
fn status_hint(app: &App, width: u16) -> &'static str {
    if width < MEDIUM_BREAKPOINT {
        return "j/k select · Esc back";
    }
    match app.screen {
        Screen::Home => "j/k select · Tab focus · Enter inspect · Esc back",
        // Enter on the Projects list opens the project; `D` inspects the row.
        Screen::Inbox => "j/k select · Enter inspect · D inspect · Esc back",
        Screen::Projects => "j/k select · Enter open · D inspect · Esc back",
        Screen::Agents => "j/k select · Enter inspect · D inspect · Esc back",
        Screen::Project => "j/k select · h/l component · Enter inspect · Esc back",
    }
}

/// A shorter reminder for terminals where the full one does not fit.
fn short_status_hint(app: &App) -> &'static str {
    match app.screen {
        Screen::Project => "h/l component · Enter inspect",
        _ => "Tab focus · Enter inspect",
    }
}

pub(super) fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let style = match app.status.kind {
        StatusKind::Success => app.theme.success(),
        StatusKind::Warning | StatusKind::Pending => app.theme.warning(),
        StatusKind::Error => app.theme.danger(),
        StatusKind::Info => app.theme.muted(),
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    draw_action_bar(frame, rows[0], app);
    // The quit hint keeps its exact wording: it is the documented way out.
    let quit = " Ctrl+Q quit · q quit (not typing) ";
    let width = usize::from(rows[1].width);
    // The status message is the freshest information on screen, so it is never
    // truncated; a key reminder is only added when it fits beside it.
    let used = quit.chars().count() + app.status.text.chars().count() + 2;
    let room = width.saturating_sub(used);
    let connected_hint = app
        .client
        .is_connected()
        .then_some("! pending · f flush · : commands");
    let hint = connected_hint
        .into_iter()
        .chain([status_hint(app, rows[1].width), short_status_hint(app)])
        .find(|text| text.chars().count() + 3 <= room);
    let mut spans = vec![
        Span::styled(quit, style),
        Span::styled(format!(" {} ", app.status.text), style),
    ];
    if let Some(hint) = hint {
        spans.push(Span::styled(format!("  {hint}"), app.theme.muted()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), rows[1]);
}
/// Bordered pane. Titles are truncated to the pane so a long component or
/// project title never eats the frame's corner.
pub(super) fn panel_block(title: &str, focused: bool, app: &App, width: u16) -> Block<'static> {
    let room = usize::from(width.saturating_sub(4)).max(4);
    let title = truncate_ellipsis(title, room);
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(if focused {
            app.theme.focused_border()
        } else {
            app.theme.border()
        })
        .title(Line::from(Span::styled(
            format!(" {title} "),
            if focused {
                app.theme.accent()
            } else {
                app.theme.muted()
            },
        )))
}

pub(super) fn modal_block(title: &str, app: &App) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(app.theme.focused_border())
        .title(Line::from(Span::styled(
            format!(" {title} "),
            app.theme.accent(),
        )))
}
pub(super) fn truncate_ellipsis(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if value.chars().count() <= width {
        return value.to_owned();
    }
    let mut output: String = value.chars().take(width.saturating_sub(1)).collect();
    output.push('…');
    output
}
