//! Transport-independent semantic sync primitives for Orbit Core.
//!
//! Sync exchanges immutable semantic changes. The live SQLite database file is
//! never a sync unit and transport adapters (Git/cloud/LAN) must not infer state
//! from database-file diffs.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MIN_SUPPORTED_WIRE_VERSION: u32 = 1;
pub const SYNC_WIRE_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    EmptyDeviceId,
    InvalidSequence,
    EmptyEntityId,
    EmptyFieldName,
    DuplicateFieldMutation,
    UnsupportedWireVersion { received: u32, supported: u32 },
    InvalidWire(String),
    ConflictNotFound,
    ConflictAlreadyResolved,
    ResolutionSequenceGap { expected: u64, received: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> Result<Self, SyncError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(SyncError::EmptyDeviceId);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChangeId {
    device_id: DeviceId,
    sequence: u64,
}

impl ChangeId {
    pub fn new(device_id: DeviceId, sequence: u64) -> Result<Self, SyncError> {
        if sequence == 0 {
            return Err(SyncError::InvalidSequence);
        }
        Ok(Self {
            device_id,
            sequence,
        })
    }
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Project,
    Milestone,
    Task,
    Habit,
    HabitOccurrence,
    Routine,
    KnowledgeItem,
    KnowledgeResource,
    ScheduleBlock,
    PlanApplication,
    Metric,
    Attachment,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
}

impl EntityRef {
    pub fn new(kind: EntityKind, id: impl Into<String>) -> Result<Self, SyncError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(SyncError::EmptyEntityId);
        }
        Ok(Self { kind, id })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMutation {
    pub field: String,
    /// `None` represents semantic deletion/unset of the field.
    pub value: Option<String>,
    /// Field version observed by the author. Different fields can therefore
    /// merge independently while concurrent writes to the same field conflict.
    pub base: Option<ChangeId>,
}

impl FieldMutation {
    pub fn new(
        field: impl Into<String>,
        value: Option<String>,
        base: Option<ChangeId>,
    ) -> Result<Self, SyncError> {
        let field = field.into();
        if field.trim().is_empty() {
            return Err(SyncError::EmptyFieldName);
        }
        Ok(Self { field, value, base })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticChange {
    pub entity: EntityRef,
    pub mutations: Vec<FieldMutation>,
}

impl SemanticChange {
    pub fn new(entity: EntityRef, mut mutations: Vec<FieldMutation>) -> Result<Self, SyncError> {
        mutations.sort_by(|a, b| a.field.cmp(&b.field));
        let mut fields = BTreeSet::new();
        for mutation in &mutations {
            if !fields.insert(mutation.field.clone()) {
                return Err(SyncError::DuplicateFieldMutation);
            }
        }
        Ok(Self { entity, mutations })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEnvelope {
    pub id: ChangeId,
    pub recorded_at_ms: i64,
    pub change: SemanticChange,
}

impl ChangeEnvelope {
    pub fn new(id: ChangeId, recorded_at_ms: i64, change: SemanticChange) -> Self {
        Self {
            id,
            recorded_at_ms,
            change,
        }
    }

    pub fn to_wire_json(&self) -> Result<String, SyncError> {
        serde_json::to_string(&VersionedEnvelopeRef {
            version: SYNC_WIRE_VERSION,
            envelope: self,
        })
        .map_err(|error| SyncError::InvalidWire(error.to_string()))
    }

    pub fn from_wire_json(input: &str) -> Result<Self, SyncError> {
        let value: serde_json::Value = serde_json::from_str(input)
            .map_err(|error| SyncError::InvalidWire(error.to_string()))?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| SyncError::InvalidWire("missing integer wire version".into()))?
            as u32;
        ensure_supported_wire_version(version)?;
        let wire: VersionedEnvelope = serde_json::from_value(value)
            .map_err(|error| SyncError::InvalidWire(error.to_string()))?;
        Ok(wire.envelope)
    }
}

#[derive(Serialize)]
struct VersionedEnvelopeRef<'a> {
    version: u32,
    envelope: &'a ChangeEnvelope,
}

#[derive(Deserialize)]
struct VersionedEnvelope {
    #[allow(dead_code)]
    version: u32,
    envelope: ChangeEnvelope,
}

fn ensure_supported_wire_version(version: u32) -> Result<(), SyncError> {
    if !(MIN_SUPPORTED_WIRE_VERSION..=SYNC_WIRE_VERSION).contains(&version) {
        return Err(SyncError::UnsupportedWireVersion {
            received: version,
            supported: SYNC_WIRE_VERSION,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedValue {
    pub value: Option<String>,
    pub change_id: ChangeId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConflictId {
    pub entity: EntityRef,
    pub field: String,
    pub local_change: ChangeId,
    pub incoming_change: ChangeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub id: ConflictId,
    pub entity: EntityRef,
    pub field: String,
    pub local: VersionedValue,
    pub incoming_value: Option<String>,
    pub incoming_change: ChangeId,
    pub incoming_base: Option<ChangeId>,
}

impl SyncConflict {
    fn new(
        entity: EntityRef,
        field: String,
        local: VersionedValue,
        incoming_value: Option<String>,
        incoming_change: ChangeId,
        incoming_base: Option<ChangeId>,
    ) -> Self {
        let id = ConflictId {
            entity: entity.clone(),
            field: field.clone(),
            local_change: local.change_id.clone(),
            incoming_change: incoming_change.clone(),
        };
        Self {
            id,
            entity,
            field,
            local,
            incoming_value,
            incoming_change,
            incoming_base,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictResolution {
    pub conflict_id: ConflictId,
    pub resolved_by: ChangeId,
    pub resolved_at_ms: i64,
    /// Chosen semantic value. `None` means the field is intentionally unset.
    pub value: Option<String>,
}

impl ConflictResolution {
    pub fn new(
        conflict_id: ConflictId,
        resolved_by: ChangeId,
        resolved_at_ms: i64,
        value: Option<String>,
    ) -> Self {
        Self {
            conflict_id,
            resolved_by,
            resolved_at_ms,
            value,
        }
    }

    pub fn to_wire_json(&self) -> Result<String, SyncError> {
        serde_json::to_string(&VersionedResolutionRef {
            version: SYNC_WIRE_VERSION,
            resolution: self,
        })
        .map_err(|error| SyncError::InvalidWire(error.to_string()))
    }

    pub fn from_wire_json(input: &str) -> Result<Self, SyncError> {
        let value: serde_json::Value = serde_json::from_str(input)
            .map_err(|error| SyncError::InvalidWire(error.to_string()))?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| SyncError::InvalidWire("missing integer wire version".into()))?
            as u32;
        ensure_supported_wire_version(version)?;
        let wire: VersionedResolution = serde_json::from_value(value)
            .map_err(|error| SyncError::InvalidWire(error.to_string()))?;
        Ok(wire.resolution)
    }
}

#[derive(Serialize)]
struct VersionedResolutionRef<'a> {
    version: u32,
    resolution: &'a ConflictResolution,
}

#[derive(Deserialize)]
struct VersionedResolution {
    #[allow(dead_code)]
    version: u32,
    resolution: ConflictResolution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    Applied,
    AppliedWithConflicts(Vec<SyncConflict>),
    Duplicate,
    DeferredSequenceGap { expected: u64, received: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    pub version: u32,
    /// Highest contiguous source sequence materialized for each device.
    pub max_sequence_by_device: BTreeMap<DeviceId, u64>,
}

impl SyncCheckpoint {
    pub fn to_wire_json(&self) -> Result<String, SyncError> {
        serde_json::to_string(self).map_err(|error| SyncError::InvalidWire(error.to_string()))
    }

    pub fn from_wire_json(input: &str) -> Result<Self, SyncError> {
        let checkpoint: Self = serde_json::from_str(input)
            .map_err(|error| SyncError::InvalidWire(error.to_string()))?;
        ensure_supported_wire_version(checkpoint.version)?;
        Ok(checkpoint)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaSnapshot {
    pub version: u32,
    pub applied: Vec<ChangeId>,
    pub max_sequence_by_device: BTreeMap<DeviceId, u64>,
    pub fields: Vec<(EntityRef, String, VersionedValue)>,
    pub conflicts: Vec<SyncConflict>,
    pub resolutions: Vec<ConflictResolution>,
}

impl ReplicaSnapshot {
    pub fn to_wire_json(&self) -> Result<String, SyncError> {
        serde_json::to_string(self).map_err(|error| SyncError::InvalidWire(error.to_string()))
    }

    pub fn from_wire_json(input: &str) -> Result<Self, SyncError> {
        let snapshot: Self = serde_json::from_str(input)
            .map_err(|error| SyncError::InvalidWire(error.to_string()))?;
        ensure_supported_wire_version(snapshot.version)?;
        Ok(snapshot)
    }
}

/// Small deterministic replica ledger used by transports/materializers.
///
/// It intentionally operates at semantic field granularity. Independent fields
/// merge automatically; competing writes to the same observed version produce
/// explicit conflicts rather than line-level or last-write-wins resolution.
#[derive(Debug, Clone, Default)]
pub struct ReplicaState {
    applied: BTreeSet<ChangeId>,
    max_sequence_by_device: BTreeMap<DeviceId, u64>,
    fields: BTreeMap<(EntityRef, String), VersionedValue>,
    conflicts: BTreeMap<ConflictId, SyncConflict>,
    resolutions: BTreeMap<ConflictId, ConflictResolution>,
}

impl ReplicaState {
    pub fn has_applied(&self, id: &ChangeId) -> bool {
        self.applied.contains(id)
    }
    pub fn conflicts(&self) -> impl Iterator<Item = &SyncConflict> {
        self.conflicts.values()
    }
    pub fn resolutions(&self) -> impl Iterator<Item = &ConflictResolution> {
        self.resolutions.values()
    }

    pub fn unresolved_conflicts(&self) -> impl Iterator<Item = &SyncConflict> {
        self.conflicts
            .values()
            .filter(|conflict| !self.resolutions.contains_key(&conflict.id))
    }

    pub fn field(&self, entity: &EntityRef, field: &str) -> Option<&VersionedValue> {
        self.fields.get(&(entity.clone(), field.to_owned()))
    }

    pub fn fields(&self) -> impl Iterator<Item = (&EntityRef, &str, &VersionedValue)> {
        self.fields
            .iter()
            .map(|((entity, field), value)| (entity, field.as_str(), value))
    }

    pub fn has_unresolved_conflict(&self, entity: &EntityRef, field: &str) -> bool {
        self.unresolved_conflicts()
            .any(|conflict| &conflict.entity == entity && conflict.field == field)
    }

    pub fn checkpoint(&self) -> SyncCheckpoint {
        SyncCheckpoint {
            version: SYNC_WIRE_VERSION,
            max_sequence_by_device: self.max_sequence_by_device.clone(),
        }
    }

    pub fn snapshot(&self) -> ReplicaSnapshot {
        ReplicaSnapshot {
            version: SYNC_WIRE_VERSION,
            applied: self.applied.iter().cloned().collect(),
            max_sequence_by_device: self.max_sequence_by_device.clone(),
            fields: self
                .fields
                .iter()
                .map(|((entity, field), value)| (entity.clone(), field.clone(), value.clone()))
                .collect(),
            conflicts: self.conflicts.values().cloned().collect(),
            resolutions: self.resolutions.values().cloned().collect(),
        }
    }

    pub fn from_snapshot(snapshot: ReplicaSnapshot) -> Result<Self, SyncError> {
        ensure_supported_wire_version(snapshot.version)?;
        Ok(Self {
            applied: snapshot.applied.into_iter().collect(),
            max_sequence_by_device: snapshot.max_sequence_by_device,
            fields: snapshot
                .fields
                .into_iter()
                .map(|(entity, field, value)| ((entity, field), value))
                .collect(),
            conflicts: snapshot
                .conflicts
                .into_iter()
                .map(|conflict| (conflict.id.clone(), conflict))
                .collect(),
            resolutions: snapshot
                .resolutions
                .into_iter()
                .map(|resolution| (resolution.conflict_id.clone(), resolution))
                .collect(),
        })
    }

    pub fn import(&mut self, envelope: ChangeEnvelope) -> ImportOutcome {
        if self.applied.contains(&envelope.id) {
            return ImportOutcome::Duplicate;
        }

        let expected = self
            .max_sequence_by_device
            .get(envelope.id.device_id())
            .copied()
            .unwrap_or(0)
            + 1;
        if envelope.id.sequence() != expected {
            return ImportOutcome::DeferredSequenceGap {
                expected,
                received: envelope.id.sequence(),
            };
        }

        let mut new_conflicts = Vec::new();
        for mutation in &envelope.change.mutations {
            let key = (envelope.change.entity.clone(), mutation.field.clone());
            match self.fields.get(&key) {
                None if mutation.base.is_some() => {
                    let synthetic_local = VersionedValue {
                        value: None,
                        change_id: mutation.base.clone().expect("checked above"),
                    };
                    let conflict = SyncConflict::new(
                        envelope.change.entity.clone(),
                        mutation.field.clone(),
                        synthetic_local,
                        mutation.value.clone(),
                        envelope.id.clone(),
                        mutation.base.clone(),
                    );
                    self.conflicts
                        .entry(conflict.id.clone())
                        .or_insert_with(|| conflict.clone());
                    new_conflicts.push(conflict);
                }
                Some(local) if mutation.base.as_ref() != Some(&local.change_id) => {
                    let conflict = SyncConflict::new(
                        envelope.change.entity.clone(),
                        mutation.field.clone(),
                        local.clone(),
                        mutation.value.clone(),
                        envelope.id.clone(),
                        mutation.base.clone(),
                    );
                    self.conflicts
                        .entry(conflict.id.clone())
                        .or_insert_with(|| conflict.clone());
                    new_conflicts.push(conflict);
                }
                _ => {
                    self.fields.insert(
                        key,
                        VersionedValue {
                            value: mutation.value.clone(),
                            change_id: envelope.id.clone(),
                        },
                    );
                }
            }
        }

        self.max_sequence_by_device
            .insert(envelope.id.device_id().clone(), envelope.id.sequence());
        self.applied.insert(envelope.id.clone());
        if new_conflicts.is_empty() {
            ImportOutcome::Applied
        } else {
            ImportOutcome::AppliedWithConflicts(new_conflicts)
        }
    }

    pub fn resolve_conflict(&mut self, resolution: ConflictResolution) -> Result<(), SyncError> {
        let conflict = self
            .conflicts
            .get(&resolution.conflict_id)
            .ok_or(SyncError::ConflictNotFound)?;
        if self.resolutions.contains_key(&resolution.conflict_id) {
            return Err(SyncError::ConflictAlreadyResolved);
        }

        let expected = self
            .max_sequence_by_device
            .get(resolution.resolved_by.device_id())
            .copied()
            .unwrap_or(0)
            + 1;
        if resolution.resolved_by.sequence() != expected {
            return Err(SyncError::ResolutionSequenceGap {
                expected,
                received: resolution.resolved_by.sequence(),
            });
        }

        self.fields.insert(
            (conflict.entity.clone(), conflict.field.clone()),
            VersionedValue {
                value: resolution.value.clone(),
                change_id: resolution.resolved_by.clone(),
            },
        );
        self.max_sequence_by_device.insert(
            resolution.resolved_by.device_id().clone(),
            resolution.resolved_by.sequence(),
        );
        self.applied.insert(resolution.resolved_by.clone());
        self.resolutions
            .insert(resolution.conflict_id.clone(), resolution);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }
    fn cid(device_id: &str, sequence: u64) -> ChangeId {
        ChangeId::new(device(device_id), sequence).unwrap()
    }
    fn task() -> EntityRef {
        EntityRef::new(EntityKind::Task, "task-1").unwrap()
    }
    fn title_change(
        device_id: &str,
        sequence: u64,
        value: &str,
        base: Option<ChangeId>,
    ) -> ChangeEnvelope {
        ChangeEnvelope::new(
            cid(device_id, sequence),
            10,
            SemanticChange::new(
                task(),
                vec![FieldMutation::new("title", Some(value.into()), base).unwrap()],
            )
            .unwrap(),
        )
    }

    #[test]
    fn repeated_import_is_idempotent() {
        let envelope = title_change("a", 1, "A", None);
        let mut state = ReplicaState::default();
        assert_eq!(state.import(envelope.clone()), ImportOutcome::Applied);
        assert_eq!(state.import(envelope), ImportOutcome::Duplicate);
    }

    #[test]
    fn different_fields_merge_without_conflict() {
        let mut state = ReplicaState::default();
        state.import(title_change("a", 1, "A", None));
        let outcome = state.import(ChangeEnvelope::new(
            cid("b", 1),
            11,
            SemanticChange::new(
                task(),
                vec![FieldMutation::new("status", Some("ready".into()), None).unwrap()],
            )
            .unwrap(),
        ));
        assert_eq!(outcome, ImportOutcome::Applied);
    }

    #[test]
    fn concurrent_same_field_write_is_explicit_conflict() {
        let mut state = ReplicaState::default();
        state.import(title_change("a", 1, "A", None));
        let outcome = state.import(title_change("b", 1, "B", None));
        assert!(matches!(outcome, ImportOutcome::AppliedWithConflicts(_)));
        assert_eq!(
            state.field(&task(), "title").unwrap().value.as_deref(),
            Some("A")
        );
        assert_eq!(state.unresolved_conflicts().count(), 1);
    }

    #[test]
    fn per_device_sequence_gaps_fail_closed() {
        let mut state = ReplicaState::default();
        let outcome = state.import(ChangeEnvelope::new(
            cid("a", 2),
            10,
            SemanticChange::new(task(), vec![]).unwrap(),
        ));
        assert_eq!(
            outcome,
            ImportOutcome::DeferredSequenceGap {
                expected: 1,
                received: 2
            }
        );
        assert!(!state.has_applied(&cid("a", 2)));
    }

    #[test]
    fn resolution_is_a_new_version_and_is_auditable() {
        let mut state = ReplicaState::default();
        state.import(title_change("a", 1, "A", None));
        let outcome = state.import(title_change("b", 1, "B", None));
        let conflict = match outcome {
            ImportOutcome::AppliedWithConflicts(conflicts) => conflicts.into_iter().next().unwrap(),
            other => panic!("expected conflict, got {other:?}"),
        };

        let resolution =
            ConflictResolution::new(conflict.id.clone(), cid("a", 2), 12, Some("B".into()));
        state.resolve_conflict(resolution.clone()).unwrap();

        assert_eq!(
            state.field(&task(), "title").unwrap().value.as_deref(),
            Some("B")
        );
        assert_eq!(
            state.field(&task(), "title").unwrap().change_id,
            cid("a", 2)
        );
        assert_eq!(state.unresolved_conflicts().count(), 0);
        assert_eq!(state.resolutions().next(), Some(&resolution));
    }

    #[test]
    fn same_conflict_cannot_be_resolved_twice() {
        let mut state = ReplicaState::default();
        state.import(title_change("a", 1, "A", None));
        let conflict = match state.import(title_change("b", 1, "B", None)) {
            ImportOutcome::AppliedWithConflicts(conflicts) => conflicts.into_iter().next().unwrap(),
            other => panic!("expected conflict, got {other:?}"),
        };
        state
            .resolve_conflict(ConflictResolution::new(
                conflict.id.clone(),
                cid("a", 2),
                12,
                Some("A".into()),
            ))
            .unwrap();
        let err = state
            .resolve_conflict(ConflictResolution::new(
                conflict.id,
                cid("a", 3),
                13,
                Some("B".into()),
            ))
            .unwrap_err();
        assert_eq!(err, SyncError::ConflictAlreadyResolved);
    }

    #[test]
    fn canonical_wire_serialization_is_stable_and_round_trips() {
        let envelope = ChangeEnvelope::new(
            cid("a", 1),
            10,
            SemanticChange::new(
                task(),
                vec![
                    FieldMutation::new("z", Some("last".into()), None).unwrap(),
                    FieldMutation::new("a", Some("first".into()), None).unwrap(),
                ],
            )
            .unwrap(),
        );
        let first = envelope.to_wire_json().unwrap();
        let second = envelope.to_wire_json().unwrap();
        assert_eq!(first, second);
        assert!(
            first.find("\\\"field\\\":\\\"a\\\"").is_none(),
            "JSON is not double encoded"
        );
        assert!(first.find("\"field\":\"a\"").unwrap() < first.find("\"field\":\"z\"").unwrap());
        assert_eq!(ChangeEnvelope::from_wire_json(&first).unwrap(), envelope);
    }

    #[test]
    fn future_wire_versions_fail_closed() {
        let json = r#"{"version":4,"envelope":{"id":{"device_id":"a","sequence":1},"recorded_at_ms":10,"change":{"entity":{"kind":"task","id":"task-1"},"mutations":[]}}}"#;
        assert_eq!(
            ChangeEnvelope::from_wire_json(json).unwrap_err(),
            SyncError::UnsupportedWireVersion {
                received: 4,
                supported: 3
            },
        );
    }

    #[test]
    fn version_one_envelopes_remain_readable_after_knowledge_extension() {
        let json = r#"{"version":1,"envelope":{"id":{"device_id":"a","sequence":1},"recorded_at_ms":10,"change":{"entity":{"kind":"task","id":"task-1"},"mutations":[]}}}"#;
        let parsed = ChangeEnvelope::from_wire_json(json).unwrap();
        assert_eq!(parsed.id.sequence(), 1);
    }

    #[test]
    fn version_two_serializes_knowledge_entity_kind() {
        let envelope = ChangeEnvelope::new(
            cid("a", 1),
            10,
            SemanticChange::new(
                EntityRef::new(EntityKind::KnowledgeItem, "knowledge-1").unwrap(),
                vec![FieldMutation::new("title", Some("Decision".into()), None).unwrap()],
            )
            .unwrap(),
        );
        let json = envelope.to_wire_json().unwrap();
        assert!(json.contains("\"version\":3"));
        assert!(json.contains("\"kind\":\"knowledge_item\""));
        assert_eq!(ChangeEnvelope::from_wire_json(&json).unwrap(), envelope);
    }

    #[test]
    fn attachment_entity_kind_serializes_and_round_trips() {
        let envelope = ChangeEnvelope::new(
            cid("a", 1),
            10,
            SemanticChange::new(
                EntityRef::new(EntityKind::Attachment, "attachment-1").unwrap(),
                vec![FieldMutation::new("rich_state", Some("state".into()), None).unwrap()],
            )
            .unwrap(),
        );
        let json = envelope.to_wire_json().unwrap();
        assert!(json.contains("\"kind\":\"attachment\""));
        assert_eq!(ChangeEnvelope::from_wire_json(&json).unwrap(), envelope);
    }

    #[test]
    fn conflict_resolution_wire_round_trip_is_versioned() {
        let mut state = ReplicaState::default();
        state.import(title_change("a", 1, "A", None));
        let conflict = match state.import(title_change("b", 1, "B", None)) {
            ImportOutcome::AppliedWithConflicts(conflicts) => conflicts.into_iter().next().unwrap(),
            other => panic!("expected conflict, got {other:?}"),
        };
        let resolution = ConflictResolution::new(conflict.id, cid("a", 2), 12, Some("B".into()));
        let json = resolution.to_wire_json().unwrap();
        assert!(json.contains("\"version\":3"));
        assert_eq!(
            ConflictResolution::from_wire_json(&json).unwrap(),
            resolution
        );
    }

    #[test]
    fn snapshot_round_trip_preserves_conflict_and_checkpoint_state() {
        let mut state = ReplicaState::default();
        state.import(title_change("a", 1, "A", None));
        state.import(title_change("b", 1, "B", None));
        let snapshot_json = state.snapshot().to_wire_json().unwrap();
        let restored =
            ReplicaState::from_snapshot(ReplicaSnapshot::from_wire_json(&snapshot_json).unwrap())
                .unwrap();
        assert_eq!(restored.checkpoint(), state.checkpoint());
        assert_eq!(
            restored.field(&task(), "title"),
            state.field(&task(), "title")
        );
        assert_eq!(restored.unresolved_conflicts().count(), 1);
    }
}
