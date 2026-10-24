//! Pending-command, conflict and console state plus connected command execution.
use super::*;

/// Connected mode cannot purge unreferenced content objects because the
/// authoritative server owns attachment garbage collection.
pub const CONNECTED_PURGE_REFUSED: &str = "Attachment content cleanup is not available in connected mode; the authoritative server owns attachment garbage collection.";

impl App {
    /// Open the pending-command inspector. Connected deployments only: the
    /// local authority commits synchronously and has no outbox.
    pub fn open_pending(&mut self) {
        if !self.client.is_connected() {
            self.notify(
                "Pending command inspection requires a connected replica; the local authority commits synchronously",
                StatusKind::Warning,
            );
            return;
        }
        self.refresh_pending_count();
        match self.client.pending_commands() {
            Ok(rows) => {
                let detail = rows
                    .first()
                    .map(|row| self.pending_detail_text(&row.id))
                    .unwrap_or_else(empty_pending_detail);
                self.modal = Some(Modal::Pending {
                    rows,
                    selected: 0,
                    detail,
                    detail_scroll: 0,
                });
                self.notify(
                    "Queued intent only: [pending] and [conflict] rows are never accepted data",
                    StatusKind::Info,
                );
            }
            Err(error) => self.notify(
                format!("Pending commands unavailable: {error}"),
                StatusKind::Error,
            ),
        }
    }

    pub(crate) fn pending_detail_text(&self, id: &str) -> String {
        self.client
            .pending_command_detail(id)
            .unwrap_or_else(|error| format!("Detail unavailable: {error}"))
    }

    /// Flush queued commands, then report each one's durable state. Refresh
    /// never promotes an unacknowledged command: anything still pending or
    /// rejected keeps its row and its warning.
    ///
    /// A non-empty flush can change accepted state (and staged uploads), so the
    /// accepted workspace and the docked detail cache are refreshed before the
    /// outbox is re-listed. Staged uploads that are still pending keep the
    /// status on `Pending`, never `Success`.
    ///
    /// The caller owns the open modal, so this returns the refreshed rows and
    /// the detail text for the first row instead of mutating `self.modal`.
    pub fn pending_flush_report(&mut self, ids: &[String]) -> (Vec<PendingCommand>, String) {
        let before = self.client.pending_count();
        let (message, mut kind) = match self.client.flush_pending() {
            Ok((message, blocked)) => (
                message,
                if blocked {
                    StatusKind::Warning
                } else {
                    StatusKind::Success
                },
            ),
            Err(error) => (
                format!("Flush failed: {}", error.message),
                StatusKind::Error,
            ),
        };
        if before > 0 && kind != StatusKind::Error {
            // A flush can accept commands and staged uploads; the accepted view
            // and the detail counts are stale until they are re-read.
            self.refresh_after_mutation();
            self.invalidate_detail_cache();
        }
        self.refresh_pending_count();
        let outcomes: Vec<(String, MutationOutcome)> = ids
            .iter()
            .map(|id| (id.clone(), self.client.command_outcome(id)))
            .collect();
        let accepted = outcomes
            .iter()
            .filter(|(_, outcome)| outcome.is_accepted())
            .count();
        let unresolved = outcomes.len() - accepted;
        let outcome_lines: Vec<String> = outcomes
            .into_iter()
            .map(|(id, outcome)| match outcome {
                MutationOutcome::Accepted => format!("{id}: accepted"),
                MutationOutcome::Pending { state, .. } => format!("{id}: pending ({state})"),
                MutationOutcome::Conflict { reason, .. } => format!("{id}: rejected — {reason}"),
            })
            .collect();
        if kind == StatusKind::Success && unresolved > 0 {
            kind = StatusKind::Warning;
        }
        let uploads = self.client.pending_attachment_count().unwrap_or_default();
        let mut text = message;
        if !outcome_lines.is_empty() {
            text.push_str(&format!(
                " | {accepted} accepted by the server, {unresolved} still pending/rejected"
            ));
            text.push_str(&format!(" | {}", outcome_lines.join("; ")));
        }
        if uploads > 0 {
            // Staged uploads are not accepted content; never let a flush that
            // leaves them queued read as success.
            text.push_str(&format!(
                " | {uploads} staged attachment upload(s) still pending"
            ));
            if kind == StatusKind::Success {
                kind = StatusKind::Pending;
            }
        }
        let rows = self.client.pending_commands().unwrap_or_default();
        let detail = rows
            .first()
            .map(|row| self.pending_detail_text(&row.id))
            .unwrap_or_else(empty_pending_detail);
        self.notify(text, kind);
        (rows, detail)
    }

    fn conflict_review_lines(
        &self,
        review: &orbit_client::authority::ConflictReview,
    ) -> Vec<String> {
        // The authority's rejection reason is the fact a reviewer needs. It is
        // recorded on the unresolved row, not on the review token, so read it
        // back by command identity and state plainly when it is unavailable.
        let reason = match self.client.command_outcome(&review.rejected_id) {
            MutationOutcome::Conflict { reason, .. } => reason,
            MutationOutcome::Pending { state, .. } => {
                format!("unavailable (recorded state: {state})")
            }
            MutationOutcome::Accepted => "unavailable (no rejection recorded)".to_owned(),
        };
        vec![
            format!("Command: {} ({})", review.rejected_id, review.kind),
            format!("Local intent (NOT accepted): {}", review.rejected_action),
            format!("Outbox position: {}", review.ordinal),
            format!("Accepted server revision: {}", review.accepted_revision),
            format!("Accepted history: {}", review.history_fingerprint),
            format!("Rejection reason: {reason}"),
            String::new(),
            "This command was REJECTED by the authority, not accepted.".into(),
            "Nothing is rebased automatically. A replacement is separate pending intent; the rejected audit row is retained."
                .into(),
        ]
    }

    /// Snapshot-bound conflict review. When the review cannot be established the
    /// modal still opens, but with `review: None`, so Enter refuses to queue.
    fn build_conflict_state(&mut self, id: &str, replacement: String) -> ConflictModal {
        match self.client.review_connected_conflict(id) {
            Ok(review) => {
                let mut review_lines = self.conflict_review_lines(&review);
                if let Ok(detail) = self.client.pending_command_detail(id) {
                    review_lines.push(String::new());
                    review_lines.push("Recorded command state:".into());
                    review_lines.extend(detail.lines().map(str::to_owned));
                }
                ConflictModal {
                    command_id: id.to_owned(),
                    review_lines,
                    replacement,
                    error: None,
                    review: Some(review),
                }
            }
            Err(error) => ConflictModal {
                command_id: id.to_owned(),
                review_lines: vec![
                    format!("Command: {id}"),
                    format!("Conflict review unavailable: {error}"),
                    String::new(),
                    "This command is rejected, not accepted; its pending row is preserved.".into(),
                    "No replacement can be queued without a reviewed conflict (fail closed)."
                        .into(),
                ],
                replacement,
                error: Some("conflict review unavailable".into()),
                review: None,
            },
        }
    }

    pub fn open_conflict(&mut self, id: &str) {
        self.modal_scroll = 0;
        let state = self.build_conflict_state(id, String::new());
        let reviewed = state.review.is_some();
        self.modal = Some(Modal::Conflict {
            command_id: state.command_id,
            review_lines: state.review_lines,
            replacement: state.replacement,
            error: state.error,
            review: state.review,
        });
        self.notify(
            if reviewed {
                "Rejected command review: nothing has been accepted or auto-rebased"
            } else {
                "Conflict review failed; no replacement can be queued"
            },
            if reviewed {
                StatusKind::Warning
            } else {
                StatusKind::Error
            },
        );
    }

    /// Validate and queue the replacement in a conflict review. Requires the
    /// review token: no review means no queue (fail closed).
    pub fn resolve_conflict(
        &mut self,
        command_id: &str,
        review: Option<&orbit_client::authority::ConflictReview>,
        replacement: &str,
    ) -> Result<(String, StatusKind), String> {
        let Some(review) = review else {
            return Err(
                "No reviewed conflict; replacement refused. The original pending command is preserved."
                    .into(),
            );
        };
        if replacement.trim().is_empty() {
            return Err(
                "Enter a replacement command, or press Esc to keep the rejected command unchanged"
                    .into(),
            );
        }
        let replacement_id = crate::backend::next_command_id();
        let intent = connected_actions::parse_intent(replacement, &replacement_id)
            .map_err(|error| format!("Replacement rejected; nothing was queued: {error}"))?;
        self.client
            .resolve_connected_conflict(review, &replacement_id, intent.kind, &intent.action)
            .map_err(|error| {
                format!("Conflict replacement rejected; the original command is preserved: {error}")
            })?;
        Ok(self.mutation_status(format!(
            "{command_id} replaced by {replacement_id} (PENDING; not accepted); the rejected audit row is retained"
        )))
    }

    /// Open the connected command console. Local deployments have no outbox or
    /// connected transport, so the entry stays absent there.
    pub fn open_connected_console(&mut self) {
        if !self.client.is_connected() {
            self.notify(
                "Connected commands require a connected deployment; local edits use the normal forms",
                StatusKind::Warning,
            );
            return;
        }
        self.modal_scroll = 0;
        self.modal = Some(Modal::Console {
            input: String::new(),
            suggestions: self.console_suggestions(""),
            selected: 0,
            output: vec![
                "Connected commands queue PENDING intentions; nothing is accepted until the server acknowledges it."
                    .into(),
                "Read-only: sync.status · sync.observe · note.search <q> · note.sync · note.cached · note.show <id|path>"
                    .into(),
                "Read-only planning: plan.portfolio · plan.compare · plan.review · plan.focus <task> · plan.group --project <id>"
                    .into(),
                "Read-only accepted lists: routine.list · strategy.list · milestone.list · schedule.list"
                    .into(),
                "Approval: plan.review first, then plan.approve --confirm at the same accepted revision".into(),
            ],
            error: None,
        });
    }

    /// Filter the shared command grammar for the console suggestion list.
    pub fn console_suggestions(&self, query: &str) -> Vec<String> {
        let prefix = query
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        let mut seen = std::collections::BTreeSet::new();
        console_commands()
            .into_iter()
            .filter(|command| seen.insert(*command))
            .filter(|command| prefix.is_empty() || command.to_lowercase().contains(&prefix))
            .take(12)
            .map(str::to_owned)
            .collect()
    }

    /// Resolve one console line. Read-only commands return output immediately;
    /// mutations come back as a confirmation so nothing is queued on Enter
    /// alone. Invalid input never queues anything.
    pub fn console_run(&mut self, source: &str) -> ConsoleOutcome {
        let source = source.trim();
        if source.is_empty() {
            return ConsoleOutcome::Error("Enter a command or press Esc to close".into());
        }
        match source {
            "sync.status" => {
                return ConsoleOutcome::Lines(vec![
                    self.client
                        .connected_sync_report()
                        .unwrap_or_else(|error| format!("Sync status unavailable: {error}")),
                ]);
            }
            "note.sync" => {
                return ConsoleOutcome::Lines(match self.client.connected_sync_notes() {
                    Ok(message) => vec![message],
                    Err(error) => vec![format!(
                        "Note synchronization failed; previous accepted mirror preserved: {error}"
                    )],
                });
            }
            "note.cached" => {
                return ConsoleOutcome::Lines(match self.client.connected_cached_notes() {
                    Ok(lines) => lines,
                    Err(error) => vec![format!("Cached notes unavailable: {error}")],
                });
            }
            _ => {}
        }
        if let Some(query) = source.strip_prefix("note.search ") {
            return match self.client.connected_notes_search(query.trim()) {
                Ok(lines) => ConsoleOutcome::Lines(lines),
                Err(error) => ConsoleOutcome::Lines(vec![format!("Note search failed: {error}")]),
            };
        }
        // Read-only planning analyses, context views, note reads, live sync
        // observation and explicit scenario approval. Parsing is pure and
        // revision-fenced; only `plan.approve --confirm` can write.
        let accepted_revision = if source
            .split_whitespace()
            .next()
            .is_some_and(|op| op.starts_with("plan.") && op != "plan.approve")
        {
            self.client.accepted_revision().ok()
        } else {
            None
        };
        match connected_ops::parse(
            source,
            now_ms(),
            accepted_revision,
            &crate::backend::next_command_id(),
        ) {
            Ok(Some(command)) => return self.console_run_connected(command),
            Ok(None) => {}
            Err(error) => return ConsoleOutcome::Error(error),
        }
        if let Some(script) = source.strip_prefix("batch ") {
            return match connected_actions::parse_batch(script, crate::backend::next_command_id) {
                Ok(intents) => ConsoleOutcome::Confirm {
                    title: "Queue connected batch".into(),
                    body: format!(
                        "{}\n\nNothing is accepted: every command is queued PENDING in the durable outbox and the accepted workspace stays unchanged.",
                        connected_actions::batch_preview(&intents).join("\n")
                    ),
                    pending: PendingAction::QueueBatch { intents },
                },
                Err(error) => {
                    ConsoleOutcome::Error(format!("Batch rejected; no command was queued: {error}"))
                }
            };
        }
        if let Some(tail) = source.strip_prefix("resolve ") {
            let mut parts = tail.splitn(2, ' ');
            let id = parts.next().unwrap_or_default().trim();
            let replacement = parts.next().unwrap_or_default().trim();
            if id.is_empty() || replacement.is_empty() {
                return ConsoleOutcome::Error(
                    "usage: resolve <rejected-command-id> <replacement-command>".into(),
                );
            }
            if let Err(error) =
                connected_actions::parse_intent(replacement, &crate::backend::next_command_id())
            {
                return ConsoleOutcome::Error(format!(
                    "Replacement rejected; nothing was queued: {error}"
                ));
            }
            let state = self.build_conflict_state(id, replacement.to_owned());
            return ConsoleOutcome::Conflict(Box::new(state));
        }
        match connected_actions::parse_intent(source, &crate::backend::next_command_id()) {
            Ok(intent) => ConsoleOutcome::Confirm {
                title: "Queue connected command".into(),
                body: format!(
                    "{}.\n\nThis is queued PENDING in the durable outbox — not accepted. The accepted workspace does not change until the server acknowledges it.",
                    intent.description
                ),
                pending: PendingAction::QueueIntent { intent },
            },
            Err(error) => ConsoleOutcome::Error(error),
        }
    }

    /// Apply one parsed [`connected_ops::Command`]. Read-only commands return
    /// output immediately; approval consumes the reviewed scenario and reports
    /// the authoritative receipt. Nothing here queues an intention.
    fn console_run_connected(&mut self, command: connected_ops::Command) -> ConsoleOutcome {
        use connected_ops::Command;
        match command {
            Command::Portfolio(request) => match self.client.connected_portfolio(&request) {
                Ok(report) => ConsoleOutcome::Lines(planning_ui::render_portfolio(&report)),
                Err(error) => ConsoleOutcome::Error(format!("Cannot inspect portfolio: {error}")),
            },
            Command::Compare(request) => match self.client.connected_scenario_compare(&request) {
                Ok(report) => ConsoleOutcome::Lines(planning_ui::render_scenarios(&report)),
                Err(error) => {
                    ConsoleOutcome::Error(format!("Cannot compare planning alternatives: {error}"))
                }
            },
            Command::Review(request) => match self.client.connected_scenario_review(&request) {
                Ok(review) => {
                    let mut lines = planning_ui::render_scenario_review(&review);
                    lines.push(String::new());
                    lines.push(
                        "Approve these exact slots with: plan.approve --confirm (any revision change requires a new review)"
                            .into(),
                    );
                    self.pending_scenario = Some(review);
                    ConsoleOutcome::Lines(lines)
                }
                Err(error) => {
                    // A refused review leaves nothing approvable behind.
                    self.pending_scenario = None;
                    ConsoleOutcome::Error(format!("Scenario review rejected: {error}"))
                }
            },
            Command::Approve => {
                // Consume the review either way: a stale or refused approval
                // must not be retried without a fresh review.
                let review = self.pending_scenario.take();
                match (review, self.client.accepted_revision()) {
                    (None, _) => ConsoleOutcome::Error(
                        "No reviewed scenario is awaiting approval; use plan.review first".into(),
                    ),
                    (Some(_), Err(error)) => ConsoleOutcome::Error(format!(
                        "Approval requires the accepted revision: {error}"
                    )),
                    (Some(review), Ok(revision)) => {
                        match connected_ops::approve_request(Some(&review), revision) {
                            Ok(request) => {
                                match self.client.connected_apply_approved_schedule(&request) {
                                    Ok(receipt) => {
                                        ConsoleOutcome::Lines(vec![connected_ops::receipt_line(
                                            &receipt,
                                        )])
                                    }
                                    Err(error) => ConsoleOutcome::Error(format!(
                                        "{error}; no retry without a fresh review"
                                    )),
                                }
                            }
                            Err(error) => ConsoleOutcome::Error(error),
                        }
                    }
                }
            }
            Command::FocusTask(request) => match self.task_focus_lines(request.task_id) {
                Ok(lines) => ConsoleOutcome::Lines(lines),
                Err(error) => ConsoleOutcome::Error(format!("Task context unavailable: {error}")),
            },
            Command::FocusGroup(request) => {
                match self.project_focus_lines(request.project_id, request.milestone_id) {
                    Ok(lines) => ConsoleOutcome::Lines(lines),
                    Err(error) => {
                        ConsoleOutcome::Error(format!("Project context unavailable: {error}"))
                    }
                }
            }
            Command::NoteShow(target) => match self.client.connected_note_show(&target) {
                Ok(lines) => ConsoleOutcome::Lines(lines),
                Err(error) => ConsoleOutcome::Error(format!("Note read failed: {error}")),
            },
            Command::RoutineList => match self.client.accepted_routines() {
                Ok(rows) => ConsoleOutcome::Lines(planning_ui::render_accepted_routines(&rows)),
                Err(error) => ConsoleOutcome::Error(format!(
                    "Accepted routines unavailable: {}",
                    error.message
                )),
            },
            Command::StrategyList => match self.client.accepted_strategies() {
                Ok(rows) => ConsoleOutcome::Lines(planning_ui::render_accepted_strategies(&rows)),
                Err(error) => ConsoleOutcome::Error(format!(
                    "Accepted strategies unavailable: {}",
                    error.message
                )),
            },
            Command::MilestoneList => match self.client.accepted_milestones() {
                Ok(rows) => ConsoleOutcome::Lines(planning_ui::render_accepted_milestones(&rows)),
                Err(error) => ConsoleOutcome::Error(format!(
                    "Accepted milestones unavailable: {}",
                    error.message
                )),
            },
            Command::ScheduleList => match self.client.accepted_schedule() {
                Ok(rows) => ConsoleOutcome::Lines(planning_ui::render_accepted_schedule(&rows)),
                Err(error) => ConsoleOutcome::Error(format!(
                    "Accepted schedule unavailable: {}",
                    error.message
                )),
            },
            Command::ObserveSync => match self.client.connected_observe_sync_report() {
                Ok(line) => ConsoleOutcome::Lines(vec![line]),
                Err(error) => ConsoleOutcome::Error(error),
            },
        }
    }
}

/// Placeholder detail text: an empty inspector proves nothing was queued.
fn empty_pending_detail() -> String {
    "No pending commands. Nothing is queued and nothing was accepted by this view.".into()
}

/// Every command template the connected console accepts, without descriptions.
/// The grammar itself stays in `connected_actions`. Help prose is filtered out:
/// a suggestion must be command-shaped (its first token carries the `.` verb
/// namespace), so Tab can never fill a sentence that cannot parse.
fn console_commands() -> Vec<&'static str> {
    let mut commands = vec![
        "routine.list",
        "strategy.list",
        "milestone.list",
        "schedule.list",
        "batch <command; command>",
        "resolve <rejected-command-id> <replacement-command>",
        "sync.status",
        "sync.observe",
        "note.search <query>",
        "note.show <note-id|path.md>",
        "note.sync",
        "note.cached",
        "plan.portfolio [--limit N] [--project ID]",
        "plan.compare [--minutes N] [--pin ID] [--exclude ID] [--alt-minutes N]",
        "plan.review [--hours N] [--baseline]",
        "plan.approve --confirm",
        "plan.focus <task-id>",
        "plan.group --project <id> [--milestone <id>]",
    ];
    for section in connected_actions::Section::ALL {
        for line in connected_actions::help(section) {
            let command = line.split(" — ").next().unwrap_or(line).trim();
            if is_command_shaped(command) && !commands.contains(&command) {
                commands.push(command);
            }
        }
    }
    commands
}

/// A console suggestion must start with a `verb` token in the dotted namespace;
/// section headers and help prose are never filled into the prompt.
fn is_command_shaped(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let Some(verb) = words.next() else {
        return false;
    };
    verb.contains('.')
        && verb
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '.' | '_' | '-'))
}
