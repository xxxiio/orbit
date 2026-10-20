//! Deterministic server-owned semantic projection over durable relay history.
//!
//! A projection is complete only when all change and resolution dependencies
//! can be replayed. It does not mistake relay receipt for canonical acceptance.
use orbit_sync::{
    ChangeEnvelope, ConflictResolution, ImportOutcome, ReplicaSnapshot, ReplicaState,
};
use serde::Serialize;

use crate::store::AuthorityEvent;

#[derive(Debug, Serialize)]
pub struct CanonicalView {
    pub authority_revision: i64,
    pub applied_events: usize,
    pub pending_events: usize,
    pub unresolved_conflicts: usize,
    pub complete: bool,
    pub snapshot: ReplicaSnapshot,
}

enum Record {
    Change(ChangeEnvelope),
    Resolution(ConflictResolution),
}

/// Validate wire identity without mutating relay state.
pub fn decode_and_identify(
    device: &str,
    kind: &str,
    wire: &str,
) -> Result<(String, u64), crate::store::AuthorityError> {
    use crate::store::AuthorityError;
    let (author, sequence) = match kind {
        "change" => {
            let change = ChangeEnvelope::from_wire_json(wire)
                .map_err(|_| AuthorityError::Invalid("invalid change"))?;
            (
                change.id.device_id().as_str().to_owned(),
                change.id.sequence(),
            )
        }
        "resolution" => {
            let resolution = ConflictResolution::from_wire_json(wire)
                .map_err(|_| AuthorityError::Invalid("invalid resolution"))?;
            (
                resolution.resolved_by.device_id().as_str().to_owned(),
                resolution.resolved_by.sequence(),
            )
        }
        _ => return Err(AuthorityError::Invalid("invalid kind")),
    };
    if author != device {
        return Err(AuthorityError::Unauthorized);
    }
    Ok((author, sequence))
}

/// Rebuild from committed relay records, including out-of-order arrivals. A
/// missing dependency remains pending rather than being silently discarded.
pub fn project(events: &[AuthorityEvent]) -> Result<CanonicalView, &'static str> {
    let mut remaining = Vec::with_capacity(events.len());
    for event in events {
        let record = match event.kind.as_str() {
            "change" => Record::Change(
                ChangeEnvelope::from_wire_json(&event.wire)
                    .map_err(|_| "invalid stored semantic change")?,
            ),
            "resolution" => Record::Resolution(
                ConflictResolution::from_wire_json(&event.wire)
                    .map_err(|_| "invalid stored semantic resolution")?,
            ),
            _ => return Err("invalid stored record kind"),
        };
        remaining.push(record);
    }
    let mut replica = ReplicaState::default();
    let mut applied = 0;
    loop {
        let mut deferred = Vec::new();
        let mut progressed = false;
        for record in remaining {
            match record {
                Record::Change(change) => match replica.import(change.clone()) {
                    ImportOutcome::Applied
                    | ImportOutcome::AppliedWithConflicts(_)
                    | ImportOutcome::Duplicate => {
                        progressed = true;
                        applied += 1;
                    }
                    ImportOutcome::DeferredSequenceGap { .. } => {
                        deferred.push(Record::Change(change))
                    }
                },
                Record::Resolution(resolution) => {
                    if replica.resolve_conflict(resolution.clone()).is_ok() {
                        progressed = true;
                        applied += 1;
                    } else {
                        deferred.push(Record::Resolution(resolution));
                    }
                }
            }
        }
        if !progressed || deferred.is_empty() {
            let pending = deferred.len();
            return Ok(CanonicalView {
                authority_revision: events.last().map_or(0, |e| e.cursor),
                applied_events: applied,
                pending_events: pending,
                unresolved_conflicts: replica.unresolved_conflicts().count(),
                complete: pending == 0,
                snapshot: replica.snapshot(),
            });
        }
        remaining = deferred;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_sync::{ChangeId, DeviceId, EntityKind, EntityRef, FieldMutation, SemanticChange};

    fn event(seq: u64) -> AuthorityEvent {
        let change = ChangeEnvelope::new(
            ChangeId::new(DeviceId::new("device-a").unwrap(), seq).unwrap(),
            10,
            SemanticChange::new(
                EntityRef::new(EntityKind::Task, "task-1").unwrap(),
                vec![
                    FieldMutation::new(format!("field-{seq}"), Some("value".into()), None).unwrap(),
                ],
            )
            .unwrap(),
        );
        AuthorityEvent {
            cursor: seq as i64,
            device_id: "device-a".into(),
            sequence: seq,
            kind: "change".into(),
            wire: change.to_wire_json().unwrap(),
        }
    }

    #[test]
    fn out_of_order_history_becomes_complete() {
        let view = project(&[event(2), event(1)]).unwrap();
        assert!(view.complete);
        assert_eq!(view.applied_events, 2);
        assert_eq!(view.pending_events, 0);
        assert_eq!(view.snapshot.fields.len(), 2);
    }
    #[test]
    fn missing_predecessor_never_looks_complete() {
        let view = project(&[event(2)]).unwrap();
        assert!(!view.complete);
        assert_eq!(view.pending_events, 1);
        assert!(view.snapshot.fields.is_empty());
    }
}

#[cfg(test)]
mod authoritative_gate_tests {
    use super::*;
    use crate::store::{AuthorityError, AuthorityStore};
    use orbit_sync::{ChangeId, DeviceId, EntityKind, EntityRef, FieldMutation, SemanticChange};

    fn wire(seq: u64) -> String {
        ChangeEnvelope::new(
            ChangeId::new(DeviceId::new("device-a").unwrap(), seq).unwrap(),
            10,
            SemanticChange::new(
                EntityRef::new(EntityKind::Task, "task-1").unwrap(),
                vec![
                    FieldMutation::new(format!("field-{seq}"), Some("value".into()), None).unwrap(),
                ],
            )
            .unwrap(),
        )
        .to_wire_json()
        .unwrap()
    }

    #[test]
    fn strict_commit_rejects_stale_and_missing_predecessors_without_writes() {
        let path = std::env::temp_dir().join(format!(
            "orbit-m18-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let mut store = AuthorityStore::open(&path).unwrap();
            store.enroll("device-a", &"s".repeat(32)).unwrap();
            assert!(matches!(
                store.commit_semantic("device-a", 0, "change", &wire(2)),
                Err(AuthorityError::IncompleteProjection)
            ));
            assert_eq!(store.tip().unwrap(), 0);
            assert!(
                !store
                    .commit_semantic("device-a", 0, "change", &wire(1))
                    .unwrap()
                    .duplicate
            );
            assert_eq!(store.tip().unwrap(), 1);
            assert!(matches!(
                store.commit_semantic("device-a", 0, "change", &wire(2)),
                Err(AuthorityError::StaleRevision)
            ));
            assert_eq!(store.tip().unwrap(), 1);
            assert!(
                !store
                    .commit_semantic("device-a", 1, "change", &wire(2))
                    .unwrap()
                    .duplicate
            );
            assert_eq!(store.tip().unwrap(), 2);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
