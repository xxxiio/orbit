use crate::app::{self, App, FormFieldKind, Modal, StatusKind};
use crate::input::text::{text_backspace, text_delete, text_insert};
use crossterm::event::{KeyCode, KeyModifiers};

pub(crate) fn handle_modal(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Result<bool, String> {
    let Some(mut modal) = app.modal.take() else {
        return Ok(false);
    };
    if matches!(code, KeyCode::Char('q'))
        && !matches!(
            modal,
            Modal::Palette { .. }
                | Modal::Search { .. }
                | Modal::Form(_)
                | Modal::QuitConfirm { .. }
                | Modal::Criteria(_)
                | Modal::Attachments(_)
                | Modal::Console { .. }
        )
    {
        if matches!(modal, Modal::Help) {
            app.modal = None;
            return Ok(false);
        }
        if matches!(modal, Modal::Conflict { .. } | Modal::Pending { .. }) {
            // Restore the modal so unsaved conflict/console text still needs
            // the same confirmation as a dirty form.
            app.modal = Some(modal);
            return Ok(app.request_quit());
        }
        return Ok(app.request_quit());
    }
    let mut keep = true;
    match &mut modal {
        Modal::QuitConfirm { resume } => match code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                return Ok(true);
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                keep = false;
                app.modal = resume.take().map(|modal| *modal);
                app.notify("Quit cancelled; input preserved", StatusKind::Info);
            }
            _ => {}
        },
        Modal::Form(form) => {
            // Ctrl+S saves from any field, including multi-line text where
            // Enter inserts a newline instead of submitting.
            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('s')) {
                let form = form.clone();
                app.submit_form(form);
                return Ok(false);
            }
            // Never type control chords into a field: only Ctrl+S is a form
            // command, everything else is ignored so shortcuts cannot corrupt
            // ordinary text entry.
            if modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(false);
            }
            match code {
                KeyCode::Esc => {
                    if form.dirty {
                        let form = form.clone();
                        keep = false;
                        app.modal = Some(Modal::Confirm {
                            title: "Discard unsaved changes?".into(),
                            body: "The form has unsaved edits.".into(),
                            pending: app::PendingAction::DiscardFormChanges {
                                form: Box::new(form),
                            },
                        });
                    } else {
                        keep = false;
                        app.notify("Form cancelled", StatusKind::Info);
                    }
                }
                KeyCode::Enter => {
                    let multiline = form
                        .fields
                        .get(form.focused)
                        .is_some_and(|field| matches!(field.kind, FormFieldKind::Multiline));
                    if multiline {
                        form.clear_error_for_focused();
                        if let Some(field) = form.fields.get_mut(form.focused) {
                            text_insert(field, '\n');
                        }
                        form.dirty = true;
                    } else {
                        let form = form.clone();
                        keep = false;
                        app.submit_form(form);
                    }
                }
                KeyCode::Tab => {
                    if !form.fields.is_empty() {
                        form.focused = (form.focused + 1) % form.fields.len();
                    }
                }
                KeyCode::BackTab => {
                    if !form.fields.is_empty() {
                        form.focused = (form.focused + form.fields.len() - 1) % form.fields.len();
                    }
                }
                KeyCode::Down => {
                    if !form.fields.is_empty() {
                        form.focused = (form.focused + 1).min(form.fields.len() - 1);
                    }
                }
                KeyCode::Up => {
                    form.focused = form.focused.saturating_sub(1);
                }
                KeyCode::Left | KeyCode::Right => {
                    form.clear_error_for_focused();
                    let delta = if matches!(code, KeyCode::Right) {
                        1
                    } else {
                        -1
                    };
                    if let Some(field) = form.fields.get_mut(form.focused) {
                        match &field.kind {
                            FormFieldKind::Choice(choices) if !choices.is_empty() => {
                                let current = choices
                                    .iter()
                                    .position(|choice| choice.value == field.value)
                                    .unwrap_or(0)
                                    as isize;
                                let next =
                                    (current + delta).clamp(0, choices.len() as isize - 1) as usize;
                                field.value = choices[next].value.clone();
                                form.dirty = true;
                            }
                            FormFieldKind::Boolean => {
                                field.value = if field.value == "true" {
                                    "false".into()
                                } else {
                                    "true".into()
                                };
                                form.dirty = true;
                            }
                            FormFieldKind::Text
                            | FormFieldKind::Multiline
                            | FormFieldKind::Integer => {
                                let len = field.value.chars().count();
                                let next = (field.cursor as isize + delta).clamp(0, len as isize);
                                field.cursor = next as usize;
                            }
                            FormFieldKind::Choice(_) => {}
                        }
                    }
                }
                KeyCode::Home | KeyCode::End => {
                    form.clear_error_for_focused();
                    let end = matches!(code, KeyCode::End);
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        field.cursor = if end { field.value.chars().count() } else { 0 };
                    }
                }
                KeyCode::Delete => {
                    form.clear_error_for_focused();
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        text_delete(field);
                        form.dirty = true;
                    }
                }
                KeyCode::Char(' ') => {
                    form.clear_error_for_focused();
                    if let Some(field) = form.fields.get_mut(form.focused) {
                        match field.kind {
                            FormFieldKind::Boolean => {
                                field.value = if field.value == "true" {
                                    "false".into()
                                } else {
                                    "true".into()
                                };
                                form.dirty = true;
                            }
                            FormFieldKind::Text
                            | FormFieldKind::Multiline
                            | FormFieldKind::Integer => {
                                text_insert(field, ' ');
                                form.dirty = true;
                            }
                            FormFieldKind::Choice(_) => {}
                        }
                    }
                }
                KeyCode::Backspace => {
                    form.clear_error_for_focused();
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        text_backspace(field);
                        form.dirty = true;
                    }
                }
                KeyCode::Char(character) => {
                    form.clear_error_for_focused();
                    if let Some(field) = form.fields.get_mut(form.focused)
                        && matches!(
                            field.kind,
                            FormFieldKind::Text | FormFieldKind::Multiline | FormFieldKind::Integer
                        )
                    {
                        text_insert(field, character);
                        form.dirty = true;
                    }
                }
                _ => {}
            }
        }
        Modal::Confirm {
            title,
            body,
            pending,
        } => match code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let pending = pending.clone();
                let title = title.clone();
                keep = false;
                app.notify(format!("Submitting {title}"), StatusKind::Pending);
                app.execute_pending(pending);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                keep = false;
                if let app::PendingAction::DiscardFormChanges { form } = pending.clone() {
                    app.modal = Some(Modal::Form(*form));
                    app.notify("Form restored; unsaved changes preserved", StatusKind::Info);
                } else {
                    app.notify("Cancelled; no change was submitted", StatusKind::Info);
                }
            }
            _ => {
                let _ = body;
            }
        },
        Modal::ActionMenu {
            actions, selected, ..
        } => match code {
            KeyCode::Esc => {
                keep = false;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(actions.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                *selected = selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                *selected = (*selected + 8).min(actions.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                *selected = selected.saturating_sub(8);
            }
            KeyCode::Enter => {
                let action = actions.get(*selected).cloned();
                if let Some(action) = action {
                    if !action.enabled {
                        // Keep the menu open: the reason is shown for this row,
                        // and nothing is executed.
                        app.notify(
                            action
                                .disabled_reason
                                .clone()
                                .unwrap_or_else(|| format!("{} is unavailable", action.label)),
                            StatusKind::Warning,
                        );
                    } else {
                        keep = false;
                        app.execute_action(action);
                    }
                } else {
                    keep = false;
                }
            }
            _ => {}
        },
        Modal::Palette {
            query,
            entries,
            selected,
        } => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+U clears the query; other control chords are commands,
                // never query text.
                if matches!(code, KeyCode::Char('u')) {
                    query.clear();
                    *entries = app.palette_entries(query);
                    *selected = 0;
                }
            } else {
                match code {
                    KeyCode::Esc => keep = false,
                    KeyCode::Down => {
                        *selected = (*selected + 1).min(entries.len().saturating_sub(1))
                    }
                    KeyCode::Up => *selected = selected.saturating_sub(1),
                    KeyCode::PageDown => {
                        *selected = (*selected + 8).min(entries.len().saturating_sub(1))
                    }
                    KeyCode::PageUp => *selected = selected.saturating_sub(8),
                    KeyCode::Backspace => {
                        query.pop();
                        *entries = app.palette_entries(query);
                        *selected = 0;
                    }
                    KeyCode::Char(character) => {
                        query.push(character);
                        *entries = app.palette_entries(query);
                        *selected = 0;
                    }
                    KeyCode::Enter => {
                        let entry = entries.get(*selected).cloned();
                        match entry {
                            Some(entry) if !entry.enabled => {
                                app.notify(
                                    format!(
                                        "{} is unavailable: {}",
                                        entry.label, entry.description
                                    ),
                                    StatusKind::Warning,
                                );
                            }
                            Some(entry) => {
                                keep = false;
                                if matches!(entry.action, app::PaletteAction::Quit) {
                                    app.modal = Some(Modal::Palette {
                                        query: query.clone(),
                                        entries: entries.clone(),
                                        selected: *selected,
                                    });
                                    return Ok(app.request_quit());
                                }
                                if app.execute_palette_action(entry.action) {
                                    return Ok(true);
                                }
                            }
                            None => keep = false,
                        }
                    }
                    _ => {}
                }
            }
        }
        Modal::Help => match code {
            KeyCode::Esc | KeyCode::Char('?') => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal_scroll = app.modal_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal_scroll = app.modal_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => app.modal_scroll = app.modal_scroll.saturating_add(10),
            KeyCode::PageUp => app.modal_scroll = app.modal_scroll.saturating_sub(10),
            _ => {}
        },
        Modal::Criteria(state) => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                state.selected = (state.selected + 1).min(state.items.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
            }
            KeyCode::Char(' ') => {
                if let Some(item) = state.items.get(state.selected).cloned() {
                    match app.client.set_acceptance_criterion_completed(
                        state.task_id.clone(),
                        item.id,
                        !item.completed,
                    ) {
                        Ok(items) => {
                            state.items = items;
                            state.selected =
                                state.selected.min(state.items.len().saturating_sub(1));
                            let label = if item.completed {
                                "Acceptance criterion reopened"
                            } else {
                                "Acceptance criterion checked"
                            };
                            let (message, kind) = app.mutation_status(label);
                            app.notify(message, kind);
                        }
                        Err(error) => app.notify(
                            format!("Updating the criterion failed: {}", error.message),
                            StatusKind::Error,
                        ),
                    }
                }
            }
            KeyCode::Char('c') => {
                let task_id = state.task_id.clone();
                keep = false;
                app.start_form(app::FormMode::AddCriterion { task_id });
            }
            KeyCode::Char('r') => {
                if let Some(item) = state.items.get(state.selected) {
                    let task_id = state.task_id.clone();
                    let criterion_id = item.id.clone();
                    keep = false;
                    app.start_form(app::FormMode::EditCriterion {
                        task_id,
                        criterion_id,
                    });
                }
            }
            KeyCode::Char('J') => {
                if let Some(item) = state.items.get(state.selected).cloned() {
                    let position = (state.selected + 1).min(state.items.len().saturating_sub(1));
                    match app.client.move_acceptance_criterion(
                        state.task_id.clone(),
                        item.id,
                        position,
                    ) {
                        Ok(items) => {
                            state.items = items;
                            state.selected = position;
                            let (message, kind) =
                                app.mutation_status("Acceptance criterion reordered");
                            app.notify(message, kind);
                        }
                        Err(error) => app.notify(
                            format!("Reordering the criterion failed: {}", error.message),
                            StatusKind::Error,
                        ),
                    }
                }
            }
            KeyCode::Char('K') => {
                if let Some(item) = state.items.get(state.selected).cloned() {
                    let position = state.selected.saturating_sub(1);
                    match app.client.move_acceptance_criterion(
                        state.task_id.clone(),
                        item.id,
                        position,
                    ) {
                        Ok(items) => {
                            state.items = items;
                            state.selected = position;
                            let (message, kind) =
                                app.mutation_status("Acceptance criterion reordered");
                            app.notify(message, kind);
                        }
                        Err(error) => app.notify(
                            format!("Reordering the criterion failed: {}", error.message),
                            StatusKind::Error,
                        ),
                    }
                }
            }
            KeyCode::Char('x') => {
                if let Some(item) = state.items.get(state.selected) {
                    let task_id = state.task_id.clone();
                    let criterion_id = item.id.clone();
                    let text = item.text.clone();
                    keep = false;
                    app.modal = Some(Modal::Confirm {
                        title: "Remove acceptance criterion".into(),
                        body: format!("Remove '{text}'?"),
                        pending: app::PendingAction::RemoveCriterion {
                            task_id,
                            criterion_id,
                            task_title: state.task_title.clone(),
                        },
                    });
                }
            }
            _ => {}
        },
        Modal::Attachments(state) => match code {
            KeyCode::Esc => keep = false,
            KeyCode::Enter => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    keep = false;
                    app.preview_attachment(attachment_id);
                } else {
                    keep = false;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.selected = (state.selected + 1).min(state.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
            }
            KeyCode::Char('A') => {
                let owner_kind = state.owner_kind.clone();
                let owner_id = state.owner_id.clone();
                keep = false;
                app.start_form(app::FormMode::AttachFile {
                    owner_kind,
                    owner_id,
                });
            }
            KeyCode::Char('o') => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    let filename = item.filename.clone();
                    keep = false;
                    app.modal = Some(Modal::Confirm {
                        title: "Open attachment".into(),
                        body: format!(
                            "Export '{}' and hand it to your configured external viewer? Orbit never executes file content.",
                            item.filename
                        ),
                        pending: app::PendingAction::OpenAttachment {
                            attachment_id,
                            filename,
                        },
                    });
                }
            }
            KeyCode::Char('E') => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    keep = false;
                    app.start_form(app::FormMode::ExportAttachment { attachment_id });
                }
            }
            KeyCode::Char('x') => {
                if let Some(item) = state.selected_attachment() {
                    let attachment_id = item.id.clone();
                    let filename = item.filename.clone();
                    let pending = state.selected_is_pending();
                    keep = false;
                    app.modal = Some(Modal::Confirm {
                        title: if pending {
                            "Discard staged upload".into()
                        } else {
                            "Remove attachment".into()
                        },
                        body: if pending {
                            format!(
                                "Discard the locally staged upload for '{filename}'? Nothing was accepted on the server."
                            )
                        } else {
                            format!(
                                "Remove the association for '{filename}'? This requires the server and deletes content only when nothing else references it."
                            )
                        },
                        pending: app::PendingAction::RemoveAttachment {
                            attachment_id,
                            owner_kind: state.owner_kind.clone(),
                            owner_id: state.owner_id.clone(),
                            title: state.title.clone(),
                        },
                    });
                }
            }
            _ => {}
        },
        Modal::Activity(state) => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => state.scroll = state.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => state.scroll = state.scroll.saturating_sub(1),
            KeyCode::PageDown => state.scroll = state.scroll.saturating_add(10),
            KeyCode::PageUp => state.scroll = state.scroll.saturating_sub(10),
            _ => {}
        },
        Modal::Detail(_) => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal_scroll = app.modal_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal_scroll = app.modal_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => app.modal_scroll = app.modal_scroll.saturating_add(10),
            KeyCode::PageUp => app.modal_scroll = app.modal_scroll.saturating_sub(10),
            _ => {}
        },
        Modal::TodayActions { menu, selected } => match code {
            KeyCode::Esc => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(menu.actions.len().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Enter => {
                let index = *selected;
                keep = false;
                app.today_select_action(index);
            }
            _ => {}
        },
        Modal::Search {
            query,
            hits,
            selected,
        } => match code {
            KeyCode::Esc => keep = false,
            KeyCode::Down => *selected = (*selected + 1).min(hits.len().saturating_sub(1)),
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Backspace => {
                query.pop();
                *hits = app.search_hits(query);
                *selected = 0;
            }
            KeyCode::Char(character) => {
                query.push(character);
                *hits = app.search_hits(query);
                *selected = 0;
            }
            KeyCode::Enter => {
                let hit = hits.get(*selected).cloned();
                keep = false;
                if let Some(hit) = hit {
                    app.execute_palette_action(hit.action);
                }
            }
            _ => {}
        },
        Modal::Message { .. } => match code {
            KeyCode::Esc | KeyCode::Enter => keep = false,
            KeyCode::Down | KeyCode::Char('j') => {
                app.modal_scroll = app.modal_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.modal_scroll = app.modal_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => app.modal_scroll = app.modal_scroll.saturating_add(10),
            KeyCode::PageUp => app.modal_scroll = app.modal_scroll.saturating_sub(10),
            _ => {}
        },
        Modal::Pending {
            rows,
            selected,
            detail,
            detail_scroll,
        } => match code {
            KeyCode::Esc => {
                keep = false;
                app.notify(
                    "Pending inspector closed; queued intentions are preserved",
                    StatusKind::Info,
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !rows.is_empty() {
                    *selected = (*selected + 1).min(rows.len() - 1);
                }
                *detail = rows
                    .get(*selected)
                    .map(|row| app.pending_detail_text(&row.id))
                    .unwrap_or_else(|| "No pending commands.".into());
                *detail_scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                *selected = selected.saturating_sub(1);
                *detail = rows
                    .get(*selected)
                    .map(|row| app.pending_detail_text(&row.id))
                    .unwrap_or_else(|| "No pending commands.".into());
                *detail_scroll = 0;
            }
            KeyCode::PageDown => *detail_scroll = detail_scroll.saturating_add(5),
            KeyCode::PageUp => *detail_scroll = detail_scroll.saturating_sub(5),
            KeyCode::Enter => {
                *detail = rows
                    .get(*selected)
                    .map(|row| app.pending_detail_text(&row.id))
                    .unwrap_or_else(|| "No pending commands.".into());
                *detail_scroll = 0;
            }
            KeyCode::Char('r') => match app.client.pending_commands() {
                Ok(fresh) => {
                    *selected = (*selected).min(fresh.len().saturating_sub(1));
                    *detail = fresh
                        .get(*selected)
                        .map(|row| app.pending_detail_text(&row.id))
                        .unwrap_or_else(|| "No pending commands.".into());
                    *rows = fresh;
                    *detail_scroll = 0;
                }
                Err(error) => app.notify(
                    format!("Pending commands unavailable: {error}"),
                    StatusKind::Error,
                ),
            },
            KeyCode::Char('f') => {
                let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
                let (fresh, fresh_detail) = app.pending_flush_report(&ids);
                *selected = (*selected).min(fresh.len().saturating_sub(1));
                *rows = fresh;
                *detail = fresh_detail;
                *detail_scroll = 0;
            }
            KeyCode::Char('c') => {
                let target = rows
                    .get(*selected)
                    .map(|row| (row.id.clone(), row.conflict));
                match target {
                    Some((id, true)) => {
                        keep = false;
                        app.open_conflict(&id);
                    }
                    Some((_, false)) => app.notify(
                        "That command is pending, not rejected; only a rejected command can be reviewed",
                        StatusKind::Warning,
                    ),
                    None => app.notify("No pending command selected", StatusKind::Warning),
                }
            }
            _ => {}
        },
        Modal::Conflict {
            command_id,
            replacement,
            error,
            review,
            ..
        } => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                if matches!(code, KeyCode::Char('u')) {
                    replacement.clear();
                    *error = None;
                }
            } else {
                match code {
                    KeyCode::Esc => {
                        keep = false;
                        app.notify(
                            "Conflict review cancelled; the rejected command is preserved",
                            StatusKind::Info,
                        );
                    }
                    KeyCode::Enter => {
                        match app.resolve_conflict(command_id, review.as_ref(), replacement) {
                            Ok((message, kind)) => {
                                keep = false;
                                app.notify(message, kind);
                            }
                            Err(message) => *error = Some(message),
                        }
                    }
                    KeyCode::Backspace => {
                        replacement.pop();
                        *error = None;
                    }
                    KeyCode::Char(character) => {
                        replacement.push(character);
                        *error = None;
                    }
                    _ => {}
                }
            }
        }
        Modal::Console {
            input,
            suggestions,
            selected,
            output,
            error,
        } => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                if matches!(code, KeyCode::Char('u')) {
                    input.clear();
                    *suggestions = app.console_suggestions(input);
                    *selected = 0;
                    *error = None;
                }
            } else {
                match code {
                    KeyCode::Esc => keep = false,
                    KeyCode::Down => {
                        *selected = (*selected + 1).min(suggestions.len().saturating_sub(1))
                    }
                    KeyCode::Up => *selected = selected.saturating_sub(1),
                    KeyCode::PageDown => {
                        *selected = (*selected + 8).min(suggestions.len().saturating_sub(1))
                    }
                    KeyCode::PageUp => *selected = selected.saturating_sub(8),
                    KeyCode::Tab => {
                        if let Some(suggestion) = suggestions.get(*selected) {
                            *input = suggestion.clone();
                            *suggestions = app.console_suggestions(input);
                            *selected = 0;
                        }
                    }
                    KeyCode::Backspace => {
                        input.pop();
                        *suggestions = app.console_suggestions(input);
                        *selected = 0;
                        *error = None;
                    }
                    KeyCode::Enter => {
                        if input.trim().is_empty() {
                            if let Some(suggestion) = suggestions.get(*selected).cloned() {
                                *input = suggestion;
                                *suggestions = app.console_suggestions(input);
                                *selected = 0;
                            }
                        } else {
                            let source = input.clone();
                            match app.console_run(&source) {
                                app::ConsoleOutcome::Lines(lines) => {
                                    input.clear();
                                    *suggestions = app.console_suggestions(input);
                                    *selected = 0;
                                    *error = None;
                                    output.extend(lines);
                                    if output.len() > 200 {
                                        output.drain(0..output.len() - 200);
                                    }
                                }
                                app::ConsoleOutcome::Error(message) => {
                                    *error = Some(message);
                                }
                                app::ConsoleOutcome::Confirm {
                                    title,
                                    body,
                                    pending,
                                } => {
                                    keep = false;
                                    app.modal = Some(Modal::Confirm {
                                        title,
                                        body,
                                        pending,
                                    });
                                }
                                app::ConsoleOutcome::Conflict(state) => {
                                    keep = false;
                                    app.modal = Some(Modal::Conflict {
                                        command_id: state.command_id,
                                        review_lines: state.review_lines,
                                        replacement: state.replacement,
                                        error: state.error,
                                        review: state.review,
                                    });
                                }
                            }
                        }
                    }
                    KeyCode::Char(character) => {
                        input.push(character);
                        *suggestions = app.console_suggestions(input);
                        *selected = 0;
                        *error = None;
                    }
                    _ => {}
                }
            }
        }
    }
    if keep {
        app.modal = Some(modal);
    }
    Ok(false)
}
