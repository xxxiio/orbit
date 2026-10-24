//! Workspace component bodies: text, rows, tables, boards, metrics, the
//! projects screen and the detail pane.
use super::layout::{allocate, component_rects, layout_fits, stack};
use super::shell::{panel_block, truncate_ellipsis};
use crate::app::{App, FocusPane, Screen};
use crate::theme::LEGEND_HEIGHT;
use orbit_ui::model::*;
use orbit_ui::{ComponentBody, LoadStatus, ResolvedComponent, ResolvedView};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};

pub(super) fn draw_components(frame: &mut Frame, area: Rect, app: &App, view: &ResolvedView) {
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
        // Selection belongs to the active component even when another pane
        // owns the keyboard; siblings must not paint a phantom cursor.
        let owns_selection = index == app.active_component;
        draw_component(frame, rect, app, component, active, owns_selection);
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
    draw_component(frame, rows[1], app, &view.components[index], active, true);
}
fn draw_component(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    component: &ResolvedComponent,
    active: bool,
    owns_selection: bool,
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
            draw_kanban(
                frame,
                inner,
                app,
                component,
                active,
                owns_selection,
                columns,
            );
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
    owns_selection: bool,
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
    let selected = if owns_selection && total > 0 {
        Some(app.selected.min(total - 1))
    } else {
        None
    };
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
            hints.push("S status".into());
            hints.push("E quick edit".into());
            hints.push("Enter open".into());
        }
        frame.render_widget(Paragraph::new(fitted_hints(app, &hints, area.width)), area);
    }
}

pub(super) fn draw_projects_screen(frame: &mut Frame, area: Rect, app: &App) {
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

pub(super) fn draw_detail(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == FocusPane::Detail;
    let block = panel_block("Details", focused, app, area.width);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = usize::from(inner.width);
    let mut lines: Vec<Line> = Vec::new();
    // Cache-only: rendering must never issue a backend read. The cache is
    // seeded by every input path that changes the subject; a miss shows the
    // neutral placeholder below for one frame.
    if let Some(detail) = app.cached_detail_model() {
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
    let max = if total <= height || height < 2 {
        total.saturating_sub(height)
    } else {
        total.saturating_sub(height - 1)
    };
    app.detail_max_scroll.set(max);
    if total <= height || height < 2 {
        let scroll = if scrollable {
            u16::try_from(requested.min(max)).unwrap_or(u16::MAX)
        } else {
            0
        };
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
        return;
    }
    let visible = height - 1;
    let max_scroll = max;
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
pub(super) fn render_selectable_list<'a>(
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
pub(super) fn human_bytes(bytes: u64) -> String {
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
