//! Canonical project-strategy commands. This is the single authority write path
//! for strategy configuration and success-criterion evaluation in both modes.
//! Strategy changes share the project ledger, revision fence, receipt and SQLite
//! transaction with all other domain commands.
use crate::command_engine::{CommandAdmission, PendingCommand};
use crate::store::{AuthorityError, AuthorityStore};
use orbit_sync::{ChangeEnvelope, EntityKind, EntityRef, FieldMutation, SemanticChange};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StrategyCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: StrategyAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrategyAction {
    Configure {
        project_id: String,
        mode: String,
        objective: Option<String>,
        progress_signals: Vec<String>,
        criteria: Vec<CriterionInput>,
    },
    SetObjective {
        project_id: String,
        objective: Option<String>,
    },
    SetCriterionStatus {
        project_id: String,
        criterion_key: String,
        status: String,
    },
    SetProgressSignals {
        project_id: String,
        progress_signals: Vec<String>,
    },
}
impl StrategyAction {
    pub fn project_id(&self) -> &str {
        match self {
            Self::Configure { project_id, .. }
            | Self::SetObjective { project_id, .. }
            | Self::SetCriterionStatus { project_id, .. }
            | Self::SetProgressSignals { project_id, .. } => project_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CriterionInput {
    pub key: String,
    pub description: String,
    pub required: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CriterionRecord {
    pub key: String,
    pub description: String,
    pub required: bool,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StrategyRecord {
    pub project_id: String,
    pub mode: String,
    pub objective: Option<String>,
    pub progress_signals: Vec<String>,
    pub criteria: Vec<CriterionRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StrategyResult {
    pub revision: i64,
    pub duplicate: bool,
    pub strategy: StrategyRecord,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StrategySnapshot {
    pub revision: i64,
    pub strategies: Vec<StrategyRecord>,
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
fn valid_mode(mode: &str) -> bool {
    matches!(
        mode,
        "standard" | "learning" | "habit_driven" | "metric_target" | "maintenance" | "custom"
    )
}
fn valid_status(status: &str) -> bool {
    matches!(status, "pending" | "satisfied" | "waived")
}
fn valid_signal(signal: &str) -> bool {
    matches!(
        signal,
        "milestones"
            | "tasks"
            | "blockers"
            | "forecast"
            | "habit_consistency"
            | "time_invested"
            | "learning_evidence"
            | "metric_value"
            | "knowledge_evidence"
    )
}
fn valid_objective(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_none_or(|text| !text.trim().is_empty() && text.len() <= 8192)
}
fn validate_signals(signals: &[String]) -> Result<(), AuthorityError> {
    if signals.is_empty() || signals.len() > 9 {
        return Err(AuthorityError::Invalid(
            "project needs 1-9 progress signals",
        ));
    }
    let mut seen = HashSet::new();
    for signal in signals {
        if !valid_signal(signal) || !seen.insert(signal.as_str()) {
            return Err(AuthorityError::Invalid(
                "invalid or duplicate progress signal",
            ));
        }
    }
    Ok(())
}
fn validate_criteria(criteria: &[CriterionInput]) -> Result<(), AuthorityError> {
    if criteria.len() > 100 {
        return Err(AuthorityError::Invalid("too many project success criteria"));
    }
    let mut seen = HashSet::new();
    for criterion in criteria {
        if !valid_id(&criterion.key)
            || !seen.insert(criterion.key.as_str())
            || criterion.description.trim().is_empty()
            || criterion.description.len() > 4096
        {
            return Err(AuthorityError::Invalid(
                "invalid or duplicated success criterion",
            ));
        }
    }
    Ok(())
}
fn require_project(conn: &Connection, id: &str) -> Result<(), AuthorityError> {
    if !valid_id(id) {
        return Err(AuthorityError::Invalid("invalid project identity"));
    }
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
        [id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(AuthorityError::Invalid("strategy project does not exist"));
    }
    Ok(())
}
fn fetch(conn: &Connection, id: &str) -> Result<StrategyRecord, AuthorityError> {
    require_project(conn, id)?;
    let (mode, objective, signals): (String, Option<String>, String) = conn.query_row(
        "SELECT mode,objective,progress_signals FROM projects WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if !valid_mode(&mode) || !valid_objective(&objective) {
        return Err(AuthorityError::Invalid("invalid stored project strategy"));
    }
    let progress_signals: Vec<String> = signals.split(',').map(str::to_owned).collect();
    validate_signals(&progress_signals)?;
    let mut statement = conn.prepare(
        "SELECT criterion_key,description,required,status FROM project_success_criteria WHERE project_id=?1 ORDER BY position")?;
    let criteria = statement
        .query_map([id], |row| {
            Ok(CriterionRecord {
                key: row.get(0)?,
                description: row.get(1)?,
                required: row.get::<_, i64>(2)? != 0,
                status: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = HashSet::new();
    for criterion in &criteria {
        if !valid_id(&criterion.key)
            || !seen.insert(criterion.key.as_str())
            || criterion.description.trim().is_empty()
            || !valid_status(&criterion.status)
        {
            return Err(AuthorityError::Invalid("invalid stored success criterion"));
        }
    }
    Ok(StrategyRecord {
        project_id: id.to_owned(),
        mode,
        objective,
        progress_signals,
        criteria,
    })
}
fn latest_version(
    conn: &Connection,
    id: &str,
    field: &str,
) -> Result<Option<orbit_sync::ChangeId>, AuthorityError> {
    let mut stmt = conn
        .prepare("SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    for row in rows {
        let envelope =
            ChangeEnvelope::from_wire_json(&row?).map_err(|_| AuthorityError::MigrationConflict)?;
        if envelope.change.entity.kind == EntityKind::Project
            && envelope.change.entity.id == id
            && envelope
                .change
                .mutations
                .iter()
                .any(|item| item.field == field)
        {
            return Ok(Some(envelope.id));
        }
    }
    Ok(None)
}
fn mutation(
    conn: &Connection,
    id: &str,
    field: &str,
    value: Option<String>,
) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(field, value, latest_version(conn, id, field)?)
        .map_err(|_| AuthorityError::Invalid("invalid strategy semantic mutation"))
}
fn replace_criteria(
    conn: &Connection,
    id: &str,
    criteria: &[CriterionInput],
) -> Result<(), AuthorityError> {
    validate_criteria(criteria)?;
    conn.execute(
        "DELETE FROM project_success_criteria WHERE project_id=?1",
        [id],
    )?;
    for (position, criterion) in criteria.iter().enumerate() {
        conn.execute("INSERT INTO project_success_criteria(project_id,position,criterion_key,description,required,status) VALUES(?1,?2,?3,?4,?5,'pending')",
            params![id,position as i64,criterion.key,criterion.description,i64::from(criterion.required)])?;
    }
    Ok(())
}
impl AuthorityStore {
    /// All strategies come from the accepted SQLite state at one history tip.
    pub fn authority_strategies(&self) -> Result<StrategySnapshot, AuthorityError> {
        let tx = self.conn.unchecked_transaction()?;
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(cursor),0) FROM authority_history",
            [],
            |r| r.get(0),
        )?;
        let project_ids = {
            let mut stmt = tx.prepare("SELECT id FROM projects ORDER BY id")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let strategies = project_ids
            .iter()
            .map(|id| fetch(&tx, id))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit()?;
        Ok(StrategySnapshot {
            revision,
            strategies,
        })
    }

    /// The full command and semantic history commit in one IMMEDIATE SQLite
    /// transaction. All validation failures leave revision and receipt intact.
    pub fn authority_strategy_command(
        &mut self,
        actor: &str,
        command: StrategyCommand,
    ) -> Result<StrategyResult, AuthorityError> {
        if actor.trim().is_empty()
            || !valid_id(&command.command_id)
            || !valid_id(command.action.project_id())
            || command.expected_revision < 0
        {
            return Err(AuthorityError::Invalid("invalid strategy command envelope"));
        }
        let payload = serde_json::to_string(&command)
            .map_err(|_| AuthorityError::Invalid("strategy command serialization"))?;
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
                let mut receipt: StrategyResult = serde_json::from_str(&raw)
                    .map_err(|_| AuthorityError::Invalid("stored strategy receipt is invalid"))?;
                receipt.duplicate = true;
                return Ok(receipt);
            }
            CommandAdmission::New(revision) => revision,
        };
        let project_id = command.action.project_id().to_owned();
        require_project(&tx, &project_id)?;
        let changes = match command.action {
            StrategyAction::Configure {
                mode,
                objective,
                progress_signals,
                criteria,
                ..
            } => {
                if !valid_mode(&mode) || !valid_objective(&objective) {
                    return Err(AuthorityError::Invalid(
                        "invalid strategy mode or objective",
                    ));
                }
                validate_signals(&progress_signals)?;
                validate_criteria(&criteria)?;
                // Precompute previous field versions before mutation; all of
                // the changes are one semantic event at this revision.
                let signal_value = progress_signals.join(",");
                let criteria_value = serde_json::to_string(&criteria)
                    .map_err(|_| AuthorityError::Invalid("invalid success criteria"))?;
                let changes = vec![
                    mutation(&tx, &project_id, "mode", Some(mode.clone()))?,
                    mutation(&tx, &project_id, "objective", objective.clone())?,
                    mutation(
                        &tx,
                        &project_id,
                        "progress_signals",
                        Some(signal_value.clone()),
                    )?,
                    mutation(&tx, &project_id, "success_criteria", Some(criteria_value))?,
                ];
                tx.execute(
                    "UPDATE projects SET mode=?1,objective=?2,progress_signals=?3 WHERE id=?4",
                    params![mode, objective, signal_value, project_id],
                )?;
                replace_criteria(&tx, &project_id, &criteria)?;
                changes
            }
            StrategyAction::SetObjective { objective, .. } => {
                if !valid_objective(&objective) {
                    return Err(AuthorityError::Invalid("invalid strategy objective"));
                }
                let change = mutation(&tx, &project_id, "objective", objective.clone())?;
                tx.execute(
                    "UPDATE projects SET objective=?1 WHERE id=?2",
                    params![objective, project_id],
                )?;
                vec![change]
            }
            StrategyAction::SetCriterionStatus {
                criterion_key,
                status,
                ..
            } => {
                if !valid_id(&criterion_key) || !valid_status(&status) {
                    return Err(AuthorityError::Invalid("invalid criterion update"));
                }
                let before: Option<String> = tx.query_row(
                    "SELECT status FROM project_success_criteria WHERE project_id=?1 AND criterion_key=?2",
                    params![project_id,criterion_key], |r| r.get(0)).optional()?;
                let Some(before) = before else {
                    return Err(AuthorityError::Invalid("criterion does not exist"));
                };
                if !valid_status(&before) {
                    return Err(AuthorityError::Invalid("invalid stored criterion status"));
                }
                let field_name = format!("criterion_status:{criterion_key}");
                let change = mutation(&tx, &project_id, &field_name, Some(status.clone()))?;
                tx.execute("UPDATE project_success_criteria SET status=?1 WHERE project_id=?2 AND criterion_key=?3",
                    params![status,project_id,criterion_key])?;
                vec![change]
            }
            StrategyAction::SetProgressSignals {
                progress_signals, ..
            } => {
                validate_signals(&progress_signals)?;
                let value = progress_signals.join(",");
                let change = mutation(&tx, &project_id, "progress_signals", Some(value.clone()))?;
                tx.execute(
                    "UPDATE projects SET progress_signals=?1 WHERE id=?2",
                    params![value, project_id],
                )?;
                vec![change]
            }
        };
        let strategy = fetch(&tx, &project_id)?;
        let semantic = SemanticChange::new(
            EntityRef::new(EntityKind::Project, project_id)
                .map_err(|_| AuthorityError::Invalid("invalid strategy entity"))?,
            changes,
        )
        .map_err(|_| AuthorityError::Invalid("invalid strategy change"))?;
        let result = StrategyResult {
            revision,
            duplicate: false,
            strategy,
        };
        let receipt = serde_json::to_string(&result)
            .map_err(|_| AuthorityError::Invalid("invalid strategy receipt"))?;
        pending.commit(&tx, revision, semantic, &receipt)?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_commands::ProjectCommand;
    use std::path::Path;

    fn db() -> AuthorityStore {
        AuthorityStore::open(Path::new(":memory:")).unwrap()
    }
    fn project(db: &mut AuthorityStore, id: &str, revision: i64) {
        db.authority_project_command(
            "owner",
            ProjectCommand {
                command_id: format!("make-{id}"),
                expected_revision: revision,
                project_id: id.into(),
                name: format!("Project {id}"),
            },
        )
        .unwrap();
    }
    fn criterion(key: &str, required: bool) -> CriterionInput {
        CriterionInput {
            key: key.into(),
            description: format!("Demonstrate {key}"),
            required,
        }
    }
    fn configure(id: &str, rev: i64, project: &str) -> StrategyCommand {
        StrategyCommand {
            command_id: id.into(),
            expected_revision: rev,
            action: StrategyAction::Configure {
                project_id: project.into(),
                mode: "learning".into(),
                objective: Some("Finish the course".into()),
                progress_signals: vec!["learning_evidence".into(), "time_invested".into()],
                criteria: vec![criterion("exam", true), criterion("review", false)],
            },
        }
    }
    fn command(id: &str, revision: i64, action: StrategyAction) -> StrategyCommand {
        StrategyCommand {
            command_id: id.into(),
            expected_revision: revision,
            action,
        }
    }
    fn tip(db: &AuthorityStore) -> i64 {
        db.authority_strategies().unwrap().revision
    }
    #[test]
    fn configure_then_read_complete_strategy_snapshot() {
        let mut db = db();
        project(&mut db, "course", 0);
        let result = db
            .authority_strategy_command("owner", configure("strategy-one", 1, "course"))
            .unwrap();
        assert_eq!(result.revision, 2);
        assert!(!result.duplicate);
        assert_eq!(result.strategy.project_id, "course");
        assert_eq!(result.strategy.mode, "learning");
        assert_eq!(
            result.strategy.objective.as_deref(),
            Some("Finish the course")
        );
        assert_eq!(result.strategy.criteria.len(), 2);
        assert_eq!(result.strategy.criteria[0].status, "pending");
        assert_eq!(
            result.strategy.progress_signals,
            vec!["learning_evidence", "time_invested"]
        );
        let snapshot = db.authority_strategies().unwrap();
        assert_eq!(snapshot.revision, 2);
        assert_eq!(snapshot.strategies, vec![result.strategy]);
    }
    #[test]
    fn retry_returns_original_receipt_without_advancing_history() {
        let mut db = db();
        project(&mut db, "p", 0);
        let cmd = configure("config", 1, "p");
        let first = db.authority_strategy_command("owner", cmd.clone()).unwrap();
        let second = db.authority_strategy_command("owner", cmd).unwrap();
        assert!(!first.duplicate);
        assert!(second.duplicate);
        assert_eq!(first.revision, second.revision);
        assert_eq!(first.strategy, second.strategy);
        assert_eq!(tip(&db), 2);
    }
    #[test]
    fn command_id_cannot_be_reused_with_different_payload() {
        let mut db = db();
        project(&mut db, "p", 0);
        db.authority_strategy_command("owner", configure("config", 1, "p"))
            .unwrap();
        let mut changed = configure("config", 1, "p");
        if let StrategyAction::Configure { objective, .. } = &mut changed.action {
            *objective = Some("Different".into());
        }
        assert!(db.authority_strategy_command("owner", changed).is_err());
        assert_eq!(tip(&db), 2);
    }
    #[test]
    fn stale_revision_is_rejected_before_domain_updates() {
        let mut db = db();
        project(&mut db, "p", 0);
        let command = configure("config", 0, "p");
        assert!(matches!(
            db.authority_strategy_command("owner", command),
            Err(AuthorityError::StaleRevision)
        ));
        assert_eq!(tip(&db), 1);
        assert_eq!(
            db.authority_strategies().unwrap().strategies[0].mode,
            "standard"
        );
    }
    #[test]
    fn missing_project_does_not_create_a_receipt() {
        let mut db = db();
        assert!(
            db.authority_strategy_command("owner", configure("missing", 0, "p"))
                .is_err()
        );
        assert_eq!(tip(&db), 0);
        project(&mut db, "p", 0);
        db.authority_strategy_command("owner", configure("missing", 1, "p"))
            .unwrap();
        assert_eq!(tip(&db), 2);
    }
    #[test]
    fn success_criterion_status_is_independently_changeable() {
        let mut db = db();
        project(&mut db, "p", 0);
        db.authority_strategy_command("owner", configure("config", 1, "p"))
            .unwrap();
        let result = db
            .authority_strategy_command(
                "owner",
                command(
                    "status",
                    2,
                    StrategyAction::SetCriterionStatus {
                        project_id: "p".into(),
                        criterion_key: "exam".into(),
                        status: "satisfied".into(),
                    },
                ),
            )
            .unwrap();
        assert_eq!(result.revision, 3);
        assert_eq!(result.strategy.criteria[0].status, "satisfied");
        assert_eq!(result.strategy.criteria[1].status, "pending");
        assert_eq!(
            db.authority_strategies().unwrap().strategies[0].criteria[0].status,
            "satisfied"
        );
    }
    #[test]
    fn unknown_criterion_and_bad_status_are_atomic_failures() {
        let mut db = db();
        project(&mut db, "p", 0);
        db.authority_strategy_command("owner", configure("config", 1, "p"))
            .unwrap();
        for (name, key, status) in [
            ("missing", "unknown", "satisfied"),
            ("bad", "exam", "expired"),
        ] {
            assert!(
                db.authority_strategy_command(
                    "owner",
                    command(
                        name,
                        2,
                        StrategyAction::SetCriterionStatus {
                            project_id: "p".into(),
                            criterion_key: key.into(),
                            status: status.into(),
                        }
                    )
                )
                .is_err()
            );
            assert_eq!(tip(&db), 2);
        }
        db.authority_strategy_command(
            "owner",
            command(
                "good",
                2,
                StrategyAction::SetCriterionStatus {
                    project_id: "p".into(),
                    criterion_key: "exam".into(),
                    status: "waived".into(),
                },
            ),
        )
        .unwrap();
    }
    #[test]
    fn replacing_configuration_resets_criteria_only_as_explicit_action() {
        let mut db = db();
        project(&mut db, "p", 0);
        db.authority_strategy_command("owner", configure("first", 1, "p"))
            .unwrap();
        db.authority_strategy_command(
            "owner",
            command(
                "status",
                2,
                StrategyAction::SetCriterionStatus {
                    project_id: "p".into(),
                    criterion_key: "exam".into(),
                    status: "satisfied".into(),
                },
            ),
        )
        .unwrap();
        let result = db
            .authority_strategy_command("owner", configure("second", 3, "p"))
            .unwrap();
        assert_eq!(result.strategy.criteria[0].status, "pending");
        assert_eq!(result.strategy.criteria.len(), 2);
    }
    #[test]
    fn replacing_signals_preserves_objective_and_criteria() {
        let mut db = db();
        project(&mut db, "p", 0);
        let initial = db
            .authority_strategy_command("owner", configure("config", 1, "p"))
            .unwrap();
        let next = db
            .authority_strategy_command(
                "owner",
                command(
                    "signals",
                    2,
                    StrategyAction::SetProgressSignals {
                        project_id: "p".into(),
                        progress_signals: vec!["tasks".into(), "forecast".into()],
                    },
                ),
            )
            .unwrap();
        assert_eq!(next.strategy.progress_signals, vec!["tasks", "forecast"]);
        assert_eq!(next.strategy.objective, initial.strategy.objective);
        assert_eq!(next.strategy.criteria, initial.strategy.criteria);
    }
    #[test]
    fn clearing_objective_keeps_other_configuration_unchanged() {
        let mut db = db();
        project(&mut db, "p", 0);
        let first = db
            .authority_strategy_command("owner", configure("config", 1, "p"))
            .unwrap();
        let result = db
            .authority_strategy_command(
                "owner",
                command(
                    "clear",
                    2,
                    StrategyAction::SetObjective {
                        project_id: "p".into(),
                        objective: None,
                    },
                ),
            )
            .unwrap();
        assert_eq!(result.strategy.objective, None);
        assert_eq!(result.strategy.mode, first.strategy.mode);
        assert_eq!(result.strategy.criteria, first.strategy.criteria);
        assert_eq!(
            result.strategy.progress_signals,
            first.strategy.progress_signals
        );
    }
    #[test]
    fn multiple_projects_keep_strategies_isolated() {
        let mut db = db();
        project(&mut db, "a", 0);
        project(&mut db, "b", 1);
        db.authority_strategy_command("owner", configure("config-a", 2, "a"))
            .unwrap();
        let snapshot = db.authority_strategies().unwrap();
        assert_eq!(snapshot.strategies.len(), 2);
        assert_eq!(snapshot.strategies[0].project_id, "a");
        assert_eq!(snapshot.strategies[1].project_id, "b");
        assert_eq!(snapshot.strategies[0].mode, "learning");
        assert_eq!(snapshot.strategies[1].mode, "standard");
    }
    #[test]
    fn invalid_modes_never_touch_history() {
        let mut db = db();
        project(&mut db, "p", 0);
        let mut command = configure("config", 1, "p");
        if let StrategyAction::Configure { mode, .. } = &mut command.action {
            *mode = "fake".into();
        }
        assert!(db.authority_strategy_command("owner", command).is_err());
        assert_eq!(tip(&db), 1);
    }
    #[test]
    fn duplicate_signals_cannot_be_committed() {
        let mut db = db();
        project(&mut db, "p", 0);
        let mut command = configure("config", 1, "p");
        if let StrategyAction::Configure {
            progress_signals, ..
        } = &mut command.action
        {
            *progress_signals = vec!["tasks".into(), "tasks".into()];
        }
        assert!(db.authority_strategy_command("owner", command).is_err());
        assert_eq!(tip(&db), 1);
    }
    #[test]
    fn duplicated_success_criterion_keys_are_rejected() {
        let mut db = db();
        project(&mut db, "p", 0);
        let mut command = configure("config", 1, "p");
        if let StrategyAction::Configure { criteria, .. } = &mut command.action {
            criteria.push(criterion("exam", false));
        }
        assert!(db.authority_strategy_command("owner", command).is_err());
        assert_eq!(tip(&db), 1);
    }
    #[test]
    fn long_objectives_and_empty_descriptions_fail_before_commit() {
        let mut db = db();
        project(&mut db, "p", 0);
        for (name, objective) in [
            ("long", Some("x".repeat(8193))),
            ("blank", Some("   ".into())),
        ] {
            assert!(
                db.authority_strategy_command(
                    "owner",
                    command(
                        name,
                        1,
                        StrategyAction::SetObjective {
                            project_id: "p".into(),
                            objective
                        }
                    )
                )
                .is_err()
            );
        }
        let mut command = configure("empty-description", 1, "p");
        if let StrategyAction::Configure { criteria, .. } = &mut command.action {
            criteria[0].description = "  ".into();
        }
        assert!(db.authority_strategy_command("owner", command).is_err());
        assert_eq!(tip(&db), 1);
    }
    #[test]
    fn unknown_serde_fields_and_action_types_fail_closed() {
        let cmd = configure("config", 1, "p");
        let mut wire = serde_json::to_value(cmd).unwrap();
        wire["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<StrategyCommand>(wire).is_err());
        let invalid = serde_json::json!({"command_id":"x","expected_revision":1,
            "action":{"type":"drop_database","project_id":"p"}});
        assert!(serde_json::from_value::<StrategyCommand>(invalid).is_err());
    }
    #[test]
    fn invalid_envelopes_rejected_before_transaction() {
        let mut db = db();
        project(&mut db, "p", 0);
        for (id, revision) in [("", 1), ("has spaces", 1), ("x", -1)] {
            assert!(
                db.authority_strategy_command(
                    "owner",
                    command(
                        id,
                        revision,
                        StrategyAction::SetObjective {
                            project_id: "p".into(),
                            objective: None
                        }
                    )
                )
                .is_err()
            );
        }
        assert_eq!(tip(&db), 1);
    }
}

#[cfg(test)]
mod strategy_matrix_tests {
    use super::*;
    use crate::domain_commands::ProjectCommand;
    use std::path::Path;

    fn db() -> AuthorityStore {
        AuthorityStore::open(Path::new(":memory:")).unwrap()
    }
    fn seed(db: &mut AuthorityStore) {
        db.authority_project_command(
            "owner",
            ProjectCommand {
                command_id: "seed".into(),
                expected_revision: 0,
                project_id: "proj".into(),
                name: "Testing".into(),
            },
        )
        .unwrap();
    }
    fn configure(
        id: &str,
        revision: i64,
        signals: Vec<&str>,
        criteria: Vec<CriterionInput>,
    ) -> StrategyCommand {
        StrategyCommand {
            command_id: id.into(),
            expected_revision: revision,
            action: StrategyAction::Configure {
                project_id: "proj".into(),
                mode: "custom".into(),
                objective: Some("Clear acceptance".into()),
                progress_signals: signals.into_iter().map(str::to_owned).collect(),
                criteria,
            },
        }
    }
    fn criterion(id: &str) -> CriterionInput {
        CriterionInput {
            key: id.into(),
            description: format!("Achieve {id}"),
            required: true,
        }
    }
    #[test]
    fn all_supported_project_modes_round_trip() {
        for mode in [
            "standard",
            "learning",
            "habit_driven",
            "metric_target",
            "maintenance",
            "custom",
        ] {
            let mut db = db();
            seed(&mut db);
            let mut cmd = configure("configure", 1, vec!["tasks"], vec![]);
            if let StrategyAction::Configure {
                mode: ref mut configured,
                ..
            } = cmd.action
            {
                *configured = mode.into();
            }
            db.authority_strategy_command("owner", cmd).unwrap();
            assert_eq!(db.authority_strategies().unwrap().strategies[0].mode, mode);
        }
    }
    #[test]
    fn all_supported_progress_signals_are_accepted() {
        let signals = [
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
        for signal in signals {
            let mut db = db();
            seed(&mut db);
            db.authority_strategy_command("owner", configure("configure", 1, vec![signal], vec![]))
                .unwrap();
            assert_eq!(
                db.authority_strategies().unwrap().strategies[0].progress_signals,
                vec![signal]
            );
        }
    }
    #[test]
    fn maximum_distinct_signals_fit_without_loss() {
        let mut db = db();
        seed(&mut db);
        let all = vec![
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
        let result = db
            .authority_strategy_command("owner", configure("all", 1, all.clone(), vec![]))
            .unwrap();
        assert_eq!(result.strategy.progress_signals, all);
    }
    #[test]
    fn empty_and_unknown_signals_fail_without_receipt() {
        for signals in [vec![], vec!["invalid"], vec!["tasks", "invalid"]] {
            let mut db = db();
            seed(&mut db);
            assert!(
                db.authority_strategy_command("owner", configure("invalid", 1, signals, vec![]))
                    .is_err()
            );
            assert_eq!(db.authority_strategies().unwrap().revision, 1);
        }
    }
    #[test]
    fn success_criteria_preserve_requested_order() {
        let mut db = db();
        seed(&mut db);
        let input = vec![criterion("last"), criterion("first"), criterion("middle")];
        let result = db
            .authority_strategy_command("owner", configure("configure", 1, vec!["tasks"], input))
            .unwrap();
        let keys: Vec<_> = result
            .strategy
            .criteria
            .iter()
            .map(|criterion| criterion.key.as_str())
            .collect();
        assert_eq!(keys, vec!["last", "first", "middle"]);
    }
    #[test]
    fn success_criteria_require_unique_key_not_unique_description() {
        let mut db = db();
        seed(&mut db);
        let same_description = vec![criterion("first"), criterion("second")];
        let result = db
            .authority_strategy_command(
                "owner",
                configure("configure", 1, vec!["tasks"], same_description),
            )
            .unwrap();
        assert_eq!(result.strategy.criteria.len(), 2);
    }
    #[test]
    fn at_most_one_hundred_criteria_are_accepted() {
        let mut db = db();
        seed(&mut db);
        let many = (0..100)
            .map(|i| criterion(&format!("criterion_{i}")))
            .collect();
        let result = db
            .authority_strategy_command("owner", configure("configure", 1, vec!["tasks"], many))
            .unwrap();
        assert_eq!(result.strategy.criteria.len(), 100);
    }
    #[test]
    fn too_many_criteria_rejected_atomically() {
        let mut db = db();
        seed(&mut db);
        let many = (0..101)
            .map(|i| criterion(&format!("criterion_{i}")))
            .collect();
        assert!(
            db.authority_strategy_command("owner", configure("configure", 1, vec!["tasks"], many))
                .is_err()
        );
        assert_eq!(db.authority_strategies().unwrap().revision, 1);
    }
    #[test]
    fn all_criterion_states_round_trip() {
        let mut db = db();
        seed(&mut db);
        db.authority_strategy_command(
            "owner",
            configure("configure", 1, vec!["tasks"], vec![criterion("delivery")]),
        )
        .unwrap();
        for (revision, status) in [(2, "satisfied"), (3, "waived"), (4, "pending")] {
            let result = db
                .authority_strategy_command(
                    "owner",
                    StrategyCommand {
                        command_id: format!("status-{revision}"),
                        expected_revision: revision,
                        action: StrategyAction::SetCriterionStatus {
                            project_id: "proj".into(),
                            criterion_key: "delivery".into(),
                            status: status.into(),
                        },
                    },
                )
                .unwrap();
            assert_eq!(result.strategy.criteria[0].status, status);
        }
        assert_eq!(db.authority_strategies().unwrap().revision, 5);
    }
    #[test]
    fn setting_progress_signals_does_not_reset_completed_criterion() {
        let mut db = db();
        seed(&mut db);
        db.authority_strategy_command(
            "owner",
            configure("config", 1, vec!["tasks"], vec![criterion("delivery")]),
        )
        .unwrap();
        db.authority_strategy_command(
            "owner",
            StrategyCommand {
                command_id: "complete".into(),
                expected_revision: 2,
                action: StrategyAction::SetCriterionStatus {
                    project_id: "proj".into(),
                    criterion_key: "delivery".into(),
                    status: "satisfied".into(),
                },
            },
        )
        .unwrap();
        let result = db
            .authority_strategy_command(
                "owner",
                StrategyCommand {
                    command_id: "signals".into(),
                    expected_revision: 3,
                    action: StrategyAction::SetProgressSignals {
                        project_id: "proj".into(),
                        progress_signals: vec!["forecast".into()],
                    },
                },
            )
            .unwrap();
        assert_eq!(result.strategy.criteria[0].status, "satisfied");
    }
    #[test]
    fn objective_updates_do_not_reset_completed_criterion() {
        let mut db = db();
        seed(&mut db);
        db.authority_strategy_command(
            "owner",
            configure("config", 1, vec!["tasks"], vec![criterion("delivery")]),
        )
        .unwrap();
        db.authority_strategy_command(
            "owner",
            StrategyCommand {
                command_id: "complete".into(),
                expected_revision: 2,
                action: StrategyAction::SetCriterionStatus {
                    project_id: "proj".into(),
                    criterion_key: "delivery".into(),
                    status: "satisfied".into(),
                },
            },
        )
        .unwrap();
        let result = db
            .authority_strategy_command(
                "owner",
                StrategyCommand {
                    command_id: "objective".into(),
                    expected_revision: 3,
                    action: StrategyAction::SetObjective {
                        project_id: "proj".into(),
                        objective: Some("Updated".into()),
                    },
                },
            )
            .unwrap();
        assert_eq!(result.strategy.criteria[0].status, "satisfied");
        assert_eq!(result.strategy.objective.as_deref(), Some("Updated"));
    }
    #[test]
    fn empty_criteria_are_allowed_for_exploratory_projects() {
        let mut db = db();
        seed(&mut db);
        let result = db
            .authority_strategy_command(
                "owner",
                configure("configure", 1, vec!["milestones"], vec![]),
            )
            .unwrap();
        assert!(result.strategy.criteria.is_empty());
        assert_eq!(result.strategy.mode, "custom");
    }
    #[test]
    fn only_one_logical_command_can_own_a_command_id() {
        let mut db = db();
        seed(&mut db);
        let first = configure("same", 1, vec!["tasks"], vec![criterion("done")]);
        db.authority_strategy_command("owner", first.clone())
            .unwrap();
        let duplicate = db.authority_strategy_command("owner", first).unwrap();
        assert!(duplicate.duplicate);
        let second = configure("same", 1, vec!["forecast"], vec![criterion("done")]);
        assert!(db.authority_strategy_command("owner", second).is_err());
        assert_eq!(db.authority_strategies().unwrap().revision, 2);
    }
    #[test]
    fn missing_criterion_fails_even_when_other_criteria_exist() {
        let mut db = db();
        seed(&mut db);
        db.authority_strategy_command(
            "owner",
            configure(
                "config",
                1,
                vec!["tasks"],
                vec![criterion("first"), criterion("second")],
            ),
        )
        .unwrap();
        let invalid = StrategyCommand {
            command_id: "missing".into(),
            expected_revision: 2,
            action: StrategyAction::SetCriterionStatus {
                project_id: "proj".into(),
                criterion_key: "third".into(),
                status: "satisfied".into(),
            },
        };
        assert!(db.authority_strategy_command("owner", invalid).is_err());
        let snapshot = db.authority_strategies().unwrap();
        assert_eq!(snapshot.revision, 2);
        assert!(
            snapshot.strategies[0]
                .criteria
                .iter()
                .all(|c| c.status == "pending")
        );
    }
}
