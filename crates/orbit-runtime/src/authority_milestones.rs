//! Project milestone commands executed inside the canonical authority ledger.
//! A milestone is always attached to an existing project; status transitions
//! and moves are revision-fenced, replayable and acknowledged only on commit.
//! Milestones group tasks and workflow work items, and expose live progress.
use crate::command_engine::{CommandAdmission, PendingCommand};
use crate::store::{AuthorityError, AuthorityStore, assign_task_milestone};
use orbit_domain::MilestoneStatus;
use orbit_sync::{ChangeEnvelope, EntityKind, EntityRef, FieldMutation, SemanticChange};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MilestoneCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: MilestoneAction,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MilestoneAction {
    Create {
        milestone_id: String,
        project_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_at_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_at_ms: Option<i64>,
    },
    Rename {
        milestone_id: String,
        name: String,
    },
    /// Replace the editable fields. Status has its own action so the edges stay
    /// server-enforced in one place.
    Update {
        milestone_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        priority: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_at_ms: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_at_ms: Option<i64>,
    },
    ChangeStatus {
        milestone_id: String,
        status: String,
    },
    /// Set or clear the checkpoint date. It is a marker, not a duration.
    SetTarget {
        milestone_id: String,
        target_at_ms: Option<i64>,
    },
    MoveProject {
        milestone_id: String,
        project_id: String,
    },
    /// Move a task into this milestone. A task belongs to at most one milestone
    /// and both sides must share a project.
    AssignTask {
        milestone_id: String,
        task_id: String,
    },
    UnassignTask {
        task_id: String,
    },
    /// Move a workflow-defined work item into this milestone.
    AssignWorkItem {
        milestone_id: String,
        item_id: String,
    },
    UnassignWorkItem {
        item_id: String,
    },
    /// Only an empty milestone can be deleted or moved. Completing or cancelling
    /// is the safe way to retire one that still owns work.
    Delete {
        milestone_id: String,
    },
}
impl MilestoneAction {
    pub fn milestone_id(&self) -> Option<&str> {
        match self {
            Self::Create { milestone_id, .. }
            | Self::Rename { milestone_id, .. }
            | Self::Update { milestone_id, .. }
            | Self::ChangeStatus { milestone_id, .. }
            | Self::SetTarget { milestone_id, .. }
            | Self::MoveProject { milestone_id, .. }
            | Self::AssignTask { milestone_id, .. }
            | Self::AssignWorkItem { milestone_id, .. }
            | Self::Delete { milestone_id } => Some(milestone_id),
            Self::UnassignTask { .. } | Self::UnassignWorkItem { .. } => None,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneProgress {
    /// Non-cancelled child work.
    pub total: usize,
    pub completed: usize,
    pub blocked: usize,
    /// 0–100, integer; an empty milestone is always 0%.
    pub percent: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneActivityRecord {
    pub id: String,
    pub milestone_id: String,
    pub kind: String,
    pub actor: String,
    pub occurred_at_ms: i64,
    pub summary: String,
}
struct MilestoneRow {
    id: String,
    project_id: String,
    name: String,
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
pub struct MilestoneRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub status: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub start_at_ms: Option<i64>,
    /// Checkpoint date, when one was scheduled. Dateless milestones remain valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_at_ms: Option<i64>,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub task_ids: Vec<String>,
    #[serde(default)]
    pub work_item_ids: Vec<String>,
    #[serde(default)]
    pub progress: MilestoneProgress,
    #[serde(default)]
    pub activities: Vec<MilestoneActivityRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneResult {
    pub revision: i64,
    pub duplicate: bool,
    pub milestone: Option<MilestoneRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneSnapshot {
    pub revision: i64,
    pub milestones: Vec<MilestoneRecord>,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 4096
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
fn status_allowed(status: &str) -> bool {
    parse_status(status).is_some()
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
        |r| r.get(0),
    )?;
    if !exists {
        return Err(AuthorityError::Invalid("milestone project does not exist"));
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
/// Live work-item memberships: rows that still join an existing blueprint work
/// item, exactly the join `record` and `progress` expose. An orphan membership
/// must not block a move or delete while staying invisible to every read.
fn attached_work_items(conn: &Connection, milestone_id: &str) -> Result<i64, AuthorityError> {
    if !table_exists(conn, "orbit_blueprint_work_items")? {
        return Ok(0);
    }
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM milestone_work_items w JOIN orbit_blueprint_work_items i ON i.id=w.item_id WHERE w.milestone_id=?1",
        [milestone_id],
        |r| r.get(0),
    )?)
}
/// Current live membership set, sorted like every other semantic edge set.
fn work_item_ids(conn: &Connection, milestone_id: &str) -> Result<Vec<String>, AuthorityError> {
    let mut ids = Vec::new();
    if !table_exists(conn, "orbit_blueprint_work_items")? {
        return Ok(ids);
    }
    let mut stmt = conn.prepare(
        "SELECT w.item_id FROM milestone_work_items w JOIN orbit_blueprint_work_items i ON i.id=w.item_id WHERE w.milestone_id=?1 ORDER BY w.item_id",
    )?;
    for row in stmt.query_map([milestone_id], |row| row.get::<_, String>(0))? {
        ids.push(row?);
    }
    Ok(ids)
}
fn record(conn: &Connection, id: &str) -> Result<Option<MilestoneRecord>, AuthorityError> {
    let base: Option<MilestoneRow> = conn
        .query_row(
            "SELECT id,project_id,name,description,status,priority,owner,start_at_ms,target_at_ms,created_at_ms,updated_at_ms FROM milestones WHERE id=?1",
            [id],
            |row| {
                Ok(MilestoneRow {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
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
    let Some(MilestoneRow {
        id,
        project_id,
        name,
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
        let mut stmt = conn.prepare("SELECT id FROM tasks WHERE milestone_id=?1 ORDER BY id")?;
        for row in stmt.query_map([&id], |row| row.get::<_, String>(0))? {
            task_ids.push(row?);
        }
    }
    let mut work_item_ids = Vec::new();
    if table_exists(conn, "orbit_blueprint_work_items")? {
        let mut stmt = conn.prepare(
            "SELECT w.item_id FROM milestone_work_items w JOIN orbit_blueprint_work_items i ON i.id=w.item_id WHERE w.milestone_id=?1 ORDER BY w.item_id",
        )?;
        for row in stmt.query_map([&id], |row| row.get::<_, String>(0))? {
            work_item_ids.push(row?);
        }
    }
    let progress = progress(conn, &id)?;
    let mut activities = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id,subject_id,activity_kind,COALESCE(origin_value,''),occurred_at_ms,summary FROM activities WHERE subject_kind='milestone' AND subject_id=?1 ORDER BY occurred_at_ms DESC, id DESC LIMIT 100",
        )?;
        for row in stmt.query_map([&id], |row| {
            Ok(MilestoneActivityRecord {
                id: row.get(0)?,
                milestone_id: row.get(1)?,
                kind: row.get(2)?,
                actor: row.get(3)?,
                occurred_at_ms: row.get(4)?,
                summary: row.get(5)?,
            })
        })? {
            activities.push(row?);
        }
    }
    Ok(Some(MilestoneRecord {
        id,
        project_id,
        name,
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
fn required(conn: &Connection, id: &str) -> Result<MilestoneRecord, AuthorityError> {
    record(conn, id)?.ok_or(AuthorityError::Invalid("milestone does not exist"))
}
fn progress(conn: &Connection, milestone_id: &str) -> Result<MilestoneProgress, AuthorityError> {
    let mut total = 0usize;
    let mut completed = 0usize;
    let mut blocked = 0usize;
    {
        let mut stmt = conn.prepare("SELECT status FROM tasks WHERE milestone_id=?1")?;
        for row in stmt.query_map([milestone_id], |row| row.get::<_, String>(0))? {
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
            "SELECT i.state FROM orbit_blueprint_work_items i JOIN milestone_work_items w ON w.item_id=i.id WHERE w.milestone_id=?1",
        )?;
        for row in stmt.query_map([milestone_id], |row| row.get::<_, String>(0))? {
            let state = row?;
            total += 1;
            if matches!(state.as_str(), "done" | "completed" | "closed") {
                completed += 1;
            }
        }
    }
    let percent = if total == 0 {
        0
    } else {
        u8::try_from(
            completed
                .saturating_mul(100)
                .checked_div(total)
                .unwrap_or(0),
        )
        .unwrap_or(100)
    };
    Ok(MilestoneProgress {
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
    let wires = stmt.query_map([], |r| r.get::<_, String>(0))?;
    for wire in wires {
        let envelope = ChangeEnvelope::from_wire_json(&wire?)
            .map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == EntityKind::Milestone
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
        .map_err(|_| AuthorityError::Invalid("invalid milestone semantic mutation"))
}
fn activity(
    conn: &Connection,
    command_id: &str,
    milestone_id: &str,
    actor: &str,
    kind: &str,
    summary: &str,
) -> Result<(), AuthorityError> {
    conn.execute(
        "INSERT INTO activities(id,subject_kind,subject_id,activity_kind,activity_kind_value,origin_kind,origin_value,occurred_at_ms,summary) \
         VALUES(?1,'milestone',?2,?3,NULL,'external',?4,?5,?6)",
        params![
            format!("milestone-activity-{command_id}"),
            milestone_id,
            kind,
            actor,
            now_ms()?,
            summary
        ],
    )?;
    Ok(())
}
fn validate_record(conn: &Connection, m: &MilestoneRecord) -> Result<(), AuthorityError> {
    if !valid_id(&m.id)
        || !valid_name(&m.name)
        || m.description.len() > 65_536
        || !valid_priority(m.priority.as_ref())
        || !optional_text(m.owner.as_ref(), 256)
        || !status_allowed(&m.status)
        || !valid_window(m.start_at_ms, m.target_at_ms)
        || m.created_at_ms < 0
        || m.updated_at_ms < 0
    {
        return Err(AuthorityError::Invalid("invalid stored milestone"));
    }
    ensure_project(conn, &m.project_id)
}
impl AuthorityStore {
    pub fn authority_milestones(&self) -> Result<MilestoneSnapshot, AuthorityError> {
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let milestone_ids = {
            let mut stmt =
                tx.prepare("SELECT id FROM milestones ORDER BY project_id,created_at_ms,id")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut milestones = Vec::with_capacity(milestone_ids.len());
        for id in milestone_ids {
            milestones.push(required(&tx, &id)?);
        }
        for item in &milestones {
            validate_record(&tx, item)?;
        }
        tx.commit()?;
        Ok(MilestoneSnapshot {
            revision,
            milestones,
        })
    }
    pub fn authority_milestone_command(
        &mut self,
        actor: &str,
        command: MilestoneCommand,
    ) -> Result<MilestoneResult, AuthorityError> {
        if actor.trim().is_empty()
            || !valid_id(&command.command_id)
            || command.expected_revision < 0
            || command
                .action
                .milestone_id()
                .is_some_and(|id| !valid_id(id))
        {
            return Err(AuthorityError::Invalid(
                "invalid milestone command envelope",
            ));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("milestone serialization"))?;
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
                let mut receipt: MilestoneResult = serde_json::from_str(&raw)
                    .map_err(|_| AuthorityError::Invalid("invalid stored milestone receipt"))?;
                receipt.duplicate = true;
                return Ok(receipt);
            }
            CommandAdmission::New(n) => n,
        };

        let subject: Option<String>;
        let mut semantic_kind = EntityKind::Milestone;
        let mut semantic_id: Option<String> = None;
        let mutations: Vec<FieldMutation>;
        let mut activity_kind = "updated";
        let mut activity_summary = String::new();

        match command.action {
            MilestoneAction::Create {
                milestone_id,
                project_id,
                name,
                description,
                priority,
                owner,
                start_at_ms,
                target_at_ms,
            } => {
                if !valid_id(&milestone_id)
                    || !valid_name(&name)
                    || description.len() > 65_536
                    || !valid_priority(priority.as_ref())
                    || !optional_text(owner.as_ref(), 256)
                    || !valid_window(start_at_ms, target_at_ms)
                {
                    return Err(AuthorityError::Invalid("invalid milestone"));
                }
                ensure_project(&tx, &project_id)?;
                let now = now_ms()?;
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO milestones(id,project_id,name,description,status,priority,owner,start_at_ms,target_at_ms,created_at_ms,updated_at_ms) \
                     VALUES(?1,?2,?3,?4,'planned',?5,?6,?7,?8,?9,?9)",
                    params![
                        milestone_id,
                        project_id,
                        name,
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
                    mutation(&tx, &milestone_id, "project_id", Some(project_id))?,
                    mutation(&tx, &milestone_id, "name", Some(name.clone()))?,
                    mutation(&tx, &milestone_id, "description", Some(description.clone()))?,
                    mutation(&tx, &milestone_id, "status", Some("planned".into()))?,
                    mutation(&tx, &milestone_id, "priority", priority.clone())?,
                    mutation(&tx, &milestone_id, "owner", owner.clone())?,
                    mutation(
                        &tx,
                        &milestone_id,
                        "start_at_ms",
                        start_at_ms.map(|value| value.to_string()),
                    )?,
                    mutation(
                        &tx,
                        &milestone_id,
                        "target_at_ms",
                        target_at_ms.map(|value| value.to_string()),
                    )?,
                ];
                activity_kind = "created";
                activity_summary = format!("Milestone {name} created");
                subject = Some(milestone_id);
            }
            MilestoneAction::Rename { milestone_id, name } => {
                required(&tx, &milestone_id)?;
                if !valid_name(&name) {
                    return Err(AuthorityError::Invalid("invalid milestone name"));
                }
                mutations = vec![mutation(&tx, &milestone_id, "name", Some(name.clone()))?];
                tx.execute(
                    "UPDATE milestones SET name=?1,updated_at_ms=?2 WHERE id=?3",
                    params![name, now_ms()?, milestone_id],
                )?;
                activity_summary = format!("Milestone {name} updated");
                subject = Some(milestone_id);
            }
            MilestoneAction::Update {
                milestone_id,
                name,
                description,
                priority,
                owner,
                start_at_ms,
                target_at_ms,
            } => {
                required(&tx, &milestone_id)?;
                if !valid_name(&name)
                    || description.len() > 65_536
                    || !valid_priority(priority.as_ref())
                    || !optional_text(owner.as_ref(), 256)
                    || !valid_window(start_at_ms, target_at_ms)
                {
                    return Err(AuthorityError::Invalid("invalid milestone"));
                }
                mutations = vec![
                    mutation(&tx, &milestone_id, "name", Some(name.clone()))?,
                    mutation(&tx, &milestone_id, "description", Some(description.clone()))?,
                    mutation(&tx, &milestone_id, "priority", priority.clone())?,
                    mutation(&tx, &milestone_id, "owner", owner.clone())?,
                    mutation(
                        &tx,
                        &milestone_id,
                        "start_at_ms",
                        start_at_ms.map(|value| value.to_string()),
                    )?,
                    mutation(
                        &tx,
                        &milestone_id,
                        "target_at_ms",
                        target_at_ms.map(|value| value.to_string()),
                    )?,
                ];
                tx.execute(
                    "UPDATE milestones SET name=?2,description=?3,priority=?4,owner=?5,start_at_ms=?6,target_at_ms=?7,updated_at_ms=?8 WHERE id=?1",
                    params![
                        milestone_id,
                        name,
                        description,
                        priority,
                        owner,
                        start_at_ms,
                        target_at_ms,
                        now_ms()?
                    ],
                )?;
                activity_summary = format!("Milestone {name} updated");
                subject = Some(milestone_id);
            }
            MilestoneAction::ChangeStatus {
                milestone_id,
                status,
            } => {
                let current = required(&tx, &milestone_id)?;
                let from = parse_status(&current.status)
                    .ok_or(AuthorityError::Invalid("invalid stored milestone status"))?;
                let to = parse_status(&status)
                    .ok_or(AuthorityError::Invalid("invalid milestone status"))?;
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
                mutations = vec![mutation(
                    &tx,
                    &milestone_id,
                    "status",
                    Some(to.as_str().into()),
                )?];
                tx.execute(
                    "UPDATE milestones SET status=?1,updated_at_ms=?2 WHERE id=?3",
                    params![to.as_str(), now_ms()?, milestone_id],
                )?;
                activity_kind = "status_changed";
                activity_summary = format!("Milestone status changed to {}", to.as_str());
                subject = Some(milestone_id);
            }
            MilestoneAction::SetTarget {
                milestone_id,
                target_at_ms,
            } => {
                let current = required(&tx, &milestone_id)?;
                if !valid_window(current.start_at_ms, target_at_ms) {
                    return Err(AuthorityError::Invalid("invalid milestone target date"));
                }
                let change = mutation(
                    &tx,
                    &milestone_id,
                    "target_at_ms",
                    target_at_ms.map(|at| at.to_string()),
                )?;
                tx.execute(
                    "UPDATE milestones SET target_at_ms=?1,updated_at_ms=?2 WHERE id=?3",
                    params![target_at_ms, now_ms()?, milestone_id],
                )?;
                activity_summary = "Milestone target date updated".to_owned();
                mutations = vec![change];
                subject = Some(milestone_id);
            }
            MilestoneAction::MoveProject {
                milestone_id,
                project_id,
            } => {
                required(&tx, &milestone_id)?;
                ensure_project(&tx, &project_id)?;
                let tasks: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM tasks WHERE milestone_id=?1",
                    [&milestone_id],
                    |r| r.get(0),
                )?;
                // Only a membership whose work item still exists is live work.
                // Counting orphan rows would let an invisible row block a move.
                let work_items: i64 = attached_work_items(&tx, &milestone_id)?;
                if tasks > 0 || work_items > 0 {
                    return Err(AuthorityError::Invalid(
                        "cannot move milestone with attached work",
                    ));
                }
                mutations = vec![mutation(
                    &tx,
                    &milestone_id,
                    "project_id",
                    Some(project_id.clone()),
                )?];
                tx.execute(
                    "UPDATE milestones SET project_id=?1,updated_at_ms=?2 WHERE id=?3",
                    params![project_id, now_ms()?, milestone_id],
                )?;
                activity_summary = format!("Milestone moved to project {project_id}");
                subject = Some(milestone_id);
            }
            MilestoneAction::AssignTask {
                milestone_id,
                task_id,
            } => {
                required(&tx, &milestone_id)?;
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
                // The task command owns membership validation and SQL; a
                // milestone command must not invent a second copy.
                let change = assign_task_milestone(
                    &tx,
                    &task_id,
                    task_project.as_deref(),
                    Some(&milestone_id),
                )?;
                mutations = vec![change];
                semantic_kind = EntityKind::Task;
                semantic_id = Some(task_id.clone());
                tx.execute(
                    "UPDATE milestones SET updated_at_ms=?2 WHERE id=?1",
                    params![milestone_id, now_ms()?],
                )?;
                activity_summary = format!("Task {task_id} assigned to this milestone");
                subject = Some(milestone_id);
            }
            MilestoneAction::UnassignTask { task_id } => {
                let prior: Option<(Option<String>, Option<String>)> = tx
                    .query_row(
                        "SELECT project_id,milestone_id FROM tasks WHERE id=?1",
                        [&task_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                let Some((task_project, milestone_id)) = prior else {
                    return Err(AuthorityError::Invalid("task does not exist"));
                };
                let Some(milestone_id) = milestone_id else {
                    return Err(AuthorityError::Invalid("task is not in a milestone"));
                };
                let change = assign_task_milestone(&tx, &task_id, task_project.as_deref(), None)?;
                mutations = vec![change];
                semantic_kind = EntityKind::Task;
                semantic_id = Some(task_id.clone());
                tx.execute(
                    "UPDATE milestones SET updated_at_ms=?2 WHERE id=?1",
                    params![milestone_id, now_ms()?],
                )?;
                activity_summary = format!("Task {task_id} removed from this milestone");
                subject = Some(milestone_id);
            }
            MilestoneAction::AssignWorkItem {
                milestone_id,
                item_id,
            } => {
                let milestone = required(&tx, &milestone_id)?;
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
                if item_project != milestone.project_id {
                    return Err(AuthorityError::Invalid(
                        "work item and milestone must be in the same project",
                    ));
                }
                tx.execute(
                    "INSERT INTO milestone_work_items(item_id,milestone_id) VALUES(?1,?2) \
                     ON CONFLICT(item_id) DO UPDATE SET milestone_id=excluded.milestone_id",
                    params![item_id, milestone_id],
                )?;
                tx.execute(
                    "UPDATE milestones SET updated_at_ms=?2 WHERE id=?1",
                    params![milestone_id, now_ms()?],
                )?;
                let ids = work_item_ids(&tx, &milestone_id)?;
                let base = field_version(&tx, &milestone_id, "work_item_ids")?;
                mutations = vec![
                    FieldMutation::new(
                        "work_item_ids",
                        Some(serde_json::to_string(&ids).map_err(|_| {
                            AuthorityError::Invalid("work item membership serialization")
                        })?),
                        base,
                    )
                    .map_err(|_| AuthorityError::Invalid("invalid work item mutation"))?,
                ];
                activity_summary = format!("Work item {item_id} assigned to this milestone");
                subject = Some(milestone_id);
            }
            MilestoneAction::UnassignWorkItem { item_id } => {
                let milestone_id: Option<String> = tx
                    .query_row(
                        "SELECT milestone_id FROM milestone_work_items WHERE item_id=?1",
                        [&item_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(milestone_id) = milestone_id else {
                    return Err(AuthorityError::Invalid("work item is not in a milestone"));
                };
                tx.execute(
                    "DELETE FROM milestone_work_items WHERE item_id=?1",
                    [&item_id],
                )?;
                tx.execute(
                    "UPDATE milestones SET updated_at_ms=?2 WHERE id=?1",
                    params![milestone_id, now_ms()?],
                )?;
                let ids = work_item_ids(&tx, &milestone_id)?;
                let base = field_version(&tx, &milestone_id, "work_item_ids")?;
                mutations = vec![
                    FieldMutation::new(
                        "work_item_ids",
                        Some(serde_json::to_string(&ids).map_err(|_| {
                            AuthorityError::Invalid("work item membership serialization")
                        })?),
                        base,
                    )
                    .map_err(|_| AuthorityError::Invalid("invalid work item mutation"))?,
                ];
                activity_summary = format!("Work item {item_id} removed from this milestone");
                subject = Some(milestone_id);
            }
            MilestoneAction::Delete { milestone_id } => {
                required(&tx, &milestone_id)?;
                let tasks: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM tasks WHERE milestone_id=?1",
                    [&milestone_id],
                    |r| r.get(0),
                )?;
                let work_items: i64 = attached_work_items(&tx, &milestone_id)?;
                if tasks > 0 || work_items > 0 {
                    return Err(AuthorityError::Invalid(
                        "cannot delete milestone with attached work",
                    ));
                }
                tx.execute("DELETE FROM milestones WHERE id=?1", [&milestone_id])?;
                mutations = vec![mutation(
                    &tx,
                    &milestone_id,
                    "deleted",
                    Some("true".into()),
                )?];
                subject = Some(milestone_id);
            }
        }

        let id = subject.ok_or(AuthorityError::Invalid("milestone command has no subject"))?;
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
        let semantic_id = semantic_id.unwrap_or_else(|| id.clone());
        let semantic = SemanticChange::new(
            EntityRef::new(semantic_kind, semantic_id)
                .map_err(|_| AuthorityError::Invalid("invalid milestone identity"))?,
            mutations,
        )
        .map_err(|_| AuthorityError::Invalid("invalid milestone change"))?;
        let record = record(&tx, &id)?;
        let result = MilestoneResult {
            revision,
            duplicate: false,
            milestone: record,
        };
        let receipt = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("milestone receipt"))?;
        pending.commit(&tx, revision, semantic, &receipt)?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_commands::{ProjectCommand, TaskAction, TaskCommand};
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn store() -> AuthorityStore {
        AuthorityStore::open(Path::new(":memory:")).unwrap()
    }
    fn command(id: &str, rev: i64, action: MilestoneAction) -> MilestoneCommand {
        MilestoneCommand {
            command_id: id.into(),
            expected_revision: rev,
            action,
        }
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
    fn create(id: &str, project: &str, name: &str) -> MilestoneAction {
        MilestoneAction::Create {
            milestone_id: id.into(),
            project_id: project.into(),
            name: name.into(),
            description: String::new(),
            priority: None,
            owner: None,
            start_at_ms: None,
            target_at_ms: None,
        }
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
                    milestone_id: None,
                    description: String::new(),
                    status: None,
                    priority: None,
                    estimated_minutes: None,
                    deadline_at_ms: None,
                    earliest_start_at_ms: None,
                    acceptance_criteria: Vec::new(),
                    prerequisite_ids: Vec::new(),
                },
            },
        )
        .unwrap();
    }
    fn work_item(db: &AuthorityStore, id: &str, project: &str, state: &str) {
        db.conn
            .execute(
                "INSERT OR IGNORE INTO orbit_blueprints(id,version,manifest_json) VALUES('blueprint',1,'{}')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO orbit_blueprint_work_items(id,project_id,kind,state,properties_json,revision,blueprint_id,blueprint_version) VALUES(?1,?2,'task',?3,'{}',0,'blueprint',1)",
                params![id, project, state],
            )
            .unwrap();
    }
    fn temp_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "orbit-milestone-{label}-{}-{unique}.db",
            std::process::id()
        ))
    }
    fn remove_db(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn create_rename_move_finish_and_retry() {
        let mut db = store();
        project(&mut db, "one", 0);
        project(&mut db, "two", 1);
        let created = command("create", 2, create("m", "one", "Prototype"));
        let accepted = db
            .authority_milestone_command("user", created.clone())
            .unwrap();
        assert_eq!(accepted.revision, 3);
        assert_eq!(accepted.milestone.unwrap().status, "planned");
        assert!(
            db.authority_milestone_command("user", created)
                .unwrap()
                .duplicate
        );
        db.authority_milestone_command(
            "user",
            command(
                "rename",
                3,
                MilestoneAction::Rename {
                    milestone_id: "m".into(),
                    name: "Alpha".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "move",
                4,
                MilestoneAction::MoveProject {
                    milestone_id: "m".into(),
                    project_id: "two".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "progress",
                5,
                MilestoneAction::ChangeStatus {
                    milestone_id: "m".into(),
                    status: "in_progress".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "finish",
                6,
                MilestoneAction::ChangeStatus {
                    milestone_id: "m".into(),
                    status: "completed".into(),
                },
            ),
        )
        .unwrap();
        let snapshot = db.authority_milestones().unwrap();
        assert_eq!(snapshot.revision, 7);
        assert_eq!(snapshot.milestones[0].name, "Alpha");
        assert_eq!(snapshot.milestones[0].project_id, "two");
        assert_eq!(snapshot.milestones[0].status, "completed");
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "reopen",
                    7,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "in_progress".into(),
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 7);
    }
    #[test]
    fn rich_create_and_update_round_trip() {
        let mut db = store();
        project(&mut db, "p", 0);
        let created = db
            .authority_milestone_command(
                "user",
                command(
                    "create",
                    1,
                    MilestoneAction::Create {
                        milestone_id: "m".into(),
                        project_id: "p".into(),
                        name: "Launch".into(),
                        description: "Ship the thing".into(),
                        priority: Some("high".into()),
                        owner: Some("alex".into()),
                        start_at_ms: Some(1_000),
                        target_at_ms: Some(2_000),
                    },
                ),
            )
            .unwrap()
            .milestone
            .unwrap();
        assert_eq!(created.description, "Ship the thing");
        assert_eq!(created.priority.as_deref(), Some("high"));
        assert_eq!(created.owner.as_deref(), Some("alex"));
        assert_eq!(created.start_at_ms, Some(1_000));
        assert_eq!(created.target_at_ms, Some(2_000));
        assert!(created.created_at_ms > 0);
        assert_eq!(created.created_at_ms, created.updated_at_ms);
        assert_eq!(created.progress.total, 0);
        assert_eq!(created.progress.percent, 0);
        assert_eq!(created.activities.len(), 1);

        let updated = db
            .authority_milestone_command(
                "user",
                command(
                    "update",
                    2,
                    MilestoneAction::Update {
                        milestone_id: "m".into(),
                        name: "Launch v2".into(),
                        description: "Updated".into(),
                        priority: Some("urgent".into()),
                        owner: None,
                        start_at_ms: Some(1_500),
                        target_at_ms: Some(2_500),
                    },
                ),
            )
            .unwrap()
            .milestone
            .unwrap();
        assert_eq!(updated.name, "Launch v2");
        assert_eq!(updated.description, "Updated");
        assert_eq!(updated.priority.as_deref(), Some("urgent"));
        assert_eq!(updated.owner, None);
        assert_eq!(updated.start_at_ms, Some(1_500));
        assert_eq!(updated.target_at_ms, Some(2_500));
        assert!(updated.updated_at_ms >= updated.created_at_ms);
    }
    #[test]
    fn failed_missing_project_and_stale_revision_are_atomic() {
        let mut db = store();
        let no_project = command("missing", 0, create("m", "none", "Bad"));
        assert!(db.authority_milestone_command("user", no_project).is_err());
        assert_eq!(db.authority_milestones().unwrap().revision, 0);
        project(&mut db, "p", 0);
        let stale = command("stale", 0, create("m", "p", "Bad"));
        assert!(db.authority_milestone_command("user", stale).is_err());
        assert!(db.authority_milestones().unwrap().milestones.is_empty());
    }
    #[test]
    fn delete_clears_live_projection_but_preserves_command_receipt() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command("user", command("create", 1, create("m", "p", "Ready")))
            .unwrap();
        let deletion = command(
            "delete",
            2,
            MilestoneAction::Delete {
                milestone_id: "m".into(),
            },
        );
        assert!(
            db.authority_milestone_command("user", deletion.clone())
                .unwrap()
                .milestone
                .is_none()
        );
        assert!(
            db.authority_milestone_command("user", deletion)
                .unwrap()
                .duplicate
        );
        assert!(db.authority_milestones().unwrap().milestones.is_empty());
        assert_eq!(db.event_count().unwrap(), 3);
    }
    #[test]
    fn milestone_serialization_rejects_unknown_fields() {
        let valid = r#"{"command_id":"cmd","expected_revision":0,"action":{"type":"delete","milestone_id":"m"}}"#;
        let decoded: MilestoneCommand = serde_json::from_str(valid).unwrap();
        assert_eq!(decoded.action.milestone_id(), Some("m"));
        let bad = r#"{"command_id":"cmd","expected_revision":0,"unexpected":true,"action":{"type":"delete","milestone_id":"m"}}"#;
        assert!(serde_json::from_str::<MilestoneCommand>(bad).is_err());
        // Older create payloads without the rich fields still decode.
        let legacy = r#"{"command_id":"cmd","expected_revision":0,"action":{"type":"create","milestone_id":"m","project_id":"p","name":"Old"}}"#;
        let decoded: MilestoneCommand = serde_json::from_str(legacy).unwrap();
        match decoded.action {
            MilestoneAction::Create {
                description,
                priority,
                owner,
                start_at_ms,
                target_at_ms,
                ..
            } => {
                assert!(description.is_empty());
                assert!(priority.is_none());
                assert!(owner.is_none());
                assert!(start_at_ms.is_none());
                assert!(target_at_ms.is_none());
            }
            _ => panic!("expected create"),
        }
        // Older receipts without the rich record fields still decode.
        let receipt = r#"{"revision":1,"duplicate":false,"milestone":{"id":"m","project_id":"p","name":"Old","status":"planned"}}"#;
        let decoded: MilestoneResult = serde_json::from_str(receipt).unwrap();
        let milestone = decoded.milestone.unwrap();
        assert_eq!(milestone.name, "Old");
        assert_eq!(milestone.progress.total, 0);
        assert!(milestone.task_ids.is_empty());
    }

    #[test]
    fn milestone_state_machine_matches_domain_transitions() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command("user", command("create", 1, create("m", "p", "Outcome")))
            .unwrap();
        // Planned milestones cannot skip directly to completion.
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "skip",
                    2,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "completed".into()
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 2);
        let transitions = [
            ("block", "blocked"),
            ("resume", "in_progress"),
            ("reblock", "blocked"),
            ("continue", "in_progress"),
            ("complete", "completed"),
        ];
        for (index, (id, status)) in transitions.into_iter().enumerate() {
            db.authority_milestone_command(
                "user",
                command(
                    id,
                    2 + index as i64,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: status.into(),
                    },
                ),
            )
            .unwrap();
            assert_eq!(
                db.authority_milestones().unwrap().milestones[0].status,
                status
            );
        }
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "reset",
                    7,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "planned".into()
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 7);
    }

    #[test]
    fn milestone_movement_and_delete_respect_linked_task_references() {
        let mut db = store();
        project(&mut db, "p", 0);
        project(&mut db, "other", 1);
        db.authority_milestone_command("user", command("create", 2, create("m", "p", "Milestone")))
            .unwrap();
        // Simulate the existing rich task layer associating its task with a
        // milestone: the authority must not orphan or cross-link it.
        db.conn.execute("INSERT INTO tasks(id,title,status,project_id,milestone_id) VALUES('t','Task','ready','p','m')",[]).unwrap();
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "move",
                    3,
                    MilestoneAction::MoveProject {
                        milestone_id: "m".into(),
                        project_id: "other".into()
                    }
                )
            )
            .is_err()
        );
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "delete",
                    3,
                    MilestoneAction::Delete {
                        milestone_id: "m".into()
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 3);
        assert_eq!(
            db.authority_milestones().unwrap().milestones[0].project_id,
            "p"
        );
        db.conn
            .execute("UPDATE tasks SET milestone_id=NULL WHERE id='t'", [])
            .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "move",
                3,
                MilestoneAction::MoveProject {
                    milestone_id: "m".into(),
                    project_id: "other".into(),
                },
            ),
        )
        .unwrap();
        assert_eq!(
            db.authority_milestones().unwrap().milestones[0].project_id,
            "other"
        );
    }

    #[test]
    fn task_membership_is_same_project_and_cross_project_is_atomic() {
        let mut db = store();
        project(&mut db, "p", 0);
        project(&mut db, "other", 1);
        task(&mut db, "same", "p", 2);
        task(&mut db, "elsewhere", "other", 3);
        db.authority_milestone_command("user", command("create", 4, create("m", "p", "Group")))
            .unwrap();
        let assigned = db
            .authority_milestone_command(
                "user",
                command(
                    "assign-same",
                    5,
                    MilestoneAction::AssignTask {
                        milestone_id: "m".into(),
                        task_id: "same".into(),
                    },
                ),
            )
            .unwrap();
        assert_eq!(assigned.milestone.unwrap().task_ids, vec!["same"]);
        let revision = db.authority_milestones().unwrap().revision;
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "assign-other",
                    6,
                    MilestoneAction::AssignTask {
                        milestone_id: "m".into(),
                        task_id: "elsewhere".into(),
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, revision);
        assert_eq!(
            db.authority_milestones().unwrap().milestones[0].task_ids,
            vec!["same"]
        );
        assert_eq!(
            db.tasks()
                .unwrap()
                .1
                .into_iter()
                .find(|task| task.id == "elsewhere")
                .unwrap()
                .milestone_id,
            None
        );
        let unassigned = db
            .authority_milestone_command(
                "user",
                command(
                    "unassign",
                    6,
                    MilestoneAction::UnassignTask {
                        task_id: "same".into(),
                    },
                ),
            )
            .unwrap()
            .milestone
            .unwrap();
        assert!(unassigned.task_ids.is_empty());
        assert!(!unassigned.activities.is_empty());
    }

    #[test]
    fn work_item_membership_and_project_refusal() {
        let mut db = store();
        project(&mut db, "p", 0);
        project(&mut db, "other", 1);
        work_item(&db, "w", "p", "open");
        work_item(&db, "x", "other", "open");
        db.authority_milestone_command("user", command("create", 2, create("m", "p", "Group")))
            .unwrap();
        let assigned_result = db
            .authority_milestone_command(
                "user",
                command(
                    "assign-work",
                    3,
                    MilestoneAction::AssignWorkItem {
                        milestone_id: "m".into(),
                        item_id: "w".into(),
                    },
                ),
            )
            .unwrap();
        let assigned = assigned_result.milestone.clone().unwrap();
        assert_eq!(assigned.work_item_ids, vec!["w"]);
        assert_eq!(assigned.progress.total, 1);
        // The accepted change must describe the membership itself, not just the
        // timestamp projection, so replicas can replay the milestone edge set.
        let fields = milestone_wire_mutations(&db, assigned_result.revision);
        assert_eq!(
            fields.get("work_item_ids").map(String::as_str),
            Some(r#"["w"]"#)
        );
        let revision = db.authority_milestones().unwrap().revision;
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "assign-other-work",
                    4,
                    MilestoneAction::AssignWorkItem {
                        milestone_id: "m".into(),
                        item_id: "x".into(),
                    }
                )
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, revision);
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "move",
                    4,
                    MilestoneAction::MoveProject {
                        milestone_id: "m".into(),
                        project_id: "other".into(),
                    }
                )
            )
            .is_err()
        );
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "delete",
                    4,
                    MilestoneAction::Delete {
                        milestone_id: "m".into()
                    }
                )
            )
            .is_err()
        );
        let unassign_result = db
            .authority_milestone_command(
                "user",
                command(
                    "unassign-work",
                    4,
                    MilestoneAction::UnassignWorkItem {
                        item_id: "w".into(),
                    },
                ),
            )
            .unwrap();
        let unassigned = unassign_result.milestone.clone().unwrap();
        assert!(unassigned.work_item_ids.is_empty());
        let fields = milestone_wire_mutations(&db, unassign_result.revision);
        assert_eq!(fields.get("work_item_ids").map(String::as_str), Some("[]"));
        // Replaying the full accepted history still yields a complete view.
        let view = crate::canonical::project(&db.history().unwrap()).unwrap();
        assert!(view.complete);
        assert_eq!(view.unresolved_conflicts, 0);
        db.authority_milestone_command(
            "user",
            command(
                "delete",
                5,
                MilestoneAction::Delete {
                    milestone_id: "m".into(),
                },
            ),
        )
        .unwrap();
        assert!(db.authority_milestones().unwrap().milestones.is_empty());
    }

    #[test]
    fn progress_counts_tasks_and_work_items() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command("user", command("create", 1, create("m", "p", "Group")))
            .unwrap();
        for (id, status) in [
            ("done", "done"),
            ("blocked", "blocked"),
            ("cancelled", "cancelled"),
            ("running", "in_progress"),
        ] {
            db.conn
                .execute(
                    "INSERT INTO tasks(id,title,status,project_id,milestone_id) VALUES(?1,?2,?3,'p','m')",
                    params![id, id, status],
                )
                .unwrap();
        }
        work_item(&db, "w1", "p", "done");
        work_item(&db, "w2", "p", "open");
        db.authority_milestone_command(
            "user",
            command(
                "assign-w1",
                2,
                MilestoneAction::AssignWorkItem {
                    milestone_id: "m".into(),
                    item_id: "w1".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "assign-w2",
                3,
                MilestoneAction::AssignWorkItem {
                    milestone_id: "m".into(),
                    item_id: "w2".into(),
                },
            ),
        )
        .unwrap();
        let milestone = &db.authority_milestones().unwrap().milestones[0];
        assert_eq!(
            milestone.task_ids,
            vec!["blocked", "cancelled", "done", "running"]
        );
        assert_eq!(milestone.work_item_ids, vec!["w1", "w2"]);
        assert_eq!(milestone.progress.total, 5);
        assert_eq!(milestone.progress.completed, 2);
        assert_eq!(milestone.progress.blocked, 1);
        assert_eq!(milestone.progress.percent, 40);
        assert!(milestone.activities.len() >= 3);
    }

    #[test]
    fn empty_completed_milestone_still_reports_zero_percent() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command("user", command("create", 1, create("m", "p", "Empty")))
            .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "start",
                2,
                MilestoneAction::ChangeStatus {
                    milestone_id: "m".into(),
                    status: "in_progress".into(),
                },
            ),
        )
        .unwrap();
        db.authority_milestone_command(
            "user",
            command(
                "done",
                3,
                MilestoneAction::ChangeStatus {
                    milestone_id: "m".into(),
                    status: "completed".into(),
                },
            ),
        )
        .unwrap();
        let milestone = &db.authority_milestones().unwrap().milestones[0];
        assert_eq!(milestone.progress.total, 0);
        assert_eq!(milestone.progress.percent, 0);
    }

    #[test]
    fn invalid_status_and_name_never_commit_an_authority_receipt() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command("user", command("create", 1, create("m", "p", "Milestone")))
            .unwrap();
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "bad-name",
                    2,
                    MilestoneAction::Rename {
                        milestone_id: "m".into(),
                        name: "  ".into()
                    }
                )
            )
            .is_err()
        );
        assert!(
            db.authority_milestone_command(
                "user",
                command(
                    "bad-status",
                    2,
                    MilestoneAction::ChangeStatus {
                        milestone_id: "m".into(),
                        status: "unknown".into()
                    }
                )
            )
            .is_err()
        );
        assert!(
            db.authority_milestone_command(
                "other",
                command("create", 1, create("m", "p", "Milestone"))
            )
            .is_err()
        );
        assert_eq!(db.authority_milestones().unwrap().revision, 2);
        assert_eq!(db.event_count().unwrap(), 2);
    }
    #[test]
    fn target_date_is_a_clearable_checkpoint_marker() {
        let mut db = store();
        project(&mut db, "p", 0);
        db.authority_milestone_command("user", command("create", 1, create("m", "p", "Ready")))
            .unwrap();
        let set = db
            .authority_milestone_command(
                "user",
                command(
                    "set-target",
                    2,
                    MilestoneAction::SetTarget {
                        milestone_id: "m".into(),
                        target_at_ms: Some(1_790_000_000_000),
                    },
                ),
            )
            .unwrap();
        assert_eq!(set.milestone.unwrap().target_at_ms, Some(1_790_000_000_000));
        let cleared = db
            .authority_milestone_command(
                "user",
                command(
                    "clear-target",
                    3,
                    MilestoneAction::SetTarget {
                        milestone_id: "m".into(),
                        target_at_ms: None,
                    },
                ),
            )
            .unwrap();
        assert_eq!(cleared.milestone.unwrap().target_at_ms, None);
    }
    #[test]
    fn rich_milestone_persists_across_reopen() {
        let path = temp_path("persist");
        {
            let mut db = AuthorityStore::open(&path).unwrap();
            project(&mut db, "p", 0);
            task(&mut db, "t", "p", 1);
            work_item(&db, "w", "p", "closed");
            db.authority_milestone_command(
                "user",
                command(
                    "create",
                    2,
                    MilestoneAction::Create {
                        milestone_id: "m".into(),
                        project_id: "p".into(),
                        name: "Persisted".into(),
                        description: "durable".into(),
                        priority: Some("medium".into()),
                        owner: Some("owner".into()),
                        start_at_ms: Some(10),
                        target_at_ms: Some(20),
                    },
                ),
            )
            .unwrap();
            for (index, action) in [
                MilestoneAction::AssignTask {
                    milestone_id: "m".into(),
                    task_id: "t".into(),
                },
                MilestoneAction::AssignWorkItem {
                    milestone_id: "m".into(),
                    item_id: "w".into(),
                },
            ]
            .into_iter()
            .enumerate()
            {
                db.authority_milestone_command(
                    "user",
                    command(&format!("attach-{index}"), 3 + index as i64, action),
                )
                .unwrap();
            }
        }
        {
            let db = AuthorityStore::open(&path).unwrap();
            let milestone = &db.authority_milestones().unwrap().milestones[0];
            assert_eq!(milestone.name, "Persisted");
            assert_eq!(milestone.description, "durable");
            assert_eq!(milestone.priority.as_deref(), Some("medium"));
            assert_eq!(milestone.owner.as_deref(), Some("owner"));
            assert_eq!(milestone.start_at_ms, Some(10));
            assert_eq!(milestone.target_at_ms, Some(20));
            assert_eq!(milestone.task_ids, vec!["t"]);
            assert_eq!(milestone.work_item_ids, vec!["w"]);
            assert_eq!(milestone.progress.total, 2);
            assert_eq!(milestone.progress.completed, 1);
            assert!(milestone.activities.len() >= 3);
        }
        remove_db(&path);
    }

    fn milestone_wire_mutations(
        db: &AuthorityStore,
        revision: i64,
    ) -> std::collections::BTreeMap<String, String> {
        let wire: String = db
            .conn
            .query_row(
                "SELECT wire FROM authority_history WHERE sequence=?1",
                [revision],
                |row| row.get(0),
            )
            .unwrap();
        let envelope = orbit_sync::ChangeEnvelope::from_wire_json(&wire).unwrap();
        envelope
            .change
            .mutations
            .into_iter()
            .filter_map(|mutation| mutation.value.map(|value| (mutation.field, value)))
            .collect()
    }

    #[test]
    fn legacy_milestone_create_wire_reserializes_identically_and_retries_as_duplicate() {
        let mut db = store();
        // The pre-upgrade create payload contained only these three fields.
        let legacy = r#"{"command_id":"legacy-create","expected_revision":0,"action":{"type":"create","milestone_id":"m","project_id":"p","name":"Old"}}"#;
        let decoded: MilestoneCommand = serde_json::from_str(legacy).unwrap();
        assert_eq!(
            serde_json::to_string(&decoded).unwrap(),
            legacy,
            "a legacy milestone create wire must serialize back to its exact pre-upgrade bytes"
        );
        let receipt = r#"{"revision":1,"duplicate":false,"milestone":{"id":"m","project_id":"p","name":"Old","status":"planned"}}"#;
        db.conn
            .execute(
                "INSERT INTO authority_commands(command_id,actor,payload,result_json) VALUES(?1,'user',?2,?3)",
                params!["legacy-create", legacy, receipt],
            )
            .unwrap();
        let retried = db.authority_milestone_command("user", decoded).unwrap();
        assert!(retried.duplicate);
        assert_eq!(retried.revision, 1);
        assert_eq!(retried.milestone.unwrap().name, "Old");
    }
}
