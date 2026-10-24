//! Every overlay: forms, help, palette, criteria, attachments, activity,
//! search, pending/conflict review and the connected console.
use super::components::{human_bytes, render_selectable_list};
use super::layout::centered_rect;
use super::shell::{modal_block, truncate_ellipsis};
use crate::app::{App, FormFieldKind, FormState, Modal, PaletteEntry, Screen};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, ListItem, Paragraph, Wrap},
};

pub(super) fn draw_modal(frame: &mut Frame, area: Rect, app: &App) {
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
                let hotkey = app.action_key(action).filter(|key| {
                    actions
                        .iter()
                        .filter(|other| app.action_key(other) == Some(*key))
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
        Modal::Pending {
            rows,
            selected,
            detail,
            detail_scroll,
        } => {
            let block = modal_block(
                &format!("Pending commands · {} queued (not accepted)", rows.len()),
                app,
            );
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                    Constraint::Length(6),
                    Constraint::Length(1),
                ])
                .split(inner);
            let items = if rows.is_empty() {
                vec![ListItem::new(Line::from(Span::styled(
                    "No pending commands. Nothing is queued.",
                    app.theme.muted(),
                )))]
            } else {
                rows.iter()
                    .enumerate()
                    .map(|(index, row)| {
                        let is_selected = index == *selected;
                        let tag = if row.conflict {
                            "[conflict]"
                        } else {
                            "[pending]"
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                if is_selected { "▸ " } else { "  " },
                                if is_selected {
                                    app.theme.accent()
                                } else {
                                    app.theme.muted()
                                },
                            ),
                            Span::styled(tag, app.theme.warning()),
                            Span::styled(
                                format!(" {:<10}", truncate_ellipsis(&row.kind, 10)),
                                app.theme.muted(),
                            ),
                            Span::styled(
                                row.summary.clone(),
                                if is_selected {
                                    app.theme.selected()
                                } else {
                                    app.theme.text()
                                },
                            ),
                        ]))
                    })
                    .collect()
            };
            render_selectable_list(frame, chunks[0], items, *selected);
            frame.render_widget(
                Paragraph::new(detail.clone())
                    .wrap(Wrap { trim: false })
                    .scroll(((*detail_scroll).min(u16::MAX as usize) as u16, 0)),
                chunks[1],
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "f flush · c review conflict · Enter detail · r refresh · Esc close · queued ≠ accepted",
                    app.theme.muted(),
                ))),
                chunks[2],
            );
        }
        Modal::Conflict {
            review_lines,
            replacement,
            error,
            ..
        } => {
            let block = modal_block("Conflict review · rejected, not accepted", app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),
                    Constraint::Length(2),
                    Constraint::Length(1),
                ])
                .split(inner);
            frame.render_widget(
                Paragraph::new(
                    review_lines
                        .iter()
                        .cloned()
                        .map(Line::from)
                        .collect::<Vec<_>>(),
                )
                .wrap(Wrap { trim: false })
                .scroll((app.modal_scroll.min(u16::MAX as usize) as u16, 0)),
                chunks[0],
            );
            let input = Line::from(vec![
                Span::styled("> replacement: ", app.theme.accent()),
                Span::styled(replacement.clone(), app.theme.title()),
                Span::styled("_", app.theme.muted()),
            ]);
            let message = error
                .clone()
                .map(|error| Span::styled(error, app.theme.danger()))
                .unwrap_or_else(|| {
                    Span::styled(
                        "A replacement is new pending intent, never an accepted change.",
                        app.theme.muted(),
                    )
                });
            frame.render_widget(
                Paragraph::new(vec![input, Line::from(message)]).wrap(Wrap { trim: false }),
                chunks[1],
            );
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Enter validate + queue PENDING replacement · ↑/↓ PgUp/PgDn scroll · Ctrl+U clear · Esc cancel (original kept)",
                    app.theme.muted(),
                ))),
                chunks[2],
            );
        }
        Modal::Console {
            input,
            suggestions,
            selected,
            output,
            error,
        } => {
            let block = modal_block("Connected commands · queued intents are PENDING", app);
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Min(3),
                    Constraint::Length(7),
                    Constraint::Length(1),
                ])
                .split(inner);
            let mut prompt = vec![Line::from(vec![
                Span::styled("> ", app.theme.accent()),
                Span::styled(input.clone(), app.theme.title()),
                Span::styled("_", app.theme.muted()),
            ])];
            prompt.push(Line::from(Span::styled(
                error
                    .clone()
                    .unwrap_or_else(|| "Enter validates · Tab fills a suggestion".into()),
                if error.is_some() {
                    app.theme.danger()
                } else {
                    app.theme.muted()
                },
            )));
            frame.render_widget(Paragraph::new(prompt).wrap(Wrap { trim: false }), chunks[0]);
            let output = output
                .iter()
                .rev()
                .take(usize::from(chunks[1].height).max(1))
                .rev()
                .cloned()
                .map(Line::from)
                .collect::<Vec<_>>();
            frame.render_widget(
                Paragraph::new(output)
                    .wrap(Wrap { trim: false })
                    .scroll((app.modal_scroll.min(u16::MAX as usize) as u16, 0)),
                chunks[1],
            );
            let items = suggestions
                .iter()
                .enumerate()
                .map(|(index, suggestion)| {
                    let is_selected = index == *selected;
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            if is_selected { "▸ " } else { "  " },
                            if is_selected {
                                app.theme.accent()
                            } else {
                                app.theme.muted()
                            },
                        ),
                        Span::styled(
                            suggestion.clone(),
                            if is_selected {
                                app.theme.selected()
                            } else {
                                app.theme.muted()
                            },
                        ),
                    ]))
                })
                .collect::<Vec<_>>();
            render_selectable_list(frame, chunks[2], items, *selected);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Enter run · Tab fill · ↑/↓ suggest · Ctrl+U clear · Esc close · nothing is accepted on queue",
                    app.theme.muted(),
                ))),
                chunks[3],
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
    let mut caret_row = row_of_line.get(caret_line).copied().unwrap_or(0) + row_in_line;
    let mut caret_row_column = caret_column.saturating_sub(row_in_line * width);
    // A caret at the end of an exactly full wrapped row needs a next row to be
    // visible: on the final line insert one, otherwise clamp to the last cell
    // of that row so the following line keeps its place.
    if caret_column > 0 && caret_column == line_length && caret_column.is_multiple_of(width) {
        if caret_line + 1 == lines.len() {
            let insert_at = row_of_line.get(caret_line).copied().unwrap_or(0) + line_rows;
            let insert_at = insert_at.min(rows.len());
            rows.insert(insert_at, String::new());
            caret_row = insert_at;
            caret_row_column = 0;
        } else {
            caret_row_column = width - 1;
        }
    }
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
        Line::from("E                 quick edit a task (title, priority)"),
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
        Line::from("!                 connected mode: inspect pending and rejected commands"),
        Line::from(": Connected commands…  queue schedule/routine/note commands (PENDING)"),
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
        Modal::Pending { rows, .. } => (rows.len() as u16 + 12)
            .min(area.height.saturating_sub(4))
            .max(14),
        Modal::Conflict { .. } => area.height.saturating_sub(4).clamp(14, 28),
        Modal::Console { .. } => area.height.saturating_sub(4).clamp(14, 26),
        _ => 18,
    };
    (width, height)
}
#[cfg(test)]
mod tests {
    use super::multiline_preview;

    #[test]
    fn caret_at_a_full_wrap_boundary_moves_to_the_next_row() {
        let value = "0123456789";
        let (rows, caret_row, caret_column) = multiline_preview(value, value.len(), 10, 4);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0], "0123456789");
        assert_eq!(rows[1], "");
        assert_eq!((caret_row, caret_column), (1, 0));
    }

    #[test]
    fn caret_at_a_wrap_boundary_before_another_line_stays_on_that_row() {
        let value = "0123456789\nabc";
        let (rows, caret_row, caret_column) = multiline_preview(value, 10, 10, 4);
        assert_eq!(rows, vec!["0123456789", "abc"], "{rows:?}");
        assert_eq!((caret_row, caret_column), (0, 9));
    }

    #[test]
    fn caret_inside_a_wrapped_row_stays_on_that_row() {
        let value = "0123456789";
        let (rows, caret_row, caret_column) = multiline_preview(value, 4, 10, 4);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!((caret_row, caret_column), (0, 4));
    }
}
