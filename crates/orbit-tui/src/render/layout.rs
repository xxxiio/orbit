//! Space allocation: the two-region shell plan and the per-component rects that
//! turn a declared view layout into concrete rectangles.
use crate::app::App;
use crate::theme::{SIDEBAR_MAX, SIDEBAR_MIN, SIDEBAR_WIDTH};
use orbit_ui::{ComponentBody, ResolvedComponent};
use ratatui::layout::{Direction, Rect};

/// Widths for the two-region shell. There is never a third column: the item
/// inspector is an overlay, not a docked pane.
pub(super) struct ShellPlan {
    pub(super) sidebar: u16,
    pub(super) workspace_floor: u16,
}

/// Decide the horizontal budget before drawing anything.
///
/// * the sidebar is sized to its labels inside a compact band;
/// * the workspace is a `Min` constraint, so it absorbs every column beyond the
///   sidebar.
pub(super) fn shell_plan(width: u16, app: &App) -> ShellPlan {
    let sidebar = sidebar_width(width, app);
    let workspace_floor = workspace_min_width(app);
    ShellPlan {
        sidebar,
        workspace_floor: workspace_floor.min(width.saturating_sub(sidebar)),
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
        .map(|index| {
            match index
                .checked_sub(4)
                .and_then(|position| app.projects.get(position))
            {
                // The renderer draws the project name plus a status glyph.
                Some(project) => project.name.chars().count(),
                None => app.sidebar_label(index).chars().count(),
            }
        })
        .max()
        .unwrap_or(0);
    // 2 columns marker + 2 columns current-screen dot + 2 columns project
    // status glyph + 2 border columns.
    let wanted = u16::try_from(widest).unwrap_or(u16::MAX).saturating_add(8);
    wanted.clamp(SIDEBAR_MIN, SIDEBAR_MAX)
}

/// Width the active component needs to stay readable.
fn workspace_min_width(app: &App) -> u16 {
    app.current_component()
        .map(component_min_width)
        .unwrap_or(30)
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

pub(super) fn component_min_width(component: &ResolvedComponent) -> u16 {
    component_min_size(component).0
}

pub(super) fn component_min_height(component: &ResolvedComponent) -> u16 {
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
pub(super) fn layout_fits(
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

pub(super) fn component_rects(
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
pub(super) fn stack(area: Rect, sizes: &[u16], direction: Direction) -> Vec<Rect> {
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
pub(super) fn allocate(total: u16, minimums: &[u16], weights: &[u16]) -> Vec<u16> {
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
pub(super) fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
