//! Thin Rust terminal client for Orbit.
//! Business rules remain in orbit-client/application/domain.

mod connected;
mod connected_actions;
mod planning_ui;

use std::{
    env, io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use orbit_client::{
    CreateKnowledgeRequest, CreateTaskRequest, OrbitClient, PlanningPolicyDto,
    RecordHabitOccurrenceRequest, SetTaskStatusRequest, TaskPlanProposalDto,
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Today,
    Inbox,
    Projects,
    Knowledge,
    HabitsPlan,
    Agent,
}
impl Tab {
    fn index(self) -> usize {
        match self {
            Self::Today => 0,
            Self::Inbox => 1,
            Self::Projects => 2,
            Self::Knowledge => 3,
            Self::HabitsPlan => 4,
            Self::Agent => 5,
        }
    }
    fn next(self) -> Self {
        match self {
            Self::Today => Self::Inbox,
            Self::Inbox => Self::Projects,
            Self::Projects => Self::Knowledge,
            Self::Knowledge => Self::HabitsPlan,
            Self::HabitsPlan => Self::Agent,
            Self::Agent => Self::Today,
        }
    }
    fn prev(self) -> Self {
        match self {
            Self::Today => Self::Agent,
            Self::Inbox => Self::Today,
            Self::Projects => Self::Inbox,
            Self::Knowledge => Self::Projects,
            Self::HabitsPlan => Self::Knowledge,
            Self::Agent => Self::HabitsPlan,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PromptKind {
    Task,
    Knowledge,
    Search,
    Term,
    EvidenceBind,
    EvidenceUnbind,
    MetricTarget,
    MetricObserve,
}
struct Prompt {
    kind: PromptKind,
    value: String,
}

struct App {
    client: OrbitClient,
    tab: Tab,
    selected: usize,
    prompt: Option<Prompt>,
    knowledge_query: String,
    knowledge_refs: Vec<orbit_application::KnowledgeTermOccurrence>,
    knowledge_detail: Option<(String, usize)>,
    message: String,
    pending_plan: Option<TaskPlanProposalDto>,
    today: Option<planning_ui::PlanningDocument>,
    today_scroll: usize,
    today_details: Option<Vec<String>>,
    today_action_menu: Option<(
        orbit_runtime::planning_brief::WorkBrief,
        orbit_runtime::planning_actions::PlanningActionMenu,
    )>,
    today_error: Option<String>,
    today_hours: i64,
    today_project: Option<String>,
}

impl App {
    fn open(path: String) -> Result<Self, String> {
        let mut app = Self {
            client: OrbitClient::open(&path).map_err(|e| format!("{e:?}"))?,
            tab: Tab::Today,
            selected: 0,
            prompt: None,
            knowledge_query: String::new(),
            knowledge_refs: Vec::new(),
            knowledge_detail: None,
            message: "q quit • ←/→ tabs • c task • n note • / search".into(),
            pending_plan: None,
            today: None,
            today_scroll: 0,
            today_details: None,
            today_action_menu: None,
            today_error: None,
            today_hours: 12,
            today_project: None,
        };
        app.refresh_today();
        Ok(app)
    }
    fn refresh_today(&mut self) {
        self.today_details = None;
        self.today_action_menu = None;
        match planning_ui::request(now_ms(), self.today_hours, 100, self.today_project.clone())
            .and_then(|request| {
                self.client
                    .work_brief(&request)
                    .map_err(|error| error.message)
            }) {
            Ok(brief) => {
                let document = planning_ui::PlanningDocument::from_brief(&brief);
                self.today_scroll = self
                    .today_scroll
                    .min(document.lines.len().saturating_sub(1));
                self.today = Some(document);
                self.today_error = None;
                self.message = format!(
                    "Today / Next rev {} • {}h • project {} • [/] horizon • J/K task • d impact • r refresh",
                    brief.revision,
                    self.today_hours,
                    self.today_project.as_deref().unwrap_or("all")
                );
            }
            Err(error) => {
                // Keep previously accepted content visible, clearly labelled stale.
                self.today_error = Some(error.clone());
                self.message = format!("Today refresh failed (previous view retained): {error}");
            }
        }
    }

    fn adjust_today_horizon(&mut self, longer: bool) {
        self.today_hours = if longer {
            self.today_hours.saturating_mul(2).min(168)
        } else {
            (self.today_hours / 2).max(1)
        };
        self.today_scroll = 0;
        self.refresh_today();
    }

    fn filter_today_to_selected_project(&mut self) {
        let Some(project_id) = self.selected_project_id() else {
            self.message = "Select a project first to filter Today".into();
            return;
        };
        self.today_project = Some(project_id);
        self.today_scroll = 0;
        self.tab = Tab::Today;
        self.refresh_today();
    }

    fn jump_today(&mut self, forward: bool) {
        if let Some(doc) = &self.today {
            self.today_scroll = if forward {
                doc.next_task(self.today_scroll)
            } else {
                doc.previous_task(self.today_scroll)
            };
        }
    }

    fn today_impact(&mut self) {
        let Some(task_id) = self
            .today
            .as_ref()
            .and_then(|doc| doc.selected_task(self.today_scroll))
            .map(str::to_owned)
        else {
            self.message = "No accepted task selected in Today / Next".into();
            return;
        };
        let request = orbit_runtime::task_impact::CompletionImpactRequest {
            task_id,
            now_ms: now_ms(),
            next_limit: 20,
        };
        match self.client.completion_impact(&request) {
            Ok(impact) => {
                self.today_details = Some(planning_ui::render_impact(&impact));
                self.message = "Read-only completion impact • Esc to return to Today".into();
            }
            Err(error) => self.message = format!("Impact preview unavailable: {}", error.message),
        }
    }

    fn today_focus(&mut self) {
        let Some(task_id) = self
            .today
            .as_ref()
            .and_then(|doc| doc.selected_task(self.today_scroll))
            .map(str::to_owned)
        else {
            self.message = "No accepted Today task selected".into();
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            let request =
                planning_ui::request(now_ms(), self.today_hours, 100, self.today_project.clone())?;
            let brief = self.client.work_brief(&request).map_err(|e| e.message)?;
            let focus = self
                .client
                .planning_focus(
                    &brief,
                    &orbit_runtime::planning_focus::FocusRequest {
                        task_id,
                        max_neighbors: 12,
                    },
                )
                .map_err(|error| error.message)?;
            let menu = self
                .client
                .planning_task_actions(
                    &brief,
                    &orbit_runtime::planning_actions::TaskActionRequest {
                        task_id: focus.task.id.clone(),
                        authority_available: false,
                    },
                )
                .map_err(|error| error.message)?;
            let mut lines = planning_ui::render_focus(&focus);
            lines.push(String::new());
            lines.extend(planning_ui::render_action_menu(&menu));
            Ok(lines)
        })();
        match result {
            Ok(lines) => {
                self.today_details = Some(lines);
                self.message = "Accepted task navigation • Esc returns to Today".into();
            }
            Err(error) => self.message = format!("Task navigation unavailable: {error}"),
        }
    }

    fn today_actions(&mut self) {
        let Some(task_id) = self
            .today
            .as_ref()
            .and_then(|doc| doc.selected_task(self.today_scroll))
            .map(str::to_owned)
        else {
            self.message = "No accepted Today task selected".into();
            return;
        };
        let result = (|| -> Result<_, String> {
            let request =
                planning_ui::request(now_ms(), self.today_hours, 100, self.today_project.clone())?;
            let brief = self.client.work_brief(&request).map_err(|e| e.message)?;
            let menu = self
                .client
                .planning_task_actions(
                    &brief,
                    &orbit_runtime::planning_actions::TaskActionRequest {
                        task_id,
                        authority_available: true,
                    },
                )
                .map_err(|e| e.message)?;
            Ok((brief, menu))
        })();
        match result {
            Ok((brief, menu)) => {
                self.today_details = Some(planning_ui::render_action_menu(&menu));
                self.today_action_menu = Some((brief, menu));
                self.message = "Press 1-9 to inspect a reviewed action • Esc to return".into();
            }
            Err(error) => self.message = format!("Action menu unavailable: {error}"),
        }
    }

    fn today_select_action(&mut self, index: usize) {
        let Some((brief, menu)) = self.today_action_menu.clone() else {
            self.message = "Open an accepted action menu with 'a' first".into();
            return;
        };
        let result = (|| -> Result<Vec<String>, String> {
            let selection = orbit_runtime::planning_handoff::select_from_menu(&menu, index)
                .map_err(str::to_owned)?;
            // The accepted revision may have changed since the menu was drawn.
            let latest = self
                .client
                .work_brief(&planning_ui::request(
                    brief.evaluated_at_ms,
                    (brief.ends_at_ms - brief.evaluated_at_ms) / 3_600_000,
                    100,
                    self.today_project.clone(),
                )?)
                .map_err(|e| e.message)?;
            if latest.revision != brief.revision {
                return Err("Accepted state changed: reopen action menu".into());
            }
            let request = orbit_runtime::planning_actions::TaskActionRequest {
                task_id: menu
                    .selected_task_id
                    .clone()
                    .ok_or("Action menu missing task target")?,
                authority_available: true,
            };
            let handoff = self
                .client
                .planning_task_handoff(&latest, &request, &selection)
                .map_err(|e| e.message)?;
            if let orbit_runtime::planning_handoff::HandoffView::Preview(preview) = &handoff.view
                && preview.operation
                    == orbit_runtime::planning_handoff::PreviewOperation::CompletionImpact
            {
                let report = self
                    .client
                    .completion_impact(&orbit_runtime::task_impact::CompletionImpactRequest {
                        task_id: preview.task_id.clone(),
                        now_ms: preview.evaluated_at_ms,
                        next_limit: 20,
                    })
                    .map_err(|e| e.message)?;
                if report.revision != handoff.revision {
                    return Err("Impact preview revision changed".into());
                }
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_impact(&report));
                return Ok(lines);
            }
            if let Some(request) = planning_ui::dependency_request_from_handoff(&handoff)? {
                let report = self
                    .client
                    .preview_task_dependency(&request)
                    .map_err(|e| e.message)?;
                if report.accepted_revision != handoff.revision {
                    return Err("Dependency preview revision changed".into());
                }
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_dependency_preview(&report));
                return Ok(lines);
            }
            if let Some(request) = planning_ui::calendar_request_from_handoff(&handoff)? {
                let plan = self.client.schedule_plan(&request).map_err(|e| e.message)?;
                let mut lines = planning_ui::render_handoff(&handoff);
                lines.push(String::new());
                lines.extend(planning_ui::render_task_calendar_preview(&handoff, &plan)?);
                return Ok(lines);
            }
            Ok(planning_ui::render_handoff(&handoff))
        })();
        match result {
            Ok(lines) => {
                self.today_details = Some(lines);
                self.today_action_menu = None;
                self.message = "Read-only action destination • Esc returns to Today".into();
            }
            Err(error) => {
                self.today_action_menu = None;
                self.message = format!("Action rejected: {error}");
            }
        }
    }

    fn today_project_focus(&mut self) {
        let selected = self
            .today
            .as_ref()
            .and_then(|doc| doc.selected_task(self.today_scroll))
            .map(str::to_owned);
        let result = (|| -> Result<Vec<String>, String> {
            // The task's project must come from the accepted planning read
            // model, not from an unrelated legacy application task table.
            let full_request = planning_ui::request(now_ms(), self.today_hours, 100, None)?;
            let full_brief = self
                .client
                .work_brief(&full_request)
                .map_err(|e| e.message)?;
            let project_id = selected
                .as_ref()
                .and_then(|id| full_brief.items.iter().find(|row| &row.id == id))
                .and_then(|row| row.project_id.clone())
                .or_else(|| self.today_project.clone())
                .ok_or("Select a task assigned to a project, or filter Today by project")?;
            let request =
                planning_ui::request(now_ms(), self.today_hours, 100, Some(project_id.clone()))?;
            let brief = self.client.work_brief(&request).map_err(|e| e.message)?;
            let group = self
                .client
                .planning_group(
                    &brief,
                    &orbit_runtime::planning_focus::GroupFocusRequest {
                        project_id,
                        milestone_id: None,
                        max_tasks: 12,
                    },
                )
                .map_err(|error| error.message)?;
            Ok(planning_ui::render_group_focus(&group))
        })();
        match result {
            Ok(lines) => {
                self.today_details = Some(lines);
                self.message = "Accepted project context • Esc returns to Today".into();
            }
            Err(error) => self.message = format!("Project context unavailable: {error}"),
        }
    }

    fn tasks(&self) -> Vec<orbit_client::TaskDto> {
        self.client.list_tasks().unwrap_or_default()
    }
    fn projects(&self) -> Vec<orbit_client::ProjectDto> {
        self.client.list_projects().unwrap_or_default()
    }
    fn knowledge(&self) -> Vec<orbit_client::KnowledgeItemDto> {
        let q = self.knowledge_query.to_lowercase();
        self.client
            .list_knowledge()
            .unwrap_or_default()
            .into_iter()
            .filter(|i| {
                q.is_empty()
                    || i.title.to_lowercase().contains(&q)
                    || i.body_markdown.to_lowercase().contains(&q)
                    || i.kind.contains(&q)
            })
            .collect()
    }
    fn start_prompt(&mut self, kind: PromptKind) {
        if matches!(
            kind,
            PromptKind::EvidenceBind
                | PromptKind::EvidenceUnbind
                | PromptKind::MetricTarget
                | PromptKind::MetricObserve
        ) && self.tab != Tab::Projects
        {
            self.message = "Select the Projects tab first".into();
            return;
        }
        self.prompt = Some(Prompt {
            kind,
            value: String::new(),
        })
    }
    fn selected_project_id(&self) -> Option<String> {
        self.projects().get(self.selected).map(|p| p.id.clone())
    }
    fn submit_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let v = prompt.value.trim().to_owned();
        if v.is_empty() {
            self.message = "Cancelled empty input".into();
            return;
        }
        match prompt.kind {
            PromptKind::Task => match self.client.create_task(CreateTaskRequest {
                id: None,
                title: v,
                project_id: None,
                milestone_id: None,
            }) {
                Ok(t) => self.message = format!("Captured task {}", t.id),
                Err(e) => self.message = format!("Capture failed: {}", e.message),
            },
            PromptKind::Knowledge => match self.client.create_knowledge(CreateKnowledgeRequest {
                id: None,
                kind: "note".into(),
                title: v,
                body_markdown: String::new(),
            }) {
                Ok(k) => self.message = format!("Captured note {}", k.id),
                Err(e) => self.message = format!("Capture failed: {}", e.message),
            },
            PromptKind::Term => match self.client.markdown_term_references(&v, 25) {
                Ok(hits) => {
                    self.tab = Tab::Knowledge;
                    self.knowledge_refs = hits.clone();
                    self.knowledge_detail = None;
                    self.selected = 0;
                    self.message = if hits.is_empty() {
                        format!("No references: {v}")
                    } else {
                        let first = &hits[0];
                        format!(
                            "{} references to '{}'; first: {} {}:{}:{} — {}",
                            hits.len(),
                            v,
                            first.item_id.as_str(),
                            first.field,
                            first.line,
                            first.column,
                            first.preview
                        )
                    };
                }
                Err(err) => self.message = format!("Reference lookup failed: {}", err.message),
            },
            PromptKind::Search => {
                self.knowledge_detail = None;
                self.knowledge_refs.clear();
                self.knowledge_query = v;
                self.tab = Tab::Knowledge;
                self.selected = 0;
                self.message = format!("Knowledge filter: {}", self.knowledge_query)
            }
            PromptKind::EvidenceBind | PromptKind::EvidenceUnbind => {
                let Some(project_id) = self.selected_project_id() else {
                    self.message = "No project selected".into();
                    return;
                };
                let Some((kind, evidence_id)) = v.split_once(':') else {
                    self.message = "Expected habit:<id> or metric:<id>".into();
                    return;
                };
                let bind = matches!(prompt.kind, PromptKind::EvidenceBind);
                match self.client.change_project_evidence(
                    project_id,
                    kind.into(),
                    evidence_id.into(),
                    bind,
                ) {
                    Ok(()) => {
                        self.message = if bind {
                            "Evidence bound (queued for sync)".into()
                        } else {
                            "Evidence unbound (queued for sync)".into()
                        }
                    }
                    Err(e) => self.message = format!("Evidence change rejected: {}", e.message),
                }
            }
            PromptKind::MetricTarget => {
                let parts: Vec<_> = v.split_whitespace().collect();
                if parts.len() != 4 {
                    self.message = "Expected: metric-id baseline target at_least|at_most".into();
                    return;
                }
                let (Ok(baseline), Ok(target)) = (parts[1].parse::<i64>(), parts[2].parse::<i64>())
                else {
                    self.message = "Baseline and target must be integers".into();
                    return;
                };
                let dto = orbit_client::MetricTargetDto {
                    metric_id: parts[0].into(),
                    baseline,
                    target,
                    direction: parts[3].into(),
                };
                match self.client.set_metric_target(dto) {
                    Ok(()) => self.message = "Metric target queued for sync".into(),
                    Err(e) => self.message = format!("Metric target rejected: {}", e.message),
                }
            }
            PromptKind::MetricObserve => {
                let parts: Vec<_> = v.split_whitespace().collect();
                if parts.len() != 2 {
                    self.message = "Expected: metric-id value".into();
                    return;
                }
                let Ok(value) = parts[1].parse::<i64>() else {
                    self.message = "Observation value must be an integer".into();
                    return;
                };
                match self
                    .client
                    .record_metric_observation(parts[0].into(), now_ms(), value)
                {
                    Ok(()) => self.message = "Metric observation queued for sync".into(),
                    Err(e) => self.message = format!("Metric observation rejected: {}", e.message),
                }
            }
        }
    }
    fn open_selected_reference(&mut self) {
        if self.tab != Tab::Knowledge {
            return;
        }
        let Some(reference) = self.knowledge_refs.get(self.selected).cloned() else {
            return;
        };
        self.knowledge_detail = Some((reference.item_id.as_str().to_owned(), reference.line));
        self.message = format!(
            "Opened {} at {}:{}:{} (Esc to return)",
            reference.item_id.as_str(),
            reference.field,
            reference.line,
            reference.column
        );
    }
    fn triage_selected(&mut self, status: &str) {
        if self.tab != Tab::Inbox {
            return;
        }
        let inbox: Vec<_> = self
            .tasks()
            .into_iter()
            .filter(|t| t.status == "inbox")
            .collect();
        if let Some(t) = inbox.get(self.selected.min(inbox.len().saturating_sub(1))) {
            match self.client.set_task_status(SetTaskStatusRequest {
                id: t.id.clone(),
                status: status.into(),
            }) {
                Ok(x) => self.message = format!("{} → {}", x.title, x.status),
                Err(e) => self.message = format!("Triage failed: {}", e.message),
            }
        }
    }

    fn complete_selected_habit(&mut self) {
        if self.tab != Tab::HabitsPlan {
            return;
        }
        let habits = self
            .client
            .list_habits()
            .unwrap_or_default()
            .into_iter()
            .filter(|h| h.active)
            .collect::<Vec<_>>();
        if let Some(h) = habits.get(self.selected.min(habits.len().saturating_sub(1))) {
            match self
                .client
                .record_habit_occurrence(RecordHabitOccurrenceRequest {
                    id: None,
                    habit_id: h.id.clone(),
                    occurred_at_ms: Some(now_ms()),
                    duration_minutes: None,
                }) {
                Ok(_) => self.message = format!("Checked habit: {}", h.name),
                Err(e) => self.message = format!("Habit check failed: {}", e.message),
            }
        }
    }
    fn preview_plan(&mut self) {
        if self.tab != Tab::HabitsPlan {
            return;
        }
        let start = now_ms();
        let end = start + 8 * 60 * 60 * 1000;
        let policy = PlanningPolicyDto {
            protected_minutes: 60,
            max_new_work_minutes: Some(240),
            fixed_buffer_before_minutes: 5,
            fixed_buffer_after_minutes: 5,
        };
        match self.client.preview_task_plan(start, end, policy) {
            Ok(plan) => {
                let n = plan.scheduled.len();
                let u = plan.unscheduled.len();
                self.pending_plan = Some(plan);
                self.message =
                    format!("Plan preview: {n} scheduled, {u} unscheduled • Shift+P apply")
            }
            Err(e) => self.message = format!("Plan preview failed: {}", e.message),
        }
    }
    fn apply_plan(&mut self) {
        if self.tab != Tab::HabitsPlan {
            return;
        }
        let Some(plan) = self.pending_plan.clone() else {
            self.message = "No pending plan; press p first".into();
            return;
        };
        match self.client.apply_task_plan(plan) {
            Ok(blocks) => {
                self.pending_plan = None;
                self.message = format!(
                    "Applied {} schedule blocks after revalidation",
                    blocks.len()
                )
            }
            Err(e) => self.message = format!("Plan apply rejected: {}", e.message),
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("orbit-tui: {e}")
    }
}
fn run() -> Result<(), String> {
    let path = match orbit_client::deployment::from_env().map_err(|error| error.to_string())? {
        orbit_client::deployment::Deployment::Connected { replica, server } => {
            return connected::run(replica.to_string_lossy().into_owned(), server);
        }
        orbit_client::deployment::Deployment::Local { database } => database,
    };
    let mut app = App::open(path.to_string_lossy().into_owned())?;
    enable_raw_mode().map_err(err)?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(err)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(err)?;
    let result = event_loop(&mut terminal, &mut app);
    disable_raw_mode().map_err(err)?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(err)?;
    terminal.show_cursor().map_err(err)?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    loop {
        terminal.draw(|f| draw(f, app)).map_err(err)?;
        if !event::poll(Duration::from_millis(250)).map_err(err)? {
            continue;
        }
        let Event::Key(key) = event::read().map_err(err)? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if app.prompt.is_some() {
            match key.code {
                KeyCode::Esc => {
                    app.prompt = None;
                    app.message = "Prompt cancelled".into()
                }
                KeyCode::Enter => app.submit_prompt(),
                KeyCode::Backspace => {
                    if let Some(prompt) = app.prompt.as_mut() {
                        prompt.value.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(prompt) = app.prompt.as_mut() {
                        prompt.value.push(c);
                    }
                }
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Char('q') => break,
            KeyCode::Esc => {
                app.today_details = None;
                app.today_action_menu = None;
                app.knowledge_detail = None;
            }
            KeyCode::Enter => app.open_selected_reference(),
            KeyCode::Right | KeyCode::Tab => {
                app.tab = app.tab.next();
                app.selected = 0
            }
            KeyCode::Left => {
                app.tab = app.tab.prev();
                app.selected = 0
            }
            KeyCode::Down | KeyCode::Char('j') if app.tab == Tab::Today => {
                app.today_scroll = app.today_scroll.saturating_add(1)
            }
            KeyCode::Down | KeyCode::Char('j') => app.selected = app.selected.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') if app.tab == Tab::Today => {
                app.today_scroll = app.today_scroll.saturating_sub(1)
            }
            KeyCode::Up | KeyCode::Char('k') => app.selected = app.selected.saturating_sub(1),
            KeyCode::Char('c') => app.start_prompt(PromptKind::Task),
            KeyCode::Char('n') => app.start_prompt(PromptKind::Knowledge),
            KeyCode::Char('/') => app.start_prompt(PromptKind::Search),
            KeyCode::Char('f') => app.start_prompt(PromptKind::Term),
            KeyCode::Char('a') if app.tab != Tab::Today => app.triage_selected("ready"),
            KeyCode::Char('i') => app.triage_selected("planned"),
            KeyCode::Char('x') => app.triage_selected("cancelled"),
            KeyCode::Char('r') if app.tab == Tab::Today => app.refresh_today(),
            KeyCode::Char('r') => app.message = "Refreshed from canonical Orbit state".into(),
            KeyCode::Char('J') if app.tab == Tab::Today => app.jump_today(true),
            KeyCode::Char('K') if app.tab == Tab::Today => app.jump_today(false),
            KeyCode::Char('d') if app.tab == Tab::Today => app.today_impact(),
            KeyCode::Char('v') if app.tab == Tab::Today => app.today_focus(),
            KeyCode::Char('a') if app.tab == Tab::Today => app.today_actions(),
            KeyCode::Char(digit)
                if app.tab == Tab::Today
                    && app.today_action_menu.is_some()
                    && ('1'..='9').contains(&digit) =>
            {
                app.today_select_action((digit as u8 - b'1') as usize)
            }
            KeyCode::Char('g') if app.tab == Tab::Today => app.today_project_focus(),
            KeyCode::Char('[') if app.tab == Tab::Today => app.adjust_today_horizon(false),
            KeyCode::Char(']') if app.tab == Tab::Today => app.adjust_today_horizon(true),
            KeyCode::Char('F') if app.tab == Tab::Projects => {
                app.filter_today_to_selected_project()
            }
            KeyCode::Char('F') if app.tab == Tab::Today => {
                app.today_project = None;
                app.today_scroll = 0;
                app.refresh_today();
            }
            KeyCode::Char('b') => app.start_prompt(PromptKind::EvidenceBind),
            KeyCode::Char('U') => app.start_prompt(PromptKind::EvidenceUnbind),
            KeyCode::Char('t') => app.start_prompt(PromptKind::MetricTarget),
            KeyCode::Char('o') => app.start_prompt(PromptKind::MetricObserve),
            KeyCode::Char(' ') => app.complete_selected_habit(),
            KeyCode::Char('p') => app.preview_plan(),
            KeyCode::Char('P') => app.apply_plan(),
            KeyCode::Char('1') => app.tab = Tab::Today,
            KeyCode::Char('2') => app.tab = Tab::Inbox,
            KeyCode::Char('3') => app.tab = Tab::Projects,
            KeyCode::Char('4') => app.tab = Tab::Knowledge,
            KeyCode::Char('5') => app.tab = Tab::HabitsPlan,
            KeyCode::Char('6') => app.tab = Tab::Agent,
            _ => {}
        }
    }
    Ok(())
}

fn draw(f: &mut ratatui::Frame, app: &App) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(f.area());
    let titles: Vec<Line> = [
        "Today",
        "Inbox",
        "Projects",
        "Knowledge",
        "Habits + Plan",
        "Agent",
    ]
    .into_iter()
    .map(Line::from)
    .collect();
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .block(Block::default().borders(Borders::ALL).title("Orbit"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(tabs, areas[0]);
    match app.tab {
        Tab::Today => draw_today(f, areas[1], app),
        Tab::Inbox => draw_inbox(f, areas[1], app),
        Tab::Projects => draw_projects(f, areas[1], app),
        Tab::Knowledge => draw_knowledge(f, areas[1], app),
        Tab::HabitsPlan => draw_habits_plan(f, areas[1], app),
        Tab::Agent => draw_agent(f, areas[1], app),
    };
    let footer = if let Some(p) = &app.prompt {
        format!("> {}_", p.value)
    } else {
        app.message.clone()
    };
    f.render_widget(
        Paragraph::new(footer).block(Block::default().borders(Borders::ALL)),
        areas[2],
    );
}

fn draw_today(f: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(73), Constraint::Percentage(27)])
        .split(area);
    let (lines, title) = if let Some(details) = &app.today_details {
        (details.clone(), "Completion impact (Esc to return)")
    } else if let Some(document) = &app.today {
        (
            document.lines.clone(),
            "Today / Next • j/k scroll • J/K task • [/] hours • F all • d impact",
        )
    } else {
        (
            vec!["Planning unavailable • press r to retry".into()],
            "Today / Next",
        )
    };
    let mut visible = lines;
    if let Some(error) = &app.today_error {
        visible.insert(0, format!("STALE ACCEPTED VIEW • refresh failed: {error}"));
    }
    let scroll = app.today_scroll.min(visible.len().saturating_sub(1));
    f.render_widget(
        Paragraph::new(visible.join("\n"))
            .block(Block::default().borders(Borders::ALL).title(title))
            .scroll((scroll.min(u16::MAX as usize) as u16, 0))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    let habits = app
        .client
        .list_habits()
        .unwrap_or_default()
        .into_iter()
        .filter(|habit| habit.active)
        .map(|habit| ListItem::new(format!("{} ({})", habit.name, habit.cadence)))
        .collect::<Vec<_>>();
    f.render_widget(
        List::new(habits).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Active habits"),
        ),
        chunks[1],
    );
}

fn draw_inbox(f: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &App) {
    let inbox: Vec<_> = app
        .tasks()
        .into_iter()
        .filter(|t| t.status == "inbox")
        .collect();
    let items = inbox
        .iter()
        .enumerate()
        .map(|(i, t)| {
            ListItem::new(Line::from(vec![
                if i == app.selected {
                    Span::styled("> ", Style::default().add_modifier(Modifier::BOLD))
                } else {
                    Span::raw("  ")
                },
                Span::raw(&t.title),
            ]))
        })
        .collect::<Vec<_>>();
    let title = "Inbox  a=Ready  i=Planned  x=Cancel  c=Capture";
    f.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn draw_projects(f: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &App) {
    let projects = app.projects();
    let rows = projects
        .iter()
        .map(|p| {
            let summary = app
                .client
                .project_evidence_summary(&p.id, now_ms())
                .unwrap_or_else(|_| "health/evidence unavailable".into());
            ListItem::new(format!(
                "{} [{}|{}] {} criteria:{}/{}{}",
                p.name,
                p.status,
                p.mode,
                summary,
                p.success_criteria
                    .iter()
                    .filter(|c| c.status == "satisfied" || c.status == "waived")
                    .count(),
                p.success_criteria.len(),
                if p.completion_ready {
                    " ready-to-complete"
                } else {
                    ""
                }
            ))
        })
        .collect::<Vec<_>>();
    f.render_widget(
        List::new(rows).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Projects • 7d evidence | b bind • U unbind • t target • o observe"),
        ),
        area,
    );
}

fn draw_knowledge(f: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &App) {
    if let Some((id, line)) = &app.knowledge_detail
        && let Some(note) = app
            .client
            .list_knowledge()
            .unwrap_or_default()
            .into_iter()
            .find(|n| &n.id == id)
    {
        let context = note
            .body_markdown
            .lines()
            .enumerate()
            .filter(|(i, _)| (*i as isize + 1 - *line as isize).abs() <= 8)
            .map(|(i, text)| {
                format!(
                    "{:>5} {}{}",
                    i + 1,
                    if i + 1 == *line { "> " } else { "  " },
                    text
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        f.render_widget(
            Paragraph::new(context).wrap(Wrap { trim: false }).block(
                Block::default().borders(Borders::ALL).title(format!(
                    "{} • line {} • Esc to references",
                    note.title, line
                )),
            ),
            area,
        );
        return;
    }
    let items = if app.knowledge_refs.is_empty() {
        app.knowledge()
            .into_iter()
            .enumerate()
            .map(|(i, k)| {
                ListItem::new(format!(
                    "{} [{}:{}] {}",
                    if i == app.selected { ">" } else { " " },
                    k.kind,
                    k.status,
                    k.title
                ))
            })
            .collect::<Vec<_>>()
    } else {
        app.knowledge_refs
            .iter()
            .enumerate()
            .map(|(i, r)| {
                ListItem::new(format!(
                    "{} {} {}:{}:{}  {}",
                    if i == app.selected { ">" } else { " " },
                    r.item_id.as_str(),
                    r.field,
                    r.line,
                    r.column,
                    r.preview
                ))
            })
            .collect::<Vec<_>>()
    };
    let title = if !app.knowledge_refs.is_empty() {
        "References  Enter=Open  /=Search  f=Find references".into()
    } else if app.knowledge_query.is_empty() {
        "Knowledge  /=Search  f=Find references  n=Capture".into()
    } else {
        format!("Knowledge filter: {}", app.knowledge_query)
    };
    f.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn draw_habits_plan(f: &mut ratatui::Frame, area: ratatui::layout::Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);
    let habits = app
        .client
        .list_habits()
        .unwrap_or_default()
        .into_iter()
        .filter(|h| h.active)
        .collect::<Vec<_>>();
    let rows = habits
        .iter()
        .enumerate()
        .map(|(i, h)| {
            ListItem::new(format!(
                "{}{}  ({})",
                if i == app.selected { "> " } else { "  " },
                h.name,
                h.cadence
            ))
        })
        .collect::<Vec<_>>();
    f.render_widget(
        List::new(rows).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Habits • Space = check off"),
        ),
        chunks[0],
    );
    let text = if let Some(plan) = &app.pending_plan {
        let scheduled = plan
            .scheduled
            .iter()
            .map(|b| format!("{}: {} → {}", b.task_id, b.start_ms, b.end_ms))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Deterministic task plan preview\n\n{}\n\nUnscheduled: {}\n\np = regenerate • Shift+P = apply/revalidate",
            scheduled,
            plan.unscheduled.len()
        )
    } else {
        "No pending plan.\n\np = preview next 8h\nPolicy: 60m protected, max 240m new work, 5m fixed buffers".into()
    };
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Planner")),
        chunks[1],
    );
}

fn draw_agent(f: &mut ratatui::Frame, area: ratatui::layout::Rect, _app: &App) {
    let agent = env::var("ORBIT_MCP_AGENT_ID").unwrap_or_else(|_| "not configured".into());
    let session = env::var("ORBIT_MCP_SESSION_ID").unwrap_or_else(|_| "default".into());
    let mode = env::var("ORBIT_MCP_MODE").unwrap_or_else(|_| "read_only".into());
    let projects =
        env::var("ORBIT_MCP_PROJECTS").unwrap_or_else(|_| "all project-scoped data".into());
    let text = format!(
        "Local agent configuration\n\nagent: {agent}\nsession: {session}\nmode: {mode}\nproject allowlist: {projects}\n\nConnected-mode synchronization uses the native authoritative replica, not Git."
    );
    f.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Agent inspection"),
        ),
        area,
    );
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}
fn err<E: std::fmt::Debug>(e: E) -> String {
    format!("{e:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let filename = std::env::temp_dir().join(format!(
            "orbit-tui-planning-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        App::open(filename.to_string_lossy().into_owned()).unwrap()
    }
    #[test]
    fn today_starts_with_an_accepted_read_only_brief() {
        let app = app();
        assert!(
            app.today.is_some(),
            "fresh local authority should build an empty accepted brief"
        );
        assert!(
            app.today
                .as_ref()
                .unwrap()
                .lines
                .iter()
                .any(|line| line.contains("TODAY / NEXT"))
        );
        assert!(app.today_details.is_none());
    }
    #[test]
    fn horizon_controls_remain_bounded_and_recompute_the_brief() {
        let mut app = app();
        for _ in 0..20 {
            app.adjust_today_horizon(true);
        }
        assert_eq!(app.today_hours, 168);
        assert!(app.today.is_some());
        for _ in 0..20 {
            app.adjust_today_horizon(false);
        }
        assert_eq!(app.today_hours, 1);
        assert!(app.today.is_some());
    }
    #[test]
    fn impact_of_empty_brief_never_mutates_state() {
        let mut app = app();
        let before = app.client.list_tasks().unwrap();
        app.today_impact();
        assert!(app.message.contains("No accepted task selected"));
        assert_eq!(app.client.list_tasks().unwrap().len(), before.len());
    }
    #[test]
    fn today_navigation_handles_zero_tasks() {
        let mut app = app();
        app.jump_today(true);
        app.jump_today(false);
        assert!(app.today_details.is_none());
    }
    #[test]
    fn tabs_wrap_without_business_logic() {
        assert_eq!(Tab::Agent.next(), Tab::Today);
        assert_eq!(Tab::Today.prev(), Tab::Agent);
        assert_eq!(Tab::Knowledge.next(), Tab::HabitsPlan);
    }
}
