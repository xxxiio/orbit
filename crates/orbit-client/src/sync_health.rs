//! Unified interpretation of connected replica health for CLI, MCP, TUI and
//! future application clients. This module never writes to the local store or
//! infers server acceptance from a queued client-side command.
use crate::authority::{
    AuthorityError, ConvergenceCycle, ConvergenceObservation, ConvergenceStatus,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncCondition {
    NoAcceptedSnapshot,
    OutboxCorrupted,
    ConflictNeedsReview,
    PendingSubmission,
    MirrorIncomplete,
    LiveAuthorityUnobserved,
    ServerHistoryChanged,
    LiveObservationIncomplete,
    SyncCycleIncomplete,
    ConvergedAtObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    RefreshAcceptedState,
    InspectOutbox,
    ReviewConflicts,
    SubmitPending,
    RefreshMarkdownMirror,
    ObserveLiveAuthority,
    Reconnect,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncHealth {
    /// The first blocking condition in an intentional, user-actionable order.
    pub primary: SyncCondition,
    pub conditions: Vec<SyncCondition>,
    pub actions: Vec<SyncAction>,
    pub accepted_revision: Option<i64>,
    pub observed_server_revision: Option<i64>,
    pub pending_count: usize,
    pub conflict_count: usize,
    pub markdown_document_count: usize,
    pub markdown_mirror_complete: bool,
    /// Only meaningful with a live observation; never cached as a promise.
    pub converged_at_observation: bool,
}

impl SyncHealth {
    /// A local cache alone cannot prove server convergence. Callers that
    /// cannot access the network must use this constructor.
    pub fn offline(local: &ConvergenceStatus) -> Self {
        Self::build(local, None, false)
    }

    /// A point-in-time comparison with the authority. The observation is not
    /// an ongoing subscription and has no implications for subsequent writes.
    pub fn observed(observation: &ConvergenceObservation) -> Self {
        Self::build(&observation.local, Some(observation), false)
    }

    /// A complete convergence cycle includes outcome of the mirror refresh.
    /// A successful snapshot comparison must not conceal an incomplete cycle.
    pub fn cycle(cycle: &ConvergenceCycle) -> Self {
        Self::build(
            &cycle.observation.local,
            Some(&cycle.observation),
            !cycle.fully_synchronized_at_observation,
        )
    }

    fn build(
        local: &ConvergenceStatus,
        observed: Option<&ConvergenceObservation>,
        incomplete_cycle: bool,
    ) -> Self {
        let mut conditions = Vec::new();
        let mut actions = Vec::new();
        fn add(
            conditions: &mut Vec<SyncCondition>,
            actions: &mut Vec<SyncAction>,
            condition: SyncCondition,
            action: SyncAction,
        ) {
            conditions.push(condition);
            if !actions.contains(&action) {
                actions.push(action);
            }
        }
        if local.accepted_revision.is_none() {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::NoAcceptedSnapshot,
                SyncAction::RefreshAcceptedState,
            );
        }
        if !local.outbox_healthy {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::OutboxCorrupted,
                SyncAction::InspectOutbox,
            );
        }
        if local.conflicts > 0 {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::ConflictNeedsReview,
                SyncAction::ReviewConflicts,
            );
        }
        if local.pending > 0 {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::PendingSubmission,
                SyncAction::SubmitPending,
            );
        }
        // A changed server history is the root cause when the accepted snapshot
        // itself is stale. Report it before mirror staleness, which is a
        // consequence of that same stale accepted revision, so a newer server
        // is never masked by a `MirrorIncomplete` classification.
        if let Some(observation) = observed
            && !observation.accepted_snapshot_matches
        {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::ServerHistoryChanged,
                SyncAction::RefreshAcceptedState,
            );
        }
        if !local.markdown_mirror.complete
            || !local.markdown_mirror.source_inventory_verified
            || local.markdown_mirror.document_count != local.cached_markdown
            || local.markdown_mirror.source_revision != local.accepted_revision
            || observed.is_some_and(|state| {
                local.markdown_mirror.source_revision != Some(state.server_revision)
            })
        {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::MirrorIncomplete,
                SyncAction::RefreshMarkdownMirror,
            );
        }
        if observed.is_none() {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::LiveAuthorityUnobserved,
                SyncAction::ObserveLiveAuthority,
            );
        }
        if incomplete_cycle {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::SyncCycleIncomplete,
                SyncAction::Reconnect,
            );
        }
        if let Some(observation) = observed
            && !observation.converged_when_observed
            && conditions.is_empty()
        {
            add(
                &mut conditions,
                &mut actions,
                SyncCondition::LiveObservationIncomplete,
                SyncAction::Reconnect,
            );
        }
        let converged = conditions.is_empty()
            && observed.is_some_and(|obs| obs.converged_when_observed && obs.clean_outbox);
        if converged {
            conditions.push(SyncCondition::ConvergedAtObservation);
            actions.push(SyncAction::None);
        } else if actions.is_empty() {
            actions.push(SyncAction::Reconnect);
        }
        Self {
            primary: conditions[0],
            conditions,
            actions,
            accepted_revision: local.accepted_revision,
            observed_server_revision: observed.map(|observation| observation.server_revision),
            pending_count: local.pending,
            conflict_count: local.conflicts,
            markdown_document_count: local.cached_markdown,
            markdown_mirror_complete: local.markdown_mirror.complete
                && local.markdown_mirror.source_inventory_verified
                && local.markdown_mirror.document_count == local.cached_markdown
                && local.markdown_mirror.source_revision == local.accepted_revision
                && observed.is_none_or(|state| {
                    local.markdown_mirror.source_revision == Some(state.server_revision)
                }),
            converged_at_observation: converged,
        }
    }
}

/// An exact, deterministic breakdown of the durable outbox across *all*
/// authoritative command families. Counts are grouped by the persisted
/// command kind and status, never by an action field that an untrusted client
/// can provide. Accepted receipts and superseded commands are kept visible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainQueueStatus {
    pub family: String,
    pub pending: usize,
    pub conflicted: usize,
    pub accepted: usize,
    pub superseded: usize,
    pub first_unresolved_id: Option<String>,
    pub first_unresolved_ordinal: Option<i64>,
}

impl DomainQueueStatus {
    fn new(family: String) -> Self {
        Self {
            family,
            pending: 0,
            conflicted: 0,
            accepted: 0,
            superseded: 0,
            first_unresolved_id: None,
            first_unresolved_ordinal: None,
        }
    }
    fn count(&mut self, state: &str) -> Result<(), AuthorityError> {
        let value = match state {
            "pending" => &mut self.pending,
            "conflict" => &mut self.conflicted,
            "accepted" => &mut self.accepted,
            "superseded" => &mut self.superseded,
            _ => {
                return Err(AuthorityError::Invalid(format!(
                    "unexpected persisted outbox state {state}"
                )));
            }
        };
        *value = (*value)
            .checked_add(1)
            .ok_or_else(|| AuthorityError::Invalid("outbox count overflow".into()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainQueueReport {
    /// Includes every supported kind, including zero-count families.
    pub families: Vec<DomainQueueStatus>,
    pub pending: usize,
    pub conflicted: usize,
    pub accepted: usize,
    pub superseded: usize,
    /// Global, rather than per-family, ordering remains authoritative.
    pub first_unresolved_id: Option<String>,
    pub first_unresolved_family: Option<String>,
    pub first_unresolved_ordinal: Option<i64>,
}

impl DomainQueueReport {
    /// Inspect the queue without modifying it. A stable ordinal is used for
    /// each unresolved command, regardless of its domain family.
    pub(crate) fn read(db: &Connection) -> Result<Self, AuthorityError> {
        const FAMILIES: [&str; 12] = [
            "project",
            "task",
            "evidence",
            "schedule",
            "routine",
            "milestone",
            "strategy",
            "markdown",
            "folder_create",
            "folder_move",
            "agent",
            "blueprint",
        ];
        let mut map: BTreeMap<String, DomainQueueStatus> = FAMILIES
            .iter()
            .map(|family| {
                (
                    family.to_string(),
                    DomainQueueStatus::new(family.to_string()),
                )
            })
            .collect();
        let mut report = Self {
            families: Vec::new(),
            pending: 0,
            conflicted: 0,
            accepted: 0,
            superseded: 0,
            first_unresolved_id: None,
            first_unresolved_family: None,
            first_unresolved_ordinal: None,
        };
        let mut stmt = db.prepare(
            "SELECT command_id,kind,state,ordinal FROM authority_outbox ORDER BY ordinal ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut last_ordinal = None;
        for row in rows {
            let (id, family, state, ordinal) = row?;
            if ordinal < 0 || last_ordinal.is_some_and(|previous| ordinal <= previous) {
                return Err(AuthorityError::Invalid(
                    "invalid outbox global ordering".into(),
                ));
            }
            last_ordinal = Some(ordinal);
            let family_status = map.get_mut(&family).ok_or_else(|| {
                AuthorityError::Invalid(format!("unknown command family {family}"))
            })?;
            family_status.count(&state)?;
            match state.as_str() {
                "pending" => {
                    report.pending = report
                        .pending
                        .checked_add(1)
                        .ok_or_else(|| AuthorityError::Invalid("pending count overflow".into()))?
                }
                "conflict" => {
                    report.conflicted = report
                        .conflicted
                        .checked_add(1)
                        .ok_or_else(|| AuthorityError::Invalid("conflict count overflow".into()))?
                }
                "accepted" => {
                    report.accepted = report
                        .accepted
                        .checked_add(1)
                        .ok_or_else(|| AuthorityError::Invalid("accepted count overflow".into()))?
                }
                "superseded" => {
                    report.superseded = report.superseded.checked_add(1).ok_or_else(|| {
                        AuthorityError::Invalid("superseded count overflow".into())
                    })?
                }
                _ => unreachable!("validated by family_status.count"),
            }
            if state == "pending" || state == "conflict" {
                if family_status.first_unresolved_id.is_none() {
                    family_status.first_unresolved_id = Some(id.clone());
                    family_status.first_unresolved_ordinal = Some(ordinal);
                }
                if report.first_unresolved_id.is_none() {
                    report.first_unresolved_id = Some(id);
                    report.first_unresolved_family = Some(family);
                    report.first_unresolved_ordinal = Some(ordinal);
                }
            }
        }
        report.families = map.into_values().collect();
        Ok(report)
    }

    /// A blocked global head prevents all later commands, regardless of
    /// family. It is a routing hint, not permission to reorder the queue.
    pub fn blocking_head(&self) -> Option<(&str, &str, i64)> {
        match (
            &self.first_unresolved_id,
            &self.first_unresolved_family,
            self.first_unresolved_ordinal,
        ) {
            (Some(id), Some(family), Some(ordinal)) => Some((id, family, ordinal)),
            _ => None,
        }
    }

    pub fn unresolved_total(&self) -> usize {
        self.pending + self.conflicted
    }
}

/// One cross-domain read-only report. A client can choose to perform the
/// suggested action, but asking for this report alone never submits a command
/// or fetches a live server snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSyncReport {
    pub health: SyncHealth,
    pub domains: DomainQueueReport,
}

impl WorkspaceSyncReport {
    pub(crate) fn offline(
        db: &Connection,
        local: &ConvergenceStatus,
    ) -> Result<Self, AuthorityError> {
        let domains = DomainQueueReport::read(db)?;
        if domains.pending != local.pending
            || domains.conflicted != local.conflicts
            || domains.accepted != local.accepted
            || domains.superseded != local.superseded
        {
            return Err(AuthorityError::Invalid(
                "domain outbox counts disagree with convergence state".into(),
            ));
        }
        Ok(Self {
            health: SyncHealth::offline(local),
            domains,
        })
    }
    pub(crate) fn observed(
        db: &Connection,
        observation: &ConvergenceObservation,
    ) -> Result<Self, AuthorityError> {
        let mut report = Self::offline(db, &observation.local)?;
        report.health = SyncHealth::observed(observation);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::MarkdownMirrorStatus;

    fn local() -> ConvergenceStatus {
        ConvergenceStatus {
            accepted_revision: Some(7),
            history_fingerprint: Some("a".repeat(64)),
            pending: 0,
            conflicts: 0,
            accepted: 3,
            superseded: 0,
            cached_markdown: 2,
            markdown_mirror: MarkdownMirrorStatus {
                complete: true,
                source_inventory_verified: true,
                document_count: 2,
                inventory_digest: Some("b".repeat(64)),
                source_revision: Some(7),
            },
            outbox_healthy: true,
            first_unresolved: None,
            server_convergence_verified: false,
        }
    }
    fn observation(local: ConvergenceStatus) -> ConvergenceObservation {
        ConvergenceObservation {
            server_revision: 7,
            server_history_fingerprint: "a".repeat(64),
            local,
            accepted_snapshot_matches: true,
            clean_outbox: true,
            converged_when_observed: true,
        }
    }
    #[test]
    fn offline_is_never_converged() {
        let status = SyncHealth::offline(&local());
        assert_eq!(status.primary, SyncCondition::LiveAuthorityUnobserved);
        assert!(!status.converged_at_observation);
        assert_eq!(status.actions, vec![SyncAction::ObserveLiveAuthority]);
    }
    #[test]
    fn online_clean_is_observed_only() {
        let status = SyncHealth::observed(&observation(local()));
        assert_eq!(status.primary, SyncCondition::ConvergedAtObservation);
        assert!(status.converged_at_observation);
        assert_eq!(status.actions, vec![SyncAction::None]);
    }
    #[test]
    fn pending_is_not_accepted() {
        let mut state = local();
        state.pending = 1;
        let status = SyncHealth::observed(&observation(state));
        assert_eq!(status.primary, SyncCondition::PendingSubmission);
        assert!(!status.converged_at_observation);
    }
    #[test]
    fn corrupted_outbox_precedes_pending() {
        let mut state = local();
        state.outbox_healthy = false;
        state.pending = 2;
        let status = SyncHealth::offline(&state);
        assert_eq!(status.primary, SyncCondition::OutboxCorrupted);
        assert_eq!(status.actions[0], SyncAction::InspectOutbox);
    }
    #[test]
    fn stale_server_is_visible_even_if_local_queue_empty() {
        let mut obs = observation(local());
        obs.server_revision = 8;
        obs.accepted_snapshot_matches = false;
        obs.converged_when_observed = false;
        let status = SyncHealth::observed(&obs);
        assert_eq!(status.primary, SyncCondition::ServerHistoryChanged);
        assert_eq!(status.observed_server_revision, Some(8));
    }
    #[test]
    fn partial_mirror_blocks_convergence() {
        let mut state = local();
        state.markdown_mirror.complete = false;
        let status = SyncHealth::observed(&observation(state));
        assert_eq!(status.primary, SyncCondition::MirrorIncomplete);
        assert!(!status.converged_at_observation);
    }
    #[test]
    fn cached_document_count_mismatch_is_not_complete() {
        let mut state = local();
        state.cached_markdown = 1;
        let status = SyncHealth::observed(&observation(state));
        assert_eq!(status.primary, SyncCondition::MirrorIncomplete);
    }
    #[test]
    fn conflict_precedes_pending_and_preserves_both_actions() {
        let mut state = local();
        state.conflicts = 1;
        state.pending = 3;
        let status = SyncHealth::offline(&state);
        assert_eq!(status.primary, SyncCondition::ConflictNeedsReview);
        assert!(status.actions.contains(&SyncAction::ReviewConflicts));
        assert!(status.actions.contains(&SyncAction::SubmitPending));
    }
    #[test]
    fn cycle_failure_does_not_claim_success() {
        // An observation alone may agree while a synchronization attempt failed.
        // The cycle completeness bit must remain authoritative here.
        let observation = observation(local());
        // Test the equivalent cycle-failure condition through the internal builder,
        // so no synthetic network or storage success is needed.
        let status = SyncHealth::build(&observation.local, Some(&observation), true);
        assert_eq!(status.primary, SyncCondition::SyncCycleIncomplete);
        assert!(!status.converged_at_observation);
    }
    #[test]
    fn serializes_machine_readable_repair_instructions() {
        let report = SyncHealth::offline(&local());
        let payload = serde_json::to_value(&report).unwrap();
        assert_eq!(payload["primary"], "live_authority_unobserved");
        assert_eq!(payload["actions"][0], "observe_live_authority");
    }
}

#[cfg(test)]
mod domain_tests {
    use super::*;
    use rusqlite::{Connection, params};
    use serde_json::json;

    fn memory() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE authority_outbox (
            command_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            state TEXT NOT NULL,
            ordinal INTEGER NOT NULL UNIQUE);",
        )
        .unwrap();
        db
    }
    fn put(db: &Connection, id: &str, family: &str, state: &str, ordinal: i64) {
        db.execute(
            "INSERT INTO authority_outbox(command_id,kind,state,ordinal)
            VALUES(?1,?2,?3,?4)",
            params![id, family, state, ordinal],
        )
        .unwrap();
    }
    fn accepted_state() -> ConvergenceStatus {
        ConvergenceStatus {
            accepted_revision: Some(11),
            history_fingerprint: Some("a".repeat(64)),
            pending: 0,
            conflicts: 0,
            accepted: 0,
            superseded: 0,
            cached_markdown: 0,
            markdown_mirror: crate::authority::MarkdownMirrorStatus {
                complete: true,
                source_inventory_verified: true,
                document_count: 0,
                inventory_digest: Some("b".repeat(64)),
                source_revision: Some(11),
            },
            outbox_healthy: true,
            first_unresolved: None,
            server_convergence_verified: false,
        }
    }
    #[test]
    fn empty_queue_has_all_canonical_families() {
        let db = memory();
        let report = DomainQueueReport::read(&db).unwrap();
        assert_eq!(report.families.len(), 12);
        assert_eq!(report.unresolved_total(), 0);
        assert!(report.blocking_head().is_none());
        for family in report.families {
            assert_eq!(family.pending, 0);
            assert_eq!(family.conflicted, 0);
            assert_eq!(family.accepted, 0);
            assert_eq!(family.superseded, 0);
            assert!(family.first_unresolved_id.is_none());
        }
    }
    #[test]
    fn every_supported_domain_is_reported() {
        let db = memory();
        for (i, family) in [
            "project",
            "task",
            "evidence",
            "schedule",
            "routine",
            "milestone",
            "strategy",
            "markdown",
            "folder_create",
            "folder_move",
            "agent",
            "blueprint",
        ]
        .iter()
        .enumerate()
        {
            put(
                &db,
                &format!("command-{i}"),
                family,
                "pending",
                i as i64 + 1,
            );
        }
        let report = DomainQueueReport::read(&db).unwrap();
        assert_eq!(report.pending, 12);
        assert_eq!(report.unresolved_total(), 12);
        assert_eq!(report.blocking_head(), Some(("command-0", "project", 1)));
        assert!(report.families.iter().all(|family| family.pending == 1));
    }
    #[test]
    fn global_ordering_does_not_prioritize_one_family() {
        let db = memory();
        put(&db, "late-project", "project", "pending", 40);
        put(&db, "earlier-markdown", "markdown", "conflict", 3);
        put(&db, "middle-task", "task", "pending", 9);
        let report = DomainQueueReport::read(&db).unwrap();
        assert_eq!(
            report.blocking_head(),
            Some(("earlier-markdown", "markdown", 3))
        );
        assert_eq!(report.pending, 2);
        assert_eq!(report.conflicted, 1);
        assert_eq!(
            report
                .families
                .iter()
                .find(|f| f.family == "project")
                .unwrap()
                .first_unresolved_ordinal,
            Some(40)
        );
    }
    #[test]
    fn terminal_commands_not_treated_as_pending() {
        let db = memory();
        put(&db, "accepted-1", "task", "accepted", 1);
        put(&db, "superseded-2", "task", "superseded", 2);
        let report = DomainQueueReport::read(&db).unwrap();
        assert_eq!(report.accepted, 1);
        assert_eq!(report.superseded, 1);
        assert_eq!(report.pending, 0);
        assert_eq!(report.conflicted, 0);
        assert_eq!(report.first_unresolved_id, None);
    }
    #[test]
    fn an_accepted_older_command_does_not_block_newer_pending() {
        let db = memory();
        put(&db, "accepted-1", "markdown", "accepted", 1);
        put(&db, "pending-2", "task", "pending", 2);
        put(&db, "conflict-3", "project", "conflict", 3);
        let report = DomainQueueReport::read(&db).unwrap();
        assert_eq!(report.blocking_head(), Some(("pending-2", "task", 2)));
    }
    #[test]
    fn malformed_unknown_family_fails_closed() {
        let db = memory();
        put(&db, "wrong", "other", "pending", 1);
        assert!(DomainQueueReport::read(&db).is_err());
    }
    #[test]
    fn malformed_unknown_state_fails_closed() {
        let db = memory();
        put(&db, "wrong", "task", "discarded", 1);
        assert!(DomainQueueReport::read(&db).is_err());
    }
    #[test]
    fn negative_ordinal_fails_closed() {
        let db = memory();
        put(&db, "wrong", "task", "pending", -1);
        assert!(DomainQueueReport::read(&db).is_err());
    }
    #[test]
    fn idempotent_read_does_not_change_data() {
        let db = memory();
        put(&db, "one", "task", "pending", 1);
        let before: i64 = db
            .query_row("SELECT COUNT(*) FROM authority_outbox", [], |r| r.get(0))
            .unwrap();
        let first = DomainQueueReport::read(&db).unwrap();
        let second = DomainQueueReport::read(&db).unwrap();
        let after: i64 = db
            .query_row("SELECT COUNT(*) FROM authority_outbox", [], |r| r.get(0))
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(before, after);
    }
    #[test]
    fn report_disagrees_with_local_summary_fail_closed() {
        let db = memory();
        put(&db, "one", "task", "pending", 1);
        let local = accepted_state();
        assert!(WorkspaceSyncReport::offline(&db, &local).is_err());
    }
    #[test]
    fn matching_local_summary_is_accepted() {
        let db = memory();
        put(&db, "one", "task", "pending", 1);
        put(&db, "two", "markdown", "accepted", 2);
        let mut local = accepted_state();
        local.pending = 1;
        local.accepted = 1;
        let report = WorkspaceSyncReport::offline(&db, &local).unwrap();
        assert_eq!(report.health.primary, SyncCondition::PendingSubmission);
        assert_eq!(report.domains.pending, 1);
        assert_eq!(report.domains.accepted, 1);
        assert_eq!(
            report.domains.first_unresolved_family.as_deref(),
            Some("task")
        );
    }
    #[test]
    fn online_observation_never_hides_pending() {
        let db = memory();
        put(&db, "one", "task", "pending", 1);
        let mut local = accepted_state();
        local.pending = 1;
        let observation = ConvergenceObservation {
            server_revision: 11,
            server_history_fingerprint: "a".repeat(64),
            local,
            accepted_snapshot_matches: true,
            clean_outbox: false,
            converged_when_observed: false,
        };
        let report = WorkspaceSyncReport::observed(&db, &observation).unwrap();
        assert_eq!(report.health.primary, SyncCondition::PendingSubmission);
        assert!(!report.health.converged_at_observation);
    }
    #[test]
    fn report_includes_agent_and_blueprint_transport_families() {
        let db = memory();
        put(&db, "agent-1", "agent", "pending", 1);
        put(&db, "blueprint-1", "blueprint", "pending", 2);
        let report = DomainQueueReport::read(&db).unwrap();
        assert_eq!(report.families.len(), 12);
        assert!(
            report
                .families
                .iter()
                .any(|family| family.family == "agent")
        );
        assert!(
            report
                .families
                .iter()
                .any(|family| family.family == "blueprint")
        );
        assert_eq!(report.pending, 2);
    }

    #[test]
    fn serialized_report_preserves_remediation_sequence() {
        let db = memory();
        put(&db, "conflict-1", "schedule", "conflict", 1);
        put(&db, "pending-2", "markdown", "pending", 2);
        let mut local = accepted_state();
        local.conflicts = 1;
        local.pending = 1;
        let report = WorkspaceSyncReport::offline(&db, &local).unwrap();
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(
            value["domains"]["first_unresolved_family"],
            json!("schedule")
        );
        assert_eq!(value["health"]["actions"][0], json!("review_conflicts"));
        assert_eq!(value["health"]["actions"][1], json!("submit_pending"));
    }
    #[test]
    fn read_only_report_can_be_reused_without_network() {
        let db = memory();
        let local = accepted_state();
        let report = WorkspaceSyncReport::offline(&db, &local).unwrap();
        assert_eq!(
            report.health.primary,
            SyncCondition::LiveAuthorityUnobserved
        );
        assert!(!report.health.converged_at_observation);
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;
    use crate::authority::MarkdownMirrorStatus;
    fn state() -> ConvergenceStatus {
        ConvergenceStatus {
            accepted_revision: Some(1),
            history_fingerprint: Some("f".repeat(64)),
            pending: 0,
            conflicts: 0,
            accepted: 0,
            superseded: 0,
            cached_markdown: 0,
            markdown_mirror: MarkdownMirrorStatus {
                complete: true,
                source_inventory_verified: true,
                document_count: 0,
                inventory_digest: Some("f".repeat(64)),
                source_revision: Some(1),
            },
            outbox_healthy: true,
            first_unresolved: None,
            server_convergence_verified: false,
        }
    }
    fn live(local: ConvergenceStatus) -> ConvergenceObservation {
        ConvergenceObservation {
            server_revision: 1,
            server_history_fingerprint: "f".repeat(64),
            local,
            accepted_snapshot_matches: true,
            clean_outbox: true,
            converged_when_observed: true,
        }
    }
    #[test]
    fn contradictory_observation_does_not_panic_or_converge() {
        let mut obs = live(state());
        obs.converged_when_observed = false;
        let report = SyncHealth::observed(&obs);
        assert_eq!(report.primary, SyncCondition::LiveObservationIncomplete);
        assert_eq!(report.actions, vec![SyncAction::Reconnect]);
        assert!(!report.converged_at_observation);
    }
    #[test]
    fn explicit_cycle_failure_is_distinct_from_markdown_incompleteness() {
        let obs = live(state());
        let report = SyncHealth::build(&obs.local, Some(&obs), true);
        assert_eq!(report.primary, SyncCondition::SyncCycleIncomplete);
        assert!(report.markdown_mirror_complete);
        assert!(!report.converged_at_observation);
    }
    #[test]
    fn missing_accepted_snapshot_requires_refresh_even_when_mirror_looks_complete() {
        let mut local = state();
        local.accepted_revision = None;
        let report = SyncHealth::offline(&local);
        assert_eq!(report.primary, SyncCondition::NoAcceptedSnapshot);
        assert_eq!(report.actions[0], SyncAction::RefreshAcceptedState);
    }
    #[test]
    fn mirror_provenance_without_verified_inventory_is_incomplete() {
        let mut local = state();
        local.markdown_mirror.source_inventory_verified = false;
        let report = SyncHealth::observed(&live(local));
        assert_eq!(report.primary, SyncCondition::MirrorIncomplete);
    }
    #[test]
    fn one_observation_does_not_override_corrupted_outbox() {
        let mut local = state();
        local.outbox_healthy = false;
        let report = SyncHealth::observed(&live(local));
        assert_eq!(report.primary, SyncCondition::OutboxCorrupted);
        assert!(!report.converged_at_observation);
    }
    #[test]
    fn conflicting_command_cannot_be_hidden_by_clean_server_fingerprint() {
        let mut local = state();
        local.conflicts = 4;
        let report = SyncHealth::observed(&live(local));
        assert_eq!(report.primary, SyncCondition::ConflictNeedsReview);
        assert_eq!(report.conflict_count, 4);
    }
    #[test]
    fn multiple_issues_have_stable_priority() {
        let mut local = state();
        local.accepted_revision = None;
        local.outbox_healthy = false;
        local.pending = 3;
        local.conflicts = 2;
        local.markdown_mirror.complete = false;
        let report = SyncHealth::offline(&local);
        assert_eq!(
            report.conditions,
            vec![
                SyncCondition::NoAcceptedSnapshot,
                SyncCondition::OutboxCorrupted,
                SyncCondition::ConflictNeedsReview,
                SyncCondition::PendingSubmission,
                SyncCondition::MirrorIncomplete,
                SyncCondition::LiveAuthorityUnobserved
            ]
        );
        assert_eq!(
            report.actions,
            vec![
                SyncAction::RefreshAcceptedState,
                SyncAction::InspectOutbox,
                SyncAction::ReviewConflicts,
                SyncAction::SubmitPending,
                SyncAction::RefreshMarkdownMirror,
                SyncAction::ObserveLiveAuthority
            ]
        );
    }
    #[test]
    fn online_status_encodes_current_server_revision() {
        let obs = live(state());
        let report = SyncHealth::observed(&obs);
        assert_eq!(
            serde_json::to_value(report).unwrap()["observed_server_revision"],
            1
        );
    }
    #[test]
    fn offline_status_never_records_observed_revision() {
        let report = SyncHealth::offline(&state());
        assert!(report.observed_server_revision.is_none());
    }
    #[test]
    fn healthy_mirror_has_consistent_document_count() {
        let mut local = state();
        local.cached_markdown = 100;
        local.markdown_mirror.document_count = 100;
        let report = SyncHealth::observed(&live(local));
        assert!(report.markdown_mirror_complete);
        assert_eq!(report.markdown_document_count, 100);
    }
}

#[cfg(test)]
mod replica_integration_tests {
    use super::*;
    use crate::authority::{AuthorityReplica, CommandKind};
    use serde_json::json;

    #[test]
    fn mixed_domain_intentions_are_reported_as_pending_not_accepted() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "project-1",
                CommandKind::Project,
                &json!({"project_id":"p","name":"Work"}),
            )
            .unwrap();
        replica
            .queue(
                "schedule-2",
                CommandKind::Schedule,
                &json!({"type":"create","id":"s"}),
            )
            .unwrap();
        replica
            .queue(
                "routine-3",
                CommandKind::Routine,
                &json!({"type":"create","id":"r"}),
            )
            .unwrap();
        replica
            .queue(
                "markdown-4",
                CommandKind::Markdown,
                &json!({"type":"update","id":"n"}),
            )
            .unwrap();
        let report = replica.workspace_sync_report().unwrap();
        assert_eq!(report.domains.pending, 4);
        assert_eq!(report.domains.accepted, 0);
        assert_eq!(
            report.domains.first_unresolved_id.as_deref(),
            Some("project-1")
        );
        assert_eq!(report.health.primary, SyncCondition::NoAcceptedSnapshot);
        assert!(replica.snapshot().unwrap().is_none());
    }
    #[test]
    fn batch_staging_and_per_family_report_have_identical_counts() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue_batch(&[
                (
                    "task-1".to_string(),
                    CommandKind::Task,
                    json!({"task_id":"t","status":"done"}),
                ),
                (
                    "milestone-2".to_string(),
                    CommandKind::Milestone,
                    json!({"type":"status","id":"m"}),
                ),
                (
                    "strategy-3".to_string(),
                    CommandKind::Strategy,
                    json!({"type":"configure","id":"p"}),
                ),
            ])
            .unwrap();
        let report = replica.workspace_sync_report().unwrap();
        assert_eq!(report.domains.pending, 3);
        assert_eq!(report.health.pending_count, 3);
        assert_eq!(
            report.domains.first_unresolved_family.as_deref(),
            Some("task")
        );
    }
    #[test]
    fn rejected_batch_does_not_affect_report() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue("existing", CommandKind::Project, &json!({"name":"First"}))
            .unwrap();
        let before = replica.workspace_sync_report().unwrap();
        let failed = replica.queue_batch(&[
            (
                "new".to_string(),
                CommandKind::Task,
                json!({"status":"done"}),
            ),
            (
                "existing".to_string(),
                CommandKind::Project,
                json!({"name":"Other"}),
            ),
        ]);
        assert!(failed.is_err());
        let after = replica.workspace_sync_report().unwrap();
        assert_eq!(before.domains, after.domains);
        assert_eq!(before.health, after.health);
    }
    #[test]
    fn replay_of_idempotent_command_does_not_double_pending_count() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let action = json!({"name":"Planning"});
        replica
            .queue("project-1", CommandKind::Project, &action)
            .unwrap();
        replica
            .queue("project-1", CommandKind::Project, &action)
            .unwrap();
        assert_eq!(replica.workspace_sync_report().unwrap().domains.pending, 1);
    }
    #[test]
    fn report_never_mutates_queue_or_accepted_snapshot() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "habit-1",
                CommandKind::Evidence,
                &json!({"type":"habit","id":"h"}),
            )
            .unwrap();
        let before = replica.unresolved().unwrap();
        for _ in 0..4 {
            let report = replica.workspace_sync_report().unwrap();
            assert_eq!(report.domains.pending, 1);
        }
        assert_eq!(replica.unresolved().unwrap(), before);
        assert!(replica.snapshot().unwrap().is_none());
    }
    #[test]
    fn all_command_families_remain_globally_ordered() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let families = [
            CommandKind::Project,
            CommandKind::Task,
            CommandKind::Evidence,
            CommandKind::Schedule,
            CommandKind::Routine,
            CommandKind::Milestone,
            CommandKind::Strategy,
            CommandKind::Markdown,
            CommandKind::FolderCreate,
            CommandKind::FolderMove,
            CommandKind::Agent,
            CommandKind::Blueprint,
        ];
        for (index, kind) in families.iter().enumerate() {
            let command_id = format!("cmd-{index}");
            match kind {
                CommandKind::Agent => replica
                    .queue_agent_transport(
                        &command_id,
                        "agent-jobs/start",
                        &json!({"job_id":format!("job-{index}"),"task_id":"task"}),
                    )
                    .unwrap(),
                CommandKind::Blueprint => replica
                    .queue_blueprint_transport(
                        &command_id,
                        "blueprints/install",
                        &json!({"id":"development"}),
                    )
                    .unwrap(),
                _ => replica
                    .queue(&command_id, *kind, &json!({"index":index}))
                    .unwrap(),
            }
        }
        let report = replica.workspace_sync_report().unwrap();
        assert_eq!(report.domains.pending, 12);
        assert_eq!(report.domains.families.len(), 12);
        assert_eq!(report.domains.first_unresolved_id.as_deref(), Some("cmd-0"));
        assert_eq!(
            report.domains.first_unresolved_family.as_deref(),
            Some("project")
        );
    }
    #[test]
    fn queued_markdown_does_not_make_accepted_mirror_complete() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "note-1",
                CommandKind::Markdown,
                &json!({"type":"update","id":"n"}),
            )
            .unwrap();
        let report = replica.workspace_sync_report().unwrap();
        assert!(!report.health.markdown_mirror_complete);
        assert_eq!(report.domains.pending, 1);
        assert_eq!(report.domains.accepted, 0);
    }
    #[test]
    fn serialization_has_no_command_body_or_note_content() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "note-1",
                CommandKind::Markdown,
                &json!({"type":"update","secret":"sensitive-note-body"}),
            )
            .unwrap();
        let report = replica.workspace_sync_report().unwrap();
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("sensitive-note-body"));
        assert!(serialized.contains("note-1"));
    }
}

#[cfg(test)]
mod protocol_compatibility_tests {
    use super::*;
    #[test]
    fn machine_readable_condition_variants_remain_stable() {
        let variants = [
            (SyncCondition::NoAcceptedSnapshot, "no_accepted_snapshot"),
            (SyncCondition::OutboxCorrupted, "outbox_corrupted"),
            (SyncCondition::ConflictNeedsReview, "conflict_needs_review"),
            (SyncCondition::PendingSubmission, "pending_submission"),
            (SyncCondition::MirrorIncomplete, "mirror_incomplete"),
            (
                SyncCondition::LiveAuthorityUnobserved,
                "live_authority_unobserved",
            ),
            (
                SyncCondition::ServerHistoryChanged,
                "server_history_changed",
            ),
            (
                SyncCondition::LiveObservationIncomplete,
                "live_observation_incomplete",
            ),
            (SyncCondition::SyncCycleIncomplete, "sync_cycle_incomplete"),
            (
                SyncCondition::ConvergedAtObservation,
                "converged_at_observation",
            ),
        ];
        for (variant, name) in variants {
            assert_eq!(
                serde_json::to_string(&variant).unwrap(),
                format!("\"{name}\"")
            );
        }
    }
    #[test]
    fn machine_readable_action_variants_remain_stable() {
        let variants = [
            (SyncAction::RefreshAcceptedState, "refresh_accepted_state"),
            (SyncAction::InspectOutbox, "inspect_outbox"),
            (SyncAction::ReviewConflicts, "review_conflicts"),
            (SyncAction::SubmitPending, "submit_pending"),
            (SyncAction::RefreshMarkdownMirror, "refresh_markdown_mirror"),
            (SyncAction::ObserveLiveAuthority, "observe_live_authority"),
            (SyncAction::Reconnect, "reconnect"),
            (SyncAction::None, "none"),
        ];
        for (variant, name) in variants {
            assert_eq!(
                serde_json::to_string(&variant).unwrap(),
                format!("\"{name}\"")
            );
        }
    }
}
