//! Ratatui renderer for the project-centric shell. All data arrives through
//! `App` and resolved `orbit-ui` models; this module never queries storage.
use crate::app::{
    App, FocusPane, FormFieldKind, FormState, Modal, PaletteEntry, Screen, StatusKind,
};
use crate::theme::{DETAIL_WIDTH, MEDIUM_BREAKPOINT, SIDEBAR_WIDTH, WIDE_BREAKPOINT};
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
        Span::styled(segment, app.theme.title()),
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
    if app.client.is_connected() {
        spans.push(Span::styled(
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
        ));
    } else if let Some(workspace) = &app.workspace {
        spans.push(Span::styled(
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
        ));
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
        if app.focus == FocusPane::Sidebar {
            draw_sidebar(frame, area, app);
        } else {
            draw_main(frame, area, app);
        }
        return;
    }
    let show_detail = area.width >= WIDE_BREAKPOINT && app.show_detail;
    let constraints = if show_detail {
        vec![
            Constraint::Length(SIDEBAR_WIDTH),
            Constraint::Min(30),
            Constraint::Length(DETAIL_WIDTH),
        ]
    } else {
        vec![Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(30)]
    };
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    draw_sidebar(frame, chunks[0], app);
    draw_main(frame, chunks[1], app);
    if show_detail {
        draw_detail(frame, chunks[2], app);
    }
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == FocusPane::Sidebar;
    let block = panel_block("Orbit", focused, app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
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
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                marker,
                if selected {
                    app.theme.accent()
                } else {
                    app.theme.muted()
                },
            ),
            Span::styled(current_marker, app.theme.muted()),
            Span::styled(app.sidebar_label(index), style),
        ])));
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
    let mut spans = Vec::new();
    for (index, view) in project.views.iter().enumerate() {
        let selected = index == project.active_view;
        let style = if selected {
            app.theme.accent()
        } else {
            app.theme.muted()
        };
        spans.push(Span::styled(
            format!(" {} {} ", if selected { "▸" } else { " " }, view.title),
            if selected {
                style.add_modifier(Modifier::UNDERLINED)
            } else {
                style
            },
        ));
        if index + 1 < project.views.len() {
            spans.push(Span::styled("│", app.theme.muted()));
        }
    }
    spans.push(Span::styled(
        if project.generated {
            "   generated default · [ ] switch view"
        } else {
            "   [ ] switch view"
        },
        app.theme.muted(),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_components(frame: &mut Frame, area: Rect, app: &App, view: &ResolvedView) {
    let count = view.components.len();
    if count == 0 {
        frame.render_widget(
            Paragraph::new("This view declares no components.").block(panel_block(
                &view.title,
                app.focus == FocusPane::Workspace,
                app,
            )),
            area,
        );
        return;
    }
    if (area.width < 60 || area.height < 10) && count > 1 {
        let index = app.active_component.min(count - 1);
        let active = app.focus == FocusPane::Workspace;
        let mut focused = area;
        focused.y = focused.y.saturating_add(1);
        focused.height = focused.height.saturating_sub(1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("component {}/{} · h/l move", index + 1, count),
                app.theme.muted(),
            ))),
            area,
        );
        draw_component(frame, focused, app, &view.components[index], active);
        return;
    }
    let rects = component_rects(area, &view.layout, &view.components);
    for (index, (component, rect)) in view.components.iter().zip(rects).enumerate() {
        let active = app.focus == FocusPane::Workspace && index == app.active_component;
        draw_component(frame, rect, app, component, active);
    }
}

fn preferred_height(component: &ResolvedComponent) -> u16 {
    match &component.body {
        ComponentBody::Text { lines, .. } => 3 + lines.len().min(12) as u16,
        ComponentBody::Metric { .. } => 5,
        ComponentBody::Detail { properties, note } => {
            5 + properties.len().min(10) as u16 + note.lines().take(6).count() as u16
        }
        ComponentBody::Kanban { .. } => 14,
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

fn component_rects(
    area: Rect,
    layout: &orbit_blueprint::presentation::ViewLayout,
    components: &[ResolvedComponent],
) -> Vec<Rect> {
    use orbit_blueprint::presentation::ViewLayoutKind;
    let count = components.len();
    match layout.kind {
        ViewLayoutKind::Vertical => {
            let weights: Vec<u16> = components.iter().map(preferred_height).collect();
            let total: u16 = weights.iter().sum::<u16>().max(1);
            let constraints = weights
                .iter()
                .map(|weight| Constraint::Ratio(u32::from(*weight), u32::from(total)))
                .collect::<Vec<_>>();
            Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(area)
                .to_vec()
        }
        ViewLayoutKind::Horizontal => {
            let constraints = vec![Constraint::Ratio(1, count as u32); count];
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints(constraints)
                .split(area)
                .to_vec()
        }
        ViewLayoutKind::Grid => {
            let columns = usize::from(layout.columns.max(1)).min(count.max(1));
            let rows = count.div_ceil(columns);
            let row_weights: Vec<u16> = (0..rows)
                .map(|row| {
                    components[row * columns..count.min((row + 1) * columns)]
                        .iter()
                        .map(preferred_height)
                        .max()
                        .unwrap_or(5)
                })
                .collect();
            let total: u16 = row_weights.iter().sum::<u16>().max(1);
            let row_rects = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    row_weights
                        .iter()
                        .map(|weight| Constraint::Ratio(u32::from(*weight), u32::from(total)))
                        .collect::<Vec<_>>(),
                )
                .split(area);
            let mut rects = Vec::new();
            for row in row_rects.iter() {
                let in_row = count.saturating_sub(rects.len()).min(columns);
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(vec![Constraint::Ratio(1, columns as u32); columns])
                    .split(*row);
                rects.extend(cols.iter().copied().take(in_row));
            }
            rects
        }
    }
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
    let block = panel_block(&title, active, app);
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
            let mut text = vec![Line::from(Span::styled(heading.clone(), app.theme.title()))];
            text.extend(lines.iter().cloned().map(Line::from));
            let scroll = if active {
                app.scroll.min(u16::MAX as usize) as u16
            } else {
                0
            };
            frame.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                inner,
            );
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
            let mut lines: Vec<Line> = properties
                .iter()
                .map(|property| {
                    Line::from(vec![
                        Span::styled(format!("{:<16}", property.key), app.theme.muted()),
                        Span::styled(property.value.clone(), app.theme.text()),
                    ])
                })
                .collect();
            if !note.is_empty() {
                if !lines.is_empty() {
                    lines.push(Line::from(""));
                }
                lines.extend(note.lines().map(Line::from));
            }
            let scroll = if active {
                app.scroll.min(u16::MAX as usize) as u16
            } else {
                0
            };
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((scroll, 0)),
                inner,
            );
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
    let title_width = (width * 2 / 3).saturating_sub(4).max(6);
    let subtitle_width = width / 3;
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
                Span::styled(
                    format!("  {}", truncate_ellipsis(&item.subtitle, subtitle_width)),
                    app.theme.muted(),
                ),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible), area);
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
    if columns.is_empty() {
        return;
    }
    let width = usize::from(area.width);
    let status_columns = columns
        .iter()
        .filter(|column| matches!(column.as_str(), "status" | "state"))
        .count();
    let available = width.saturating_sub(2 + status_columns * 2);
    let column_width = (available / columns.len()).max(4);
    let header = format!(
        "  {}",
        columns
            .iter()
            .map(|column| {
                format!(
                    "{:<width$}",
                    truncate_ellipsis(&column.to_uppercase(), column_width.saturating_sub(1)),
                    width = column_width
                )
            })
            .collect::<String>()
    );
    let mut lines = vec![Line::from(Span::styled(header, app.theme.muted()))];
    let height = usize::from(area.height.saturating_sub(1));
    let offset = if active {
        app.selected.saturating_sub(height.saturating_sub(1))
    } else {
        0
    };
    for (index, row) in rows.iter().enumerate().skip(offset).take(height) {
        let selected = active && index == app.selected;
        let mut spans = vec![Span::styled(
            if selected { "▸ " } else { "  " },
            if selected {
                app.theme.accent()
            } else {
                app.theme.muted()
            },
        )];
        for (cell_index, cell) in row.cells.iter().enumerate() {
            if columns
                .get(cell_index)
                .is_some_and(|c| c.as_str() == "status" || c.as_str() == "state")
            {
                spans.push(Span::styled(
                    format!("{} ", crate::theme::Theme::status_glyph(cell)),
                    app.theme.status(cell),
                ));
            }
            let cell_width = if columns
                .get(cell_index)
                .is_some_and(|c| c.as_str() == "status" || c.as_str() == "state")
            {
                column_width.saturating_sub(2)
            } else {
                column_width
            };
            spans.push(Span::styled(
                format!(
                    "{:<width$}",
                    truncate_ellipsis(cell, cell_width.saturating_sub(1)),
                    width = cell_width
                ),
                if selected {
                    app.theme.selected()
                } else {
                    app.theme.text()
                },
            ));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_kanban(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    _component: &ResolvedComponent,
    active: bool,
    columns: &[KanbanColumnModel],
) {
    if columns.is_empty() {
        return;
    }
    let min_column_width = 18u16;
    let visible = (area.width / min_column_width).clamp(1, columns.len() as u16) as usize;
    let mut selected_column = 0usize;
    let mut counted = 0usize;
    for (index, column) in columns.iter().enumerate() {
        if active && app.selected < counted + column.items.len() {
            selected_column = index;
            break;
        }
        counted += column.items.len();
    }
    let start = selected_column
        .saturating_sub(visible / 2)
        .min(columns.len().saturating_sub(visible));
    let window = &columns[start..start + visible];
    let body_area = if columns.len() > visible {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(
                    "columns {}-{} of {} · selection follows cards",
                    start + 1,
                    start + visible,
                    columns.len()
                ),
                app.theme.muted(),
            ))),
            chunks[0],
        );
        chunks[1]
    } else {
        area
    };
    let constraints = vec![Constraint::Ratio(1, window.len() as u32); window.len()];
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(body_area);
    let mut next_index = columns[..start]
        .iter()
        .map(|column| column.items.len())
        .sum::<usize>();
    for (column, rect) in window.iter().zip(chunks.iter()) {
        let column_start = next_index;
        next_index += column.items.len();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(app.theme.border())
            .title(Line::from(Span::styled(
                format!(" {} ({}) ", column.title, column.items.len()),
                app.theme.title(),
            )));
        let inner = block.inner(*rect);
        frame.render_widget(block, *rect);
        let height = usize::from(inner.height);
        let column_selected = active
            && app.selected >= column_start
            && app.selected < column_start + column.items.len();
        let offset = if column_selected {
            (app.selected - column_start).saturating_sub(height.saturating_sub(1))
        } else {
            0
        };
        let mut lines = Vec::new();
        for (item_index, item) in column.items.iter().enumerate().skip(offset).take(height) {
            let selected = active && column_start + item_index == app.selected;
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
                    format!("{} ", crate::theme::Theme::status_glyph(&item.status)),
                    app.theme.status(&item.status),
                ),
                Span::styled(
                    truncate(&item.title, usize::from(inner.width).saturating_sub(4)),
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ),
            ]));
        }
        if column.items.is_empty() {
            lines.push(Line::from(Span::styled("  empty", app.theme.muted())));
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn draw_projects_screen(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(3)])
        .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Projects", app.theme.title()),
            Span::styled(
                "  Enter open · c new project · : palette · q quit · ? help",
                app.theme.muted(),
            ),
        ])),
        chunks[0],
    );
    let block = panel_block(
        &format!("{} project(s)", app.projects.len()),
        app.focus == FocusPane::Workspace,
        app,
    );
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);
    let height = usize::from(inner.height);
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
                    format!("{:<28}", truncate(&project.name, 27)),
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ),
                Span::styled(
                    format!("{} ", crate::theme::Theme::status_glyph(&project.status)),
                    app.theme.status(&project.status),
                ),
                Span::styled(
                    format!(
                        "{} · criteria {}/{}{}",
                        project.mode,
                        project.criteria_satisfied,
                        project.criteria_total,
                        if project.completion_ready {
                            " · ready"
                        } else {
                            ""
                        }
                    ),
                    app.theme.muted(),
                ),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel_block("Details", app.focus == FocusPane::Detail, app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let detail = app.detail_model();
    let mut lines = Vec::new();
    if let Some(detail) = detail {
        lines.push(Line::from(Span::styled(
            detail.title.clone(),
            app.theme.title(),
        )));
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", crate::theme::Theme::status_glyph(&detail.status)),
                app.theme.status(&detail.status),
            ),
            Span::styled(detail.subtitle.clone(), app.theme.muted()),
        ]));
        lines.push(Line::from(""));
        for (key, value) in detail.fields.iter() {
            lines.push(Line::from(vec![
                Span::styled(format!("{key}: "), app.theme.muted()),
                Span::styled(value.clone(), app.theme.text()),
            ]));
        }
        if !detail.note.is_empty() {
            lines.push(Line::from(""));
            let note: Vec<String> = detail.note.lines().map(str::to_owned).collect();
            lines.extend(note.into_iter().map(Line::from));
        }
        if !detail.actions.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("{} actions · a menu", detail.actions.len()),
                app.theme.muted(),
            )));
        }
    } else if app.screen == Screen::Projects {
        lines.push(Line::from(Span::styled("Projects", app.theme.title())));
        lines.push(Line::from(Span::styled(
            "Select a project and press Enter to open its blueprint views.",
            app.theme.muted(),
        )));
    } else if let Some(project) = &app.project {
        lines.push(Line::from(Span::styled(
            project.name.clone(),
            app.theme.title(),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "{} · {}",
                project.blueprint_label, project.data.project.status
            ),
            app.theme.muted(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "{} tasks · {} work items · {} milestones",
                project.data.tasks.len(),
                project.data.work_items.len(),
                project.data.milestones.len()
            ),
            app.theme.text(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Select an item to see properties, dependencies and actions.",
            app.theme.muted(),
        )));
    }
    let scroll = if app.focus == FocusPane::Detail {
        app.detail_scroll.min(u16::MAX as usize) as u16
    } else {
        0
    };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        inner,
    );
}

/// Keys that directly execute a contextual action. Only actions with a real
/// binding appear with a key hint; everything else is reachable from the
/// action menu.
fn action_hotkey(id: &str) -> Option<&'static str> {
    match id {
        "open" | "open_knowledge" => Some("Enter"),
        "create_task" => Some("c"),
        "edit_task" | "edit_project" | "edit_work_item" | "edit_task_details" => Some("e"),
        "set_task_status" | "complete_task" => Some("s"),
        "manage_acceptance_criteria" => Some("K"),
        "task_activity" => Some("H"),
        "attach_file" => Some("i"),
        "open_attachments" => Some("F"),
        "change_workflow" | "apply_basic_workflow" => Some("W"),
        "add_dependency" => Some("d"),
        "remove_dependency" => Some("D"),
        "refresh" => Some("r"),
        "search" => Some("/"),
        "command_palette" => Some("Ctrl+P"),
        "help" => Some("?"),
        _ => None,
    }
}

/// Bottom action bar: the most useful executable operations for the current
/// selection, each with a real key binding when one exists.
fn draw_action_bar(frame: &mut Frame, area: Rect, app: &App) {
    let actions = app.available_actions();
    let mut spans = vec![Span::styled(" Actions: ", app.theme.title())];
    let mut shown = 0;
    for action in actions.iter().filter(|action| action.enabled) {
        let Some(key) = action_hotkey(&action.id) else {
            continue;
        };
        if shown == 4 {
            break;
        }
        if shown > 0 {
            spans.push(Span::styled(" · ", app.theme.muted()));
        }
        spans.push(Span::styled(format!("{key} "), app.theme.accent()));
        spans.push(Span::styled(action.label.clone(), app.theme.text()));
        shown += 1;
    }
    if shown == 0 {
        spans.push(Span::styled(
            "Enter open · a actions · / search · ? help",
            app.theme.muted(),
        ));
    } else {
        spans.push(Span::styled(" · a more actions", app.theme.muted()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let style = match app.status.kind {
        StatusKind::Success => app.theme.success(),
        StatusKind::Warning | StatusKind::Pending => app.theme.warning(),
        StatusKind::Error => app.theme.danger(),
        StatusKind::Info => app.theme.muted(),
    };
    let hint = match app.screen {
        Screen::Home => "j/k select · Tab focus · Enter detail · Esc back",
        Screen::Inbox => "j/k select · Enter detail · c capture · Esc back",
        Screen::Projects => "j/k select · Enter open · c new project · Esc back",
        Screen::Agents => "j/k select · Enter detail · r refresh · Esc back",
        Screen::Project => "j/k select · h/l component · [ ] view · Esc back",
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    draw_action_bar(frame, rows[0], app);
    let line = Line::from(vec![
        Span::styled(" Ctrl+Q quit · q quit (not typing) ", style),
        Span::styled(format!(" {} ", app.status.text), style),
        Span::styled(format!("   {hint}"), app.theme.muted()),
    ]);
    frame.render_widget(Paragraph::new(line), rows[1]);
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
            selected,
        } => {
            let block = modal_block(&format!("Actions · {title}"), app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let items = actions
                .iter()
                .enumerate()
                .map(|(index, action)| {
                    let marker = if index == *selected { "▸ " } else { "  " };
                    let style = if !action.enabled {
                        app.theme.muted()
                    } else if index == *selected {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    };
                    let suffix = action
                        .disabled_reason
                        .as_ref()
                        .map(|reason| format!("  — {reason}"))
                        .unwrap_or_default();
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            marker,
                            if index == *selected {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(action.label.clone(), style),
                        Span::styled(suffix, app.theme.muted()),
                    ]))
                })
                .collect::<Vec<_>>();
            render_selectable_list(frame, inner, items, *selected);
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
                .constraints([Constraint::Length(2), Constraint::Min(1)])
                .split(inner);
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("> ", app.theme.accent()),
                    Span::styled(query.clone(), app.theme.title()),
                    Span::styled("_", app.theme.muted()),
                ])),
                chunks[0],
            );
            let items = palette_lines(app, entries, *selected);
            render_selectable_list(frame, chunks[1], items, *selected);
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
    let block = modal_block(&form.title, app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        form.hint.clone(),
        app.theme.muted(),
    )));
    if let Some(error) = &form.error {
        lines.push(Line::from(Span::styled(
            format!("error: {error}"),
            app.theme.danger(),
        )));
    }
    lines.push(Line::from(""));
    let visible_fields = usize::from(inner.height.saturating_sub(6)).max(1);
    let start = if form.focused >= visible_fields {
        form.focused + 1 - visible_fields
    } else {
        0
    };
    for (index, field) in form
        .fields
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_fields)
    {
        let focused = index == form.focused;
        let marker = if focused { "▸ " } else { "  " };
        let label_style = if focused {
            app.theme.accent()
        } else {
            app.theme.muted()
        };
        let mut spans = vec![
            Span::styled(marker, label_style),
            Span::styled(format!("{:<22}", field.label), label_style),
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
                    format!("‹ {current} ›"),
                    if focused {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ));
            }
            FormFieldKind::Text | FormFieldKind::Integer => {
                let shown = if field.value.is_empty() {
                    field.hint.as_str()
                } else {
                    field.value.as_str()
                };
                spans.push(Span::styled(
                    shown.to_owned(),
                    if field.value.is_empty() {
                        app.theme.muted()
                    } else if focused {
                        app.theme.selected()
                    } else {
                        app.theme.text()
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
                    text,
                    if field.value.is_empty() {
                        app.theme.muted()
                    } else if focused {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ));
                if focused {
                    spans.push(Span::styled("_", app.theme.muted()));
                }
            }
        }
        if field.required {
            spans.push(Span::styled(" *", app.theme.warning()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Tab/Shift+Tab field · ←/→ or Space choose/toggle · Enter next/submit · Ctrl+S save · Esc cancel",
        app.theme.muted(),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
            ListItem::new(Line::from(vec![
                Span::styled(
                    if selected { "▸ " } else { "  " },
                    if selected {
                        app.theme.accent()
                    } else {
                        app.theme.muted()
                    },
                ),
                Span::styled(entry.label.clone(), style),
                Span::styled(format!("  {}", entry.description), app.theme.muted()),
            ]))
        })
        .collect()
}

fn panel_block(title: &str, focused: bool, app: &App) -> Block<'static> {
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

fn truncate(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut output = String::new();
    for character in value.chars() {
        if output.chars().count() >= width {
            break;
        }
        output.push(character);
    }
    output
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
