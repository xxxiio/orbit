//! Connected TUI: accepted server data, independent durable intentions, and an
//! explicit command palette for scheduling, routines, milestones and strategy.
//! No writes go through OrbitClient/SqliteStore in server-authoritative mode.
use crate::connected_actions::{self, Intent, Section};
use crate::planning_ui;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use orbit_client::authority::{AuthorityHttp, AuthorityReplica, FlushOutcome};
use orbit_ui::{ComponentBody, ComponentRegistry};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn describe(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn uid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("tui-{}-{nanos}", std::process::id())
}

#[derive(Clone, Debug)]
enum Editor {
    Closed,
    Command(String),
    Filter(String),
    Confirm(Intent),
    BatchConfirm(Vec<Intent>),
    Replace { old_id: String, intent: Intent },
    Help,
    Details,
    Planning,
    Views,
    QuitConfirm(Box<Editor>),
}
struct Connected {
    replica: AuthorityReplica,
    http: Option<AuthorityHttp>,
    message: String,
    section: Section,
    selection: usize,
    editor: Editor,
    filter: String,
    scroll: usize,
    dirty: bool,
    notes: Vec<Value>,
    planning: Option<planning_ui::PlanningDocument>,
    planning_scroll: usize,
    impact: Option<Vec<String>>,
    planning_action_menu: Option<(
        orbit_runtime::planning_brief::WorkBrief,
        orbit_runtime::planning_actions::PlanningActionMenu,
    )>,
    pending_scenario: Option<orbit_runtime::scenario_execution::ScenarioExecutionReview>,
    view_lines: Vec<String>,
}
impl Connected {
    fn snapshot(&self) -> Value {
        self.replica
            .snapshot()
            .ok()
            .flatten()
            .unwrap_or(Value::Null)
    }
    fn accepted_revision(&self) -> Option<i64> {
        self.replica.revision().ok().flatten()
    }
    fn outbox(&self) -> Vec<Value> {
        self.replica.unresolved().unwrap_or_default()
    }
    fn rows(&self) -> Vec<Value> {
        (if self.section == Section::Notes {
            self.notes.clone()
        } else {
            connected_actions::records(&self.snapshot(), self.section, &self.outbox())
        })
        .into_iter()
        .filter(|row| {
            if self.filter.is_empty() {
                return true;
            }
            let needle = self.filter.to_lowercase();
            connected_actions::summarize(self.section, row)
                .to_lowercase()
                .contains(&needle)
        })
        .collect()
    }
    fn selected(&self) -> Option<Value> {
        self.rows().get(self.selection).cloned()
    }
    fn clamp(&mut self) {
        let count = self.rows().len();
        self.selection = if count == 0 {
            0
        } else {
            self.selection.min(count - 1)
        };
        self.scroll = self.scroll.min(self.selection);
    }
    fn section(&mut self, section: Section) {
        self.section = section;
        self.selection = 0;
        self.scroll = 0;
        self.filter.clear();
        self.editor = Editor::Closed;
        self.message = format!(
            "{} • accepted snapshot only • : command • ? help",
            section.title()
        );
    }
    fn move_selection(&mut self, by: isize) {
        let count = self.rows().len();
        if count == 0 {
            self.selection = 0;
            return;
        }
        self.selection = self.selection.saturating_add_signed(by).min(count - 1);
    }
    fn queue(&mut self, intent: Intent) {
        let command_id = uid();
        let result = self.replica.queue(&command_id, intent.kind, &intent.action);
        self.message = match result {
            Ok(()) => format!(
                "{} queued as {command_id} (PENDING; not accepted). Press f to submit.",
                intent.description
            ),
            Err(e) => format!("Unable to queue {}: {e:?}", intent.description),
        };
        self.dirty = true;
    }
    fn search_notes(&mut self, query: &str) {
        if query.trim().is_empty() || query.len() > 512 {
            self.message = "note.search requires a nonempty query of at most 512 characters".into();
            return;
        }
        let Some(http) = &self.http else {
            match self.replica.search_cached_markdown(query, 100) {
                Ok(accepted) => {
                    let count = accepted.len();
                    self.notes = accepted
                        .into_iter()
                        .filter_map(|note| serde_json::to_value(note).ok())
                        .collect();
                    self.section(Section::Notes);
                    self.message = format!(
                        "{count} cached accepted Markdown notes (OFFLINE; possibly stale; pending edits excluded)"
                    );
                }
                Err(error) => self.message = format!("Offline Markdown search failed: {error:?}"),
            }
            return;
        };
        match http.post(
            "/v1/authoritative/markdown-search",
            &serde_json::json!({"query":query,"limit":100}),
        ) {
            Ok(Value::Array(rows)) => {
                // Never display a malformed response as accepted note data.
                if rows.iter().any(|row| {
                    !row.get("id").is_some_and(Value::is_string)
                        || !row.get("path").is_some_and(Value::is_string)
                        || !row.get("title").is_some_and(Value::is_string)
                }) {
                    self.message="Server returned malformed Markdown search results; previous list preserved".into();
                    return;
                }
                let count = rows.len();
                self.notes = rows;
                self.section(Section::Notes);
                self.message = format!(
                    "{count} accepted Markdown notes returned for {query:?}; pending edits excluded"
                );
            }
            Ok(_) => {
                self.message = "Invalid server note-search response; previous list preserved".into()
            }
            Err(e) => {
                self.message =
                    format!("Markdown search unavailable; previous list preserved: {e:?}")
            }
        }
    }
    fn sync_notes(&mut self) {
        let Some(http) = &self.http else {
            self.message="Offline: cannot synchronize notes; previously cached accepted notes remain searchable".into();
            return;
        };
        match self.replica.sync_markdown_mirror(http) {
            Ok(count) => {
                self.message = format!(
                    "{count} server-accepted Markdown notes mirrored atomically; deleted and moved notes reconciled; pending edits untouched"
                );
                if self.section == Section::Notes {
                    match self.replica.cached_markdown_inventory(500) {
                        Ok(notes) => {
                            self.notes = notes
                                .into_iter()
                                .filter_map(|n| serde_json::to_value(n).ok())
                                .collect();
                            self.clamp();
                        }
                        Err(error) => {
                            self.message =
                                format!("Mirror updated, but note inventory unavailable: {error:?}")
                        }
                    }
                }
            }
            Err(error) => {
                self.message = format!(
                    "Markdown synchronization failed: {error:?}; previous accepted mirror preserved"
                )
            }
        }
    }

    fn show_sync_health(&mut self, online: bool) {
        let result = if online {
            match self.http.as_ref() {
                Some(http) => self.replica.observe_workspace_sync_report(http),
                None => {
                    self.message =
                        "Offline: cannot observe live server; use sync.status for cached health"
                            .into();
                    return;
                }
            }
        } else {
            self.replica.workspace_sync_report()
        };
        match result {
            Ok(report) => {
                let status = &report.health;
                let blocked = report
                    .domains
                    .blocking_head()
                    .map(|(id, family, _)| format!(" first={family}:{id}"))
                    .unwrap_or_default();
                self.message = format!(
                    "Sync {:?} | accepted {:?} | live {:?} | pending {} | conflicts {}{} | mirror {} | next {:?}",
                    status.primary,
                    status.accepted_revision,
                    status.observed_server_revision,
                    status.pending_count,
                    status.conflict_count,
                    blocked,
                    if status.markdown_mirror_complete {
                        "complete"
                    } else {
                        "incomplete"
                    },
                    status.actions
                );
            }
            Err(error) => self.message = format!("Sync health unavailable: {error:?}"),
        }
    }

    fn accepted_brief(
        &self,
        request: &orbit_runtime::planning_brief::WorkBriefRequest,
    ) -> Result<orbit_runtime::planning_brief::WorkBrief, String> {
        let snapshot = self
            .replica
            .snapshot()
            .map_err(describe)?
            .ok_or("No accepted snapshot yet; refresh while online first")?;
        let revision = snapshot
            .get("revision")
            .and_then(Value::as_i64)
            .ok_or("Accepted snapshot is missing its revision")?;
        let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> = serde_json::from_value(
            snapshot
                .get("tasks")
                .cloned()
                .ok_or("Accepted snapshot missing tasks")?,
        )
        .map_err(describe)?;
        let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
            serde_json::from_value(
                snapshot
                    .get("task_dependencies")
                    .cloned()
                    .ok_or("Accepted snapshot missing dependencies")?,
            )
            .map_err(describe)?;
        let blocks: Vec<orbit_runtime::authority_schedule::ScheduleRecord> =
            serde_json::from_value(
                snapshot
                    .get("schedule")
                    .cloned()
                    .ok_or("Accepted snapshot missing schedule")?,
            )
            .map_err(describe)?;
        orbit_runtime::planning_brief::build_work_brief(revision, &tasks, &edges, &blocks, request)
            .map_err(str::to_owned)
    }

    fn show_today(&mut self, request: orbit_runtime::planning_brief::WorkBriefRequest) {
        match self.accepted_brief(&request) {
            Ok(brief) => {
                let document = planning_ui::PlanningDocument::from_brief(&brief);
                self.planning_scroll = self
                    .planning_scroll
                    .min(document.lines.len().saturating_sub(1));
                self.planning = Some(document);
                self.impact = None;
                self.editor = Editor::Planning;
                self.message = format!(
                    "Accepted Today / Next revision {} • j/k scroll • J/K task • d impact • Esc exit",
                    brief.revision
                );
            }
            Err(error) => {
                self.message =
                    format!("Today planning unavailable: {error}; previous view preserved");
                self.editor = Editor::Closed;
            }
        }
    }

    fn preview_planning_actions(&mut self) {
        let Some(task_id) = self
            .planning
            .as_ref()
            .and_then(|doc| doc.selected_task(self.planning_scroll))
            .map(str::to_owned)
        else {
            self.message = "No accepted task selected".into();
            return;
        };
        let result = (|| -> Result<_, String> {
            let request = planning_ui::request(planning_ui::now_ms(), 12, 100, None)?;
            let brief = self.accepted_brief(&request)?;
            let menu = orbit_runtime::planning_actions::task_actions(
                &brief,
                &orbit_runtime::planning_actions::TaskActionRequest {
                    task_id,
                    authority_available: self.http.is_some(),
                },
            )
            .map_err(str::to_owned)?;
            Ok((brief, menu))
        })();
        match result {
            Ok((brief, menu)) => {
                self.impact = Some(planning_ui::render_action_menu(&menu));
                self.planning_action_menu = Some((brief, menu));
                self.editor = Editor::Planning;
                self.message = "Press 1-9 to inspect a reviewed action • read-only".into();
            }
            Err(error) => self.message = format!("Task actions unavailable: {error}"),
        }
    }

    fn select_planning_action(&mut self, index: usize) {
        let Some((brief, menu)) = self.planning_action_menu.take() else {
            self.message = "Open a task action menu first".into();
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            let current = self
                .accepted_revision()
                .ok_or("Accepted revision unavailable")?;
            if current != brief.revision {
                return Err("Accepted revision changed: reopen action menu".into());
            }
            let selection = orbit_runtime::planning_handoff::select_from_menu(&menu, index)
                .map_err(str::to_owned)?;
            let request = orbit_runtime::planning_actions::TaskActionRequest {
                task_id: menu
                    .selected_task_id
                    .clone()
                    .ok_or("Menu missing accepted task")?,
                authority_available: self.http.is_some(),
            };
            let handoff =
                orbit_runtime::planning_handoff::dispatch_task(&brief, &request, &selection)
                    .map_err(str::to_owned)?;
            if let orbit_runtime::planning_handoff::HandoffView::Preview(preview) = &handoff.view
                && preview.operation
                    == orbit_runtime::planning_handoff::PreviewOperation::CompletionImpact
            {
                let snapshot = self
                    .replica
                    .snapshot()
                    .map_err(describe)?
                    .ok_or("Accepted snapshot unavailable")?;
                let revision = snapshot
                    .get("revision")
                    .and_then(Value::as_i64)
                    .ok_or("Accepted revision missing")?;
                if revision != handoff.revision {
                    return Err("Accepted revision changed".into());
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or("Missing accepted tasks")?,
                    )
                    .map_err(describe)?;
                let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                    serde_json::from_value(
                        snapshot
                            .get("task_dependencies")
                            .cloned()
                            .ok_or("Missing accepted dependencies")?,
                    )
                    .map_err(describe)?;
                let report = orbit_runtime::task_impact::preview_completion(
                    revision,
                    &tasks,
                    &edges,
                    &orbit_runtime::task_impact::CompletionImpactRequest {
                        task_id: preview.task_id.clone(),
                        now_ms: preview.evaluated_at_ms,
                        next_limit: 20,
                    },
                )
                .map_err(str::to_owned)?;
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_impact(&report));
                return Ok(lines);
            }
            if let Some(request) = planning_ui::dependency_request_from_handoff(&handoff)? {
                // Use only the one accepted snapshot reviewed above. A remote
                // outbox or live fetch must not be blended into a what-if preview.
                let snapshot = self
                    .replica
                    .snapshot()
                    .map_err(describe)?
                    .ok_or("Accepted snapshot unavailable")?;
                let revision = snapshot
                    .get("revision")
                    .and_then(Value::as_i64)
                    .ok_or("Accepted revision missing")?;
                if revision != handoff.revision {
                    return Err("Accepted revision changed".into());
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or("Missing accepted tasks")?,
                    )
                    .map_err(describe)?;
                let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                    serde_json::from_value(
                        snapshot
                            .get("task_dependencies")
                            .cloned()
                            .ok_or("Missing accepted dependencies")?,
                    )
                    .map_err(describe)?;
                let report = orbit_runtime::task_dependency_preview::preview_dependency(
                    revision, &tasks, &edges, &request,
                )
                .map_err(str::to_owned)?;
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_dependency_preview(&report));
                return Ok(lines);
            }
            if let Some(request) = planning_ui::calendar_request_from_handoff(&handoff)? {
                let snapshot = self
                    .replica
                    .snapshot()
                    .map_err(describe)?
                    .ok_or("Accepted snapshot unavailable")?;
                let revision = snapshot
                    .get("revision")
                    .and_then(Value::as_i64)
                    .ok_or("Accepted revision missing")?;
                if revision != handoff.revision {
                    return Err("Accepted revision changed".into());
                }
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or("Missing accepted tasks")?,
                    )
                    .map_err(describe)?;
                let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                    serde_json::from_value(
                        snapshot
                            .get("task_dependencies")
                            .cloned()
                            .ok_or("Missing accepted dependencies")?,
                    )
                    .map_err(describe)?;
                let blocks: Vec<orbit_runtime::authority_schedule::ScheduleRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("schedule")
                            .cloned()
                            .ok_or("Missing accepted schedule")?,
                    )
                    .map_err(describe)?;
                let plan = orbit_runtime::schedule_planning::plan_schedule(
                    revision, revision, &tasks, &edges, &blocks, &request,
                )
                .map_err(str::to_owned)?;
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_task_calendar_preview(&handoff, &plan)?);
                return Ok(lines);
            }
            Ok(planning_ui::render_handoff(&handoff))
        })();
        match result {
            Ok(lines) => {
                self.impact = Some(lines);
                self.message = "Read-only action destination; Esc returns to Today".into();
            }
            Err(error) => self.message = format!("Action rejected: {error}"),
        }
    }

    fn preview_planning_focus(&mut self) {
        let Some(task_id) = self
            .planning
            .as_ref()
            .and_then(|doc| doc.selected_task(self.planning_scroll))
            .map(str::to_owned)
        else {
            self.message = "No accepted task selected".into();
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            // Reconstruct from the one accepted cache. Do not resolve related
            // identities against a newer server or against pending intentions.
            let req = planning_ui::request(planning_ui::now_ms(), 12, 100, None)?;
            let brief = self.accepted_brief(&req)?;
            let focus = orbit_runtime::planning_focus::focus_task(
                &brief,
                &orbit_runtime::planning_focus::FocusRequest {
                    task_id,
                    max_neighbors: 12,
                },
            )
            .map_err(str::to_owned)?;
            let menu = orbit_runtime::planning_actions::task_actions(
                &brief,
                &orbit_runtime::planning_actions::TaskActionRequest {
                    task_id: focus.task.id.clone(),
                    authority_available: self.http.is_some(),
                },
            )
            .map_err(str::to_owned)?;
            let mut lines = planning_ui::render_focus(&focus);
            lines.push(String::new());
            lines.extend(planning_ui::render_action_menu(&menu));
            Ok(lines)
        })();
        match result {
            Ok(lines) => {
                self.impact = Some(lines);
                self.editor = Editor::Planning;
                self.message = "Accepted task context • Esc returns to Today".into();
            }
            Err(error) => self.message = format!("Task context unavailable: {error}"),
        }
    }

    fn preview_planning_impact(&mut self) {
        let Some(task_id) = self
            .planning
            .as_ref()
            .and_then(|doc| doc.selected_task(self.planning_scroll))
            .map(str::to_owned)
        else {
            self.message = "No accepted task selected".into();
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            let snapshot = self
                .replica
                .snapshot()
                .map_err(describe)?
                .ok_or("Accepted snapshot unavailable")?;
            let revision = snapshot
                .get("revision")
                .and_then(Value::as_i64)
                .ok_or("Missing accepted revision")?;
            let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> = serde_json::from_value(
                snapshot
                    .get("tasks")
                    .cloned()
                    .ok_or("Missing accepted tasks")?,
            )
            .map_err(describe)?;
            let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                serde_json::from_value(
                    snapshot
                        .get("task_dependencies")
                        .cloned()
                        .ok_or("Missing accepted dependencies")?,
                )
                .map_err(describe)?;
            let req = orbit_runtime::task_impact::CompletionImpactRequest {
                task_id,
                now_ms: planning_ui::now_ms(),
                next_limit: 20,
            };
            let report =
                orbit_runtime::task_impact::preview_completion(revision, &tasks, &edges, &req)
                    .map_err(str::to_owned)?;
            Ok(planning_ui::render_impact(&report))
        })();
        match result {
            Ok(lines) => {
                self.impact = Some(lines);
                self.message = "Read-only completion impact • Esc returns to Today".into();
            }
            Err(error) => self.message = format!("Cannot preview impact: {error}"),
        }
    }

    fn show_project_context(&mut self, project_id: &str, milestone_id: Option<&str>) {
        let result = (|| -> Result<Vec<String>, String> {
            let req =
                planning_ui::request(planning_ui::now_ms(), 12, 100, Some(project_id.to_owned()))?;
            let brief = self.accepted_brief(&req)?;
            let group = orbit_runtime::planning_focus::focus_group(
                &brief,
                &orbit_runtime::planning_focus::GroupFocusRequest {
                    project_id: project_id.into(),
                    milestone_id: milestone_id.map(str::to_owned),
                    max_tasks: 12,
                },
            )
            .map_err(str::to_owned)?;
            Ok(planning_ui::render_group_focus(&group))
        })();
        match result {
            Ok(lines) => {
                self.impact = Some(lines);
                self.editor = Editor::Planning;
                self.message = "Read-only accepted project context • Esc returns to Today".into();
            }
            Err(error) => {
                self.message = format!("Project context unavailable: {error}");
                self.editor = Editor::Command(format!("plan.group --project {project_id}"));
            }
        }
    }
    fn show_task_context(&mut self, task_id: &str) {
        let result = (|| -> Result<Vec<String>, String> {
            let request = planning_ui::request(planning_ui::now_ms(), 12, 100, None)?;
            let brief = self.accepted_brief(&request)?;
            let focus = orbit_runtime::planning_focus::focus_task(
                &brief,
                &orbit_runtime::planning_focus::FocusRequest {
                    task_id: task_id.to_owned(),
                    max_neighbors: 12,
                },
            )
            .map_err(str::to_owned)?;
            let menu = orbit_runtime::planning_actions::task_actions(
                &brief,
                &orbit_runtime::planning_actions::TaskActionRequest {
                    task_id: focus.task.id.clone(),
                    authority_available: self.http.is_some(),
                },
            )
            .map_err(str::to_owned)?;
            let mut lines = planning_ui::render_focus(&focus);
            lines.push(String::new());
            lines.extend(planning_ui::render_action_menu(&menu));
            Ok(lines)
        })();
        match result {
            Ok(lines) => {
                self.impact = Some(lines);
                self.editor = Editor::Planning;
                self.message = "Read-only accepted task context • Esc returns to Today".into();
            }
            Err(error) => {
                self.message = format!("Task context unavailable: {error}");
                self.editor = Editor::Command(format!("plan.focus {task_id}"));
            }
        }
    }

    fn submit(&mut self, source: &str) {
        let words: Vec<_> = source.split_whitespace().collect();
        if words.first() == Some(&"plan.focus") {
            if words.len() != 2 {
                self.message = "Usage: plan.focus TASK_ID".into();
                self.editor = Editor::Command(source.into());
            } else {
                self.show_task_context(words[1]);
            }
            return;
        }
        if words.first() == Some(&"plan.group") {
            if (words.len() != 3 && words.len() != 5)
                || words.get(1) != Some(&"--project")
                || (words.len() == 5 && words.get(3) != Some(&"--milestone"))
            {
                self.message =
                    "Usage: plan.group --project PROJECT_ID [--milestone MILESTONE_ID]".into();
                self.editor = Editor::Command(source.into());
            } else {
                self.show_project_context(words[2], words.get(4).copied());
            }
            return;
        }

        match planning_ui::parse_command(source, planning_ui::now_ms()) {
            Ok(Some(request)) => {
                self.show_today(request);
                return;
            }
            Err(error) => {
                self.message = error;
                self.editor = Editor::Command(source.into());
                return;
            }
            Ok(None) => {}
        }
        match planning_ui::parse_impact(source, planning_ui::now_ms()) {
            Ok(Some(request)) => {
                if let Some(doc) = &self.planning {
                    if doc
                        .task_anchors
                        .iter()
                        .any(|(_, id)| id == &request.task_id)
                    {
                        if let Some((row, _)) = doc
                            .task_anchors
                            .iter()
                            .find(|(_, id)| id == &request.task_id)
                        {
                            self.planning_scroll = *row;
                        }
                        self.preview_planning_impact();
                        self.editor = Editor::Planning;
                    } else {
                        self.message =
                            "Task not in accepted Today view; run plan.today first".into();
                        self.editor = Editor::Command(source.into());
                    }
                } else {
                    self.message = "Run plan.today before viewing task impact".into();
                    self.editor = Editor::Command(source.into());
                }
                return;
            }
            Err(error) => {
                self.message = error;
                self.editor = Editor::Command(source.into());
                return;
            }
            Ok(None) => {}
        }
        if source.trim() == "plan.approve --confirm" {
            let Some(review) = self.pending_scenario.take() else {
                self.message =
                    "No reviewed scenario is awaiting approval; use plan.review first".into();
                self.editor = Editor::Command(source.into());
                return;
            };
            let Some(http) = &self.http else {
                self.message="Offline: calendar approval requires the authoritative server; review again online".into();
                self.editor = Editor::Command(source.into());
                return;
            };
            let request = orbit_runtime::planning_approval::ApprovedScheduleRequest {
                plan: review.plan,
                approval: orbit_runtime::planning_approval::PlanApproval {
                    review: review.review,
                    approved: true,
                },
            };
            let result=(||->Result<orbit_runtime::planning_approval::ApprovedScheduleReceipt,String>{
                let payload=serde_json::to_value(&request).map_err(describe)?;
                let response=http.post("/v1/authoritative/plan-apply-approved",&payload)
                    .map_err(describe)?;
                serde_json::from_value(response).map_err(describe)
            })();
            match result {
                Ok(receipt) => {
                    self.dirty = true;
                    self.message = format!(
                        "Approved {} calendar reservations at revision {}; refresh accepted state",
                        receipt.schedule_block_ids.len(),
                        receipt.accepted_revision
                    );
                    self.editor = Editor::Closed;
                }
                Err(error) => {
                    self.message =
                        format!("Plan execution rejected; no retry without fresh review: {error}");
                    self.editor = Editor::Command(source.into());
                }
            }
            return;
        }
        if source.starts_with("plan.approve") {
            self.message = "Approval requires the exact command :plan.approve --confirm".into();
            self.editor = Editor::Command(source.into());
            return;
        }
        if source.split_whitespace().next() == Some("plan.review") {
            self.pending_scenario = None;
            let result=(||->Result<orbit_runtime::scenario_execution::ScenarioExecutionReview,String>{
                let http=self.http.as_ref().ok_or("Offline: planning approval review needs live authority")?;
                let revision=self.accepted_revision().ok_or("Accepted snapshot required before reviewing calendar changes")?;
                let request=planning_ui::parse_scenario_review(source,planning_ui::now_ms(),revision,&uid())?
                    .ok_or("Invalid scenario review request")?;
                let payload=serde_json::to_value(&request).map_err(describe)?;
                let value=http.post("/v1/authoritative/scenario-execution-review",&payload)
                    .map_err(describe)?;
                let response:orbit_runtime::scenario_execution::ScenarioExecutionReview=
                    serde_json::from_value(value).map_err(describe)?;
                if !response.read_only||response.review.expected_revision!=revision||
                    response.review.command_id!=request.command_id||
                    response.review.scheduled_slots.len()!=response.plan.items.len() {
                    return Err("Authoritative scenario review contract mismatch".into());
                }
                Ok(response)
            })();
            match result {
                Ok(review) => {
                    self.impact = Some(planning_ui::render_scenario_review(&review));
                    self.pending_scenario = Some(review);
                    self.editor = Editor::Planning;
                    self.message="Read-only reviewed schedule • inspect slots; approve explicitly with :plan.approve --confirm".into();
                }
                Err(error) => {
                    self.message = format!("Scenario review rejected: {error}");
                    self.editor = Editor::Command(source.into());
                }
            }
            return;
        }
        if source.split_whitespace().next() == Some("plan.compare") {
            let result = (|| -> Result<Vec<String>, String> {
                let snapshot = self
                    .replica
                    .snapshot()
                    .map_err(describe)?
                    .ok_or("Accepted snapshot unavailable")?;
                let revision = snapshot
                    .get("revision")
                    .and_then(Value::as_i64)
                    .ok_or("Missing accepted revision")?;
                let request = planning_ui::parse_scenario(source, planning_ui::now_ms(), revision)?
                    .ok_or("Invalid comparison command")?;
                let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                    serde_json::from_value(
                        snapshot
                            .get("tasks")
                            .cloned()
                            .ok_or("Missing accepted tasks")?,
                    )
                    .map_err(describe)?;
                let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                    serde_json::from_value(
                        snapshot
                            .get("task_dependencies")
                            .cloned()
                            .ok_or("Missing accepted dependencies")?,
                    )
                    .map_err(describe)?;
                let report = orbit_runtime::planning_scenarios::compare_scenarios(
                    revision, &tasks, &edges, &request,
                )
                .map_err(str::to_owned)?;
                Ok(planning_ui::render_scenarios(&report))
            })();
            match result {
                Ok(lines) => {
                    self.impact = Some(lines);
                    self.editor = Editor::Planning;
                    self.message = "Read-only planning comparison • Esc to close".into();
                }
                Err(error) => {
                    self.message = format!("Cannot compare planning alternatives: {error}");
                    self.editor = Editor::Command(source.into());
                }
            }
            return;
        }
        match planning_ui::parse_portfolio(source, planning_ui::now_ms()) {
            Ok(Some(request)) => {
                let result = (|| -> Result<Vec<String>, String> {
                    let snapshot = self
                        .replica
                        .snapshot()
                        .map_err(describe)?
                        .ok_or("Accepted snapshot unavailable")?;
                    let revision = snapshot
                        .get("revision")
                        .and_then(Value::as_i64)
                        .ok_or("Missing accepted revision")?;
                    let tasks: Vec<orbit_runtime::domain_commands::TaskRecord> =
                        serde_json::from_value(
                            snapshot
                                .get("tasks")
                                .cloned()
                                .ok_or("Missing accepted tasks")?,
                        )
                        .map_err(describe)?;
                    let edges: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge> =
                        serde_json::from_value(
                            snapshot
                                .get("task_dependencies")
                                .cloned()
                                .ok_or("Missing accepted dependencies")?,
                        )
                        .map_err(describe)?;
                    let report = orbit_runtime::portfolio_actions::recommend_portfolio_actions(
                        revision, &tasks, &edges, &request,
                    )
                    .map_err(str::to_owned)?;
                    Ok(planning_ui::render_portfolio(&report))
                })();
                match result {
                    Ok(lines) => {
                        self.impact = Some(lines);
                        self.editor = Editor::Planning;
                        self.message = "Accepted project-aware next actions • Esc to close".into();
                    }
                    Err(error) => {
                        self.message = format!("Cannot inspect portfolio: {error}");
                        self.editor = Editor::Command(source.into());
                    }
                }
                return;
            }
            Err(error) => {
                self.message = error;
                self.editor = Editor::Command(source.into());
                return;
            }
            Ok(None) => {}
        }
        if source.trim() == "sync.status" || source.trim() == "sync.observe" {
            self.show_sync_health(source.trim() == "sync.observe");
            self.editor = Editor::Closed;
            return;
        }
        if source.trim() == "note.sync" {
            self.sync_notes();
            self.editor = Editor::Closed;
            return;
        }
        if source.trim() == "note.cached" {
            match self.replica.cached_markdown_inventory(500) {
                Ok(notes) => {
                    let count = notes.len();
                    self.notes = notes
                        .into_iter()
                        .filter_map(|n| serde_json::to_value(n).ok())
                        .collect();
                    self.section(Section::Notes);
                    self.message = format!(
                        "{count} cached accepted Markdown notes; offline mirror may be stale"
                    );
                }
                Err(error) => self.message = format!("Cached notes unavailable: {error:?}"),
            }
            self.editor = Editor::Closed;
            return;
        }
        if let Some(query) = source.strip_prefix("note.search ") {
            self.search_notes(query.trim());
            self.editor = Editor::Closed;
            return;
        }
        if let Some(script) = source.strip_prefix("batch ") {
            match connected_actions::parse_batch(script, uid) {
                Ok(commands) => self.editor = Editor::BatchConfirm(commands),
                Err(e) => {
                    self.message = e;
                    self.editor = Editor::Command(source.into());
                }
            }
            return;
        }
        // Explicit conflict replacement is a separate operation from queuing.
        // We never auto-rebase or silently remove the rejected intent.
        if let Some(tail) = source.strip_prefix("resolve ") {
            let mut parts = tail.splitn(2, ' ');
            let old = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("");
            if old.is_empty() || rest.is_empty() {
                self.message = "usage: resolve <conflicted-command-id> <new-domain-command>".into();
                self.editor = Editor::Command(source.into());
                return;
            }
            match connected_actions::parse_intent(rest, &uid()) {
                Ok(intent) => {
                    self.editor = Editor::Replace {
                        old_id: old.into(),
                        intent,
                    }
                }
                Err(e) => {
                    self.message = e;
                    self.editor = Editor::Command(source.into());
                }
            }
            return;
        }
        match connected_actions::parse_intent(source, &uid()) {
            Ok(intent) => self.editor = Editor::Confirm(intent),
            Err(e) => {
                self.message = e;
                self.editor = Editor::Command(source.to_owned());
            }
        }
    }
    fn queue_batch(&mut self, intents: Vec<Intent>) {
        // Stage every reviewed intention atomically. Remote execution is
        // intentionally ordered and non-atomic; local staging must not be.
        let total = intents.len();
        let commands: Vec<_> = intents
            .into_iter()
            .enumerate()
            .map(|(index, intent)| (format!("{}-{index}", uid()), intent.kind, intent.action))
            .collect();
        self.message = match self.replica.queue_batch(&commands) {
            Ok(inserted) => {
                self.dirty = true;
                format!(
                    "Staged {inserted}/{total} pending intentions in one SQLite transaction. Press f to submit; none are accepted yet."
                )
            }
            Err(error) => {
                format!("Workflow staging rejected: {error:?}. No new commands were queued.")
            }
        };
    }
    fn replace(&mut self, old_id: String, intent: Intent) {
        // A disconnected replica cannot prove that another device has not
        // changed the accepted history since this resolution was reviewed.
        // Require an online refresh and freeze that accepted revision before
        // placing any replacement in the durable command queue.
        let Some(http) = self.http.as_ref() else {
            self.message="Conflict replacement requires server review; reconnect first. Original intention preserved.".into();
            return;
        };
        let new_id = uid();
        let result = self
            .replica
            .review_conflict_online(http, &old_id)
            .and_then(|review| {
                self.replica.replace_reviewed_conflict_online(
                    http,
                    &review,
                    &new_id,
                    intent.kind,
                    &intent.action,
                )?;
                Ok(review.accepted_revision)
            });
        self.message = match result {
            Ok(revision) => {
                self.dirty = true;
                format!(
                    "Conflict {old_id} superseded by pending {new_id} at reviewed server revision {revision}; original retained"
                )
            }
            Err(error) => {
                format!("Conflict replacement rejected; review latest accepted state: {error:?}")
            }
        };
    }
    fn refresh(&mut self) {
        self.message = match &self.http {
            Some(http) => match self.replica.refresh(http) {
                Ok(revision) => {
                    self.dirty = false;
                    format!("Accepted snapshot refreshed: revision {revision}")
                }
                Err(e) => format!("Refresh failed; last accepted snapshot preserved: {e:?}"),
            },
            None => "Offline: cannot refresh; accepted data stays available".into(),
        };
        self.clamp();
    }
    fn flush(&mut self) {
        self.message = match &self.http {
            Some(http) => match self.replica.flush(http) {
                Ok(FlushOutcome::Accepted(n)) => {
                    self.dirty = true;
                    format!("Authority accepted {n} command(s). Press r to refresh accepted state.")
                }
                Ok(FlushOutcome::Empty) => "No pending commands".into(),
                Ok(FlushOutcome::Blocked {
                    command_id,
                    reason,
                    accepted,
                }) => format!(
                    "Accepted {accepted}; blocked at {command_id}: {reason}. Review outbox; resolve with orbit-cli sync replica."
                ),
                Err(e) => format!("Submission failed; durable pending intents preserved: {e:?}"),
            },
            None => "Offline: command intentions are safely queued locally".into(),
        };
    }
    fn details(&self) -> String {
        let Some(record) = self.selected() else {
            return "No selected accepted record".into();
        };
        connected_actions::detail_lines(self.section, &record).join("\n")
    }
    fn main_rows(&self, height: usize) -> Vec<ListItem<'static>> {
        let rows = self.rows();
        if rows.is_empty() {
            return vec![ListItem::new(if self.section == Section::Outbox {
                "No unresolved commands".to_owned()
            } else if self.snapshot().is_null() {
                "No accepted snapshot; press r to refresh when online".to_owned()
            } else {
                "No records in this accepted collection".to_owned()
            })];
        }
        rows.into_iter()
            .enumerate()
            .skip(self.scroll)
            .take(height)
            .map(|(index, row)| {
                let caption = connected_actions::summarize(self.section, &row);
                let prefix = if index == self.selection {
                    "▶ "
                } else {
                    "  "
                };
                let item = ListItem::new(format!("{prefix}{caption}"));
                if index == self.selection {
                    item.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    item
                }
            })
            .collect()
    }
    fn adjust_scroll(&mut self, viewport: usize) {
        if self.selection < self.scroll {
            self.scroll = self.selection;
        }
        if self.selection >= self.scroll.saturating_add(viewport) {
            self.scroll = self.selection.saturating_add(1).saturating_sub(viewport);
        }
    }
    /// Read-only project view over the accepted replica snapshot. This uses the
    /// same `orbit-ui` contracts as the local renderer; pending outbox entries
    /// are deliberately excluded. Full interactive rendering remains local.
    fn open_views(&mut self) {
        let snapshot = self.snapshot();
        if snapshot.is_null() {
            self.message = "No accepted snapshot; connect and refresh before viewing".into();
            return;
        }
        let project_id = if self.section == Section::Projects {
            self.selected()
                .and_then(|row| row.get("id").and_then(Value::as_str).map(str::to_owned))
        } else {
            None
        }
        .or_else(|| {
            snapshot
                .get("projects")
                .and_then(Value::as_array)
                .and_then(|projects| projects.first())
                .and_then(|project| project.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        let Some(project_id) = project_id else {
            self.message = "No accepted project is visible".into();
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            let data = orbit_ui::project_from_snapshot(Some(&snapshot), &project_id, now_ms())
                .map_err(|error| error.to_string())?;
            let (views, generated) =
                orbit_ui::resolve_blueprint_views(None, &data, &ComponentRegistry::builtin());
            let mut lines = vec![
                format!("Project: {} ({})", data.project.name, data.project.status),
                if generated {
                    "Generated views from the accepted snapshot; the pinned blueprint view surface is not synchronized in connected mode.".into()
                } else {
                    "Pinned blueprint views".to_owned()
                },
                String::new(),
            ];
            for view in views {
                lines.push(format!("[{}]", view.title));
                for component in view.components {
                    lines.push(format!(
                        "  - {}: {}",
                        component.title,
                        summarize_body(&component.body)
                    ));
                }
                lines.push(String::new());
            }
            Ok(lines)
        })();
        match result {
            Ok(lines) => {
                self.view_lines = lines;
                self.planning_scroll = 0;
                self.editor = Editor::Views;
                self.message = "Read-only connected project views (accepted only)".into();
            }
            Err(error) => self.message = format!("Connected views unavailable: {error}"),
        }
    }

    fn footer(&self) -> String {
        let base = match &self.editor {
            Editor::Closed => self.message.clone(),
            Editor::Command(text) => format!(":{text}▏  Enter: review • Esc: cancel"),
            Editor::Filter(text) => format!("Filter: {text}▏  Enter: apply • Esc: cancel"),
            Editor::Replace { old_id, intent } => format!(
                "REPLACE CONFLICT {old_id} with {}? y: confirm • n: cancel",
                intent.description
            ),
            Editor::Confirm(intent) => format!(
                "QUEUE {}? y: confirm • n/Esc: cancel\n{}",
                intent.description, intent.action
            ),
            Editor::BatchConfirm(intents) => format!(
                "QUEUE {} pending commands in order? y: confirm • n: cancel",
                intents.len()
            ),
            Editor::Help => "Help • Esc/?: close • Tab: next section".into(),
            Editor::Details => "Record details • Esc/Enter: close".into(),
            Editor::Planning => self.message.clone(),
            Editor::Views => "Connected project views • j/k: scroll • Esc: close".into(),
            Editor::QuitConfirm(_) => {
                "Quit Orbit? Enter/y: quit • Esc/n: keep editing • Ctrl+Q/Ctrl+C: force quit".into()
            }
        };
        format!("Ctrl+Q/q quit • {base}")
    }
    fn modal_lines(&self) -> Vec<String> {
        match &self.editor {
            Editor::Help => {
                let mut lines=vec![
                    "Commands (type ':' then the full command; quoted strings supported):".to_owned(),
                    "Tab/Shift-Tab: switch sections    ↑/↓: move     /: filter".into(),
                    "r: refresh accepted     f: submit pending     o: outbox".into(),
                    "e: prefill edit command    /: filter records    x: clear filter".into(),
                    "resolve <conflict-id> <domain-command> to replace a conflicted intent".into(),
                    "batch <command>; <command>; ... to queue a reviewed workflow".into(),
                    "note.search <query> searches server online or accepted offline mirror".into(),
                    "note.sync fetches complete authoritative note mirror; note.cached browses offline".into(),
                    "plan.today [--hours N] [--limit N] [--project ID] • accepted Today view".into(),
                    "plan.impact <task-id> • read-only dependency preview".into(),
                    "plan.portfolio [--limit N] [--project ID] • cross-project task ranking".into(),
                    "plan.compare [--minutes N] [--pin ID] [--exclude ID] [--alt-minutes N] • read-only what-if".into(),
                    "Enter: inspect record      V: read-only accepted project views".into(),
                    "?: help      q: quit      pending commands are never shown as accepted".into(),
                    "'-' means unset for optional IDs or values.".into(),
                    "All writes are durable intents until accepted by the server.".into(),
                    "".into(),
                ];
                lines.extend(
                    connected_actions::help(self.section)
                        .iter()
                        .map(|s| s.to_string()),
                );
                lines
            }
            Editor::Details => self.details().lines().map(ToOwned::to_owned).collect(),
            Editor::Planning => self
                .impact
                .clone()
                .or_else(|| self.planning.as_ref().map(|doc| doc.lines.clone()))
                .unwrap_or_else(|| vec!["No accepted planning snapshot".into()]),
            Editor::Views => self.view_lines.clone(),
            Editor::QuitConfirm(_) => vec![
                "Quit Orbit and discard unsaved command text?".into(),
                "Accepted state and durable pending commands are already persisted.".into(),
                "Enter/y: quit · Esc/n: keep editing · Ctrl+Q/Ctrl+C: force quit".into(),
            ],
            Editor::BatchConfirm(intents) => connected_actions::batch_preview(intents),
            Editor::Replace { old_id, intent } => {
                vec![
                    format!("Explicit replacement of conflicted command {old_id}"),
                    "Original conflict will remain in durable audit history.".into(),
                    "Replacement gets a new command ID and remains pending until accepted.".into(),
                    "Review the currently accepted server state before resolving.".into(),
                    format!("New intent: {}", intent.description),
                    serde_json::to_string_pretty(&intent.action).unwrap_or_default(),
                    "Press y to confirm; Esc to cancel.".into(),
                ]
            }
            Editor::Confirm(intent) => {
                let mut lines = vec![format!("Confirm durable intent: {}", intent.description)];
                lines.extend(
                    serde_json::to_string_pretty(&intent.action)
                        .unwrap_or_default()
                        .lines()
                        .map(ToOwned::to_owned),
                );
                lines.push("".into());
                lines.push(
                    "This operation will NOT appear in accepted state before authority acceptance."
                        .into(),
                );
                lines
            }
            _ => Vec::new(),
        }
    }
    fn request_quit(&mut self) -> bool {
        if matches!(self.editor, Editor::QuitConfirm(_)) {
            return true;
        }
        if let Editor::Command(value) | Editor::Filter(value) = &self.editor
            && !value.trim().is_empty()
        {
            let previous = std::mem::replace(&mut self.editor, Editor::Closed);
            self.editor = Editor::QuitConfirm(Box::new(previous));
            self.message =
                "Unsaved command text: confirm quit to discard, or Esc to keep editing".into();
            return false;
        }
        true
    }

    fn on_key(&mut self, key: KeyCode, mods: KeyModifiers) -> bool {
        if matches!(key, KeyCode::Char('q'))
            && !matches!(
                self.editor,
                Editor::Command(_) | Editor::Filter(_) | Editor::QuitConfirm(_)
            )
        {
            return self.request_quit();
        }
        match &mut self.editor {
            Editor::QuitConfirm(_previous) => match key {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => return true,
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    let previous = std::mem::replace(&mut self.editor, Editor::Closed);
                    if let Editor::QuitConfirm(inner) = previous {
                        self.editor = *inner;
                    }
                    self.message = "Quit cancelled; command text preserved".into();
                }
                _ => {}
            },
            Editor::Filter(value) => match key {
                KeyCode::Esc => self.editor = Editor::Closed,
                KeyCode::Enter => {
                    self.filter = value.trim().to_owned();
                    self.selection = 0;
                    self.scroll = 0;
                    self.editor = Editor::Closed;
                    self.message = if self.filter.is_empty() {
                        "Filter cleared".into()
                    } else {
                        format!("Filter applied: {}", self.filter)
                    };
                }
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(c) => value.push(c),
                _ => {}
            },
            Editor::Command(value) => match key {
                KeyCode::Esc => self.editor = Editor::Closed,
                KeyCode::Enter => {
                    let source = value.clone();
                    self.editor = Editor::Closed;
                    if !source.trim().is_empty() {
                        self.submit(&source);
                    }
                }
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(c) => value.push(c),
                _ => {}
            },
            Editor::Confirm(_) => match key {
                KeyCode::Char('y') | KeyCode::Enter => {
                    if let Editor::Confirm(intent) =
                        std::mem::replace(&mut self.editor, Editor::Closed)
                    {
                        self.queue(intent);
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => self.editor = Editor::Closed,
                _ => {}
            },
            Editor::BatchConfirm(_) => match key {
                KeyCode::Char('y') | KeyCode::Enter => {
                    if let Editor::BatchConfirm(intents) =
                        std::mem::replace(&mut self.editor, Editor::Closed)
                    {
                        self.queue_batch(intents);
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => self.editor = Editor::Closed,
                _ => {}
            },
            Editor::Replace { .. } => match key {
                KeyCode::Char('y') | KeyCode::Enter => {
                    if let Editor::Replace { old_id, intent } =
                        std::mem::replace(&mut self.editor, Editor::Closed)
                    {
                        self.replace(old_id, intent);
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => self.editor = Editor::Closed,
                _ => {}
            },
            Editor::Planning => match key {
                KeyCode::Esc | KeyCode::Enter => {
                    if self.impact.take().is_none() {
                        self.editor = Editor::Closed;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.planning_scroll = self.planning_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.planning_scroll = self.planning_scroll.saturating_sub(1)
                }
                KeyCode::Char('J') => {
                    if let Some(doc) = &self.planning {
                        self.planning_scroll = doc.next_task(self.planning_scroll);
                    }
                }
                KeyCode::Char('K') => {
                    if let Some(doc) = &self.planning {
                        self.planning_scroll = doc.previous_task(self.planning_scroll);
                    }
                }
                KeyCode::Char('d') => self.preview_planning_impact(),
                KeyCode::Char('v') => self.preview_planning_focus(),
                KeyCode::Char('a') => self.preview_planning_actions(),
                KeyCode::Char(digit)
                    if self.planning_action_menu.is_some() && ('1'..='9').contains(&digit) =>
                {
                    self.select_planning_action((digit as u8 - b'1') as usize)
                }
                _ => {}
            },
            Editor::Views => match key {
                KeyCode::Esc | KeyCode::Enter => self.editor = Editor::Closed,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.planning_scroll = self.planning_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.planning_scroll = self.planning_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => self.planning_scroll = self.planning_scroll.saturating_add(12),
                KeyCode::PageUp => self.planning_scroll = self.planning_scroll.saturating_sub(12),
                _ => {}
            },
            Editor::Help | Editor::Details => match key {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => self.editor = Editor::Closed,
                _ => {}
            },
            Editor::Closed => match key {
                KeyCode::Tab => self.section(self.section.next()),
                KeyCode::BackTab => self.section(self.section.previous()),
                KeyCode::Char('o') => self.section(Section::Outbox),
                KeyCode::Char('?') => self.editor = Editor::Help,
                KeyCode::Char(':') => self.editor = Editor::Command(String::new()),
                KeyCode::Char('/') => self.editor = Editor::Filter(self.filter.clone()),
                KeyCode::Char('x') => {
                    self.filter.clear();
                    self.selection = 0;
                    self.scroll = 0;
                    self.message = "Record filter cleared".into();
                }
                KeyCode::Char('e') => {
                    if let Some(record) = self.selected() {
                        let options = connected_actions::edit_templates(self.section, &record);
                        if let Some((_, template)) = options.into_iter().next() {
                            self.editor = Editor::Command(template);
                        } else {
                            self.message = "No edit template for this section".into();
                        }
                    }
                }
                KeyCode::Char('V') => self.open_views(),
                KeyCode::Char('r') => self.refresh(),
                KeyCode::Char('f') => self.flush(),
                KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                KeyCode::PageDown => self.move_selection(12),
                KeyCode::PageUp => self.move_selection(-12),
                KeyCode::Home => self.selection = 0,
                KeyCode::End => self.selection = self.rows().len().saturating_sub(1),
                KeyCode::Enter => self.editor = Editor::Details,
                KeyCode::Char('c') if !mods.contains(KeyModifiers::CONTROL) => {
                    self.editor = Editor::Command("task.create ".into());
                }
                _ => {}
            },
        }
        false
    }
}

fn summarize_body(body: &ComponentBody) -> String {
    match body {
        ComponentBody::Text { lines, .. } => lines.first().cloned().unwrap_or_default(),
        ComponentBody::List { items } => format!("{} item(s)", items.len()),
        ComponentBody::Table { rows, .. } => format!("{} row(s)", rows.len()),
        ComponentBody::Metric { value, label, .. } => format!("{label}: {value}"),
        ComponentBody::Detail { properties, .. } => format!("{} property(ies)", properties.len()),
        ComponentBody::Kanban { columns } => format!("{} column(s)", columns.len()),
        ComponentBody::Timeline { events } => format!("{} event(s)", events.len()),
        ComponentBody::HabitTracker { habits } => format!("{} habit(s)", habits.len()),
        ComponentBody::MetricHistory { metrics } => format!("{} metric(s)", metrics.len()),
        ComponentBody::KnowledgeRefs { items } => format!("{} note(s)", items.len()),
        ComponentBody::AgentJobs { jobs } => format!("{} job(s)", jobs.len()),
        ComponentBody::MilestoneProgress { milestones } => {
            format!("{} milestone(s)", milestones.len())
        }
        ComponentBody::Missing { reason, .. } => reason.clone(),
    }
}

/// ORBIT_AUTHORITY_DB selects connected mode; ORBIT_AUTHORITY_SERVER binds the
/// replica to one authority identity. No local authoritative writes occur here.
pub fn run(db: String, server: String) -> Result<(), String> {
    let mut replica = AuthorityReplica::open(&db).map_err(describe)?;
    replica.bind(&server).map_err(describe)?;
    let http = std::env::var("ORBIT_AUTHORITY_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .map(|t| {
            AuthorityHttp::new(
                &server,
                &t,
                &std::env::var("ORBIT_AUTHORITY_DEVICE").map_err(|_| {
                    "ORBIT_AUTHORITY_DEVICE required for network access".to_string()
                })?,
            )
            .map_err(describe)
        })
        .transpose()?;
    let mut app = Connected {
        replica,
        http,
        message: "Connected Orbit • Tab: section • : command • ?: help • f: flush • r: refresh"
            .into(),
        section: Section::Tasks,
        selection: 0,
        editor: Editor::Closed,
        filter: String::new(),
        scroll: 0,
        dirty: false,
        notes: Vec::new(),
        planning: None,
        planning_scroll: 0,
        impact: None,
        planning_action_menu: None,
        pending_scenario: None,
        view_lines: Vec::new(),
    };
    let mut session = crate::terminal::TerminalSession::enter()?;
    let terminal = session.terminal();
    let result = loop {
        let count = app.rows().len();
        let pending = app.replica.pending_count().unwrap_or(0);
        let revision = app.accepted_revision();
        let area = terminal.size().map_err(describe)?;
        let viewport = usize::from(area.height.saturating_sub(9)).max(1);
        app.adjust_scroll(viewport);
        let lines = app.main_rows(viewport);
        let modal = app.modal_lines();
        let footer = app.footer();
        let section = app.section;
        let dirty = app.dirty;
        if let Err(e)=terminal.draw(|f|{
            let parts=Layout::default().direction(Direction::Vertical).constraints([
                Constraint::Length(3),Constraint::Length(3),Constraint::Min(4),Constraint::Length(3),
            ]).split(f.area());
            f.render_widget(Paragraph::new(format!("SERVER AUTHORITY • accepted revision: {revision:?} • unresolved: {pending} • {}",if dirty{"refresh recommended"}else{"accepted view"})).block(Block::default().borders(Borders::ALL)),parts[0]);
            let tabs=Section::ALL.iter().map(|s|if *s==section {format!("[{}]",s.title())}else{s.title().into()}).collect::<Vec<_>>().join("  ");
            f.render_widget(Paragraph::new(tabs).block(Block::default().borders(Borders::ALL)),parts[1]);
            if modal.is_empty(){
                f.render_widget(List::new(lines).block(Block::default().borders(Borders::ALL).title(format!("{} ({count}) • accepted records",section.title()))),parts[2]);
            }else{
                let offset=if matches!(app.editor,Editor::Planning | Editor::Views) && app.impact.is_none() {
                    app.planning_scroll.min(u16::MAX as usize) as u16
                } else {0};
                f.render_widget(Paragraph::new(modal.join("\n"))
                    .scroll((offset,0)).wrap(Wrap{trim:false})
                    .block(Block::default().borders(Borders::ALL).title(if matches!(app.editor,Editor::Planning){"Today / Next • accepted only"}else if matches!(app.editor,Editor::Views){"Project views • connected accepted snapshot"}else{"Orbit • review / help"})),parts[2]);
            }
            f.render_widget(Paragraph::new(footer).wrap(Wrap{trim:true}).block(Block::default().borders(Borders::ALL).title("Action")),parts[3]);
        }){break Err(describe(e));}
        match event::poll(Duration::from_millis(250)) {
            Ok(false) => continue,
            Err(e) => break Err(describe(e)),
            Ok(true) => {}
        }
        let event = match event::read() {
            Ok(e) => e,
            Err(e) => break Err(describe(e)),
        };
        let Event::Key(key) = event else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q'))
        {
            if app.request_quit() {
                break Ok(());
            }
            continue;
        }
        if app.on_key(key.code, key.modifiers) {
            break Ok(());
        }
    };
    session.restore();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn offline() -> Connected {
        Connected {
            replica: AuthorityReplica::open(":memory:").unwrap(),
            http: None,
            message: String::new(),
            section: Section::Tasks,
            selection: 0,
            editor: Editor::Closed,
            filter: String::new(),
            scroll: 0,
            dirty: false,
            notes: Vec::new(),
            planning: None,
            planning_scroll: 0,
            impact: None,
            planning_action_menu: None,
            pending_scenario: None,
            view_lines: Vec::new(),
        }
    }
    #[test]
    fn task_focus_requires_accepted_snapshot_and_never_queues() {
        let mut app = offline();
        app.submit("plan.focus task-123");
        assert!(
            app.message.contains("No accepted snapshot"),
            "{}",
            app.message
        );
        assert_eq!(app.replica.pending_count().unwrap(), 0);
        assert!(matches!(app.editor, Editor::Command(_)));
    }
    #[test]
    fn group_focus_requires_accepted_snapshot_and_never_queues() {
        let mut app = offline();
        app.submit("plan.group --project p --milestone m");
        assert!(
            app.message.contains("No accepted snapshot"),
            "{}",
            app.message
        );
        assert_eq!(app.replica.pending_count().unwrap(), 0);
        assert!(matches!(app.editor, Editor::Command(_)));
    }
    #[test]
    fn task_context_arguments_are_bounded_by_usage_contract() {
        let mut app = offline();
        app.submit("plan.focus");
        assert!(app.message.contains("Usage"));
        app.submit("plan.focus task extra");
        assert!(app.message.contains("Usage"));
        app.submit("plan.group --milestone m");
        assert!(app.message.contains("Usage"));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn project_context_rejects_malformed_milestone_switch() {
        let mut app = offline();
        app.submit("plan.group --project p --bad m");
        assert!(app.message.contains("Usage"));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn today_requires_an_accepted_snapshot() {
        let mut app = offline();
        app.submit("plan.today --hours 4 --limit 7");
        assert!(matches!(app.editor, Editor::Closed));
        assert!(app.message.contains("No accepted snapshot"));
        assert!(app.planning.is_none());
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn today_argument_errors_do_not_change_commands() {
        let mut app = offline();
        app.submit("plan.today --hours 0");
        assert!(matches!(app.editor, Editor::Command(_)));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
        assert!(app.message.contains("hours"));
    }
    #[test]
    fn impact_requires_an_accepted_planning_selection() {
        let mut app = offline();
        app.submit("plan.impact task-x");
        assert!(matches!(app.editor, Editor::Command(_)));
        assert!(app.message.contains("plan.today"));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn planning_modal_is_read_only() {
        let mut app = offline();
        app.planning = Some(planning_ui::PlanningDocument {
            lines: vec!["header".into(), "READY  a  Task A".into(), "footer".into()],
            task_anchors: vec![(1, "a".into())],
        });
        app.editor = Editor::Planning;
        app.on_key(KeyCode::Char('J'), KeyModifiers::NONE);
        assert_eq!(app.planning_scroll, 1);
        app.on_key(KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(
            app.message.contains("Cannot preview impact")
                || app.message.contains("Accepted snapshot")
        );
        app.on_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(matches!(app.editor, Editor::Closed));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn queued_intent_never_changes_accepted_state() {
        let mut app = offline();
        app.submit("task.create offline task");
        assert!(matches!(app.editor, Editor::Confirm(_)));
        app.on_key(KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(app.replica.pending_count().unwrap(), 1);
        assert!(app.snapshot().is_null());
        assert!(app.message.contains("PENDING"));
    }
    #[test]
    fn cancel_leaves_outbox_empty() {
        let mut app = offline();
        app.submit("routine.create morning - Wake up");
        app.on_key(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn invalid_command_is_never_queued() {
        let mut app = offline();
        app.submit("schedule.resize block 0");
        assert!(matches!(app.editor, Editor::Command(_)));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn can_switch_all_sections_without_network() {
        let mut app = offline();
        for _ in 0..Section::ALL.len() {
            app.on_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        assert_eq!(app.section, Section::Tasks);
        app.on_key(KeyCode::Char('o'), KeyModifiers::NONE);
        assert_eq!(app.section, Section::Outbox);
    }
    #[test]
    fn batch_preview_does_not_commit_until_confirmed() {
        let mut app = offline();
        app.submit("batch task.create First; task.create Second; routine.create morning - Morning");
        assert!(matches!(app.editor, Editor::BatchConfirm(_)));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
        app.on_key(KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(app.replica.pending_count().unwrap(), 3);
        assert!(app.snapshot().is_null());
    }
    #[test]
    fn invalid_batch_stays_unqueued() {
        let mut app = offline();
        app.submit("batch task.create First; schedule.resize slot 0");
        assert_eq!(app.replica.pending_count().unwrap(), 0);
        assert!(matches!(app.editor, Editor::Command(_)));
    }
    #[test]
    fn offline_flush_does_not_consume_pending_intent() {
        let mut app = offline();
        app.submit("milestone.create m1 p1 first milestone");
        app.on_key(KeyCode::Char('y'), KeyModifiers::NONE);
        app.flush();
        assert_eq!(app.replica.pending_count().unwrap(), 1);
    }
}

#[cfg(test)]
mod scenario_approval_tui_tests {
    use super::*;
    fn offline() -> Connected {
        Connected {
            replica: AuthorityReplica::open(":memory:").unwrap(),
            http: None,
            message: String::new(),
            section: Section::Tasks,
            selection: 0,
            editor: Editor::Closed,
            filter: String::new(),
            scroll: 0,
            dirty: false,
            notes: Vec::new(),
            planning: None,
            planning_scroll: 0,
            impact: None,
            planning_action_menu: None,
            pending_scenario: None,
            view_lines: Vec::new(),
        }
    }
    #[test]
    fn approval_without_review_fails_closed() {
        let mut app = offline();
        app.submit("plan.approve --confirm");
        assert!(app.message.contains("No reviewed scenario"));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn implicit_approval_is_rejected() {
        let mut app = offline();
        app.submit("plan.approve");
        assert!(app.message.contains("--confirm"));
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn no_offline_calendar_review() {
        let mut app = offline();
        app.submit("plan.review --hours 2");
        assert!(app.message.contains("Offline"));
        assert!(app.pending_scenario.is_none());
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }
}

#[cfg(test)]
mod m28_connected_view_tests {
    use super::*;
    use serde_json::json;

    fn offline() -> Connected {
        Connected {
            replica: AuthorityReplica::open(":memory:").unwrap(),
            http: None,
            message: String::new(),
            section: Section::Projects,
            selection: 0,
            editor: Editor::Closed,
            filter: String::new(),
            scroll: 0,
            dirty: false,
            notes: Vec::new(),
            planning: None,
            planning_scroll: 0,
            impact: None,
            planning_action_menu: None,
            pending_scenario: None,
            view_lines: Vec::new(),
        }
    }

    #[test]
    fn connected_project_views_use_accepted_snapshot_not_pending_commands() {
        let mut app = offline();
        app.replica
            .accept_snapshot(&json!({
                "revision": 3,
                "history_fingerprint": "a".repeat(64),
                "projects": [{"id":"p1","name":"Connected view","status":"active"}],
                "tasks": [{"id":"t1","title":"Accepted task","status":"ready","project_id":"p1"}],
                "task_dependencies": [],
                "habits": [],
                "habit_occurrences": [],
                "metrics": [],
                "metric_observations": [],
                "schedule": [],
                "routines": [],
                "milestones": [],
                "strategies": []
            }))
            .unwrap();
        app.open_views();
        assert!(matches!(app.editor, Editor::Views));
        assert!(
            app.view_lines
                .iter()
                .any(|line| line.contains("Connected view")),
            "{:?}",
            app.view_lines
        );
        assert!(
            app.view_lines.iter().any(|line| line.contains("Overview")),
            "{:?}",
            app.view_lines
        );
        assert_eq!(app.replica.pending_count().unwrap(), 0);
    }

    #[test]
    fn connected_views_without_an_accepted_snapshot_do_not_invent_state() {
        let mut app = offline();
        app.open_views();
        assert!(matches!(app.editor, Editor::Closed));
        assert!(app.message.contains("No accepted snapshot"));
        assert!(app.view_lines.is_empty());
    }
}

#[cfg(test)]
mod m28_connected_quit_tests {
    use super::*;

    fn connected() -> Connected {
        Connected {
            replica: AuthorityReplica::open(":memory:").unwrap(),
            http: None,
            message: String::new(),
            section: Section::Tasks,
            selection: 0,
            editor: Editor::Closed,
            filter: String::new(),
            scroll: 0,
            dirty: false,
            notes: Vec::new(),
            planning: None,
            planning_scroll: 0,
            impact: None,
            planning_action_menu: None,
            pending_scenario: None,
            view_lines: Vec::new(),
        }
    }

    #[test]
    fn command_and_filter_text_require_connected_quit_confirmation() {
        for editor in [
            Editor::Command("draft command".into()),
            Editor::Filter("draft filter".into()),
        ] {
            let mut app = connected();
            app.editor = editor;
            assert!(!app.request_quit());
            assert!(matches!(app.editor, Editor::QuitConfirm(_)));
            assert!(!app.on_key(KeyCode::Esc, KeyModifiers::NONE));
            assert!(matches!(app.editor, Editor::Command(_) | Editor::Filter(_)));
        }
    }

    #[test]
    fn connected_quit_confirmation_quits_only_on_confirm() {
        let mut app = connected();
        app.editor = Editor::QuitConfirm(Box::new(Editor::Closed));
        assert!(!app.on_key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(matches!(app.editor, Editor::QuitConfirm(_)));
        assert!(app.on_key(KeyCode::Enter, KeyModifiers::NONE));
    }

    #[test]
    fn connected_closed_editor_q_quits_and_footer_shows_shortcut() {
        let mut app = connected();
        assert!(app.footer().contains("Ctrl+Q/q quit"), "{}", app.footer());
        assert!(app.on_key(KeyCode::Char('q'), KeyModifiers::NONE));
    }
}
