//! SQLite-backed local persistence for Orbit Core.
//!
//! The SQLite database is a local materialized state store. It is deliberately
//! not a sync unit: future sync operates on semantic change records instead of
//! copying the live database file.

use orbit_application::{
    ActivityRepository, ApplicationError, HabitOccurrenceRepository, HabitRepository,
    MetricEvidenceRepository, MetricObservation, MetricTargetRecord, MilestoneRepository,
    PendingConflictResolution, PendingSemanticChange, ProjectEvidenceBinding,
    ProjectEvidenceRepository, ProjectRepository, RepositoryError, ReservedConflictResolution,
    ReservedSemanticChange, RoutineRepository, ScheduleRepository, SemanticEntityKind,
    SemanticFieldMutation, SyncFieldVersion, SyncOutboxRepository, TaskRepository, UnitOfWork,
};
use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Habit, HabitCadence,
    HabitId, HabitOccurrence, HabitOccurrenceId, KnowledgeItemId, Milestone, MilestoneId,
    MilestoneStatus, Project, ProjectId, ProjectMode, ProjectProgressSignal, ProjectStatus,
    ProjectSuccessCriterion, Routine, RoutineId, ScheduleBlock, ScheduleBlockId,
    ScheduleFlexibility, ScheduleTarget, SuccessCriterionStatus, Task, TaskId, TaskPlanning,
    TaskScope, TaskStatus,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

pub mod agent_jobs;
mod agent_launch_port;
mod agent_observation_port;
pub mod blueprint_actions;
pub mod blueprint_command_receipts;
#[cfg(test)]
mod blueprint_template_tests;
pub mod blueprint_work_items;
pub mod blueprints;
mod repositories;
pub use agent_jobs::{AgentRuntimeObservation, RuntimeActivity};

const SCHEMA_VERSION: i64 = 19;
const DEFAULT_BUSY_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteConcurrencyPolicy {
    pub busy_timeout_ms: u64,
    pub wal_enabled: bool,
    pub foreign_keys_enabled: bool,
}

impl Default for SqliteConcurrencyPolicy {
    fn default() -> Self {
        Self {
            busy_timeout_ms: DEFAULT_BUSY_TIMEOUT_MS,
            wal_enabled: true,
            foreign_keys_enabled: true,
        }
    }
}

pub struct SqliteStore {
    connection: Connection,
}

// The command ledger uses the SAME connection and transaction boundary as
// application repositories. This temporary SQL integration boundary allows
// the authoritative command handler to operate on the rich application
// tables without constructing a second SQLite connection or schema.
impl std::ops::Deref for SqliteStore {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}
impl std::ops::DerefMut for SqliteStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageIntegrityIssue {
    pub area: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StorageIntegrityReport {
    pub issues: Vec<StorageIntegrityIssue>,
}

impl StorageIntegrityReport {
    pub fn is_healthy(&self) -> bool {
        self.issues.is_empty()
    }
}

// The storage facade intentionally contains only stable public types and module wiring.
// Implementation is grouped by responsibility while all operations retain their
// original SqliteStore receiver and transaction behavior.
mod project_criteria;
mod row_mapping;
mod sqlite_helpers;
#[cfg(test)]
mod storage_tests;
mod store_core;
mod unit_of_work;

use row_mapping::*;
use sqlite_helpers::*;
