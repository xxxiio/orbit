//! Connected-mode command palette. This module is pure: it parses a user's
//! request into a canonical authority command but never mutates accepted state.
//! The connected terminal owns transport and its durable outbox.
use orbit_client::authority::CommandKind;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    pub kind: CommandKind,
    pub action: Value,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Projects,
    Tasks,
    Habits,
    Metrics,
    Schedule,
    Routines,
    Milestones,
    Strategies,
    Notes,
    Outbox,
}
impl Section {
    pub const ALL: [Section; 10] = [
        Self::Projects,
        Self::Tasks,
        Self::Habits,
        Self::Metrics,
        Self::Schedule,
        Self::Routines,
        Self::Milestones,
        Self::Strategies,
        Self::Notes,
        Self::Outbox,
    ];
}

/// Tokenization supports single and double quoted arguments and escaped
/// whitespace. The palette deliberately does not use a shell or evaluate code.
pub fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    let mut started = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' && quote != Some('\'') {
            // A backslash escapes only whitespace, a quote delimiter or another
            // backslash. Any other following character keeps the backslash
            // literal so that Windows-style separators such as `docs\secret.md`
            // reach the physical-vault path validator unchanged instead of
            // being silently collapsed into a valid relative path.
            match chars.peek().copied() {
                Some(next)
                    if next.is_whitespace() || next == '\\' || next == '\'' || next == '"' =>
                {
                    current.push(chars.next().expect("peeked character"));
                    started = true;
                }
                Some(_) => {
                    current.push(ch);
                    started = true;
                }
                None => return Err("trailing escape in command".into()),
            }
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            started = true;
        } else if ch.is_whitespace() {
            if started {
                words.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(ch);
            started = true;
        }
    }
    if quote.is_some() {
        return Err("unclosed quote in command".into());
    }
    if started {
        words.push(current);
    }
    Ok(words)
}

fn id(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(format!(
            "invalid identifier: {value:?} (use letters, digits, '-' or '_')"
        ));
    }
    Ok(value)
}
fn nonempty(value: &str) -> Result<&str, String> {
    if value.trim().is_empty() {
        return Err("value cannot be empty".into());
    }
    Ok(value)
}
fn positive(value: &str) -> Result<u32, String> {
    let n = value
        .parse::<u32>()
        .map_err(|_| format!("invalid positive integer: {value}"))?;
    if n == 0 {
        return Err("duration must be positive".into());
    }
    Ok(n)
}
fn timestamp(value: &str) -> Result<i64, String> {
    value
        .parse::<i64>()
        .map_err(|_| format!("invalid timestamp in milliseconds: {value}"))
}
fn enum_value<'a>(value: &'a str, allowed: &[&str]) -> Result<&'a str, String> {
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "invalid value {value:?}; expected {}",
            allowed.join(" | ")
        ))
    }
}
fn csv_ids(value: &str) -> Result<Vec<String>, String> {
    if value == "-" || value.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<_> = value.split(',').collect();
    let mut result = Vec::new();
    for part in parts {
        id(part)?;
        if result.iter().any(|s| s == part) {
            return Err(format!("duplicate identifier: {part}"));
        }
        result.push(part.to_string());
    }
    Ok(result)
}
fn csv_choices(value: &str, allowed: &[&str]) -> Result<Vec<String>, String> {
    let items = csv_ids(value)?;
    if items.is_empty() {
        return Err("at least one value required".into());
    }
    for item in &items {
        enum_value(item, allowed)?;
    }
    Ok(items)
}
fn maybe_id(value: &str) -> Result<Option<String>, String> {
    if value == "-" {
        Ok(None)
    } else {
        Ok(Some(id(value)?.to_owned()))
    }
}
fn optional_text(value: &str) -> Result<Option<String>, String> {
    if value == "-" {
        Ok(None)
    } else {
        Ok(Some(nonempty(value)?.to_owned()))
    }
}
// Physical-vault paths are relative to the server-owned Markdown vault.
// Never allow clients to smuggle a host path, dot segment, or backslash.
fn vault_path(value: &str, markdown: bool) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 1024
        || value.contains('\\')
        || value.contains('\0')
        || value.starts_with('/')
        || value
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.'))
        || markdown && !value.to_ascii_lowercase().ends_with(".md")
    {
        return Err(format!("invalid vault-relative path: {value:?}"));
    }
    Ok(value)
}
fn file_body(parts: &[String]) -> Result<String, String> {
    let body = parts.join(" ");
    if body.trim().is_empty() || body.len() > 512 * 1024 {
        return Err("Markdown content must be nonempty and at most 512 KiB".into());
    }
    Ok(body)
}

fn build(kind: CommandKind, action: Value, description: String) -> Intent {
    Intent {
        kind,
        action,
        description,
    }
}
fn usage(op: &str, spec: &str) -> String {
    format!("usage: {op} {spec}")
}
fn args_exact<'a>(
    op: &str,
    args: &'a [String],
    n: usize,
    spec: &str,
) -> Result<&'a [String], String> {
    if args.len() == n {
        Ok(args)
    } else {
        Err(usage(op, spec))
    }
}
fn args_min<'a>(
    op: &str,
    args: &'a [String],
    n: usize,
    spec: &str,
) -> Result<&'a [String], String> {
    if args.len() >= n {
        Ok(args)
    } else {
        Err(usage(op, spec))
    }
}
fn text_tail(args: &[String], from: usize) -> Result<String, String> {
    Ok(nonempty(&args[from..].join(" "))?.to_owned())
}
const SIGNALS: &[&str] = &[
    "milestones",
    "tasks",
    "blockers",
    "forecast",
    "habit_consistency",
    "time_invested",
    "learning_evidence",
    "metric_value",
    "knowledge_evidence",
];
const MODES: &[&str] = &[
    "standard",
    "learning",
    "habit_driven",
    "metric_target",
    "maintenance",
    "custom",
];
const TASK_STATUSES: &[&str] = &[
    "inbox",
    "planned",
    "ready",
    "in_progress",
    "waiting",
    "blocked",
    "review",
    "deferred",
    "done",
    "cancelled",
];
const MILESTONE_STATUSES: &[&str] = &[
    "planned",
    "in_progress",
    "blocked",
    "completed",
    "cancelled",
];

/// Every public palette command has one canonical command-family payload.
/// TUI forms never update the replica's accepted snapshot directly.
pub fn parse_intent(line: &str, new_id: &str) -> Result<Intent, String> {
    let tokens = tokenize(line)?;
    let Some(op) = tokens.first().map(String::as_str) else {
        return Err("enter a command; press ? for help".into());
    };
    let a = &tokens[1..];
    match op {
        // Physical Markdown and folder operations use the SAME ordered
        // authority outbox as domain commands. Pending content is never
        // written directly to the local vault or accepted snapshot.
        "note.create" => {
            args_min(op, a, 3, "<note-id> <relative-path.md> <markdown-body...>")?;
            Ok(build(
                CommandKind::Markdown,
                json!({"kind":"Create","note_id":id(&a[0])?,
                "path":vault_path(&a[1],true)?,"body":file_body(&a[2..])?}),
                "Create Markdown note (pending)".into(),
            ))
        }
        "note.update" => {
            args_exact(op, a, 3, "<note-id> <expected-body> <replacement-body>")?;
            Ok(build(
                CommandKind::Markdown,
                json!({"kind":"Update","note_id":id(&a[0])?,
                "expected":nonempty(&a[1])?,"replacement":nonempty(&a[2])?}),
                "Update Markdown note with optimistic content guard".into(),
            ))
        }
        "note.move" => {
            args_exact(
                op,
                a,
                4,
                "<note-id> <old-path.md> <new-path.md> <expected-body>",
            )?;
            let from = vault_path(&a[1], true)?;
            let to = vault_path(&a[2], true)?;
            if from == to {
                return Err("move requires different paths".into());
            }
            Ok(build(
                CommandKind::Markdown,
                json!({"kind":"Move","note_id":id(&a[0])?,
                "from":from,"to":to,"expected":nonempty(&a[3])?}),
                "Move Markdown note with optimistic content guard".into(),
            ))
        }
        "folder.create" => {
            args_exact(op, a, 1, "<relative-folder-path>")?;
            Ok(build(
                CommandKind::FolderCreate,
                json!({"path":vault_path(&a[0],false)?}),
                "Create vault folder".into(),
            ))
        }
        "folder.move" => {
            args_exact(op, a, 2, "<old-folder-path> <new-folder-path>")?;
            let from = vault_path(&a[0], false)?;
            let to = vault_path(&a[1], false)?;
            if from == to || to.starts_with(&format!("{from}/")) {
                return Err("folder cannot move into itself or its descendants".into());
            }
            Ok(build(
                CommandKind::FolderMove,
                json!({"from":from,"to":to}),
                "Move vault folder".into(),
            ))
        }
        "project.create" => {
            args_min(op, a, 2, "<project-id> <name...>")?;
            Ok(build(
                CommandKind::Project,
                json!({"project_id":id(&a[0])?,"name":text_tail(a,1)?}),
                "Create project".into(),
            ))
        }
        "habit.create" => {
            args_min(op, a, 3, "<habit-id> <daily|weekly:N> <name...>")?;
            let cadence = &a[1];
            if cadence != "daily" {
                let count = cadence
                    .strip_prefix("weekly:")
                    .ok_or("cadence must be daily or weekly:N")?
                    .parse::<u8>()
                    .map_err(|_| "invalid weekly cadence")?;
                if !(1..=7).contains(&count) {
                    return Err("weekly cadence requires 1..7".into());
                }
            }
            Ok(build(
                CommandKind::Evidence,
                json!({"type":"create_habit","habit_id":id(&a[0])?,"cadence":cadence,"name":text_tail(a,2)?}),
                "Create habit".into(),
            ))
        }
        "habit.record" => {
            args_exact(op, a, 3, "<occurrence-id> <habit-id> <occurred-at-ms>")?;
            let occurred_at_ms = timestamp(&a[2])?;
            if occurred_at_ms < 0 {
                return Err("occurrence time must not be negative".into());
            }
            Ok(build(
                CommandKind::Evidence,
                json!({"type":"record_habit","occurrence_id":id(&a[0])?,"habit_id":id(&a[1])?,"occurred_at_ms":occurred_at_ms}),
                "Record habit occurrence".into(),
            ))
        }
        "metric.target" => {
            args_exact(
                op,
                a,
                4,
                "<metric-id> <baseline> <target> <increasing|decreasing>",
            )?;
            let baseline = a[1].parse::<i64>().map_err(|_| "invalid baseline")?;
            let target = a[2].parse::<i64>().map_err(|_| "invalid target")?;
            let direction = enum_value(&a[3], &["increasing", "decreasing"])?;
            if (direction == "increasing" && baseline >= target)
                || (direction == "decreasing" && baseline <= target)
            {
                return Err("target must improve on baseline in selected direction".into());
            }
            Ok(build(
                CommandKind::Evidence,
                json!({"type":"set_metric_target","metric_id":id(&a[0])?,"baseline":baseline,"target":target,"direction":direction}),
                "Create metric target".into(),
            ))
        }
        "metric.record" => {
            args_exact(op, a, 3, "<metric-id> <observed-at-ms> <value>")?;
            let observed_at_ms = timestamp(&a[1])?;
            if observed_at_ms < 0 {
                return Err("observation time must not be negative".into());
            }
            let value = a[2].parse::<i64>().map_err(|_| "invalid observed value")?;
            Ok(build(
                CommandKind::Evidence,
                json!({"type":"record_metric","metric_id":id(&a[0])?,"observed_at_ms":observed_at_ms,"value":value}),
                "Record metric observation".into(),
            ))
        }
        "task.create" => {
            args_min(op, a, 1, "<title...>")?;
            Ok(build(
                CommandKind::Task,
                json!({"type":"create","task_id":id(new_id)?,"title":text_tail(a,0)?}),
                "Create task".into(),
            ))
        }
        "task.rename" => {
            args_min(op, a, 2, "<task-id> <title...>")?;
            Ok(build(
                CommandKind::Task,
                json!({"type":"rename","task_id":id(&a[0])?,"title":text_tail(a,1)?}),
                "Rename task".into(),
            ))
        }
        "task.status" => {
            args_exact(
                op,
                a,
                2,
                "<task-id> <todo|in_progress|blocked|done|cancelled>",
            )?;
            Ok(build(
                CommandKind::Task,
                json!({"type":"change_status","task_id":id(&a[0])?,"status":enum_value(&a[1],TASK_STATUSES)?}),
                "Set task status".into(),
            ))
        }
        "task.project" => {
            args_exact(op, a, 2, "<task-id> <project-id|->")?;
            Ok(build(
                CommandKind::Task,
                json!({"type":"assign_project","task_id":id(&a[0])?,"project_id":maybe_id(&a[1])?}),
                "Assign task project".into(),
            ))
        }
        "task.milestone" => {
            args_exact(op, a, 2, "<task-id> <milestone-id|->")?;
            Ok(build(
                CommandKind::Task,
                json!({"type":"assign_milestone","task_id":id(&a[0])?,"milestone_id":maybe_id(&a[1])?}),
                "Assign task milestone".into(),
            ))
        }
        "task.plan" => {
            args_exact(
                op,
                a,
                4,
                "<task-id> <duration-minutes|-> <earliest-ms|-> <deadline-ms|->",
            )?;
            let duration = if a[1] == "-" {
                None
            } else {
                Some(positive(&a[1])?)
            };
            let earliest = if a[2] == "-" {
                None
            } else {
                Some(timestamp(&a[2])?)
            };
            let deadline = if a[3] == "-" {
                None
            } else {
                Some(timestamp(&a[3])?)
            };
            if let (Some(x), Some(y)) = (earliest, deadline)
                && x > y
            {
                return Err("earliest start must not exceed deadline".into());
            }
            Ok(build(
                CommandKind::Task,
                json!({"type":"set_planning","task_id":id(&a[0])?,"estimated_minutes":duration,"earliest_start_at_ms":earliest,"deadline_at_ms":deadline}),
                "Plan task".into(),
            ))
        }
        "schedule.create" => {
            args_min(
                op,
                a,
                6,
                "<id> <starts-ms> <duration-minutes> <fixed|movable> <standalone|task|habit> <target-id|-> <title...>",
            )?;
            if a.len() < 7 {
                return Err(usage(
                    op,
                    "<id> <starts-ms> <duration-minutes> <fixed|movable> <standalone|task|habit> <target-id|-> <title...>",
                ));
            }
            let target_kind = enum_value(&a[4], &["standalone", "task", "habit"])?;
            let target_id = maybe_id(&a[5])?;
            if (target_kind == "standalone") != target_id.is_none() {
                return Err(
                    "target kind standalone requires '-', task/habit requires an id".into(),
                );
            }
            Ok(build(
                CommandKind::Schedule,
                json!({"type":"create","block_id":id(&a[0])?,"starts_at_ms":timestamp(&a[1])?,"duration_minutes":positive(&a[2])?,"flexibility":enum_value(&a[3], &["fixed","movable"])?,"target_kind":target_kind,"target_id":target_id,"title":text_tail(a,6)?}),
                "Create schedule block".into(),
            ))
        }
        "schedule.move" => {
            args_exact(op, a, 2, "<block-id> <starts-ms>")?;
            Ok(build(
                CommandKind::Schedule,
                json!({"type":"reschedule","block_id":id(&a[0])?,"starts_at_ms":timestamp(&a[1])?}),
                "Reschedule block".into(),
            ))
        }
        "schedule.resize" => {
            args_exact(op, a, 2, "<block-id> <duration-minutes>")?;
            Ok(build(
                CommandKind::Schedule,
                json!({"type":"resize","block_id":id(&a[0])?,"duration_minutes":positive(&a[1])?}),
                "Resize block".into(),
            ))
        }
        "schedule.rename" => {
            args_min(op, a, 2, "<block-id> <title...>")?;
            Ok(build(
                CommandKind::Schedule,
                json!({"type":"rename","block_id":id(&a[0])?,"title":text_tail(a,1)?}),
                "Rename block".into(),
            ))
        }
        "schedule.target" => {
            args_exact(op, a, 3, "<block-id> <standalone|task|habit> <target-id|->")?;
            let target_kind = enum_value(&a[1], &["standalone", "task", "habit"])?;
            let target_id = maybe_id(&a[2])?;
            if (target_kind == "standalone") != target_id.is_none() {
                return Err("invalid target kind/ID combination".into());
            }
            Ok(build(
                CommandKind::Schedule,
                json!({"type":"retarget","block_id":id(&a[0])?,"target_kind":target_kind,"target_id":target_id}),
                "Retarget block".into(),
            ))
        }
        "schedule.delete" => {
            args_exact(op, a, 1, "<block-id>")?;
            Ok(build(
                CommandKind::Schedule,
                json!({"type":"delete","block_id":id(&a[0])?}),
                "Delete block".into(),
            ))
        }
        "routine.create" => {
            args_min(op, a, 3, "<id> <habit-ids-comma-separated|-> <name...>")?;
            Ok(build(
                CommandKind::Routine,
                json!({"type":"create","routine_id":id(&a[0])?,"habit_ids":csv_ids(&a[1])?,"name":text_tail(a,2)?}),
                "Create routine".into(),
            ))
        }
        "routine.rename" => {
            args_min(op, a, 2, "<routine-id> <name...>")?;
            Ok(build(
                CommandKind::Routine,
                json!({"type":"rename","routine_id":id(&a[0])?,"name":text_tail(a,1)?}),
                "Rename routine".into(),
            ))
        }
        "routine.habits" => {
            args_exact(op, a, 2, "<routine-id> <habit-ids-comma-separated|->")?;
            Ok(build(
                CommandKind::Routine,
                json!({"type":"set_habits","routine_id":id(&a[0])?,"habit_ids":csv_ids(&a[1])?}),
                "Set routine habits".into(),
            ))
        }
        "routine.delete" => {
            args_exact(op, a, 1, "<routine-id>")?;
            Ok(build(
                CommandKind::Routine,
                json!({"type":"delete","routine_id":id(&a[0])?}),
                "Delete routine".into(),
            ))
        }
        "milestone.create" => {
            args_min(op, a, 3, "<id> <project-id> <name...>")?;
            Ok(build(
                CommandKind::Milestone,
                json!({"type":"create","milestone_id":id(&a[0])?,"project_id":id(&a[1])?,"name":text_tail(a,2)?}),
                "Create milestone".into(),
            ))
        }
        "milestone.rename" => {
            args_min(op, a, 2, "<milestone-id> <name...>")?;
            Ok(build(
                CommandKind::Milestone,
                json!({"type":"rename","milestone_id":id(&a[0])?,"name":text_tail(a,1)?}),
                "Rename milestone".into(),
            ))
        }
        "milestone.status" => {
            args_exact(
                op,
                a,
                2,
                "<milestone-id> <planned|in_progress|blocked|completed|cancelled>",
            )?;
            Ok(build(
                CommandKind::Milestone,
                json!({"type":"change_status","milestone_id":id(&a[0])?,"status":enum_value(&a[1],MILESTONE_STATUSES)?}),
                "Set milestone status".into(),
            ))
        }
        "milestone.project" => {
            args_exact(op, a, 2, "<milestone-id> <project-id>")?;
            Ok(build(
                CommandKind::Milestone,
                json!({"type":"move_project","milestone_id":id(&a[0])?,"project_id":id(&a[1])?}),
                "Move milestone".into(),
            ))
        }
        "milestone.delete" => {
            args_exact(op, a, 1, "<milestone-id>")?;
            Ok(build(
                CommandKind::Milestone,
                json!({"type":"delete","milestone_id":id(&a[0])?}),
                "Delete milestone".into(),
            ))
        }
        "strategy.objective" => {
            args_min(op, a, 2, "<project-id> <objective...|->")?;
            let objective = optional_text(&a[1..].join(" "))?;
            Ok(build(
                CommandKind::Strategy,
                json!({"type":"set_objective","project_id":id(&a[0])?,"objective":objective}),
                "Set strategy objective".into(),
            ))
        }
        "strategy.signals" => {
            args_exact(op, a, 2, "<project-id> <signals-comma-separated>")?;
            Ok(build(
                CommandKind::Strategy,
                json!({"type":"set_progress_signals","project_id":id(&a[0])?,"progress_signals":csv_choices(&a[1],SIGNALS)?}),
                "Set progress signals".into(),
            ))
        }
        "strategy.criterion" => {
            args_exact(
                op,
                a,
                3,
                "<project-id> <criterion-key> <pending|satisfied|waived>",
            )?;
            Ok(build(
                CommandKind::Strategy,
                json!({"type":"set_criterion_status","project_id":id(&a[0])?,"criterion_key":id(&a[1])?,"status":enum_value(&a[2], &["pending","satisfied","waived"])?}),
                "Set criterion status".into(),
            ))
        }
        "strategy.configure" => {
            args_min(
                op,
                a,
                4,
                "<project-id> <mode> <signals-comma-separated> <objective|-> [key:required:description ...]",
            )?;
            let project_id = id(&a[0])?;
            let mode = enum_value(&a[1], MODES)?;
            let signals = csv_choices(&a[2], SIGNALS)?;
            let objective = optional_text(&a[3])?;
            let mut criteria = Vec::new();
            for term in &a[4..] {
                let mut segments = term.splitn(3, ':');
                let key = id(segments.next().unwrap_or(""))?;
                let required = match segments.next() {
                    Some("true") => true,
                    Some("false") => false,
                    _ => return Err("criterion must have key:true|false:description".into()),
                };
                let description =
                    nonempty(segments.next().ok_or("criterion description missing")?)?;
                if criteria.iter().any(|c: &Value| c["key"] == key) {
                    return Err(format!("duplicate criterion {key}"));
                }
                criteria.push(json!({"key":key,"required":required,"description":description}));
            }
            Ok(build(
                CommandKind::Strategy,
                json!({"type":"configure","project_id":project_id,"mode":mode,"objective":objective,"progress_signals":signals,"criteria":criteria}),
                "Configure project strategy".into(),
            ))
        }
        _ => Err(format!(
            "unknown command {op:?}; press ? for supported commands"
        )),
    }
}

pub fn help(section: Section) -> &'static [&'static str] {
    match section {
        Section::Tasks => &[
            "task.create <title...>",
            "task.rename <task-id> <title...>",
            "task.status <task-id> <todo|in_progress|blocked|done|cancelled>",
            "task.project <task-id> <project-id|->",
            "task.milestone <task-id> <milestone-id|->",
            "task.plan <task-id> <minutes|-> <earliest-ms|-> <deadline-ms|->",
        ],
        Section::Schedule => &[
            "schedule.create <id> <start-ms> <minutes> <fixed|movable> <standalone|task|habit> <id|-> <title...>",
            "schedule.move <id> <start-ms>",
            "schedule.resize <id> <minutes>",
            "schedule.rename <id> <title...>",
            "schedule.target <id> <standalone|task|habit> <id|->",
            "schedule.delete <id>",
        ],
        Section::Routines => &[
            "routine.create <id> <habit1,habit2|-> <name...>",
            "routine.rename <id> <name...>",
            "routine.habits <id> <habit1,habit2|->",
            "routine.delete <id>",
        ],
        Section::Milestones => &[
            "milestone.create <id> <project-id> <name...>",
            "milestone.rename <id> <name...>",
            "milestone.status <id> <planned|in_progress|blocked|completed|cancelled>",
            "milestone.project <id> <project-id>",
            "milestone.delete <id>",
        ],
        Section::Strategies => &[
            "strategy.configure <project-id> <mode> <signals> <objective|-> [key:true|false:description ...]",
            "strategy.objective <project-id> <objective...|->",
            "strategy.signals <project-id> <signal1,signal2>",
            "strategy.criterion <project-id> <key> <pending|satisfied|waived>",
        ],
        Section::Projects => &[
            "project.create <project-id> <name...>",
            "Manage project strategies under Strategies.",
        ],
        Section::Habits => &[
            "habit.create <habit-id> <daily|weekly:N> <name...>",
            "habit.record <occurrence-id> <habit-id> <occurred-at-ms>",
        ],
        Section::Metrics => &[
            "metric.target <metric-id> <baseline> <target> <increasing|decreasing>",
            "metric.record <metric-id> <observed-at-ms> <value>",
        ],
        Section::Notes => &[
            "note.search <query...> — browse accepted server notes",
            "note.show <note-id|vault-relative-path.md> — read one accepted note body",
            "note.create <note-id> <relative-path.md> <markdown-body...>",
            "note.update <note-id> <expected-body> <replacement-body>",
            "note.move <note-id> <old-path.md> <new-path.md> <expected-body>",
            "folder.create <relative-path>",
            "folder.move <old-folder> <new-folder>",
        ],
        Section::Outbox => &[
            "note.create <note-id> <relative-path.md> <markdown-body...>",
            "note.update <note-id> <expected-body> <replacement-body>",
            "note.move <note-id> <old-path.md> <new-path.md> <expected-body>",
            "folder.create <relative-path>",
            "folder.move <old-folder> <new-folder>",
            "Review pending and rejected commands; resolve <command-id> <new-command>",
        ],
    }
}

/// Parse a batch of domain commands separated by semicolons. Semicolons
/// inside quoted names are literal. No command is queued until every command
/// has been parsed successfully, including duplicate/new ID validation.
/// Execution is explicitly sequential, not transactional: earlier queued
/// intentions survive a later storage failure and are reported to the user.
pub fn split_batch(source: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for ch in source.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            current.push(ch);
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            current.push(ch);
        } else {
            match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    current.push(ch);
                }
                ';' => {
                    if current.trim().is_empty() {
                        return Err("empty command in batch".into());
                    }
                    parts.push(current.trim().to_owned());
                    current.clear();
                }
                _ => current.push(ch),
            }
        }
    }
    if quote.is_some() || escaped {
        return Err("unterminated quoted or escaped batch command".into());
    }
    if current.trim().is_empty() {
        if parts.is_empty() {
            return Err("batch cannot be empty".into());
        }
        if source.trim_end().ends_with(';') {
            return Err("trailing semicolon in batch".into());
        }
    } else {
        parts.push(current.trim().to_owned());
    }
    if parts.len() > 100 {
        return Err("batch limited to 100 commands".into());
    }
    Ok(parts)
}

/// Building a whole batch is side-effect free. One invalid command prevents
/// any queue operation. The caller supplies a fresh stable identity generator
/// for CREATE commands. The outbox allocates each separate durable command ID.
pub fn parse_batch(
    source: &str,
    mut next_id: impl FnMut() -> String,
) -> Result<Vec<Intent>, String> {
    let lines = split_batch(source)?;
    let mut intents = Vec::new();
    let mut new_entity_ids = std::collections::HashSet::new();
    for (index, line) in lines.iter().enumerate() {
        let generated = next_id();
        let intent = parse_intent(line, &generated)
            .map_err(|e| format!("batch command {}: {e}", index + 1))?;
        if intent.action["type"] == "create"
            || intent.action["type"] == "create_habit"
            || intent.kind == CommandKind::Project
        {
            let identity = [
                "project_id",
                "task_id",
                "block_id",
                "routine_id",
                "milestone_id",
                "habit_id",
            ]
            .iter()
            .find_map(|key| intent.action.get(key).and_then(Value::as_str));
            if let Some(id) = identity {
                let domain = match intent.kind {
                    CommandKind::Project => "project",
                    CommandKind::Evidence => "habit",
                    CommandKind::Task => "task",
                    CommandKind::Schedule => "schedule",
                    CommandKind::Routine => "routine",
                    CommandKind::Milestone => "milestone",
                    _ => "other",
                };
                if !new_entity_ids.insert((domain.to_owned(), id.to_owned())) {
                    return Err(format!(
                        "batch command {} creates the same {domain} identity twice: {id}",
                        index + 1
                    ));
                }
            }
        }
        intents.push(intent);
    }
    Ok(intents)
}

/// Produce an explicit workflow preview. No results are described as committed
/// until the server sends authoritative receipts and the replica refreshes.
pub fn batch_preview(intents: &[Intent]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} command(s), executed in the listed order",
        intents.len()
    ));
    lines.push("All commands are queued pending acceptance. No atomic rollback.".into());
    lines.push(String::new());
    for (index, intent) in intents.iter().enumerate() {
        lines.push(format!("{}. {}", index + 1, intent.description));
        lines.push(format!("   {}", intent.action));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload(line: &str) -> Value {
        parse_intent(line, "new_id").unwrap().action
    }
    #[test]
    fn tokenize_quoted_arguments() {
        assert_eq!(
            tokenize("task.create 'write weekly review' \"for work\"").unwrap(),
            vec!["task.create", "write weekly review", "for work"]
        );
        assert_eq!(
            tokenize("task.create \"A \\\"quoted\\\" value\"").unwrap()[1],
            "A \"quoted\" value"
        );
        assert!(tokenize("task.create 'bad").is_err());
        assert!(tokenize("task.create trailing\\").is_err());
    }
    #[test]
    fn task_intents_are_canonical() {
        assert_eq!(
            payload("task.create First task"),
            json!({"type":"create","task_id":"new_id","title":"First task"})
        );
        assert_eq!(
            payload("task.rename t1 New title"),
            json!({"type":"rename","task_id":"t1","title":"New title"})
        );
        assert_eq!(payload("task.project t1 -")["project_id"], Value::Null);
        assert_eq!(payload("task.milestone t1 m1")["milestone_id"], "m1");
        assert_eq!(
            payload("task.plan t1 30 - 1750000000000")["estimated_minutes"],
            30
        );
        assert_eq!(
            payload("task.plan t1 - - -")["estimated_minutes"],
            Value::Null
        );
        assert!(parse_intent("task.status t1 invented", "id").is_err());
        assert!(parse_intent("task.plan t1 0 - -", "id").is_err());
        assert!(parse_intent("task.plan t1 5 99 10", "id").is_err());
    }
    #[test]
    fn project_and_evidence_lifecycle() {
        assert_eq!(
            payload("project.create p1 First project"),
            json!({"project_id":"p1","name":"First project"})
        );
        assert_eq!(
            payload("habit.create h1 daily Morning check")["type"],
            "create_habit"
        );
        assert_eq!(
            payload("habit.create h2 weekly:3 Training")["cadence"],
            "weekly:3"
        );
        assert_eq!(
            payload("habit.record occurrence1 h1 1700000000000")["occurred_at_ms"],
            1700000000000_i64
        );
        assert_eq!(
            payload("metric.target speed 10 20 increasing")["type"],
            "set_metric_target"
        );
        assert_eq!(payload("metric.record speed 1700000000000 18")["value"], 18);
        assert!(parse_intent("habit.create h1 weekly:8 Invalid", "id").is_err());
        assert!(parse_intent("habit.record o1 h1 -1", "id").is_err());
        assert!(parse_intent("metric.target speed 20 10 increasing", "id").is_err());
        assert!(parse_intent("metric.record speed -1 2", "id").is_err());
        assert!(parse_batch("project.create p One; project.create p Two", || "id".into()).is_err());
        assert!(
            parse_batch("habit.create h daily A; habit.create h daily B", || "id"
                .into())
            .is_err()
        );
    }
    #[test]
    fn schedule_lifecycle() {
        assert_eq!(
            payload("schedule.create slot1 1000 30 movable task t1 Morning focus")["type"],
            "create"
        );
        assert_eq!(payload("schedule.move slot1 2000")["starts_at_ms"], 2000);
        assert_eq!(payload("schedule.resize slot1 45")["duration_minutes"], 45);
        assert_eq!(
            payload("schedule.rename slot1 Other focus")["title"],
            "Other focus"
        );
        assert_eq!(
            payload("schedule.target slot1 standalone -")["target_id"],
            Value::Null
        );
        assert_eq!(payload("schedule.delete slot1")["type"], "delete");
        assert!(parse_intent("schedule.create slot1 0 30 movable task - invalid", "id").is_err());
        assert!(parse_intent("schedule.move slot1 tomorrow", "id").is_err());
        assert!(parse_intent("schedule.resize slot1 0", "id").is_err());
    }
    #[test]
    fn routines_and_milestones() {
        assert_eq!(
            payload("routine.create morning h1,h2 Morning routine")["habit_ids"],
            json!(["h1", "h2"])
        );
        assert_eq!(payload("routine.habits morning -")["habit_ids"], json!([]));
        assert_eq!(payload("routine.delete morning")["type"], "delete");
        assert_eq!(
            payload("milestone.create ship p1 Ship the release")["project_id"],
            "p1"
        );
        assert_eq!(
            payload("milestone.status ship completed")["status"],
            "completed"
        );
        assert_eq!(payload("milestone.project ship p2")["project_id"], "p2");
        assert!(parse_intent("routine.habits morning h1,h1", "id").is_err());
        assert!(parse_intent("milestone.status ship invented", "id").is_err());
    }
    #[test]
    fn strategies_support_criteria_and_signals() {
        let s = payload(
            "strategy.configure p1 standard tasks,milestones 'Ship product' launch:true:Released qa:false:Reviewed",
        );
        assert_eq!(s["criteria"].as_array().unwrap().len(), 2);
        assert_eq!(s["criteria"][0]["required"], true);
        assert_eq!(s["progress_signals"], json!(["tasks", "milestones"]));
        assert_eq!(payload("strategy.objective p1 -")["objective"], Value::Null);
        assert_eq!(
            payload("strategy.criterion p1 qa satisfied")["status"],
            "satisfied"
        );
        assert!(parse_intent("strategy.signals p1 bogus", "id").is_err());
        assert!(parse_intent("strategy.signals p1 tasks,tasks", "id").is_err());
        assert!(parse_intent("strategy.configure p1 wrong tasks -", "id").is_err());
    }
    #[test]
    fn batch_parsing_is_side_effect_free_and_ordered() {
        let inputs = "routine.create morning - 'Wake; read'; milestone.create ship p1 Ship release";
        let commands = parse_batch(inputs, || "generated".into()).unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].action["name"], "Wake; read");
        assert_eq!(commands[1].action["type"], "create");
        assert_eq!(batch_preview(&commands).len(), 7);
        assert!(parse_batch("task.create Good; schedule.resize bad 0", || "id".into()).is_err());
        assert!(split_batch("task.create Good;").is_err());
        assert!(split_batch("task.create 'unterminated").is_err());
    }
    #[test]
    fn batch_rejects_duplicate_creation_ids() {
        assert!(
            parse_batch(
                "routine.create x - first; routine.create x - second",
                || "id".into()
            )
            .is_err()
        );
        assert!(
            parse_batch("milestone.create m p A; milestone.create m p B", || "id"
                .into())
            .is_err()
        );
        assert!(
            parse_batch("routine.create x - one; milestone.create x p two", || "id"
                .into())
            .is_ok()
        );
    }
    #[test]
    fn physical_vault_commands_use_canonical_wire_types() {
        let create = payload("note.create n1 docs/plan.md 'Hello world'");
        assert_eq!(
            create,
            json!({"kind":"Create","note_id":"n1","path":"docs/plan.md","body":"Hello world"})
        );
        let move_note = payload("note.move n1 docs/plan.md archive/plan.md old");
        assert_eq!(move_note["kind"], "Move");
        assert_eq!(payload("note.update n1 old new")["kind"], "Update");
        assert_eq!(
            payload("folder.create docs/plans"),
            json!({"path":"docs/plans"})
        );
        assert_eq!(
            payload("folder.move docs/plans archive/plans"),
            json!({"from":"docs/plans","to":"archive/plans"})
        );
    }
    #[test]
    fn physical_vault_commands_reject_traversal_and_invalid_moves() {
        for input in [
            "note.create n1 ../secret.md body",
            "note.create n1 /absolute.md body",
            "note.create n1 .hidden.md body",
            "note.create n1 docs\\secret.md body",
            "note.create n1 docs/note.txt body",
            "note.create n1 valid.md ''",
            "note.move n1 a.md a.md expected",
            "folder.create ../outside",
            "folder.create .git",
            "folder.move a a/b",
            "folder.move a a",
        ] {
            assert!(parse_intent(input, "cmd").is_err(), "{input}");
        }
    }
    #[test]
    fn physical_vault_batches_preserve_order_without_side_effects() {
        let commands = parse_batch(
            "folder.create plans; note.create n1 plans/a.md 'Plan A'; folder.move plans archive",
            || "unique".into(),
        )
        .unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].kind, CommandKind::FolderCreate);
        assert_eq!(commands[1].kind, CommandKind::Markdown);
        assert_eq!(commands[2].kind, CommandKind::FolderMove);
        assert!(
            parse_batch(
                "folder.create plans; note.create n2 ../escape.md bad",
                || "unique".into()
            )
            .is_err()
        );
    }
}
