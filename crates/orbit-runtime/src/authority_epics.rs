//! Project epics: substantial bodies of related work inside one project.
//!
//! An epic groups tasks and supported workflow work items. It is deliberately
//! *not* a milestone: milestones are checkpoints, and a task may belong to an
//! epic and contribute to a milestone independently. Commands share the same
//! revision-fenced, replayable authority ledger as tasks and milestones, so a
//! rejected assignment never leaves a partial hierarchy behind.
use crate::command_engine::{CommandAdmission, PendingCommand};
use crate::store::{AuthorityError, AuthorityStore};
use orbit_domain::MilestoneStatus;
use orbit_sync::{ChangeEnvelope, EntityKind, EntityRef, FieldMutation, SemanticChange};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EpicCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: EpicAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EpicAction {
    Create {
        epic_id: String,
        project_id: String,
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        priority: Option<String>,
        #[serde(default)]
        owner: Option<String>,
        #[serde(default)]
        start_at_ms: Option<i64>,
        #[serde(default)]
        target_at_ms: Option<i64>,
    },
    /// Replace the editable fields. Status has its own action so the edges stay
    /// server-enforced in one place.
    Update {
        epic_id: String,
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        priority: Option<String>,
        #[serde(default)]
        owner: Option<String>,
        #[serde(default)]
        start_at_ms: Option<i64>,
        #[serde(default)]
        target_at_ms: Option<i64>,
    },
    ChangeStatus {
        epic_id: String,
        status: String,
    },
    /// Move a task into this epic. A task belongs to at most one epic and both
    /// sides must share a project.
    AssignTask {
        epic_id: String,
        task_id: String,
    },
    UnassignTask {
        task_id: String,
    },
    /// Move a workflow-defined work item into this epic.
    AssignWorkItem {
        epic_id: String,
        item_id: String,
    },
    UnassignWorkItem {
        item_id: String,
    },
    /// Only an empty epic can be deleted. Completing or cancelling is the safe
    /// way to retire one that still owns work.
    Delete {
        epic_id: String,
    },
}

impl EpicAction {
    pub fn epic_id(&self) -> Option<&str> {
        match self {
            Self::Create { epic_id, .. }
            | Self::Update { epic_id, .. }
            | Self::ChangeStatus { epic_id, .. }
            | Self::AssignTask { epic_id, .. }
            | Self::AssignWorkItem { epic_id, .. }
            | Self::Delete { epic_id } => Some(epic_id),
            Self::UnassignTask { .. } | Self::UnassignWorkItem { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EpicProgress {
    /// Non-cancelled child work.
    pub total: usize,
    pub completed: usize,
    pub blocked: usize,
    /// 0–100, integer. An empty epic is 0% unless it was marked completed.
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpicActivityRecord {
    pub id: String,
    pub epic_id: String,
    pub kind: String,
    pub actor: String,
    pub occurred_at_ms: i64,
    pub summary: String,
}

struct EpicRow {
    id: String,
    project_id: String,
    title: String,
    description: String,
    status: String,
    priority: Option<String>,
    owner: Option<String>,
    start_at_ms: Option<i64>,
    target_at_ms: Option<i64>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpicRecord {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: Option<String>,
    pub owner: Option<String>,
    pub start_at_ms: Option<i64>,
    pub target_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub task_ids: Vec<String>,
    pub work_item_ids: Vec<String>,
    pub progress: EpicProgress,
    pub activities: Vec<EpicActivityRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpicSnapshot {
    pub revision: i64,
    pub epics: Vec<EpicRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpicResult {
    pub revision: i64,
    pub duplicate: bool,
    /// `None` for a delete.
    pub epic: Option<EpicRecord>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max
}

fn optional_text(value: Option<&String>, max: usize) -> bool {
    value.is_none_or(|text| text.len() <= max)
}

fn valid_date(value: Option<i64>) -> bool {
    value.is_none_or(|at| at >= 0)
}

fn valid_window(start: Option<i64>, target: Option<i64>) -> bool {
    valid_date(start)
        && valid_date(target)
        && !matches!((start, target), (Some(s), Some(t)) if s > t)
}

fn valid_priority(priority: Option<&String>) -> bool {
    priority.is_none_or(|value| matches!(value.as_str(), "low" | "medium" | "high" | "urgent"))
}

fn parse_status(value: &str) -> Option<MilestoneStatus> {
    match value {
        "planned" => Some(MilestoneStatus::Planned),
        "in_progress" => Some(MilestoneStatus::InProgress),
        "blocked" => Some(MilestoneStatus::Blocked),
        "completed" => Some(MilestoneStatus::Completed),
        "cancelled" => Some(MilestoneStatus::Cancelled),
        _ => None,
    }
}

fn now_ms() -> Result<i64, AuthorityError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AuthorityError::Invalid("clock"))?
        .as_millis()
        .try_into()
        .map_err(|_| AuthorityError::Invalid("clock overflow"))
}

fn ensure_project(conn: &Connection, id: &str) -> Result<(), AuthorityError> {
    if !valid_id(id) {
        return Err(AuthorityError::Invalid("invalid project ID"));
    }
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
        [id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(AuthorityError::Invalid("epic project does not exist"));
    }
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, AuthorityError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?)
}

fn record(conn: &Connection, id: &str) -> Result<Option<EpicRecord>, AuthorityError> {
    let base: Option<EpicRow> = conn
        .query_row(
            "SELECT id,project_id,title,description,status,priority,owner,start_at_ms,target_at_ms,created_at_ms,updated_at_ms FROM epics WHERE id=?1",
            [id],
            |row| {
                Ok(EpicRow {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    status: row.get(4)?,
                    priority: row.get(5)?,
                    owner: row.get(6)?,
                    start_at_ms: row.get(7)?,
                    target_at_ms: row.get(8)?,
                    created_at_ms: row.get(9)?,
                    updated_at_ms: row.get(10)?,
                })
            },
        )
        .optional()?;
    let Some(EpicRow {
        id,
        project_id,
        title,
        description,
        status,
        priority,
        owner,
        start_at_ms,
        target_at_ms,
        created_at_ms,
        updated_at_ms,
    }) = base
    else {
        return Ok(None);
    };
    let mut task_ids = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT e.task_id FROM epic_tasks e JOIN tasks t ON t.id=e.task_id WHERE e.epic_id=?1 ORDER BY e.task_id",
        )?;
        for row in stmt.query_map([&id], |row| row.get::<_, String>(0))? {
            task_ids.push(row?);
        }
    }
    let mut work_item_ids = Vec::new();
    if table_exists(conn, "orbit_blueprint_work_items")? {
        let mut stmt = conn.prepare(
            "SELECT e.item_id FROM epic_work_items e JOIN orbit_blueprint_work_items w ON w.id=e.item_id WHERE e.epic_id=?1 ORDER BY e.item_id",
        )?;
        for row in stmt.query_map([&id], |row| row.get::<_, String>(0))? {
            work_item_ids.push(row?);
        }
    }
    let progress = progress(conn, &id, &status)?;
    let mut activities = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id,epic_id,kind,actor,occurred_at_ms,summary FROM epic_activities WHERE epic_id=?1 ORDER BY occurred_at_ms DESC, id DESC LIMIT 100",
        )?;
        for row in stmt.query_map([&id], |row| {
            Ok(EpicActivityRecord {
                id: row.get(0)?,
                epic_id: row.get(1)?,
                kind: row.get(2)?,
                actor: row.get(3)?,
                occurred_at_ms: row.get(4)?,
                summary: row.get(5)?,
            })
        })? {
            activities.push(row?);
        }
    }
    Ok(Some(EpicRecord {
        id,
        project_id,
        title,
        description,
        status,
        priority,
        owner,
        start_at_ms,
        target_at_ms,
        created_at_ms,
        updated_at_ms,
        task_ids,
        work_item_ids,
        progress,
        activities,
    }))
}

fn required(conn: &Connection, id: &str) -> Result<EpicRecord, AuthorityError> {
    record(conn, id)?.ok_or(AuthorityError::Invalid("epic does not exist"))
}

fn progress(
    conn: &Connection,
    epic_id: &str,
    status: &str,
) -> Result<EpicProgress, AuthorityError> {
    let mut total = 0usize;
    let mut completed = 0usize;
    let mut blocked = 0usize;
    {
        let mut stmt = conn.prepare(
            "SELECT t.status FROM tasks t JOIN epic_tasks e ON e.task_id=t.id WHERE e.epic_id=?1",
        )?;
        for row in stmt.query_map([epic_id], |row| row.get::<_, String>(0))? {
            let task_status = row?;
            if task_status == "cancelled" {
                continue;
            }
            total += 1;
            match task_status.as_str() {
                "done" => completed += 1,
                "blocked" => blocked += 1,
                _ => {}
            }
        }
    }
    if table_exists(conn, "orbit_blueprint_work_items")? {
        let mut stmt = conn.prepare(
            "SELECT w.state FROM orbit_blueprint_work_items w JOIN epic_work_items e ON e.item_id=w.id WHERE e.epic_id=?1",
        )?;
        for row in stmt.query_map([epic_id], |row| row.get::<_, String>(0))? {
            let state = row?;
            total += 1;
            if matches!(state.as_str(), "done" | "completed" | "closed") {
                completed += 1;
            }
        }
    }
    let percent = if total == 0 {
        if status == "completed" { 100 } else { 0 }
    } else {
        u8::try_from(
            completed
                .saturating_mul(100)
                .checked_div(total)
                .unwrap_or(100),
        )
        .unwrap_or(100)
    };
    Ok(EpicProgress {
        total,
        completed,
        blocked,
        percent,
    })
}

fn field_version(
    conn: &Connection,
    id: &str,
    field: &str,
) -> Result<Option<orbit_sync::ChangeId>, AuthorityError> {
    let mut stmt = conn
        .prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC")?;
    let wires = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for wire in wires {
        let envelope = ChangeEnvelope::from_wire_json(&wire?)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == EntityKind::Epic
            && envelope.change.entity.id == id
            && envelope.change.mutations.iter().any(|m| m.field == field)
        {
            return Ok(Some(envelope.id));
        }
    }
    Ok(None)
}

fn mutation(
    conn: &Connection,
    id: &str,
    key: &str,
    value: Option<String>,
) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(key, value, field_version(conn, id, key)?)
        .map_err(|_| AuthorityError::Invalid("invalid epic semantic mutation"))
}

fn activity(
    conn: &Connection,
    command_id: &str,
    epic_id: &str,
    actor: &str,
    kind: &str,
    summary: &str,
) -> Result<(), AuthorityError> {
    conn.execute(
        "INSERT INTO epic_activities(id,epic_id,kind,actor,occurred_at_ms,summary) VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            format!("epic-activity-{command_id}"),
            epic_id,
            kind,
            actor,
            now_ms()?,
            summary
        ],
    )?;
    Ok(())
}

/// Additive schema: existing databases gain epics without touching their
/// projects, tasks or milestones.
pub(crate) fn ensure_epic_schema(conn: &Connection) -> Result<(), AuthorityError> {
    conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS epics (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                priority TEXT,
                owner TEXT,
                start_at_ms INTEGER,
                target_at_ms INTEGER,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_epics_project ON epics(project_id);
            CREATE TABLE IF NOT EXISTS epic_tasks (
                task_id TEXT PRIMARY KEY NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                epic_id TEXT NOT NULL REFERENCES epics(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_epic_tasks_epic ON epic_tasks(epic_id);
            CREATE TABLE IF NOT EXISTS epic_work_items (
                item_id TEXT PRIMARY KEY NOT NULL,
                epic_id TEXT NOT NULL REFERENCES epics(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_epic_work_items_epic ON epic_work_items(epic_id);
            CREATE TABLE IF NOT EXISTS epic_activities (
                id TEXT PRIMARY KEY NOT NULL,
                epic_id TEXT NOT NULL REFERENCES epics(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                actor TEXT NOT NULL,
                occurred_at_ms INTEGER NOT NULL,
                summary TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_epic_activities_epic ON epic_activities(epic_id, occurred_at_ms);",
    )?;
    Ok(())
}

impl AuthorityStore {
    pub fn authority_epics(&self) -> Result<EpicSnapshot, AuthorityError> {
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |row| row.get(0),
        )?;
        let ids = {
            let mut stmt =
                tx.prepare("SELECT id FROM epics ORDER BY project_id,created_at_ms,id")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut epics = Vec::with_capacity(ids.len());
        for id in ids {
            epics.push(required(&tx, &id)?);
        }
        tx.commit()?;
        Ok(EpicSnapshot { revision, epics })
    }

    pub fn authority_epic_command(
        &mut self,
        actor: &str,
        command: EpicCommand,
    ) -> Result<EpicResult, AuthorityError> {
        if actor.trim().is_empty()
            || !valid_id(&command.command_id)
            || command.expected_revision < 0
        {
            return Err(AuthorityError::Invalid("invalid epic command envelope"));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("epic serialization"))?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending = PendingCommand {
            command_id: &command.command_id,
            actor,
            payload: &payload,
            expected_revision: command.expected_revision,
        };
        let revision = match pending.check(&tx)? {
            CommandAdmission::Duplicate(raw) => {
                let mut receipt: EpicResult = serde_json::from_str(&raw)
                    .map_err(|_| AuthorityError::Invalid("invalid stored epic receipt"))?;
                receipt.duplicate = true;
                return Ok(receipt);
            }
            CommandAdmission::New(sequence) => sequence,
        };

        let subject: Option<String>;
        let mut mutations: Vec<FieldMutation> = Vec::new();
        let mut activity_kind = "updated";
        let mut activity_summary = String::new();

        match command.action {
            EpicAction::Create {
                epic_id,
                project_id,
                title,
                description,
                priority,
                owner,
                start_at_ms,
                target_at_ms,
            } => {
                if !valid_id(&epic_id)
                    || !valid_text(&title, 4096)
                    || description.len() > 65_536
                    || !valid_priority(priority.as_ref())
                    || !optional_text(owner.as_ref(), 256)
                    || !valid_window(start_at_ms, target_at_ms)
                {
                    return Err(AuthorityError::Invalid("invalid epic"));
                }
                ensure_project(&tx, &project_id)?;
                let now = now_ms()?;
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO epics(id,project_id,title,description,status,priority,owner,start_at_ms,target_at_ms,created_at_ms,updated_at_ms) \
                     VALUES(?1,?2,?3,?4,'planned',?5,?6,?7,?8,?9,?9)",
                    params![
                        epic_id,
                        project_id,
                        title,
                        description,
                        priority,
                        owner,
                        start_at_ms,
                        target_at_ms,
                        now
                    ],
                )?;
                if inserted != 1 {
                    return Err(AuthorityError::ImmutableConflict);
                }
                mutations = vec![
                    mutation(&tx, &epic_id, "project_id", Some(project_id))?,
                    mutation(&tx, &epic_id, "title", Some(title.clone()))?,
                    mutation(&tx, &epic_id, "description", Some(description.clone()))?,
                    mutation(&tx, &epic_id, "status", Some("planned".into()))?,
                    mutation(&tx, &epic_id, "priority", priority.clone())?,
                    mutation(&tx, &epic_id, "owner", owner.clone())?,
                    mutation(
                        &tx,
                        &epic_id,
                        "start_at_ms",
                        start_at_ms.map(|value| value.to_string()),
                    )?,
                    mutation(
                        &tx,
                        &epic_id,
                        "target_at_ms",
                        target_at_ms.map(|value| value.to_string()),
                    )?,
                ];
                activity_kind = "created";
                activity_summary = format!("Epic {title} created");
                subject = Some(epic_id);
            }
            EpicAction::Update {
                epic_id,
                title,
                description,
                priority,
                owner,
                start_at_ms,
                target_at_ms,
            } => {
                required(&tx, &epic_id)?;
                if !valid_text(&title, 4096)
                    || description.len() > 65_536
                    || !valid_priority(priority.as_ref())
                    || !optional_text(owner.as_ref(), 256)
                    || !valid_window(start_at_ms, target_at_ms)
                {
                    return Err(AuthorityError::Invalid("invalid epic"));
                }
                mutations = vec![
                    mutation(&tx, &epic_id, "title", Some(title.clone()))?,
                    mutation(&tx, &epic_id, "description", Some(description.clone()))?,
                    mutation(&tx, &epic_id, "priority", priority.clone())?,
                    mutation(&tx, &epic_id, "owner", owner.clone())?,
                    mutation(
                        &tx,
                        &epic_id,
                        "start_at_ms",
                        start_at_ms.map(|value| value.to_string()),
                    )?,
                    mutation(
                        &tx,
                        &epic_id,
                        "target_at_ms",
                        target_at_ms.map(|value| value.to_string()),
                    )?,
                ];
                tx.execute(
                    "UPDATE epics SET title=?2,description=?3,priority=?4,owner=?5,start_at_ms=?6,target_at_ms=?7,updated_at_ms=?8 WHERE id=?1",
                    params![epic_id, title, description, priority, owner, start_at_ms, target_at_ms, now_ms()?],
                )?;
                activity_summary = format!("Epic {title} updated");
                subject = Some(epic_id);
            }
            EpicAction::ChangeStatus { epic_id, status } => {
                let current = required(&tx, &epic_id)?;
                let from = parse_status(&current.status)
                    .ok_or(AuthorityError::Invalid("invalid stored epic status"))?;
                let to =
                    parse_status(&status).ok_or(AuthorityError::Invalid("invalid epic status"))?;
                if !from.can_transition_to(to) {
                    return Err(AuthorityError::Application(
                        orbit_application::ApplicationError::Domain(
                            orbit_domain::DomainError::InvalidMilestoneStatusTransition {
                                from: from.as_str(),
                                to: to.as_str(),
                            },
                        ),
                    ));
                }
                mutations = vec![mutation(&tx, &epic_id, "status", Some(to.as_str().into()))?];
                tx.execute(
                    "UPDATE epics SET status=?2,updated_at_ms=?3 WHERE id=?1",
                    params![epic_id, to.as_str(), now_ms()?],
                )?;
                activity_kind = "status_changed";
                activity_summary = format!("Epic status changed to {}", to.as_str());
                subject = Some(epic_id);
            }
            EpicAction::AssignTask { epic_id, task_id } => {
                let epic = required(&tx, &epic_id)?;
                let project: Option<Option<String>> = tx
                    .query_row(
                        "SELECT project_id FROM tasks WHERE id=?1",
                        [&task_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(task_project) = project else {
                    return Err(AuthorityError::Invalid("task does not exist"));
                };
                if task_project.as_deref() != Some(epic.project_id.as_str()) {
                    return Err(AuthorityError::Invalid(
                        "task and epic must be in the same project",
                    ));
                }
                tx.execute(
                    "INSERT INTO epic_tasks(task_id,epic_id) VALUES(?1,?2) \
                     ON CONFLICT(task_id) DO UPDATE SET epic_id=excluded.epic_id",
                    params![task_id, epic_id],
                )?;
                tx.execute(
                    "UPDATE epics SET updated_at_ms=?2 WHERE id=?1",
                    params![epic_id, now_ms()?],
                )?;
                activity_summary = format!("Task {task_id} assigned to this epic");
                subject = Some(epic_id);
            }
            EpicAction::UnassignTask { task_id } => {
                let epic_id: Option<String> = tx
                    .query_row(
                        "SELECT epic_id FROM epic_tasks WHERE task_id=?1",
                        [&task_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(epic_id) = epic_id else {
                    return Err(AuthorityError::Invalid("task is not in an epic"));
                };
                tx.execute("DELETE FROM epic_tasks WHERE task_id=?1", [&task_id])?;
                tx.execute(
                    "UPDATE epics SET updated_at_ms=?2 WHERE id=?1",
                    params![epic_id, now_ms()?],
                )?;
                activity_summary = format!("Task {task_id} removed from this epic");
                subject = Some(epic_id);
            }
            EpicAction::AssignWorkItem { epic_id, item_id } => {
                let epic = required(&tx, &epic_id)?;
                if !table_exists(&tx, "orbit_blueprint_work_items")? {
                    return Err(AuthorityError::Invalid("no workflow work items exist yet"));
                }
                let project: Option<String> = tx
                    .query_row(
                        "SELECT project_id FROM orbit_blueprint_work_items WHERE id=?1",
                        [&item_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(item_project) = project else {
                    return Err(AuthorityError::Invalid("work item does not exist"));
                };
                if item_project != epic.project_id {
                    return Err(AuthorityError::Invalid(
                        "work item and epic must be in the same project",
                    ));
                }
                tx.execute(
                    "INSERT INTO epic_work_items(item_id,epic_id) VALUES(?1,?2) \
                     ON CONFLICT(item_id) DO UPDATE SET epic_id=excluded.epic_id",
                    params![item_id, epic_id],
                )?;
                tx.execute(
                    "UPDATE epics SET updated_at_ms=?2 WHERE id=?1",
                    params![epic_id, now_ms()?],
                )?;
                activity_summary = format!("Work item {item_id} assigned to this epic");
                subject = Some(epic_id);
            }
            EpicAction::UnassignWorkItem { item_id } => {
                let epic_id: Option<String> = tx
                    .query_row(
                        "SELECT epic_id FROM epic_work_items WHERE item_id=?1",
                        [&item_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(epic_id) = epic_id else {
                    return Err(AuthorityError::Invalid("work item is not in an epic"));
                };
                tx.execute("DELETE FROM epic_work_items WHERE item_id=?1", [&item_id])?;
                tx.execute(
                    "UPDATE epics SET updated_at_ms=?2 WHERE id=?1",
                    params![epic_id, now_ms()?],
                )?;
                activity_summary = format!("Work item {item_id} removed from this epic");
                subject = Some(epic_id);
            }
            EpicAction::Delete { epic_id } => {
                let epic = required(&tx, &epic_id)?;
                if !epic.task_ids.is_empty() || !epic.work_item_ids.is_empty() {
                    return Err(AuthorityError::Invalid(
                        "epic still has work; move it out before deleting",
                    ));
                }
                tx.execute("DELETE FROM epics WHERE id=?1", [&epic_id])?;
                mutations = vec![mutation(&tx, &epic_id, "deleted", Some("true".into()))?];
                subject = Some(epic_id);
            }
        }

        let id = subject.ok_or(AuthorityError::Invalid("epic command has no subject"))?;
        if !activity_summary.is_empty() {
            activity(
                &tx,
                &command.command_id,
                &id,
                actor,
                activity_kind,
                &activity_summary,
            )?;
        }
        let semantic = SemanticChange::new(
            EntityRef::new(EntityKind::Epic, id.clone())
                .map_err(|_| AuthorityError::Invalid("invalid epic ref"))?,
            mutations,
        )
        .map_err(|_| AuthorityError::Invalid("invalid epic mutation"))?;
        let record = record(&tx, &id)?;
        let result = EpicResult {
            revision,
            duplicate: false,
            epic: record,
        };
        let result_json = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("command result"))?;
        pending.commit(&tx, revision, semantic, &result_json)?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_commands::{ProjectCommand, TaskAction, TaskCommand};
    use std::path::Path;

    fn store() -> AuthorityStore {
        AuthorityStore::open(Path::new(":memory:")).unwrap()
    }
    fn project(db: &mut AuthorityStore, id: &str, rev: i64) {
        db.authority_project_command(
            "user",
            ProjectCommand {
                workflow_id: None,
                command_id: format!("project-{id}"),
                expected_revision: rev,
                project_id: id.into(),
                name: format!("project {id}"),
            },
        )
        .unwrap();
    }
    fn task(db: &mut AuthorityStore, id: &str, project: &str, rev: i64) {
        db.authority_task_command(
            "user",
            TaskCommand {
                command_id: format!("task-{id}"),
                expected_revision: rev,
                action: TaskAction::Create {
                    task_id: id.into(),
                    title: format!("task {id}"),
                    project_id: Some(project.into()),
                },
            },
        )
        .unwrap();
    }
    fn create(rev: i64) -> EpicCommand {
        EpicCommand {
            command_id: "create-epic".into(),
            expected_revision: rev,
            action: EpicAction::Create {
                epic_id: "epic-one".into(),
                project_id: "one".into(),
                title: "Web UX".into(),
                description: String::new(),
                priority: Some("high".into()),
                owner: None,
                start_at_ms: None,
                target_at_ms: None,
            },
        }
    }
    fn assign(rev: i64, task_id: &str) -> EpicCommand {
        EpicCommand {
            command_id: format!("assign-{task_id}"),
            expected_revision: rev,
            action: EpicAction::AssignTask {
                epic_id: "epic-one".into(),
                task_id: task_id.into(),
            },
        }
    }

    #[test]
    fn epics_group_tasks_and_report_progress_until_completion() {
        let mut db = store();
        project(&mut db, "one", 0);
        task(&mut db, "t1", "one", 1);
        task(&mut db, "t2", "one", 2);
        let created = db.authority_epic_command("user", create(3)).unwrap();
        assert_eq!(created.revision, 4);
        assert_eq!(created.epic.as_ref().unwrap().status, "planned");

        db.authority_epic_command("user", assign(4, "t1")).unwrap();
        let snapshot = db.authority_epics().unwrap();
        let epic = &snapshot.epics[0];
        assert_eq!(epic.task_ids, vec!["t1"]);
        assert_eq!(epic.progress.total, 1);
        assert_eq!(epic.progress.completed, 0);
        assert_eq!(epic.progress.percent, 0);
        assert!(!epic.activities.is_empty());
    }

    #[test]
    fn cross_project_assignment_is_refused_without_partial_state() {
        let mut db = store();
        project(&mut db, "one", 0);
        project(&mut db, "two", 1);
        task(&mut db, "t-other", "two", 2);
        db.authority_epic_command("user", create(3)).unwrap();
        let error = db
            .authority_epic_command("user", assign(4, "t-other"))
            .unwrap_err();
        assert!(matches!(error, AuthorityError::Invalid(_)));
        assert!(db.authority_epics().unwrap().epics[0].task_ids.is_empty());
    }

    #[test]
    fn a_non_empty_epic_cannot_be_deleted() {
        let mut db = store();
        project(&mut db, "one", 0);
        task(&mut db, "t1", "one", 1);
        db.authority_epic_command("user", create(2)).unwrap();
        db.authority_epic_command("user", assign(3, "t1")).unwrap();
        let error = db
            .authority_epic_command(
                "user",
                EpicCommand {
                    command_id: "delete".into(),
                    expected_revision: 4,
                    action: EpicAction::Delete {
                        epic_id: "epic-one".into(),
                    },
                },
            )
            .unwrap_err();
        assert!(matches!(error, AuthorityError::Invalid(_)));
        assert_eq!(db.authority_epics().unwrap().epics.len(), 1);
    }
}
