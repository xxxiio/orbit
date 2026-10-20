//! Ratatui renderer for the project-centric shell. All data arrives through
//! `App` and resolved `orbit-ui` models; this module never queries storage.
use crate::app::{App, FocusPane, Modal, PaletteEntry, PromptState, Screen, StatusKind};
use crate::theme::{DETAIL_WIDTH, MEDIUM_BREAKPOINT, SIDEBAR_WIDTH, WIDE_BREAKPOINT};
use orbit_ui::model::*;
use orbit_ui::{ComponentBody, LoadStatus, ResolvedComponent, ResolvedView};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.set_viewport(area.width);
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
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
    if let Some(workspace) = &app.workspace {
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
    let list = List::new(items);
    frame.render_widget(list, inner);
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
    if (area.width < 70 || area.height < 12) && count > 1 {
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
    let rects = component_rects(area, &view.layout, count);
    for (index, (component, rect)) in view.components.iter().zip(rects).enumerate() {
        let active = app.focus == FocusPane::Workspace && index == app.active_component;
        draw_component(frame, rect, app, component, active);
    }
}

fn component_rects(
    area: Rect,
    layout: &orbit_blueprint::presentation::ViewLayout,
    count: usize,
) -> Vec<Rect> {
    use orbit_blueprint::presentation::ViewLayoutKind;
    match layout.kind {
        ViewLayoutKind::Vertical => {
            let constraints = vec![Constraint::Ratio(1, count as u32); count];
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
            let row_rects = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Ratio(1, rows as u32); rows])
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
                ]),
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
            let offset = if active { app.scroll } else { 0 };
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
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        ComponentBody::HabitTracker { habits } => {
            let today = format_epoch_ms(crate::app::now_ms());
            let today = today.get(..10).unwrap_or("");
            let lines = habits
                .iter()
                .map(|habit| {
                    let checked = habit.last.starts_with(today) && habit.last != "never";
                    Line::from(vec![
                        Span::styled(if checked { "[x] " } else { "[ ] " }, app.theme.success()),
                        Span::styled(habit.name.clone(), app.theme.text()),
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
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        ComponentBody::MetricHistory { metrics } => {
            let height = usize::from(inner.height) / 2;
            let offset = if active { app.scroll } else { 0 };
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
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        ComponentBody::MilestoneProgress { milestones } => {
            let height = usize::from(inner.height);
            let offset = if active { app.scroll } else { 0 };
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
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
    let offset = if active { app.scroll } else { 0 };
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
                    item.title.clone(),
                    if selected {
                        app.theme.selected()
                    } else {
                        app.theme.text()
                    },
                ),
                Span::styled(format!("  {}", item.subtitle), app.theme.muted()),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), area);
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
    let column_width = (width / columns.len()).max(6);
    let header = columns
        .iter()
        .map(|column| {
            format!(
                "{:<width$}",
                truncate(&column.to_uppercase(), column_width - 1),
                width = column_width
            )
        })
        .collect::<String>();
    let mut lines = vec![Line::from(Span::styled(header, app.theme.muted()))];
    let height = usize::from(area.height.saturating_sub(1));
    let offset = if active { app.scroll } else { 0 };
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
            spans.push(Span::styled(
                format!(
                    "{:<width$}",
                    truncate(cell, column_width - 1),
                    width = column_width
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
    let constraints = vec![Constraint::Ratio(1, columns.len() as u32); columns.len()];
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    let mut next_index = 0usize;
    for (column, rect) in columns.iter().zip(chunks.iter()) {
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
                "  Enter open · c new project · : palette · ? help",
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
        .skip(app.project_scroll())
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
        for (key, value) in detail.fields.iter().take(12) {
            lines.push(Line::from(vec![
                Span::styled(format!("{key}: "), app.theme.muted()),
                Span::styled(value.clone(), app.theme.text()),
            ]));
        }
        if !detail.note.is_empty() {
            lines.push(Line::from(""));
            let note: Vec<String> = detail.note.lines().take(8).map(str::to_owned).collect();
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
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let style = match app.status.kind {
        StatusKind::Success => app.theme.success(),
        StatusKind::Warning | StatusKind::Pending => app.theme.warning(),
        StatusKind::Error => app.theme.danger(),
        StatusKind::Info => app.theme.muted(),
    };
    let hint = match app.screen {
        Screen::Home => "j/k select · Tab focus · Enter detail · a actions · [ ] horizon · ? help",
        Screen::Inbox => "j/k select · a actions · Enter detail · c capture · ? help",
        Screen::Projects => "j/k select · Enter open · c new project · : palette",
        Screen::Agents => "j/k select · Enter detail · r refresh · ? help",
        Screen::Project => {
            "j/k select · h/l component · [ ] view · a actions · c task · C work item"
        }
    };
    let line = Line::from(vec![
        Span::styled(format!(" {} ", app.status.text), style),
        Span::styled(format!("   {hint}"), app.theme.muted()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_modal(frame: &mut Frame, area: Rect, app: &App) {
    let Some(modal) = &app.modal else {
        return;
    };
    let (width, height) = modal_size(area, modal);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);
    match modal {
        Modal::Prompt(prompt) => draw_prompt(frame, popup, app, prompt),
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
            frame.render_widget(List::new(items), inner);
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
            frame.render_widget(List::new(items), chunks[1]);
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
                lines.push(Line::from(vec![
                    Span::styled(format!("{key}: "), app.theme.muted()),
                    Span::styled(value.clone(), app.theme.text()),
                ]));
            }
            if !detail.note.is_empty() {
                lines.push(Line::from(""));
                lines.extend(detail.note.lines().map(Line::from));
            }
            if !detail.actions.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Available actions",
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
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
            frame.render_widget(List::new(items), chunks[0]);
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
            frame.render_widget(List::new(items), chunks[1]);
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
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
    }
}

fn draw_prompt(frame: &mut Frame, area: Rect, app: &App, prompt: &PromptState) {
    let block = modal_block(&prompt.title, app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        Line::from(Span::styled(prompt.hint.clone(), app.theme.muted())),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", app.theme.accent()),
            Span::styled(prompt.value.clone(), app.theme.title()),
            Span::styled("_", app.theme.muted()),
        ]),
        Line::from(""),
        Line::from(Span::styled("Enter submit · Esc cancel", app.theme.muted())),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    let block = modal_block("Keyboard help", app);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
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
        Line::from("c / C             new task / blueprint work item"),
        Line::from("n                 new knowledge note"),
        Line::from("space             record selected habit occurrence"),
        Line::from("p / P             preview / apply plan"),
        Line::from("d / v             completion impact / task focus (Home)"),
        Line::from("A / G             Today action destinations / project focus"),
        Line::from("[ / ]            planning horizon (Home)"),
        Line::from(": or Ctrl+P       command palette"),
        Line::from("/                 search"),
        Line::from("r                 refresh accepted data"),
        Line::from("D                 toggle details"),
        Line::from("?                 this help"),
        Line::from("q                 quit (Home) / back to Home (project)"),
        Line::from("Esc               close modal or return Home"),
        Line::from(""),
        Line::from(Span::styled(
            "Color is never the only state signal: glyphs and focus borders remain.",
            app.theme.muted(),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
            let content = 8 + detail.fields.len() as u16 + detail.note.lines().count() as u16;
            content.min(area.height.saturating_sub(4)).clamp(12, 36)
        }
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
