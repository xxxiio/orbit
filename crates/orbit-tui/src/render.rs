//! Ratatui renderer for the project-centric shell. All data arrives through
//! `App` and resolved `orbit-ui` models; this module never queries storage.
use crate::app::{
    App, FocusPane, FormFieldKind, FormState, Modal, PaletteEntry, Screen, StatusKind,
};
use crate::theme::{
    DETAIL_MAX, DETAIL_MIN, DETAIL_WIDTH, LEGEND_HEIGHT, MEDIUM_BREAKPOINT, SIDEBAR_MAX,
    SIDEBAR_MIN, SIDEBAR_WIDTH, WIDE_BREAKPOINT,
};
use orbit_ui::model::*;
use orbit_ui::{ComponentBody, LoadStatus, ResolvedComponent, ResolvedView};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.set_viewport(area.width);
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    draw_header(frame, root[0], app);
    draw_body(frame, root[1], app);
    draw_status(frame, root[2], app);
    draw_modal(frame, area, app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
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
                "   CONNECTED{}{} · {} projects · {} pending",
                if app.workspace_error.is_some() {
                    " [offline/stale]"
                } else {
                    ""
                },
                "",
                app.projects.len(),
                // The client owns the pending sum so staged attachment uploads
                // are counted alongside outbox commands.
                app.client.pending_count()
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

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    // First-run guidance: never present an unexplained empty dashboard.
    if app.projects.is_empty() {
        let banner = Line::from(vec![
            Span::styled(" Getting started: ", app.theme.title()),
            Span::styled(
                "press c to create your first project. Basic is the default workflow; every project ships with Overview, Tasks, Board and Timeline views.",
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
    let mut constraints = vec![
        Constraint::Length(plan.sidebar),
        Constraint::Min(plan.workspace_floor.max(1)),
    ];
    if let Some(detail) = plan.detail {
        constraints.push(Constraint::Length(detail));
    }
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    draw_sidebar(frame, chunks[0], app);
    draw_main(frame, chunks[1], app);
    if let Some(chunk) = chunks.get(2) {
        draw_detail(frame, *chunk, app);
    }
}

/// Widths for the three-region shell.
struct ShellPlan {
    sidebar: u16,
    workspace_floor: u16,
    detail: Option<u16>,
}

/// Decide the horizontal budget before drawing anything.
///
/// * the sidebar is sized to its labels inside a compact band;
/// * the detail pane only docks when the active component still gets its
///   readable minimum (a board needs far more width than a metric row);
/// * the workspace is a `Min` constraint, so it absorbs every column the
///   sidebar/detail panes do not claim - including when details are toggled off.
fn shell_plan(width: u16, app: &App) -> ShellPlan {
    let sidebar = sidebar_width(width, app);
    let workspace_floor = workspace_min_width(app);
    let mut detail = if app.show_detail && width >= WIDE_BREAKPOINT {
        Some(detail_width(width))
    } else {
        None
    };
    if let Some(detail_width) = detail {
        let remaining = width.saturating_sub(sidebar).saturating_sub(detail_width);
        // The pane the user is working in never disappears; a docked detail
        // pane yields instead of squashing the primary content.
        if remaining < workspace_floor && app.focus != FocusPane::Detail {
            detail = None;
        }
    }
    ShellPlan {
        sidebar,
        workspace_floor: workspace_floor.min(width.saturating_sub(sidebar)),
        detail,
    }
}

/// The sidebar is sized to its longest visible label, kept inside a band so the
/// workspace always wins the argument for space on smaller terminals.
///
/// Below 160 columns the sidebar is pinned to [`SIDEBAR_WIDTH`]: a shifting
/// navigation column makes the shell feel unstable and costs the workspace
/// cells it needs more than a long project name does. From 160 columns up there
/// is slack, so the column may breathe between [`SIDEBAR_MIN`] and
/// [`SIDEBAR_MAX`].
fn sidebar_width(width: u16, app: &App) -> u16 {
    if width < 160 {
        return SIDEBAR_WIDTH;
    }
    let widest = (0..app.sidebar_len())
        .map(|index| app.sidebar_label(index).chars().count())
        .max()
        .unwrap_or(0);
    // 2 columns of marker/dot, 2 columns of block padding.
    let wanted = u16::try_from(widest).unwrap_or(u16::MAX).saturating_add(4);
    wanted.clamp(SIDEBAR_MIN, SIDEBAR_MAX)
}

/// The detail pane grows with the terminal so properties and wrapped notes stay
/// readable, but never starves the workspace.
fn detail_width(width: u16) -> u16 {
    let preferred = match width {
        w if w >= 170 => DETAIL_MAX,
        w if w >= 150 => 34,
        _ => DETAIL_WIDTH,
    };
    preferred.clamp(DETAIL_MIN, DETAIL_MAX)
}

/// Width the active component needs to stay readable.
fn workspace_min_width(app: &App) -> u16 {
    app.current_component()
        .map(component_min_width)
        .unwrap_or(30)
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
    spans.push(Span::styled(" Tab switch · D details", app.theme.muted()));
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
    // `▸ ` marker + `● ` current-screen dot.
    let label_width = usize::from(inner.width).saturating_sub(4);
    let mut items = Vec::new();
    for index in 0..app.sidebar_len() {
        let selected = index == app.sidebar_selection;
        let current = match app.screen {
            Screen::Home => index == 0,
            Screen::Inbox => index == 1,
            Screen::Projects => index == 2,
            Screen::Agents => index == 3,
            Screen::Project => index >= 4,
        };
        let marker = if selected { "▸ " } else { "  " };
        let current_marker = if current { "● " } else { "  " };
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

fn draw_components(frame: &mut Frame, area: Rect, app: &App, view: &ResolvedView) {
    let count = view.components.len();
    if count == 0 {
        frame.render_widget(
            Paragraph::new("This view declares no components.").block(panel_block(
                &view.title,
                app.focus == FocusPane::Workspace,
                app,
                area.width,
            )),
            area,
        );
        return;
    }
    let index = app.active_component.min(count - 1);
    // When the declared layout cannot give every component its readable
    // minimum, show one component at a time instead of squeezing all of them.
    if count > 1 && !layout_fits(area, &view.layout, &view.components) {
        draw_component_rail(frame, area, app, view, index);
        return;
    }
    let rects = component_rects(area, &view.layout, &view.components);
    for (index, (component, rect)) in view.components.iter().zip(rects).enumerate() {
        let active = app.focus == FocusPane::Workspace && index == app.active_component;
        draw_component(frame, rect, app, component, active);
    }
}

/// Single-component mode: keep the component legend on its own row so the
/// frame of the component itself is never clipped.
fn draw_component_rail(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &ResolvedView,
    index: usize,
) {
    if area.height == 0 {
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(LEGEND_HEIGHT), Constraint::Min(1)])
        .split(area);
    let mut spans = vec![
        Span::styled(
            format!(" component {}/{} ", index + 1, view.components.len()),
            app.theme.section(),
        ),
        Span::styled("· h/l switch", app.theme.muted()),
    ];
    if app.focus == FocusPane::Workspace {
        spans.push(Span::styled(" · focus", app.theme.accent()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), rows[0]);
    let active = app.focus == FocusPane::Workspace;
    draw_component(frame, rows[1], app, &view.components[index], active);
}

/// Rows/columns a component needs before it stops being readable. A board needs
/// a wide column, a metric row only a handful of cells.
fn component_min_size(component: &ResolvedComponent) -> (u16, u16) {
    match &component.body {
        ComponentBody::Kanban { .. } => (56, 8),
        ComponentBody::Table { .. } => (34, 5),
        ComponentBody::List { .. }
        | ComponentBody::KnowledgeRefs { .. }
        | ComponentBody::AgentJobs { .. }
        | ComponentBody::Missing { .. } => (26, 4),
        ComponentBody::Timeline { .. }
        | ComponentBody::HabitTracker { .. }
        | ComponentBody::MetricHistory { .. }
        | ComponentBody::MilestoneProgress { .. } => (30, 5),
        ComponentBody::Detail { .. } => (26, 5),
        ComponentBody::Text { .. } => (24, 4),
        ComponentBody::Metric { .. } => (16, 3),
    }
}

fn component_min_width(component: &ResolvedComponent) -> u16 {
    component_min_size(component).0
}

fn component_min_height(component: &ResolvedComponent) -> u16 {
    component_min_size(component).1
}

/// Height a component would like, used as the growth weight once the minimums
/// have been paid for.
fn preferred_height(component: &ResolvedComponent) -> u16 {
    match &component.body {
        ComponentBody::Text { lines, .. } => 3 + lines.len().min(12) as u16,
        ComponentBody::Metric { .. } => 5,
        ComponentBody::Detail { properties, note } => {
            5 + properties.len().min(10) as u16 + note.lines().take(6).count() as u16
        }
        ComponentBody::Kanban { .. } => 16,
        ComponentBody::Table { rows, .. } => 4 + rows.len().min(10) as u16,
        ComponentBody::List { items } | ComponentBody::KnowledgeRefs { items } => {
            3 + items.len().min(10) as u16
        }
        ComponentBody::AgentJobs { jobs } => 3 + jobs.len().min(10) as u16,
        ComponentBody::Timeline { events } => 3 + events.len().min(10) as u16,
        ComponentBody::HabitTracker { habits } => 3 + habits.len().min(10) as u16,
        ComponentBody::MetricHistory { metrics } => 4 + 2 * metrics.len().min(6) as u16,
        ComponentBody::MilestoneProgress { milestones } => 3 + milestones.len().min(10) as u16,
        ComponentBody::Missing { .. } => 5,
    }
}

/// Width a component would like when it shares a row with a sibling.
fn preferred_width(component: &ResolvedComponent) -> u16 {
    match &component.body {
        ComponentBody::Kanban { .. } => 88,
        ComponentBody::Table { .. } => 52,
        ComponentBody::Detail { .. } => 36,
        _ => component_min_width(component),
    }
}

/// Does the declared layout have room for every component's minimum?
fn layout_fits(
    area: Rect,
    layout: &orbit_blueprint::presentation::ViewLayout,
    components: &[ResolvedComponent],
) -> bool {
    use orbit_blueprint::presentation::ViewLayoutKind;
    if components.len() < 2 {
        return true;
    }
    let width_floor = components
        .iter()
        .map(component_min_width)
        .max()
        .unwrap_or(24);
    if area.width < width_floor || area.height < 3 {
        return false;
    }
    match layout.kind {
        ViewLayoutKind::Vertical => {
            components.iter().map(component_min_height).sum::<u16>() <= area.height
        }
        ViewLayoutKind::Horizontal => {
            components.iter().map(component_min_width).sum::<u16>() <= area.width
        }
        ViewLayoutKind::Grid => {
            let columns = grid_columns(area.width, layout.columns, components);
            let rows = components.len().div_ceil(columns);
            let heights: u16 = (0..rows)
                .map(|row| {
                    components[row * columns..components.len().min((row + 1) * columns)]
                        .iter()
                        .map(component_min_height)
                        .max()
                        .unwrap_or(4)
                })
                .sum();
            heights <= area.height
        }
    }
}

fn grid_columns(width: u16, declared: u16, components: &[ResolvedComponent]) -> usize {
    let count = components.len().max(1);
    let widest = components
        .iter()
        .map(component_min_width)
        .max()
        .unwrap_or(24)
        .max(1);
    let fits = usize::from(width / widest).max(1);
    usize::from(declared.max(1)).min(count).min(fits).max(1)
}

fn component_rects(
    area: Rect,
    layout: &orbit_blueprint::presentation::ViewLayout,
    components: &[ResolvedComponent],
) -> Vec<Rect> {
    use orbit_blueprint::presentation::ViewLayoutKind;
    match layout.kind {
        ViewLayoutKind::Vertical => {
            let minimums: Vec<u16> = components.iter().map(component_min_height).collect();
            let weights: Vec<u16> = components.iter().map(preferred_height).collect();
            let sizes = allocate(area.height, &minimums, &weights);
            stack(area, &sizes, Direction::Vertical)
        }
        ViewLayoutKind::Horizontal => {
            let minimums: Vec<u16> = components.iter().map(component_min_width).collect();
            let weights: Vec<u16> = components.iter().map(preferred_width).collect();
            let sizes = allocate(area.width, &minimums, &weights);
            stack(area, &sizes, Direction::Horizontal)
        }
        ViewLayoutKind::Grid => {
            let columns = grid_columns(area.width, layout.columns, components);
            let count = components.len();
            let rows = count.div_ceil(columns);
            let mut row_minimums = Vec::with_capacity(rows);
            let mut row_weights = Vec::with_capacity(rows);
            for row in 0..rows {
                let slice = &components[row * columns..count.min((row + 1) * columns)];
                row_minimums.push(slice.iter().map(component_min_height).max().unwrap_or(4));
                row_weights.push(slice.iter().map(preferred_height).max().unwrap_or(5));
            }
            let row_sizes = allocate(area.height, &row_minimums, &row_weights);
            let mut rects = Vec::with_capacity(count);
            for (row, row_rect) in stack(area, &row_sizes, Direction::Vertical)
                .into_iter()
                .enumerate()
            {
                let slice = &components[row * columns..count.min((row + 1) * columns)];
                let minimums: Vec<u16> = slice.iter().map(component_min_width).collect();
                let weights: Vec<u16> = slice.iter().map(preferred_width).collect();
                let sizes = allocate(row_rect.width, &minimums, &weights);
                rects.extend(stack(row_rect, &sizes, Direction::Horizontal));
            }
            rects
        }
    }
}

/// Build rects from explicit sizes along one axis.
fn stack(area: Rect, sizes: &[u16], direction: Direction) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(sizes.len());
    let mut offset = 0u16;
    for size in sizes {
        let rect = match direction {
            Direction::Horizontal => Rect {
                x: area.x.saturating_add(offset),
                y: area.y,
                width: (*size).min(area.width.saturating_sub(offset)),
                height: area.height,
            },
            _ => Rect {
                x: area.x,
                y: area.y.saturating_add(offset),
                width: area.width,
                height: (*size).min(area.height.saturating_sub(offset)),
            },
        };
        offset = offset.saturating_add(*size);
        rects.push(rect);
    }
    rects
}

/// Split `total` cells into per-component sizes: every component first receives
/// its minimum, then the remaining cells grow the components by weight. The
/// result always sums to `total` so regions tile the area exactly.
fn allocate(total: u16, minimums: &[u16], weights: &[u16]) -> Vec<u16> {
    let count = minimums.len();
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![total];
    }
    let total = usize::from(total);
    let floors: usize = minimums.iter().map(|value| usize::from(*value)).sum();
    let mut sizes: Vec<usize> = if floors >= total {
        minimums
            .iter()
            .map(|value| usize::from(*value) * total / floors.max(1))
            .collect()
    } else {
        let extra = total - floors;
        let weight_total: usize = weights
            .iter()
            .map(|value| usize::from(*value))
            .sum::<usize>();
        let weight_total = weight_total.max(1);
        minimums
            .iter()
            .zip(weights)
            .map(|(value, weight)| {
                usize::from(*value) + extra * usize::from(*weight) / weight_total
            })
            .collect()
    };
    // Hand out the rounding remainder, widest component first.
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by_key(|index| std::cmp::Reverse(weights.get(*index).copied().unwrap_or(0)));
    let mut used: usize = sizes.iter().sum();
    let mut cursor = 0usize;
    while used < total {
        sizes[order[cursor % count]] += 1;
        used += 1;
        cursor += 1;
    }
    sizes
        .into_iter()
        .map(|size| u16::try_from(size).unwrap_or(u16::MAX))
        .collect()
}

fn draw_component(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    component: &ResolvedComponent,
    active: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let badge = component.status.badge();
    let title = match badge {
        Some(badge) => format!("{}  [{}]", component.title, badge),
        None => component.title.clone(),
    };
    let block = panel_block(&title, active, app, area.width);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    match &component.body {
        ComponentBody::Text { heading, lines }
            if matches!(component.status, LoadStatus::Loading) =>
        {
            let _ = (heading, lines);
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("Loading accepted data…", app.theme.warning())),
                    Line::from(Span::styled(
                        "The previous view remains visible until resolution completes.",
                        app.theme.muted(),
                    )),
                ])
                .wrap(Wrap { trim: false }),
                inner,
            );
        }
        ComponentBody::Text { heading, lines } => {
            let width = usize::from(inner.width);
            let mut text = Vec::new();
            if !heading.is_empty() {
                text.push(Line::from(Span::styled(heading.clone(), app.theme.title())));
            }
            for line in lines {
                text.extend(wrap_text(line, width).into_iter().map(Line::from));
            }
            let scroll = if active {
                clamp_scroll(app.scroll, text.len(), inner.height)
            } else {
                0
            };
            frame.render_widget(Paragraph::new(text).scroll((scroll, 0)), inner);
        }
        ComponentBody::List { .. }
        | ComponentBody::KnowledgeRefs { .. }
        | ComponentBody::AgentJobs { .. } => {
            draw_rows(frame, inner, app, component, active);
        }
        ComponentBody::Table { columns, rows } => {
            draw_table(frame, inner, app, component, active, columns, rows);
        }
        ComponentBody::Metric {
            value,
            label,
            detail,
        } => {
            let lines = vec![
                Line::from(Span::styled(
                    value.clone(),
                    app.theme.accent().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(label.clone(), app.theme.title())),
                Line::from(Span::styled(detail.clone(), app.theme.muted())),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true }),
                inner,
            );
        }
        ComponentBody::Detail { properties, note } => {
            let width = usize::from(inner.width);
            let mut lines = property_lines(
                app,
                properties
                    .iter()
                    .map(|property| (&property.key, &property.value)),
                width,
            );
            if !note.is_empty() {
                if !lines.is_empty() {
                    lines.push(Line::from(""));
                }
                for row in note.lines() {
                    lines.extend(wrap_text(row, width).into_iter().map(Line::from));
                }
            }
            let scroll = if active {
                clamp_scroll(app.scroll, lines.len(), inner.height)
            } else {
                0
            };
            frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
        }
        ComponentBody::Kanban { columns } => {
            draw_kanban(frame, inner, app, component, active, columns);
        }
        ComponentBody::Timeline { events } => {
            let height = usize::from(inner.height);
            let offset = if active {
                app.selected.saturating_sub(height.saturating_sub(1))
            } else {
                0
            };
            let lines = events
                .iter()
                .enumerate()
                .skip(offset)
                .take(height)
                .map(|(index, event)| {
                    let selected = active && index == app.selected;
                    Line::from(vec![
                        Span::styled(
                            if selected { "▸ " } else { "  " },
                            if selected {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(format!("{:<22}", event.when), app.theme.muted()),
                        Span::styled(
                            format!("{} ", crate::theme::Theme::status_glyph(&event.status)),
                            app.theme.status(&event.status),
                        ),
                        Span::styled(
                            event.title.clone(),
                            if selected {
                                app.theme.selected()
                            } else {
                                app.theme.text()
                            },
                        ),
                        Span::styled(format!("  {}", event.detail), app.theme.muted()),
                    ])
                })
                .collect::<Vec<_>>();
            frame.render_widget(Paragraph::new(lines), inner);
        }
        ComponentBody::HabitTracker { habits } => {
            let today = format_epoch_ms(crate::app::now_ms());
            let today = today.get(..10).unwrap_or("");
            let height = usize::from(inner.height);
            let offset = if active {
                app.selected.saturating_sub(height.saturating_sub(1))
            } else {
                0
            };
            let lines = habits
                .iter()
                .enumerate()
                .skip(offset)
                .take(height)
                .map(|(index, habit)| {
                    let checked = habit.last.starts_with(today) && habit.last != "never";
                    let selected = active && index == app.selected;
                    Line::from(vec![
                        Span::styled(
                            if selected { "▸ " } else { "  " },
                            if selected {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(if checked { "[x] " } else { "[ ] " }, app.theme.success()),
                        Span::styled(
                            habit.name.clone(),
                            if selected {
                                app.theme.selected()
                            } else {
                                app.theme.text()
                            },
                        ),
                        Span::styled(
                            format!(
                                "  {} · {} in 28d · last {}",
                                habit.cadence, habit.count, habit.last
                            ),
                            app.theme.muted(),
                        ),
                    ])
                })
                .collect::<Vec<_>>();
            frame.render_widget(Paragraph::new(lines), inner);
        }
        ComponentBody::MetricHistory { metrics } => {
            let height = usize::from(inner.height) / 2;
            let offset = if active {
                app.selected.saturating_sub(height.saturating_sub(1))
            } else {
                0
            };
            let lines = metrics
                .iter()
                .enumerate()
                .skip(offset)
                .take(height.max(1))
                .flat_map(|(index, metric)| {
                    let selected = active && index == app.selected;
                    let value = metric
                        .latest
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "—".into());
                    let progress = metric.progress.unwrap_or(0);
                    vec![
                        Line::from(vec![
                            Span::styled(
                                if selected { "▸ " } else { "  " },
                                if selected {
                                    app.theme.accent()
                                } else {
                                    app.theme.muted()
                                },
                            ),
                            Span::styled(
                                metric.label.clone(),
                                if selected {
                                    app.theme.selected()
                                } else {
                                    app.theme.title()
                                },
                            ),
                            Span::styled(
                                format!("  {value} / {}", metric.target),
                                app.theme.text(),
                            ),
                            Span::styled(format!("  {}%", progress), app.theme.muted()),
                        ]),
                        Line::from(vec![
                            Span::styled(progress_bar(progress), app.theme.accent()),
                            Span::styled(
                                format!("  {} {}", metric.direction, sparkline(&metric.history)),
                                app.theme.muted(),
                            ),
                        ]),
                    ]
                })
                .collect::<Vec<_>>();
            frame.render_widget(Paragraph::new(lines), inner);
        }
        ComponentBody::MilestoneProgress { milestones } => {
            let height = usize::from(inner.height);
            let offset = if active {
                app.selected.saturating_sub(height.saturating_sub(1))
            } else {
                0
            };
            let lines = milestones
                .iter()
                .enumerate()
                .skip(offset)
                .take(height)
                .map(|(index, milestone)| {
                    let selected = active && index == app.selected;
                    let progress = milestone.progress.unwrap_or(0);
                    Line::from(vec![
                        Span::styled(
                            if selected { "▸ " } else { "  " },
                            if selected {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(
                            format!("{} ", crate::theme::Theme::status_glyph(&milestone.status)),
                            app.theme.status(&milestone.status),
                        ),
                        Span::styled(
                            format!("{:<24}", milestone.name),
                            if selected {
                                app.theme.selected()
                            } else {
                                app.theme.text()
                            },
                        ),
                        Span::styled(progress_bar(progress), app.theme.accent()),
                        Span::styled(
                            format!(" {}/{}", milestone.done, milestone.total),
                            app.theme.muted(),
                        ),
                    ])
                })
                .collect::<Vec<_>>();
            frame.render_widget(Paragraph::new(lines), inner);
        }
        ComponentBody::Missing {
            component_type,
            reason,
        } => {
            let lines = vec![
                Line::from(Span::styled(
                    format!("Missing component: {component_type}"),
                    app.theme.danger(),
                )),
                Line::from(Span::styled(reason.clone(), app.theme.muted())),
                Line::from(Span::styled(
                    "The rest of the view remains available.",
                    app.theme.muted(),
                )),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }
    if let LoadStatus::Error(error) = &component.status
        && inner.height > 2
    {
        let area = Rect {
            y: inner.y + inner.height - 1,
            height: 1,
            ..inner
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("error: {error}"),
                app.theme.danger(),
            ))),
            area,
        );
    }
}

fn draw_rows(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    component: &ResolvedComponent,
    active: bool,
) {
    let height = usize::from(area.height);
    let width = usize::from(area.width);
    if height == 0 || width == 0 {
        return;
    }
    if component.selection.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No entries yet — press c to add one.",
                app.theme.empty(),
            ))),
            area,
        );
        return;
    }
    // The selection marker and status glyph never compete with the title; the
    // subtitle only appears when there is width left to explain the row.
    let subtitle_width = if width >= 44 { width / 3 } else { 0 };
    let title_width = width.saturating_sub(4 + subtitle_width).max(6);
    let offset = if active {
        app.selected.saturating_sub(height.saturating_sub(1))
    } else {
        0
    };
    let visible = component
        .selection
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(index, item)| {
            let selected = active && index == app.selected;
            let mut spans = vec![
                Span::styled(
                    if selected { "▸ " } else { "  " },
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.muted()
                    },
                ),
                Span::styled(
                    format!("{} ", crate::theme::Theme::status_glyph(&item.status)),
                    app.theme.status(&item.status),
                ),
                Span::styled(
                    truncate_ellipsis(&item.title, title_width),
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ),
            ];
            if subtitle_width > 0 && !item.subtitle.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", truncate_ellipsis(&item.subtitle, subtitle_width)),
                    app.theme.muted(),
                ));
            }
            if selected {
                pad_selected_row(app, &mut spans, width);
            }
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), area);
}

/// Which job a table column does. Sizing follows this priority: the row title
/// always wins, then status, priority and finally dates/progress.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ColumnRole {
    Title,
    Status,
    Priority,
    Date,
    Progress,
    Other,
}

fn column_role(name: &str) -> ColumnRole {
    match name.to_ascii_lowercase().as_str() {
        "title" | "name" | "task" | "item" | "subject" | "question" | "topic" | "activity"
        | "label" => ColumnRole::Title,
        "status" | "state" | "stage" => ColumnRole::Status,
        "priority" | "severity" | "rank" => ColumnRole::Priority,
        "due" | "deadline" | "date" | "start" | "end" | "scheduled" | "updated" | "created" => {
            ColumnRole::Date
        }
        "progress" | "estimate" | "effort" | "spent" | "percent" | "completion" => {
            ColumnRole::Progress
        }
        _ => ColumnRole::Other,
    }
}

struct TableColumn {
    index: usize,
    width: usize,
    role: ColumnRole,
}

/// Choose which columns remain at this width. Secondary columns are dropped
/// rather than squashed, and every column keeps its own space so no cell becomes
/// a single character.
fn plan_table_columns(
    columns: &[String],
    rows: &[TableRowModel],
    width: usize,
) -> Vec<TableColumn> {
    let budget = width.saturating_sub(2);
    if budget == 0 || columns.is_empty() || width < 12 {
        return Vec::new();
    }
    let naturals: Vec<usize> = (0..columns.len())
        .map(|index| {
            let header = columns[index].chars().count();
            let widest = rows
                .iter()
                .filter_map(|row| row.cells.get(index))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0);
            header.max(widest).saturating_add(1)
        })
        .collect();
    let mut order: Vec<usize> = (0..columns.len()).collect();
    order.sort_by_key(|index| (column_role(&columns[*index]), *index));
    let title_index = order
        .iter()
        .copied()
        .find(|index| column_role(&columns[*index]) == ColumnRole::Title);
    // Half the row stays with the title; the rest is shared by useful extras.
    let title_reserve = if title_index.is_some() {
        (budget / 2).clamp(10, budget)
    } else {
        0
    };
    let mut remaining = budget.saturating_sub(title_reserve);
    let mut plan = Vec::new();
    for index in order
        .iter()
        .copied()
        .filter(|index| Some(*index) != title_index)
    {
        let role = column_role(&columns[index]);
        let cap = match role {
            ColumnRole::Status | ColumnRole::Priority => 12,
            _ => 18,
        };
        let floor = match role {
            ColumnRole::Status | ColumnRole::Priority => 4,
            _ => 6,
        };
        let width = naturals[index].clamp(floor, cap);
        if width > remaining {
            continue;
        }
        remaining -= width;
        plan.push(TableColumn { index, width, role });
    }
    if let Some(index) = title_index {
        plan.push(TableColumn {
            index,
            width: title_reserve + remaining,
            role: ColumnRole::Title,
        });
    }
    plan.sort_by_key(|column| column.index);
    plan
}

fn draw_table(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    _component: &ResolvedComponent,
    active: bool,
    columns: &[String],
    rows: &[TableRowModel],
) {
    let width = usize::from(area.width);
    if columns.is_empty() || width == 0 || area.height == 0 {
        return;
    }
    let plan = plan_table_columns(columns, rows, width);
    if plan.is_empty() {
        return;
    }
    let mut header = vec![Span::styled("  ", app.theme.muted())];
    for column in &plan {
        let label = truncate_ellipsis(&columns[column.index].to_uppercase(), column.width - 1);
        header.push(Span::styled(
            format!("{label:<width$}", width = column.width),
            app.theme.column_header(),
        ));
    }
    let mut lines = vec![Line::from(header)];
    let height = usize::from(area.height.saturating_sub(1));
    let offset = if active && !rows.is_empty() {
        app.selected.saturating_sub(height.saturating_sub(1))
    } else {
        0
    };
    for (index, row) in rows.iter().enumerate().skip(offset).take(height) {
        let selected = active && index == app.selected;
        let mut spans = vec![Span::styled(
            if selected { "▸ " } else { "  " },
            if selected {
                app.theme.selected()
            } else {
                app.theme.muted()
            },
        )];
        for column in &plan {
            let cell = row
                .cells
                .get(column.index)
                .map(String::as_str)
                .unwrap_or("");
            match column.role {
                ColumnRole::Status => {
                    if column.width < 6 {
                        // Not enough room for the word: the glyph still carries
                        // the state.
                        spans.push(Span::styled(
                            format!(
                                "{:<width$}",
                                crate::theme::Theme::status_glyph(cell),
                                width = column.width
                            ),
                            app.theme.status(cell),
                        ));
                    } else {
                        spans.push(Span::styled(
                            format!(
                                "{} {:<width$}",
                                crate::theme::Theme::status_glyph(cell),
                                truncate_ellipsis(cell, column.width.saturating_sub(2)),
                                width = column.width.saturating_sub(2)
                            ),
                            app.theme.status(cell),
                        ));
                    }
                }
                ColumnRole::Priority => spans.push(Span::styled(
                    format!(
                        "{:<width$}",
                        truncate_ellipsis(cell, column.width.saturating_sub(1)),
                        width = column.width
                    ),
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.warning()
                    },
                )),
                _ => spans.push(Span::styled(
                    format!(
                        "{:<width$}",
                        truncate_ellipsis(cell, column.width.saturating_sub(1)),
                        width = column.width
                    ),
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                )),
            }
        }
        if selected {
            pad_selected_row(app, &mut spans, width);
        }
        lines.push(Line::from(spans));
    }
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "No rows yet — press c to add one.",
            app.theme.empty(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Readable minimum column for cards: a marker, a status glyph and enough title
/// to recognise the card. Anything narrower becomes a sliver, so columns are
/// windowed horizontally instead.
const KANBAN_MIN_COLUMN: u16 = 20;

fn draw_kanban(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    _component: &ResolvedComponent,
    active: bool,
    columns: &[KanbanColumnModel],
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if columns.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "This board has no columns yet.",
                app.theme.empty(),
            ))),
            area,
        );
        return;
    }
    let total: usize = columns.iter().map(|column| column.items.len()).sum();
    let selected = (total > 0).then(|| app.selected.min(total - 1));
    let mut selected_column = None;
    let mut selected_row = 0usize;
    let mut counted = 0usize;
    for (index, column) in columns.iter().enumerate() {
        if let Some(position) = selected
            && position >= counted
            && position < counted + column.items.len()
        {
            selected_column = Some(index);
            selected_row = position - counted;
            break;
        }
        counted += column.items.len();
    }
    let visible = usize::from((area.width / KANBAN_MIN_COLUMN).clamp(1, columns.len() as u16));
    // The window always follows the selection, even when another pane owns the
    // keyboard: switching focus must never hide the card the user was on.
    let start = selected_column
        .unwrap_or(0)
        .saturating_sub(visible / 2)
        .min(columns.len().saturating_sub(visible));
    let end = (start + visible).min(columns.len());
    let (body, legend) = if area.height > 2 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(LEGEND_HEIGHT)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };
    let sizes = allocate(
        body.width,
        &vec![1u16; end - start],
        &vec![1u16; end - start],
    );
    let rects = stack(body, &sizes, Direction::Horizontal);
    for ((column, rect), global) in columns[start..end].iter().zip(rects).zip(start..end) {
        // One cell between columns keeps adjacent titles from running together.
        let content_width = usize::from(rect.width).saturating_sub(1);
        if content_width == 0 {
            continue;
        }
        let is_selected_column = selected_column == Some(global);
        let header_style = if is_selected_column && active {
            app.theme.accent()
        } else if is_selected_column {
            app.theme.section()
        } else {
            app.theme.column_header()
        };
        let count_label = format!(" {}", column.items.len());
        let title_room = content_width.saturating_sub(count_label.chars().count() + 2);
        let header = vec![
            Span::styled(
                format!(
                    "{} {}",
                    if is_selected_column { "▾" } else { " " },
                    truncate_ellipsis(&column.title, title_room)
                ),
                header_style,
            ),
            Span::styled(count_label, app.theme.muted()),
        ];
        let mut lines = vec![Line::from(header)];
        lines.push(Line::from(Span::styled(
            if is_selected_column { "━" } else { "─" }.repeat(content_width),
            if is_selected_column {
                app.theme.accent()
            } else {
                app.theme.border()
            },
        )));
        let card_height = usize::from(rect.height).saturating_sub(2);
        let offset = if is_selected_column {
            selected_row.saturating_sub(card_height.saturating_sub(1))
        } else {
            0
        };
        for (item_index, item) in column
            .items
            .iter()
            .enumerate()
            .skip(offset)
            .take(card_height)
        {
            let selected_card =
                active && Some(global) == selected_column && item_index == selected_row;
            let in_selected_column = Some(global) == selected_column && item_index == selected_row;
            let mut spans = vec![
                Span::styled(
                    if selected_card {
                        "▸ "
                    } else if in_selected_column {
                        "› "
                    } else {
                        "  "
                    },
                    if selected_card {
                        app.theme.selected()
                    } else if in_selected_column {
                        app.theme.accent()
                    } else {
                        app.theme.muted()
                    },
                ),
                Span::styled(
                    format!("{} ", crate::theme::Theme::status_glyph(&item.status)),
                    app.theme.status(&item.status),
                ),
                Span::styled(
                    truncate_ellipsis(&item.title, content_width.saturating_sub(4)),
                    if selected_card {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ),
            ];
            if selected_card {
                pad_selected_row(app, &mut spans, content_width);
            }
            lines.push(Line::from(spans));
        }
        if column.items.is_empty() && card_height > 0 {
            lines.push(Line::from(Span::styled("— empty", app.theme.empty())));
        }
        frame.render_widget(Paragraph::new(lines), rect);
    }
    if let Some(area) = legend {
        let mut hints = vec![if end - start < columns.len() {
            format!("columns {}-{} of {}", start + 1, end, columns.len())
        } else {
            format!("{} columns", columns.len())
        }];
        if total == 0 {
            hints.push("no cards yet — press c to add one".into());
        } else {
            hints.push("j/k card".into());
            hints.push("h/l panel".into());
            hints.push("s status".into());
            hints.push("Enter open".into());
        }
        frame.render_widget(Paragraph::new(fitted_hints(app, &hints, area.width)), area);
    }
}

fn draw_projects_screen(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(3)])
        .split(area);
    let hint = truncate_ellipsis(
        "  Enter open · c new project · : palette · q quit · ? help",
        usize::from(area.width).saturating_sub(9),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Projects", app.theme.title()),
            Span::styled(hint, app.theme.muted()),
        ])),
        chunks[0],
    );
    let block = panel_block(
        &format!("{} project(s)", app.projects.len()),
        app.focus == FocusPane::Workspace,
        app,
        chunks[1].width,
    );
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);
    let height = usize::from(inner.height);
    let width = usize::from(inner.width);
    if height == 0 || width == 0 {
        return;
    }
    if app.projects.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No projects yet — press c to create one.",
                app.theme.empty(),
            ))),
            inner,
        );
        return;
    }
    // Name first, then status; the workflow/criteria summary is the first thing
    // to go when the terminal is narrow.
    let show_meta = width >= 56;
    let name_width = if show_meta {
        width.saturating_sub(2).min(32)
    } else {
        width.saturating_sub(4)
    }
    .max(8);
    let lines = app
        .projects
        .iter()
        .enumerate()
        .skip(
            app.projects_selection
                .saturating_sub(height.saturating_sub(1)),
        )
        .take(height)
        .map(|(index, project)| {
            let selected = index == app.projects_selection;
            let mut spans = vec![
                Span::styled(
                    if selected { "▸ " } else { "  " },
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.muted()
                    },
                ),
                Span::styled(
                    format!("{} ", crate::theme::Theme::status_glyph(&project.status)),
                    app.theme.status(&project.status),
                ),
                Span::styled(
                    truncate_ellipsis(&project.name, name_width),
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ),
            ];
            if show_meta {
                let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
                let summary = format!(
                    "{} · criteria {}/{}{}",
                    project.mode,
                    project.criteria_satisfied,
                    project.criteria_total,
                    if project.completion_ready {
                        " · ready"
                    } else {
                        ""
                    }
                );
                let room = width.saturating_sub(used + 2);
                if room >= 8 {
                    spans.push(Span::styled(
                        format!("  {}", truncate_ellipsis(&summary, room)),
                        app.theme.muted(),
                    ));
                }
            }
            if selected {
                pad_selected_row(app, &mut spans, width);
            }
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == FocusPane::Detail;
    let block = panel_block("Details", focused, app, area.width);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = usize::from(inner.width);
    let mut lines: Vec<Line> = Vec::new();
    if let Some(detail) = app.detail_model() {
        for row in wrap_text(&detail.title, width) {
            lines.push(Line::from(Span::styled(row, app.theme.title())));
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", crate::theme::Theme::status_glyph(&detail.status)),
                app.theme.status(&detail.status),
            ),
            Span::styled(
                truncate_ellipsis(&detail.subtitle, width.saturating_sub(2)),
                app.theme.muted(),
            ),
        ]));
        if !detail.fields.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("PROPERTIES", app.theme.section())));
            lines.extend(property_lines(
                app,
                detail.fields.iter().map(|(key, value)| (key, value)),
                width,
            ));
        }
        if !detail.note.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("NOTE", app.theme.section())));
            for row in detail.note.lines() {
                lines.extend(wrap_text(row, width).into_iter().map(Line::from));
            }
        }
        if !detail.actions.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("ACTIONS ", app.theme.section()),
                Span::styled(
                    format!("{} · press a", detail.actions.len()),
                    app.theme.muted(),
                ),
            ]));
        }
    } else if app.screen == Screen::Projects {
        lines.push(Line::from(Span::styled("Projects", app.theme.title())));
        lines.extend(
            wrap_text(
                "Select a project and press Enter to open its blueprint views.",
                width,
            )
            .into_iter()
            .map(|row| Line::from(Span::styled(row, app.theme.empty()))),
        );
    } else if let Some(project) = &app.project {
        for row in wrap_text(&project.name, width) {
            lines.push(Line::from(Span::styled(row, app.theme.title())));
        }
        lines.push(Line::from(Span::styled(
            truncate_ellipsis(
                &format!(
                    "{} · {}",
                    project.blueprint_label, project.data.project.status
                ),
                width,
            ),
            app.theme.muted(),
        )));
        lines.push(Line::from(""));
        for row in wrap_text(
            &format!(
                "{} tasks · {} work items · {} milestones",
                project.data.tasks.len(),
                project.data.work_items.len(),
                project.data.milestones.len()
            ),
            width,
        ) {
            lines.push(Line::from(Span::styled(row, app.theme.text())));
        }
    } else {
        lines.extend(
            wrap_text(
                "Select an item to see properties, dependencies and actions.",
                width,
            )
            .into_iter()
            .map(|row| Line::from(Span::styled(row, app.theme.empty()))),
        );
    }
    render_scrolled(frame, inner, app, lines, focused, app.detail_scroll);
}

/// Render a block of text with a clamped scroll offset and, when the content is
/// taller than the pane, a position indicator in the last row. A scroll offset
/// beyond the end never blanks the pane.
fn render_scrolled(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    lines: Vec<Line<'static>>,
    scrollable: bool,
    requested: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let height = usize::from(area.height);
    let total = lines.len();
    if total <= height || height < 2 {
        let scroll = if scrollable {
            u16::try_from(requested.min(total.saturating_sub(height))).unwrap_or(u16::MAX)
        } else {
            0
        };
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
        return;
    }
    let visible = height - 1;
    let max_scroll = total.saturating_sub(visible);
    let scroll = if scrollable {
        requested.min(max_scroll)
    } else {
        0
    };
    let body = Rect {
        height: u16::try_from(visible).unwrap_or(u16::MAX),
        ..area
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    let indicator = Rect {
        y: area.y + body.height,
        height: 1,
        ..area
    };
    let end = (scroll + visible).min(total);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}-{} of {total} · ↑↓ scroll", scroll + 1, end),
            app.theme.muted(),
        ))),
        indicator,
    );
}

/// Align property values under a shared key column and wrap long values beneath
/// themselves instead of letting them run past the pane border.
fn property_lines<'a>(
    app: &App,
    fields: impl IntoIterator<Item = (&'a String, &'a String)>,
    width: usize,
) -> Vec<Line<'static>> {
    let fields: Vec<(&String, &String)> = fields.into_iter().collect();
    let key_width = fields
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0)
        .min(12);
    let indent = key_width + 2;
    let value_width = width.saturating_sub(indent).max(1);
    let mut lines = Vec::with_capacity(fields.len());
    for (key, value) in fields {
        for (index, row) in wrap_text(value, value_width).into_iter().enumerate() {
            let lead = if index == 0 {
                Span::styled(
                    format!(
                        "{:<key_width$}  ",
                        truncate_ellipsis(key, key_width),
                        key_width = key_width
                    ),
                    app.theme.muted(),
                )
            } else {
                Span::styled(" ".repeat(indent), app.theme.muted())
            };
            lines.push(Line::from(vec![lead, Span::styled(row, app.theme.text())]));
        }
    }
    lines
}

/// Bottom action bar: at most three high-value operations for the current
/// selection, each with its real key, then the stable `a more actions` and
/// `q quit` entries. Disabled actions never appear here; the menu explains them.
fn draw_action_bar(frame: &mut Frame, area: Rect, app: &App) {
    let width = usize::from(area.width);
    let actions = app.compact_actions();
    let suffix = " · a more actions · q quit";
    let mut spans = vec![Span::styled(" Actions: ", app.theme.title())];
    let mut used = " Actions: ".chars().count();
    for (shown, entry) in actions.iter().enumerate() {
        let rendered = format!("{} {} ", entry.key, entry.label);
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
        Screen::Home => "j/k select · Tab focus · D details · Esc back",
        Screen::Inbox => "j/k select · Enter detail · D details · Esc back",
        Screen::Projects => "j/k select · Enter open · D details · Esc back",
        Screen::Agents => "j/k select · Enter detail · D details · Esc back",
        Screen::Project => "j/k select · h/l component · D details · Esc back",
    }
}

/// A shorter reminder for terminals where the full one does not fit.
fn short_status_hint(app: &App) -> &'static str {
    match app.screen {
        Screen::Project => "h/l component · D details",
        _ => "Tab focus · D details",
    }
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
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
    let hint = [status_hint(app, rows[1].width), short_status_hint(app)]
        .into_iter()
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

fn draw_modal(frame: &mut Frame, area: Rect, app: &App) {
    let Some(modal) = &app.modal else {
        return;
    };
    let (width, height) = modal_size(area, modal);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);
    match modal {
        Modal::Form(form) => draw_form(frame, popup, app, form),
        Modal::QuitConfirm { .. } => {
            let block = modal_block("Quit Orbit?", app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let lines = vec![
                Line::from(Span::styled("Leave Orbit now?", app.theme.title())),
                Line::from(""),
                Line::from("Confirm quit to discard unsaved editor input."),
                Line::from("Accepted application state is already persisted."),
                Line::from("Accepted data is already persisted; only unsaved input is discarded."),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter or y: quit · Esc or n: keep editing · Ctrl+Q/Ctrl+C: force quit",
                    app.theme.warning(),
                )),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        Modal::Confirm { title, body, .. } => {
            let block = modal_block(title, app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let lines = vec![
                Line::from(body.clone()),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter/y confirm · Esc/n cancel",
                    app.theme.warning(),
                )),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        Modal::ActionMenu {
            title,
            actions,
            primary,
            selected,
        } => {
            let block = modal_block(&format!("Actions · {title}"), app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let grouped = *primary > 0 && *primary < actions.len();
            let mut items: Vec<ListItem> = Vec::new();
            let mut selected_line = *selected;
            if grouped {
                items.push(ListItem::new(Line::from(Span::styled(
                    " Suggested",
                    app.theme.title(),
                ))));
                selected_line += 1;
                if *selected >= *primary {
                    selected_line += 1;
                }
            }
            for (index, action) in actions.iter().enumerate() {
                if grouped && index == *primary {
                    items.push(ListItem::new(Line::from(Span::styled(
                        " More actions",
                        app.theme.title(),
                    ))));
                }
                let is_selected = index == *selected;
                let marker = if is_selected { "▸ " } else { "  " };
                let label_style = if !action.enabled {
                    app.theme.muted()
                } else if is_selected {
                    app.theme.selected()
                } else {
                    app.theme.text()
                };
                // Only advertise a key when it identifies this row uniquely.
                let hotkey = crate::app::App::action_hotkey(&action.id).filter(|key| {
                    actions
                        .iter()
                        .filter(|other| crate::app::App::action_hotkey(&other.id) == Some(*key))
                        .count()
                        == 1
                });
                let key_text = hotkey
                    .map(|key| format!("{key:<6}"))
                    .unwrap_or_else(|| " ".repeat(6));
                let reason = action
                    .disabled_reason
                    .as_ref()
                    .map(|reason| format!("  — {reason}"))
                    .unwrap_or_default();
                items.push(ListItem::new(Line::from(vec![
                    Span::styled(
                        marker,
                        if is_selected {
                            app.theme.accent()
                        } else {
                            app.theme.muted()
                        },
                    ),
                    Span::styled(key_text, app.theme.accent()),
                    Span::styled(action.label.clone(), label_style),
                    Span::styled(reason, app.theme.muted()),
                ])));
            }
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);
            render_selectable_list(frame, rows[0], items, selected_line);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " Enter run selected · ↑/↓/PgUp/PgDn scroll · Esc close · dimmed rows explain why",
                    app.theme.muted(),
                ))),
                rows[1],
            );
        }
        Modal::Palette {
            query,
            entries,
            selected,
        } => {
            let block = modal_block("Command palette", app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(inner);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("> ", app.theme.accent()),
                    Span::styled(query.clone(), app.theme.title()),
                    Span::styled("_", app.theme.muted()),
                    Span::styled(
                        if query.is_empty() {
                            "  type to filter · recent destinations first"
                        } else {
                            "  prefix · word · fuzzy · substring"
                        },
                        app.theme.muted(),
                    ),
                ])),
                chunks[0],
            );
            let items = palette_lines(app, entries, *selected);
            render_selectable_list(frame, chunks[1], items, *selected);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " Enter run enabled entry · ↑/↓ select · Ctrl+U clear · Esc close",
                    app.theme.muted(),
                ))),
                chunks[2],
            );
        }
        Modal::Help => draw_help(frame, popup, app),
        Modal::Detail(detail) => {
            let block = modal_block(&format!("{} · detail", detail.title), app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{} ", crate::theme::Theme::status_glyph(&detail.status)),
                        app.theme.status(&detail.status),
                    ),
                    Span::styled(detail.subtitle.clone(), app.theme.muted()),
                ]),
                Line::from(""),
            ];
            for (key, value) in &detail.fields {
                for (index, line) in value.lines().enumerate() {
                    if index == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(format!("{key}: "), app.theme.muted()),
                            Span::styled(line.to_owned(), app.theme.text()),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled(format!("{}  ", " ".repeat(key.len())), app.theme.muted()),
                            Span::styled(line.to_owned(), app.theme.text()),
                        ]));
                    }
                }
            }
            if !detail.note.is_empty() {
                lines.push(Line::from(""));
                lines.extend(detail.note.lines().map(Line::from));
            }
            if !detail.actions.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Actions (press a to run)",
                    app.theme.title(),
                )));
                for action in &detail.actions {
                    lines.push(Line::from(vec![
                        Span::styled(
                            if action.enabled { "• " } else { "× " },
                            if action.enabled {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(action.label.clone(), app.theme.text()),
                        Span::styled(
                            action
                                .disabled_reason
                                .as_ref()
                                .map(|reason| format!(" — {reason}"))
                                .unwrap_or_default(),
                            app.theme.muted(),
                        ),
                    ]));
                }
            }
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((app.modal_scroll.min(u16::MAX as usize) as u16, 0)),
                inner,
            );
        }
        Modal::Criteria(state) => {
            let block = modal_block(&format!("Acceptance criteria · {}", state.task_title), app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let mut lines = vec![Line::from(Span::styled(
                "Ordered criteria. Checking them never completes the task by itself.",
                app.theme.muted(),
            ))];
            lines.push(Line::from(""));
            if state.items.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No acceptance criteria yet. Press c to add the first one.",
                    app.theme.muted(),
                )));
            }
            for (index, item) in state.items.iter().enumerate() {
                let selected = index == state.selected;
                lines.push(Line::from(vec![
                    Span::styled(
                        if selected { "▸ " } else { "  " },
                        if selected {
                            app.theme.accent()
                        } else {
                            app.theme.muted()
                        },
                    ),
                    Span::styled(
                        if item.completed { "[x] " } else { "[ ] " },
                        if item.completed {
                            app.theme.success()
                        } else {
                            app.theme.warning()
                        },
                    ),
                    Span::styled(item.text.clone(), app.theme.text()),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "↑/↓ select · Space check/uncheck · c add · r rename · J/K move · x remove · Esc close",
                app.theme.muted(),
            )));
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        Modal::Attachments(state) => {
            let block = modal_block(&format!("Attachments · {}", state.title), app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let mut lines = Vec::new();
            if state.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No attachments yet. Press A to attach a file.",
                    app.theme.muted(),
                )));
            }
            for (index, item) in state.items.iter().enumerate() {
                let selected = index == state.selected;
                lines.push(Line::from(vec![
                    Span::styled(
                        if selected { "▸ " } else { "  " },
                        if selected {
                            app.theme.accent()
                        } else {
                            app.theme.muted()
                        },
                    ),
                    Span::styled(
                        format!("{:<28}", item.filename),
                        if selected {
                            app.theme.selected()
                        } else {
                            app.theme.text()
                        },
                    ),
                    Span::styled(format!("{:<20}", item.content_type), app.theme.muted()),
                    Span::styled(human_bytes(item.size_bytes), app.theme.muted()),
                    Span::styled(
                        if item.is_image { "  image" } else { "" },
                        app.theme.warning(),
                    ),
                ]));
            }
            for (offset, pending) in state.pending.iter().enumerate() {
                let selected = state.items.len() + offset == state.selected;
                lines.push(Line::from(vec![
                    Span::styled(
                        if selected { "▸ " } else { "  " },
                        if selected {
                            app.theme.accent()
                        } else {
                            app.theme.muted()
                        },
                    ),
                    Span::styled("[pending] ", app.theme.warning()),
                    Span::styled(
                        format!("{:<18}", pending.attachment.filename),
                        if selected {
                            app.theme.selected()
                        } else {
                            app.theme.text()
                        },
                    ),
                    Span::styled(
                        format!("{:<20}", pending.attachment.content_type),
                        app.theme.muted(),
                    ),
                    Span::styled(
                        human_bytes(pending.attachment.size_bytes),
                        app.theme.muted(),
                    ),
                    Span::styled(format!("  {}", pending.state), app.theme.warning()),
                ]));
                if let Some(reason) = &pending.last_error {
                    lines.push(Line::from(Span::styled(
                        format!("            {reason}"),
                        app.theme.muted(),
                    )));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "↑/↓ select · A attach · o open with viewer · E export · x remove · Esc close",
                app.theme.muted(),
            )));
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        Modal::Activity(state) => {
            let block = modal_block(&format!("Activity · {}", state.title), app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let mut lines = Vec::new();
            if state.items.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No recorded activity for this item yet.",
                    app.theme.muted(),
                )));
            }
            for item in &state.items {
                lines.push(Line::from(vec![
                    Span::styled(format!("{:<18}", item.kind), app.theme.muted()),
                    Span::styled(item.summary.clone(), app.theme.text()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  ", app.theme.muted()),
                    Span::styled(
                        format!("{} · {}", item.origin, item.occurred_at_ms),
                        app.theme.muted(),
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "↑/↓/PgUp/PgDn scroll · Esc close",
                app.theme.muted(),
            )));
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((state.scroll, 0)),
                inner,
            );
        }
        Modal::TodayActions { menu, selected } => {
            let block = modal_block("Today/Next actions", app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let items = menu
                .actions
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let selected = index == *selected;
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            if selected { "▸ " } else { "  " },
                            if selected {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(
                            format!("{:?} ", action.kind),
                            if selected {
                                app.theme.selected()
                            } else {
                                app.theme.text()
                            },
                        ),
                        Span::styled(action.explanation.clone(), app.theme.muted()),
                        Span::styled(
                            if action.mutates_authority {
                                "  [explicit confirmation]"
                            } else {
                                ""
                            },
                            app.theme.warning(),
                        ),
                    ]))
                })
                .collect::<Vec<_>>();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(inner);
            render_selectable_list(frame, chunks[0], items, *selected);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Read-only destinations · revision checked · Esc close",
                    app.theme.muted(),
                ))),
                chunks[1],
            );
        }
        Modal::Search {
            query,
            hits,
            selected,
        } => {
            let block = modal_block("Search", app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(2), Constraint::Min(1)])
                .split(inner);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("/ ", app.theme.accent()),
                    Span::styled(query.clone(), app.theme.title()),
                    Span::styled("_", app.theme.muted()),
                ])),
                chunks[0],
            );
            let items = hits
                .iter()
                .enumerate()
                .map(|(index, hit)| {
                    let selected = index == *selected;
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            if selected { "▸ " } else { "  " },
                            if selected {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(
                            hit.label.clone(),
                            if selected {
                                app.theme.selected()
                            } else {
                                app.theme.text()
                            },
                        ),
                        Span::styled(format!("  {}", hit.description), app.theme.muted()),
                    ]))
                })
                .collect::<Vec<_>>();
            render_selectable_list(frame, chunks[1], items, *selected);
        }
        Modal::Message { title, lines } => {
            let block = modal_block(title, app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let lines = lines
                .iter()
                .cloned()
                .map(Line::from)
                .chain(std::iter::once(Line::from("")))
                .chain(std::iter::once(Line::from(Span::styled(
                    "Enter/Esc close",
                    app.theme.muted(),
                ))))
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((app.modal_scroll.min(u16::MAX as usize) as u16, 0)),
                inner,
            );
        }
    }
}

fn draw_form(frame: &mut Frame, area: Rect, app: &App, form: &FormState) {
    let block = form_block(form, area.width, app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    // Labels share one column, but the column yields to the value on narrow
    // terminals instead of pushing the value past the border.
    let label_width = usize::from(inner.width).saturating_sub(8).clamp(10, 22);
    let value_room = usize::from(inner.width)
        .saturating_sub(label_width + 4)
        .max(4);
    let mut blocks: Vec<Vec<Line>> = Vec::new();
    for (index, field) in form.fields.iter().enumerate() {
        let focused = index == form.focused;
        let invalid = form.invalid == Some(index);
        let marker = if invalid {
            "! "
        } else if focused {
            "▸ "
        } else {
            "  "
        };
        let label_style = if invalid {
            app.theme.danger()
        } else if focused {
            app.theme.accent()
        } else {
            app.theme.muted()
        };
        let label = format!(
            "{:<label_width$}",
            truncate_ellipsis(&field.label, label_width)
        );
        let value_style = if focused {
            app.theme.selected()
        } else {
            app.theme.text()
        };
        let mut field_lines = Vec::new();
        if focused && matches!(field.kind, FormFieldKind::Multiline) {
            // Show a small window of wrapped lines around the caret instead of
            // hiding everything behind a `(+N more lines)` suffix.
            let (rows, caret_row, caret_col) =
                multiline_preview(&field.value, field.cursor, value_room, 4);
            for (row_index, row) in rows.iter().enumerate() {
                let lead = if row_index == 0 {
                    vec![
                        Span::styled(marker, label_style),
                        Span::styled(label.clone(), label_style),
                    ]
                } else {
                    vec![Span::styled(
                        format!("  {}", " ".repeat(label_width)),
                        app.theme.muted(),
                    )]
                };
                let shown = if field.value.is_empty() {
                    field.hint.clone()
                } else if row_index == caret_row {
                    insert_caret(row, caret_col)
                } else {
                    row.clone()
                };
                let style = if field.value.is_empty() {
                    app.theme.muted()
                } else if row_index == caret_row {
                    app.theme.selected()
                } else {
                    app.theme.text()
                };
                let mut spans = lead;
                spans.push(Span::styled(shown, style));
                if row_index == 0 && field.required {
                    spans.push(Span::styled(" *", app.theme.warning()));
                }
                field_lines.push(Line::from(spans));
            }
        } else {
            let mut spans = vec![
                Span::styled(marker, label_style),
                Span::styled(label, label_style),
            ];
            match &field.kind {
                FormFieldKind::Boolean => {
                    let checked = field.value == "true";
                    spans.push(Span::styled(
                        if checked { "[x] yes" } else { "[ ] no" },
                        if focused {
                            app.theme.selected()
                        } else {
                            app.theme.text()
                        },
                    ));
                }
                FormFieldKind::Choice(choices) => {
                    let current = choices
                        .iter()
                        .find(|choice| choice.value == field.value)
                        .map(|choice| choice.label.as_str())
                        .unwrap_or(if field.value.is_empty() {
                            "—"
                        } else {
                            field.value.as_str()
                        });
                    spans.push(Span::styled(
                        format!("‹ {} ›", truncate_ellipsis(current, value_room)),
                        value_style,
                    ));
                }
                FormFieldKind::Text | FormFieldKind::Integer => {
                    let shown = if field.value.is_empty() {
                        field.hint.as_str()
                    } else {
                        field.value.as_str()
                    };
                    spans.push(Span::styled(
                        truncate_ellipsis(shown, value_room),
                        if field.value.is_empty() {
                            app.theme.muted()
                        } else {
                            value_style
                        },
                    ));
                    if focused {
                        spans.push(Span::styled("_", app.theme.muted()));
                    }
                }
                FormFieldKind::Multiline => {
                    let first_line = field.value.lines().next().unwrap_or("");
                    let shown = if field.value.is_empty() {
                        field.hint.clone()
                    } else if first_line.is_empty() {
                        "⏎".to_owned()
                    } else {
                        first_line.to_owned()
                    };
                    let mut text = shown;
                    if field.value.contains('\n') {
                        text.push_str(&format!(
                            "  (+{} more line(s))",
                            field.value.lines().count().saturating_sub(1)
                        ));
                    }
                    spans.push(Span::styled(
                        truncate_ellipsis(&text, value_room),
                        if field.value.is_empty() {
                            app.theme.muted()
                        } else {
                            value_style
                        },
                    ));
                }
            }
            if field.required {
                spans.push(Span::styled(" *", app.theme.warning()));
            }
            field_lines.push(Line::from(spans));
        }
        if invalid && let Some(error) = &form.error {
            field_lines.push(Line::from(Span::styled(
                format!("    ! {error}"),
                app.theme.danger(),
            )));
        }
        blocks.push(field_lines);
    }

    let mut header: Vec<Line> = vec![Line::from(Span::styled(
        form.hint.clone(),
        app.theme.muted(),
    ))];
    if form.error.is_some()
        && form.invalid.is_none()
        && let Some(error) = &form.error
    {
        header.push(Line::from(Span::styled(
            format!("error: {error}"),
            app.theme.danger(),
        )));
    }
    header.push(Line::from(""));
    let footer = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Ctrl+S Save · Esc Cancel · Tab next field · Enter saves (adds a line in multi-line)",
            app.theme.muted(),
        )),
    ];
    let budget = usize::from(inner.height)
        .saturating_sub(header.len() + footer.len())
        .max(1);
    let total: usize = blocks.iter().map(Vec::len).sum();
    let mut start_line = 0;
    if total > budget {
        let focus_start: usize = blocks
            .iter()
            .take(form.focused.min(blocks.len()))
            .map(Vec::len)
            .sum();
        let focus_end = focus_start + blocks.get(form.focused).map(Vec::len).unwrap_or(0);
        if focus_end > budget {
            start_line = (focus_end - budget).min(total - budget);
        }
        if focus_start < start_line {
            start_line = focus_start;
        }
    }
    let mut lines = header;
    lines.extend(blocks.into_iter().flatten().skip(start_line).take(budget));
    lines.extend(footer);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn form_block(form: &FormState, width: u16, app: &App) -> Block<'static> {
    let title_room = usize::from(width).saturating_sub(34).clamp(12, 64);
    let title = truncate_ellipsis(&form.title, title_room);
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(app.theme.focused_border())
        .title_top(Line::from(Span::styled(
            format!(" {title} "),
            app.theme.accent(),
        )))
        .title_top(
            Line::from(vec![
                Span::styled("Save ", app.theme.success()),
                Span::styled("Ctrl+S", app.theme.muted()),
                Span::styled("  ·  ", app.theme.muted()),
                Span::styled("Cancel ", app.theme.warning()),
                Span::styled("Esc ", app.theme.muted()),
            ])
            .right_aligned(),
        )
}

/// A caret that is visible without colour or a blinking terminal cursor.
fn insert_caret(row: &str, column: usize) -> String {
    let mut characters: Vec<char> = row.chars().collect();
    let column = column.min(characters.len());
    characters.insert(column, '▏');
    characters.into_iter().collect()
}

/// Rows around the caret for a focused multi-line field: character wrapping
/// keeps the caret column exact, and the window guarantees the caret is shown.
fn multiline_preview(
    value: &str,
    cursor: usize,
    width: usize,
    max_rows: usize,
) -> (Vec<String>, usize, usize) {
    let width = width.max(1);
    let max_rows = max_rows.max(1);
    let lines: Vec<&str> = value.split('\n').collect();
    let mut caret_line = 0;
    let mut caret_column = cursor;
    let mut line_start = 0;
    for (index, line) in lines.iter().enumerate() {
        let length = line.chars().count();
        if cursor <= line_start + length {
            caret_line = index;
            caret_column = cursor.saturating_sub(line_start);
            break;
        }
        line_start += length + 1;
    }
    let mut rows: Vec<String> = Vec::new();
    let mut row_of_line: Vec<usize> = Vec::new();
    for line in &lines {
        row_of_line.push(rows.len());
        let characters: Vec<char> = line.chars().collect();
        if characters.is_empty() {
            rows.push(String::new());
        } else {
            for chunk in characters.chunks(width) {
                rows.push(chunk.iter().collect());
            }
        }
    }
    let line_length = lines
        .get(caret_line)
        .map(|line| line.chars().count())
        .unwrap_or(0);
    let line_rows = if line_length == 0 {
        1
    } else {
        line_length.div_ceil(width)
    };
    let row_in_line = if line_length == 0 {
        0
    } else {
        (caret_column / width).min(line_rows - 1)
    };
    let caret_row = row_of_line.get(caret_line).copied().unwrap_or(0) + row_in_line;
    let caret_row_column = caret_column.saturating_sub(row_in_line * width);
    let start = caret_row
        .saturating_sub(max_rows.saturating_sub(2))
        .min(rows.len().saturating_sub(max_rows));
    let end = (start + max_rows).min(rows.len());
    (
        rows.get(start..end)
            .map(<[String]>::to_vec)
            .unwrap_or_default(),
        caret_row.saturating_sub(start),
        caret_row_column,
    )
}

fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    let block = modal_block("Keyboard help", app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let context = match app.screen {
        Screen::Home => {
            "Home/Today: j/k or arrows select · Enter detail · [/] horizon · d impact · v focus"
        }
        Screen::Inbox => "Inbox: j/k or arrows select · Enter detail · c capture · s actions",
        Screen::Projects => {
            "Projects: j/k or arrows select · Enter open · c new project · W workflow"
        }
        Screen::Agents => "Agents: j/k or arrows select · Enter inspect · r refresh",
        Screen::Project => {
            "Project: arrows select · h/l or arrows component · [/] view · a actions · K criteria"
        }
    };
    let lines = vec![
        Line::from(Span::styled("This screen", app.theme.title())),
        Line::from(context),
        Line::from(""),
        Line::from(Span::styled("Navigation", app.theme.title())),
        Line::from("j / k, ↓ / ↑     move selection"),
        Line::from("h / l            move between components"),
        Line::from("Tab / Shift+Tab  focus sidebar, workspace, details"),
        Line::from("[ / ]             previous / next project view"),
        Line::from("1..9              jump to a view"),
        Line::from("g h / g i / g p / g a   Home, Inbox, Projects, Agents"),
        Line::from(""),
        Line::from(Span::styled("Actions", app.theme.title())),
        Line::from("Enter             open selected item / project"),
        Line::from("a or s            contextual action menu"),
        Line::from("c / C             new task / workflow item"),
        Line::from("e                 edit selected task, project or workflow item"),
        Line::from("E                 quick edit a task (title, priority, next status)"),
        Line::from("S                 status transitions for the selected task"),
        Line::from("K                 acceptance criteria for the selected task"),
        Line::from("H                 activity history for the selected task"),
        Line::from("i / F             attach a file / list attachments"),
        Line::from("W                 change the project workflow (Basic is the default)"),
        Line::from("n                 new knowledge note"),
        Line::from("space             record selected habit occurrence"),
        Line::from("p / P             preview / apply plan"),
        Line::from("d / v             completion impact / task focus (Home)"),
        Line::from("A / G             Today action destinations / project focus"),
        Line::from("[ / ]            planning horizon (Home)"),
        Line::from(": or Ctrl+P       command palette"),
        Line::from("/                 search"),
        Line::from("r                 refresh accepted data"),
        Line::from("f                 connected mode: flush pending commands"),
        Line::from("D                 toggle details"),
        Line::from("?                 this help"),
        Line::from("q                 quit when not typing in a field"),
        Line::from("Ctrl+Q / Ctrl+C  quit from any screen, field or modal"),
        Line::from("Esc               close modal or go back"),
        Line::from(""),
        Line::from(Span::styled(
            "Color is never the only state signal: glyphs and focus borders remain.",
            app.theme.muted(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.modal_scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn render_selectable_list<'a>(
    frame: &mut Frame,
    area: Rect,
    items: Vec<ListItem<'a>>,
    selected: usize,
) {
    let height = usize::from(area.height).max(1);
    let offset = selected.saturating_sub(height.saturating_sub(1));
    let mut state = ListState::default();
    *state.offset_mut() = offset;
    frame.render_stateful_widget(List::new(items), area, &mut state);
}

fn palette_lines<'a>(app: &App, entries: &[PaletteEntry], selected: usize) -> Vec<ListItem<'a>> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let selected = index == selected;
            let style = if !entry.enabled {
                app.theme.muted()
            } else if selected {
                app.theme.selected()
            } else {
                app.theme.text()
            };
            let mut spans = vec![
                Span::styled(
                    if selected {
                        "▸ "
                    } else if !entry.enabled {
                        "× "
                    } else {
                        "  "
                    },
                    if selected {
                        app.theme.accent()
                    } else {
                        app.theme.muted()
                    },
                ),
                Span::styled(entry.label.clone(), style),
            ];
            if entry.recent_rank.is_some() {
                spans.push(Span::styled("  ↺ recent", app.theme.accent()));
            }
            spans.push(Span::styled(
                format!("  {}", entry.description),
                if entry.enabled {
                    app.theme.muted()
                } else {
                    app.theme.warning()
                },
            ));
            ListItem::new(Line::from(spans))
        })
        .collect()
}

/// Bordered pane. Titles are truncated to the pane so a long component or
/// project title never eats the frame's corner.
fn panel_block(title: &str, focused: bool, app: &App, width: u16) -> Block<'static> {
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

fn modal_block(title: &str, app: &App) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(app.theme.focused_border())
        .title(Line::from(Span::styled(
            format!(" {title} "),
            app.theme.accent(),
        )))
}

fn modal_size(area: Rect, modal: &Modal) -> (u16, u16) {
    let width = area.width.saturating_sub(4).clamp(20, 96);
    let height = match modal {
        Modal::Help => area.height.saturating_sub(4).clamp(12, 32),
        Modal::Detail(detail) => {
            let content = 8
                + detail.fields.len() as u16
                + detail.note.lines().count() as u16
                + detail.actions.len() as u16;
            content.min(area.height.saturating_sub(4)).clamp(12, 36)
        }
        Modal::Form(form) => (form.fields.len() as u16 + 8)
            .min(area.height.saturating_sub(4))
            .max(12),
        Modal::Message { lines, .. } => (lines.len() as u16 + 5)
            .min(area.height.saturating_sub(4))
            .max(8),
        Modal::Palette { entries, .. } => (entries.len() as u16 + 5)
            .min(area.height.saturating_sub(4))
            .max(10),
        Modal::TodayActions { menu, .. } => (menu.actions.len() as u16 + 5)
            .min(area.height.saturating_sub(4))
            .max(10),
        Modal::Search { hits, .. } => (hits.len() as u16 + 5)
            .min(area.height.saturating_sub(4))
            .max(8),
        Modal::Criteria(state) => (state.items.len() as u16 + 8)
            .min(area.height.saturating_sub(4))
            .max(10),
        Modal::Attachments(state) => (state.len() as u16 + 10)
            .min(area.height.saturating_sub(4))
            .max(10),
        Modal::Activity(state) => (state.items.len() as u16 + 8)
            .min(area.height.saturating_sub(4))
            .max(10),
        _ => 18,
    };
    (width, height)
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn progress_bar(percent: u16) -> String {
    let filled = (percent.min(100) / 5) as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(20 - filled))
}

fn sparkline(values: &[i64]) -> String {
    if values.is_empty() {
        return "no observations".into();
    }
    let min = *values.iter().min().unwrap_or(&0);
    let max = *values.iter().max().unwrap_or(&0);
    let span = (max - min).max(1);
    values
        .iter()
        .map(|value| {
            let index = ((value - min) * 7 / span).clamp(0, 7) as usize;
            ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][index]
        })
        .collect()
}

fn truncate_ellipsis(value: &str, width: usize) -> String {
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

/// Word-aware wrapping used wherever the renderer must know how many *visual*
/// rows a value occupies (scroll clamping and the scroll indicator).
///
/// Short lines are returned untouched so indentation and spacing survive.
fn wrap_text(value: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if value.chars().count() <= width {
        return vec![value.to_owned()];
    }
    let mut rows = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for word in value.split(' ') {
        let word_width = word.chars().count();
        if word_width > width {
            if !current.is_empty() {
                rows.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            for character in word.chars() {
                chunk.push(character);
                if chunk.chars().count() == width {
                    rows.push(std::mem::take(&mut chunk));
                }
            }
            current = chunk;
            current_width = current.chars().count();
            continue;
        }
        let needed = if current_width == 0 {
            word_width
        } else {
            current_width + 1 + word_width
        };
        if needed > width {
            rows.push(std::mem::take(&mut current));
            current.push_str(word);
            current_width = word_width;
        } else {
            if current_width > 0 {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
        }
    }
    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    rows
}

/// Clamp a scroll offset so it can never sit past the end of the content.
fn clamp_scroll(scroll: usize, total: usize, height: u16) -> u16 {
    let max = total.saturating_sub(usize::from(height));
    u16::try_from(scroll.min(max)).unwrap_or(u16::MAX)
}

/// Extend a selected row with background padding so the highlight reaches the
/// pane edge instead of stopping at the last word. The `▸` marker remains the
/// colour-independent signal.
fn pad_selected_row(app: &App, spans: &mut Vec<Span<'static>>, width: usize) {
    let used: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    if used < width {
        spans.push(Span::styled(
            " ".repeat(width - used),
            app.theme.selected_fill(),
        ));
    }
}

/// Join as many hint segments as fit on one row. The first segment is always
/// kept (it carries the state), later ones are dropped rather than clipped.
fn fitted_hints(app: &App, hints: &[String], width: u16) -> Line<'static> {
    let width = usize::from(width);
    let mut text = String::new();
    for hint in hints {
        let candidate = if text.is_empty() {
            hint.clone()
        } else {
            format!("{text} · {hint}")
        };
        if !text.is_empty() && candidate.chars().count() > width {
            break;
        }
        text = candidate;
    }
    Line::from(Span::styled(
        truncate_ellipsis(&text, width),
        app.theme.muted(),
    ))
}

/// Compact human-readable byte size for attachment rows.
fn human_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}
