//! Server-owned habit and metric evidence. Commands share the canonical
//! authoritative revision, immutable semantic history and idempotency log.
use crate::store::{AuthorityError, AuthorityStore};
use orbit_sync::{ChangeId, EntityKind, EntityRef, FieldMutation, SemanticChange};
use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidenceCommand {
    pub command_id: String,
    pub expected_revision: i64,
    pub action: EvidenceAction,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvidenceAction {
    CreateHabit {
        habit_id: String,
        name: String,
        cadence: String,
    },
    RecordHabit {
        occurrence_id: String,
        habit_id: String,
        occurred_at_ms: i64,
    },
    SetMetricTarget {
        metric_id: String,
        baseline: i64,
        target: i64,
        direction: String,
    },
    RecordMetric {
        metric_id: String,
        observed_at_ms: i64,
        value: i64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceResult {
    pub revision: i64,
    pub duplicate: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HabitRecord {
    pub id: String,
    pub name: String,
    pub cadence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HabitOccurrenceRecord {
    pub id: String,
    pub habit_id: String,
    pub occurred_at_ms: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricRecord {
    pub id: String,
    pub baseline: i64,
    pub target: i64,
    pub direction: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricObservationRecord {
    pub metric_id: String,
    pub observed_at_ms: i64,
    pub value: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceSnapshot {
    pub revision: i64,
    pub history_fingerprint: String,
    pub habits: Vec<HabitRecord>,
    pub habit_occurrences: Vec<HabitOccurrenceRecord>,
    pub metrics: Vec<MetricRecord>,
    pub metric_observations: Vec<MetricObservationRecord>,
}
fn invalid() -> AuthorityError {
    AuthorityError::Invalid("invalid evidence command")
}
fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}
fn field(
    name: &str,
    value: String,
    base: Option<ChangeId>,
) -> Result<FieldMutation, AuthorityError> {
    FieldMutation::new(name, Some(value), base).map_err(|_| invalid())
}
impl AuthorityStore {
    pub fn authority_evidence_snapshot(&self) -> Result<EvidenceSnapshot, AuthorityError> {
        let (_, _, _, _, evidence) = self.authority_full_snapshot()?;
        Ok(evidence)
    }

    pub fn authority_evidence_command(
        &mut self,
        actor: &str,
        command: EvidenceCommand,
    ) -> Result<EvidenceResult, AuthorityError> {
        if actor.trim().is_empty()
            || !valid_id(&command.command_id)
            || command.command_id.len() > 128
            || command.expected_revision < 0
        {
            return Err(invalid());
        }
        let payload = serde_json::to_string(&command).map_err(|_| invalid())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending = crate::command_engine::PendingCommand {
            command_id: &command.command_id,
            actor,
            payload: &payload,
            expected_revision: command.expected_revision,
        };
        let sequence = match pending.check(&tx)? {
            crate::command_engine::CommandAdmission::Duplicate(json) => {
                let mut receipt: EvidenceResult =
                    serde_json::from_str(&json).map_err(|_| invalid())?;
                receipt.duplicate = true;
                return Ok(receipt);
            }
            crate::command_engine::CommandAdmission::New(sequence) => sequence,
        };
        let (kind, id, mutations) = match &command.action {
            EvidenceAction::CreateHabit {
                habit_id,
                name,
                cadence,
            } => {
                if !valid_id(habit_id) || name.trim().is_empty() || name.len() > 4096 {
                    return Err(invalid());
                }
                let cadence_value = if cadence == "daily" {
                    orbit_domain::HabitCadence::Daily
                } else if let Some(weekly) = cadence.strip_prefix("weekly:") {
                    let count = weekly.parse::<u8>().map_err(|_| invalid())?;
                    orbit_domain::HabitCadence::Weekly {
                        target_per_week: count,
                    }
                } else {
                    return Err(invalid());
                };
                let habit = orbit_domain::Habit::new(
                    orbit_domain::HabitId::new(habit_id.clone()).map_err(|_| invalid())?,
                    name.clone(),
                    cadence_value,
                )
                .map_err(|_| invalid())?;
                if tx.execute("INSERT OR IGNORE INTO habits(id,name,cadence_kind,target_per_week,active) VALUES(?1,?2,CASE WHEN ?3='daily' THEN 'daily' ELSE 'weekly' END,CASE WHEN ?3='daily' THEN NULL ELSE CAST(substr(?3,8) AS INTEGER) END,1)",params![habit.id().as_str(),habit.name(),cadence])?!=1 {return Err(AuthorityError::ImmutableConflict);}
                (
                    EntityKind::Habit,
                    habit_id.clone(),
                    vec![
                        field("name", name.clone(), None)?,
                        field("cadence", cadence.clone(), None)?,
                    ],
                )
            }
            EvidenceAction::RecordHabit {
                occurrence_id,
                habit_id,
                occurred_at_ms,
            } => {
                if !valid_id(occurrence_id) || !valid_id(habit_id) || *occurred_at_ms < 0 {
                    return Err(invalid());
                }
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM habits WHERE id=?1)",
                    [habit_id],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(AuthorityError::Invalid("habit does not exist"));
                }
                if tx.execute("INSERT OR IGNORE INTO habit_occurrences(id,habit_id,occurred_at_ms) VALUES(?1,?2,?3)",params![occurrence_id,habit_id,occurred_at_ms])?!=1 {return Err(AuthorityError::ImmutableConflict);}
                (
                    EntityKind::HabitOccurrence,
                    occurrence_id.clone(),
                    vec![
                        field("habit_id", habit_id.clone(), None)?,
                        field("occurred_at_ms", occurred_at_ms.to_string(), None)?,
                    ],
                )
            }
            EvidenceAction::SetMetricTarget {
                metric_id,
                baseline,
                target,
                direction,
            } => {
                if !valid_id(metric_id)
                    || (direction != "increasing" && direction != "decreasing")
                    || (direction == "increasing" && baseline >= target)
                    || (direction == "decreasing" && baseline <= target)
                {
                    return Err(invalid());
                }
                // A target can be created once; subsequent changes require an explicit versioned update contract.
                if tx.execute("INSERT OR IGNORE INTO metric_targets(metric_id,baseline,target,direction) VALUES(?1,?2,?3,CASE WHEN ?4='increasing' THEN 'at_least' ELSE 'at_most' END)",params![metric_id,baseline,target,direction])?!=1 {return Err(AuthorityError::ImmutableConflict);}
                (
                    EntityKind::Metric,
                    metric_id.clone(),
                    vec![
                        field("baseline", baseline.to_string(), None)?,
                        field("target", target.to_string(), None)?,
                        field("direction", direction.clone(), None)?,
                    ],
                )
            }
            EvidenceAction::RecordMetric {
                metric_id,
                observed_at_ms,
                value,
            } => {
                if !valid_id(metric_id) || *observed_at_ms < 0 {
                    return Err(invalid());
                }
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM metric_targets WHERE metric_id=?1)",
                    [metric_id],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(AuthorityError::Invalid("metric target does not exist"));
                }
                if tx.execute("INSERT OR IGNORE INTO metric_observations(metric_id,observed_at_ms,value) VALUES(?1,?2,?3)",params![metric_id,observed_at_ms,value])?!=1 {return Err(AuthorityError::ImmutableConflict);}
                // Use distinct observation identity to preserve historical measurements.
                (
                    EntityKind::Metric,
                    format!("{metric_id}:observation:{observed_at_ms}"),
                    vec![
                        field("metric_id", metric_id.clone(), None)?,
                        field("observed_at_ms", observed_at_ms.to_string(), None)?,
                        field("value", value.to_string(), None)?,
                    ],
                )
            }
        };
        let change =
            SemanticChange::new(EntityRef::new(kind, id).map_err(|_| invalid())?, mutations)
                .map_err(|_| invalid())?;
        let receipt = EvidenceResult {
            revision: sequence,
            duplicate: false,
        };
        let json = serde_json::to_string(&receipt).map_err(|_| invalid())?;
        pending.commit(&tx, sequence, change, &json)?;
        tx.commit()?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn command(id: &str, rev: i64, action: EvidenceAction) -> EvidenceCommand {
        EvidenceCommand {
            command_id: id.into(),
            expected_revision: rev,
            action,
        }
    }
    #[test]
    fn habit_and_metric_evidence_are_authoritative_and_retry_safe() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let habit = command(
            "habit-1",
            0,
            EvidenceAction::CreateHabit {
                habit_id: "h1".into(),
                name: "Read".into(),
                cadence: "daily".into(),
            },
        );
        assert_eq!(
            store
                .authority_evidence_command("device-a", habit.clone())
                .unwrap()
                .revision,
            1
        );
        assert!(
            store
                .authority_evidence_command("device-a", habit.clone())
                .unwrap()
                .duplicate
        );
        assert!(matches!(
            store.authority_evidence_command("device-b", habit),
            Err(AuthorityError::ImmutableConflict)
        ));
        let observed = command(
            "occ-1",
            1,
            EvidenceAction::RecordHabit {
                occurrence_id: "o1".into(),
                habit_id: "h1".into(),
                occurred_at_ms: 100,
            },
        );
        assert_eq!(
            store
                .authority_evidence_command("device-a", observed)
                .unwrap()
                .revision,
            2
        );
        assert!(matches!(
            store.authority_evidence_command(
                "device-a",
                command(
                    "bad",
                    2,
                    EvidenceAction::RecordMetric {
                        metric_id: "unknown".into(),
                        observed_at_ms: 200,
                        value: 1
                    }
                )
            ),
            Err(AuthorityError::Invalid(_))
        ));
        let metric = command(
            "metric-1",
            2,
            EvidenceAction::SetMetricTarget {
                metric_id: "m1".into(),
                baseline: 0,
                target: 10,
                direction: "increasing".into(),
            },
        );
        assert_eq!(
            store
                .authority_evidence_command("device-a", metric)
                .unwrap()
                .revision,
            3
        );
        assert_eq!(
            store
                .authority_evidence_command(
                    "device-a",
                    command(
                        "obs-1",
                        3,
                        EvidenceAction::RecordMetric {
                            metric_id: "m1".into(),
                            observed_at_ms: 200,
                            value: 4
                        }
                    )
                )
                .unwrap()
                .revision,
            4
        );
        assert!(matches!(
            store.authority_evidence_command(
                "device-a",
                command(
                    "stale",
                    1,
                    EvidenceAction::RecordMetric {
                        metric_id: "m1".into(),
                        observed_at_ms: 201,
                        value: 5
                    }
                )
            ),
            Err(AuthorityError::StaleRevision)
        ));
        let snapshot = store.authority_evidence_snapshot().unwrap();
        assert_eq!(snapshot.revision, 4);
        assert_eq!(snapshot.habits.len(), 1);
        assert_eq!(snapshot.habit_occurrences.len(), 1);
        assert_eq!(snapshot.metrics.len(), 1);
        assert_eq!(snapshot.metric_observations.len(), 1);
        let (rev, fingerprint, projects, tasks, combined) =
            store.authority_full_snapshot().unwrap();
        assert_eq!(rev, 4);
        assert!(projects.is_empty() && tasks.is_empty());
        assert_eq!(fingerprint, snapshot.history_fingerprint);
        assert_eq!(combined.habits, snapshot.habits);
        assert_eq!(combined.metric_observations, snapshot.metric_observations);

        assert_eq!(store.event_count().unwrap(), 4);
    }
    #[test]
    fn invalid_cadence_does_not_advance_revision() {
        let mut store = AuthorityStore::open(Path::new(":memory:")).unwrap();
        let bad = command(
            "bad",
            0,
            EvidenceAction::CreateHabit {
                habit_id: "h1".into(),
                name: "Run".into(),
                cadence: "weekly:8".into(),
            },
        );
        assert!(matches!(
            store.authority_evidence_command("device-a", bad),
            Err(AuthorityError::Invalid(_))
        ));
        assert_eq!(store.event_count().unwrap(), 0);
    }
}
