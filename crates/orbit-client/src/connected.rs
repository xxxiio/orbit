//! Native remote client for a hosted authoritative Orbit runtime.
//!
//! Unlike `OrbitClient`, this facade never opens or writes the local-only
//! domain database. Its cache contains exclusively server-accepted snapshots;
//! user intentions are stored separately in the durable outbox.
use crate::authority::{
    AuthorityError, AuthorityHttp, AuthorityReplica, CommandKind, FlushOutcome,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectedWorkspace {
    pub revision: Option<i64>,
    pub history_fingerprint: Option<String>,
    pub projects: Vec<Value>,
    pub tasks: Vec<Value>,
    pub task_dependencies: Vec<orbit_runtime::task_dependencies::TaskDependencyEdge>,
    pub habits: Vec<Value>,
    pub habit_occurrences: Vec<Value>,
    pub metrics: Vec<Value>,
    pub metric_observations: Vec<Value>,
    pub schedule: Vec<Value>,
    pub routines: Vec<Value>,
    pub milestones: Vec<Value>,
    pub strategies: Vec<Value>,
    pub pending_count: i64,
}
impl ConnectedWorkspace {
    fn from_snapshot(snapshot: Option<Value>, pending_count: i64) -> Result<Self, AuthorityError> {
        let Some(value) = snapshot else {
            return Ok(Self {
                revision: None,
                history_fingerprint: None,
                projects: vec![],
                tasks: vec![],
                task_dependencies: vec![],
                habits: vec![],
                habit_occurrences: vec![],
                metrics: vec![],
                metric_observations: vec![],
                schedule: vec![],
                routines: vec![],
                milestones: vec![],
                strategies: vec![],
                pending_count,
            });
        };
        fn collection(snapshot: &Value, key: &str) -> Result<Vec<Value>, AuthorityError> {
            snapshot
                .get(key)
                .and_then(Value::as_array)
                .cloned()
                .ok_or_else(|| AuthorityError::Invalid(format!("accepted snapshot missing {key}")))
        }
        Ok(Self {
            revision: Some(
                value
                    .get("revision")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| AuthorityError::Invalid("accepted revision missing".into()))?,
            ),
            history_fingerprint: Some(
                value
                    .get("history_fingerprint")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AuthorityError::Invalid("accepted fingerprint missing".into()))?
                    .to_owned(),
            ),
            projects: collection(&value, "projects")?,
            tasks: collection(&value, "tasks")?,
            task_dependencies: serde_json::from_value(
                value
                    .get("task_dependencies")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
            )
            .map_err(|e| {
                AuthorityError::Invalid(format!("invalid accepted task dependencies: {e}"))
            })?,
            habits: collection(&value, "habits")?,
            habit_occurrences: collection(&value, "habit_occurrences")?,
            metrics: collection(&value, "metrics")?,
            metric_observations: collection(&value, "metric_observations")?,
            schedule: collection(&value, "schedule")?,
            routines: collection(&value, "routines")?,
            milestones: collection(&value, "milestones")?,
            strategies: collection(&value, "strategies")?,
            pending_count,
        })
    }
}

pub struct ConnectedOrbitClient {
    replica: AuthorityReplica,
}
impl ConnectedOrbitClient {
    pub fn open(replica_db: impl AsRef<Path>, server: &str) -> Result<Self, AuthorityError> {
        let mut replica = AuthorityReplica::open(replica_db)?;
        replica.bind(server)?;
        Ok(Self { replica })
    }
    /// Offline-safe: never causes a network request or infers accepted state
    /// from queued commands.
    pub fn workspace(&self) -> Result<ConnectedWorkspace, AuthorityError> {
        ConnectedWorkspace::from_snapshot(self.replica.snapshot()?, self.replica.pending_count()?)
    }
    #[cfg(test)]
    pub(crate) fn accept_snapshot_for_testing(
        &mut self,
        snapshot: &Value,
    ) -> Result<(), AuthorityError> {
        self.replica.accept_snapshot(snapshot)
    }
    pub fn unresolved(&self) -> Result<Vec<Value>, AuthorityError> {
        self.replica.unresolved()
    }
    pub fn converge(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::authority::ConvergenceCycle, AuthorityError> {
        self.replica.converge(http)
    }
    pub fn observe_convergence(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::authority::ConvergenceObservation, AuthorityError> {
        self.replica.observe_convergence(http)
    }
    pub fn review_conflict_online(
        &mut self,
        http: &AuthorityHttp,
        id: &str,
    ) -> Result<crate::authority::ConflictReview, AuthorityError> {
        self.replica.review_conflict_online(http, id)
    }
    pub fn workspace_sync_report(
        &self,
    ) -> Result<crate::sync_health::WorkspaceSyncReport, AuthorityError> {
        self.replica.workspace_sync_report()
    }
    pub fn observe_workspace_sync_report(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::sync_health::WorkspaceSyncReport, AuthorityError> {
        self.replica.observe_workspace_sync_report(http)
    }
    pub fn sync_health(&self) -> Result<crate::sync_health::SyncHealth, AuthorityError> {
        self.replica.sync_health()
    }
    pub fn observe_sync_health(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::sync_health::SyncHealth, AuthorityError> {
        self.replica.observe_sync_health(http)
    }
    pub fn convergence_status(
        &self,
    ) -> Result<crate::authority::ConvergenceStatus, AuthorityError> {
        self.replica.convergence_status()
    }
    pub fn conflict_review(
        &self,
        id: &str,
    ) -> Result<crate::authority::ConflictReview, AuthorityError> {
        self.replica.conflict_review(id)
    }
    pub fn replace_reviewed_conflict(
        &mut self,
        http: &AuthorityHttp,
        review: &crate::authority::ConflictReview,
        replacement_id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<(), AuthorityError> {
        self.replica
            .replace_reviewed_conflict_online(http, review, replacement_id, kind, action)
    }
    pub fn replace_reviewed_conflict_online(
        &mut self,
        http: &AuthorityHttp,
        review: &crate::authority::ConflictReview,
        replacement_id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<(), AuthorityError> {
        self.replica
            .replace_reviewed_conflict_online(http, review, replacement_id, kind, action)
    }
    pub fn replace_reviewed_batch_online(
        &mut self,
        http: &AuthorityHttp,
        decisions: &[crate::authority::ReviewedReplacement],
    ) -> Result<usize, AuthorityError> {
        self.replica.replace_reviewed_batch_online(http, decisions)
    }

    /// Online-only review of all scenario calendar alternatives. The server
    /// recomputes accepted tasks, availability and revisions before returning
    /// each concrete candidate; no preview is queued or applied.
    pub fn review_scenario_options_online(
        &self,
        http: &AuthorityHttp,
        request: &orbit_runtime::scenario_execution::ScenarioExecutionRequest,
    ) -> Result<orbit_runtime::scenario_execution::ScenarioCalendarOptions, AuthorityError> {
        if self.workspace()?.revision != Some(request.comparison.expected_revision) {
            return Err(AuthorityError::Invalid(
                "refresh accepted state before reviewing options".into(),
            ));
        }
        let payload =
            serde_json::to_value(request).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let value = http.post("/v1/authoritative/scenario-options-review", &payload)?;
        let response: orbit_runtime::scenario_execution::ScenarioCalendarOptions =
            serde_json::from_value(value).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        if !response.read_only
            || response.expected_revision != request.comparison.expected_revision
            || response.options.len() != request.comparison.alternatives.len() + 1
            || response
                .options
                .iter()
                .any(|item| item.feasible != item.preview.is_some())
        {
            return Err(AuthorityError::Invalid(
                "calendar options review contract mismatch".into(),
            ));
        }
        Ok(response)
    }

    /// Review a selected portfolio scenario as exact calendar slots on the
    /// server. The returned plan must still be explicitly approved and sent
    /// through `apply_approved_schedule_online`; this call never writes.
    pub fn review_scenario_execution_online(
        &self,
        http: &AuthorityHttp,
        request: &orbit_runtime::scenario_execution::ScenarioExecutionRequest,
    ) -> Result<orbit_runtime::scenario_execution::ScenarioExecutionReview, AuthorityError> {
        if self.workspace()?.revision != Some(request.comparison.expected_revision) {
            return Err(AuthorityError::Invalid(
                "refresh accepted state before reviewing scenario".into(),
            ));
        }
        let payload =
            serde_json::to_value(request).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let result = http.post("/v1/authoritative/scenario-execution-review", &payload)?;
        let response: orbit_runtime::scenario_execution::ScenarioExecutionReview =
            serde_json::from_value(result).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        if !response.read_only
            || response.plan.expected_revision != request.comparison.expected_revision
            || response.plan.command_id != request.command_id
            || response.review.command_id != request.command_id
            || response.review.scheduled_slots.len() != response.plan.items.len()
        {
            return Err(AuthorityError::Invalid(
                "scenario execution review contract mismatch".into(),
            ));
        }
        Ok(response)
    }

    /// Online-only review of concrete calendar reservations against the
    /// current server authority. No local outbox entry is made by a review.
    pub fn review_schedule_plan_online(
        &self,
        http: &AuthorityHttp,
        request: &orbit_runtime::planning_approval::ScheduleApprovalRequest,
    ) -> Result<orbit_runtime::planning_approval::PlanReview, AuthorityError> {
        let workspace = self.workspace()?;
        if workspace.revision != Some(request.expected_revision) {
            return Err(AuthorityError::Invalid(
                "refresh accepted replica before reviewing a schedule plan".into(),
            ));
        }
        let payload =
            serde_json::to_value(request).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let response = http.post("/v1/authoritative/plan-review", &payload)?;
        let review: orbit_runtime::planning_approval::PlanReview = serde_json::from_value(response)
            .map_err(|e| {
                AuthorityError::Invalid(format!("invalid schedule review response: {e}"))
            })?;
        if review.expected_revision != request.expected_revision
            || review.command_id != request.command_id
            || !review.creates_calendar_blocks
            || !review.requires_explicit_approval
        {
            return Err(AuthorityError::Invalid(
                "server plan review contract mismatch".into(),
            ));
        }
        Ok(review)
    }

    /// Explicit authoritative execution. Unlike offline task intentions, a
    /// schedule plan must be reviewed and approved online because available
    /// calendar slots may change while a client is disconnected.
    pub fn apply_approved_schedule_online(
        &mut self,
        http: &AuthorityHttp,
        request: &orbit_runtime::planning_approval::ApprovedScheduleRequest,
    ) -> Result<orbit_runtime::planning_approval::ApprovedScheduleReceipt, AuthorityError> {
        if !request.approval.approved
            || request.approval.review.expected_revision != request.plan.expected_revision
            || request.approval.review.command_id != request.plan.command_id
        {
            return Err(AuthorityError::Invalid(
                "explicit matching schedule approval required".into(),
            ));
        }
        let payload =
            serde_json::to_value(request).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let response = http.post("/v1/authoritative/plan-apply-approved", &payload)?;
        let receipt: orbit_runtime::planning_approval::ApprovedScheduleReceipt =
            serde_json::from_value(response)
                .map_err(|e| AuthorityError::Invalid(format!("invalid schedule receipt: {e}")))?;
        // Refresh accepted state only after successful server commit. A refresh
        // failure does not reverse the authoritative command; the immutable
        // command ID safely supports retry with the same approved payload.
        self.replica.refresh(http)?;
        Ok(receipt)
    }

    /// Persisted command status, including the validated acknowledgement.
    pub fn command_status(&self, id: &str) -> Result<Option<Value>, AuthorityError> {
        self.replica.command_status(id)
    }
    pub fn accepted_receipt(&self, id: &str) -> Result<Option<Value>, AuthorityError> {
        self.replica.accepted_receipt(id)
    }
    pub fn queue_project(
        &mut self,
        command_id: &str,
        project_id: &str,
        name: &str,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(
            command_id,
            CommandKind::Project,
            &json!({"project_id":project_id,"name":name}),
        )
    }
    pub fn queue_strategy_action(
        &mut self,
        command_id: &str,
        action: &orbit_runtime::authority_strategy::StrategyAction,
    ) -> Result<(), AuthorityError> {
        let value =
            serde_json::to_value(action).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        self.replica
            .queue(command_id, CommandKind::Strategy, &value)
    }
    pub fn queue_milestone_action(
        &mut self,
        command_id: &str,
        action: &orbit_runtime::authority_milestones::MilestoneAction,
    ) -> Result<(), AuthorityError> {
        let value =
            serde_json::to_value(action).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        self.replica
            .queue(command_id, CommandKind::Milestone, &value)
    }
    pub fn queue_routine_action(
        &mut self,
        command_id: &str,
        action: &orbit_runtime::authority_routines::RoutineAction,
    ) -> Result<(), AuthorityError> {
        let value =
            serde_json::to_value(action).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        self.replica.queue(command_id, CommandKind::Routine, &value)
    }
    pub fn queue_schedule_action(
        &mut self,
        command_id: &str,
        action: &orbit_runtime::authority_schedule::ScheduleAction,
    ) -> Result<(), AuthorityError> {
        let value =
            serde_json::to_value(action).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        self.replica
            .queue(command_id, CommandKind::Schedule, &value)
    }
    pub fn queue_task(
        &mut self,
        command_id: &str,
        task_id: &str,
        title: &str,
        project_id: Option<&str>,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(
            command_id,
            CommandKind::Task,
            &json!({"type":"create","task_id":task_id,"title":title,"project_id":project_id}),
        )
    }
    pub fn queue_task_status(
        &mut self,
        command_id: &str,
        task_id: &str,
        status: &str,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(
            command_id,
            CommandKind::Task,
            &json!({"type":"change_status","task_id":task_id,"status":status}),
        )
    }
    pub fn queue_habit(
        &mut self,
        command_id: &str,
        habit_id: &str,
        name: &str,
        cadence: &str,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(
            command_id,
            CommandKind::Evidence,
            &json!({"type":"create_habit","habit_id":habit_id,"name":name,"cadence":cadence}),
        )
    }
    pub fn queue_habit_occurrence(
        &mut self,
        command_id: &str,
        occurrence_id: &str,
        habit_id: &str,
        occurred_at_ms: i64,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(command_id,CommandKind::Evidence,&json!({"type":"record_habit","occurrence_id":occurrence_id,"habit_id":habit_id,"occurred_at_ms":occurred_at_ms}))
    }
    pub fn queue_metric_target(
        &mut self,
        command_id: &str,
        metric_id: &str,
        baseline: i64,
        target: i64,
        direction: &str,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(command_id,CommandKind::Evidence,&json!({"type":"set_metric_target","metric_id":metric_id,"baseline":baseline,"target":target,"direction":direction}))
    }
    pub fn queue_metric_observation(
        &mut self,
        command_id: &str,
        metric_id: &str,
        observed_at_ms: i64,
        value: i64,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(command_id,CommandKind::Evidence,&json!({"type":"record_metric","metric_id":metric_id,"observed_at_ms":observed_at_ms,"value":value}))
    }
    /// Queue a canonical domain action with its stable idempotency identity.
    /// Stage a complete cross-domain workflow in one local transaction.
    /// Every command is either staged or none of them are staged. Remote
    /// acceptance remains sequential and may commit a partial prefix.
    pub fn queue_workflow(
        &mut self,
        commands: &[(String, CommandKind, Value)],
    ) -> Result<usize, AuthorityError> {
        self.replica.queue_batch(commands)
    }
    pub fn queue_command(
        &mut self,
        command_id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<(), AuthorityError> {
        self.replica.queue(command_id, kind, action)
    }
    pub fn queue_markdown(
        &mut self,
        id: &str,
        command: &orbit_runtime::markdown_commands::MarkdownCommand,
    ) -> Result<(), AuthorityError> {
        self.replica.queue_markdown(id, command)
    }
    pub fn queue_folder_create(
        &mut self,
        id: &str,
        command: &orbit_runtime::folder_commands::FolderCreate,
    ) -> Result<(), AuthorityError> {
        self.replica.queue_folder_create(id, command)
    }
    pub fn queue_folder_move(
        &mut self,
        id: &str,
        command: &orbit_runtime::folder_commands::FolderMove,
    ) -> Result<(), AuthorityError> {
        self.replica.queue_folder_move(id, command)
    }
    /// Explicit last-accepted Markdown cache; callers must surface staleness.
    pub fn cached_markdown(
        &self,
        id: &str,
    ) -> Result<
        Option<(
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        )>,
        AuthorityError,
    > {
        self.replica.cached_markdown(id)
    }
    /// Offline search covers only previously fetched and accepted notes.
    pub fn search_cached_markdown(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>, AuthorityError> {
        self.replica.search_cached_markdown(query, limit)
    }
    pub fn cached_markdown_inventory(
        &self,
        limit: usize,
    ) -> Result<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>, AuthorityError> {
        self.replica.cached_markdown_inventory(limit)
    }
    pub fn refresh_markdown(
        &mut self,
        id: &str,
        http: &AuthorityHttp,
    ) -> Result<
        (
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        ),
        AuthorityError,
    > {
        let mut encoded = String::new();
        for b in id.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_') {
                encoded.push(char::from(b));
            } else {
                return Err(AuthorityError::Invalid("invalid note id".into()));
            }
        }
        let response = http.get(&format!("/v1/authoritative/markdown-by-id?id={encoded}"))?;
        let note: orbit_runtime::knowledge_service::KnowledgeDocumentInfo = serde_json::from_value(
            response
                .get("note")
                .cloned()
                .ok_or_else(|| AuthorityError::Invalid("missing note".into()))?,
        )
        .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        if note.id != id {
            return Err(AuthorityError::Invalid("wrong server note ID".into()));
        }
        let body = response
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| AuthorityError::Invalid("missing Markdown body".into()))?
            .to_owned();
        self.replica.cache_markdown(&note, &body)?;
        Ok((note, body))
    }
    /// Refresh all accepted Markdown documents from one authoritative inventory.
    /// Download/validate every body before replacing the local mirror, so a
    /// network failure leaves the last complete offline mirror untouched.
    /// Pending edits stay in the independent outbox and are never projected.
    pub fn sync_markdown_mirror(&mut self, http: &AuthorityHttp) -> Result<usize, AuthorityError> {
        self.replica.sync_markdown_mirror(http)
    }

    pub fn markdown_mirror_status(
        &self,
    ) -> Result<crate::authority::MarkdownMirrorStatus, AuthorityError> {
        self.replica.markdown_mirror_status()
    }

    pub fn sync_markdown_mirror_detailed(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::authority::MarkdownMirrorSyncReport, AuthorityError> {
        self.replica.sync_markdown_mirror_detailed(http)
    }

    pub fn markdown_unresolved(&self) -> Result<Vec<Value>, AuthorityError> {
        self.replica.markdown_unresolved()
    }
    pub fn flush_markdown(&mut self, http: &AuthorityHttp) -> Result<FlushOutcome, AuthorityError> {
        self.replica.flush_markdown(http)
    }
    /// Requires network and a device bearer token; server validates all writes.
    pub fn refresh(&mut self, http: &AuthorityHttp) -> Result<ConnectedWorkspace, AuthorityError> {
        self.replica.refresh(http)?;
        self.workspace()
    }
    pub fn flush(&mut self, http: &AuthorityHttp) -> Result<FlushOutcome, AuthorityError> {
        self.replica.flush(http)
    }
    pub fn reconnect(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::authority::ReconnectReport, AuthorityError> {
        self.replica.reconnect(http)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_markdown_cache_survives_reopen_and_ignores_pending_edits() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("replica.sqlite");
        let info = orbit_runtime::knowledge_service::KnowledgeDocumentInfo {
            id: "note-1".into(),
            path: "Work/note.md".into(),
            title: "note".into(),
        };
        let first = "---\norbit_id: note-1\n---\n# Accepted";
        {
            let mut client = ConnectedOrbitClient::open(&db, "https://orbit.test").unwrap();
            assert!(client.cached_markdown("note-1").unwrap().is_none());
            client.replica.cache_markdown(&info, first).unwrap();
            client
                .queue_markdown(
                    "pending-edit",
                    &orbit_runtime::markdown_commands::MarkdownCommand::Update {
                        note_id: "note-1".into(),
                        expected: first.into(),
                        replacement: "---\norbit_id: note-1\n---\n# Pending".into(),
                    },
                )
                .unwrap();
            assert_eq!(client.cached_markdown("note-1").unwrap().unwrap().1, first);
            assert!(
                client
                    .replica
                    .cache_markdown(&info, "---\norbit_id: wrong\n---\nunsafe")
                    .is_err()
            );
        }
        let reopened = ConnectedOrbitClient::open(&db, "https://orbit.test").unwrap();
        assert_eq!(
            reopened.cached_markdown("note-1").unwrap().unwrap().1,
            first
        );
        assert_eq!(reopened.markdown_unresolved().unwrap().len(), 1);
    }
    #[test]
    fn embedded_reads_only_accepted_state_and_queues_all_six_families() {
        let mut client = ConnectedOrbitClient::open(":memory:", "http://localhost:8787").unwrap();
        let empty = client.workspace().unwrap();
        assert_eq!(empty.revision, None);
        assert!(empty.projects.is_empty());
        client.queue_project("p", "p1", "Personal").unwrap();
        client.queue_task("t", "t1", "Review", Some("p1")).unwrap();
        client.queue_task_status("ts", "t1", "ready").unwrap();
        client.queue_habit("h", "h1", "Read", "daily").unwrap();
        client
            .queue_habit_occurrence("ho", "o1", "h1", 123)
            .unwrap();
        client
            .queue_metric_target("m", "m1", 0, 10, "increasing")
            .unwrap();
        client.queue_metric_observation("mo", "m1", 124, 1).unwrap();
        let offline = client.workspace().unwrap();
        assert_eq!(offline.pending_count, 7);
        assert_eq!(offline.revision, None);
        assert!(offline.tasks.is_empty());
        assert_eq!(client.unresolved().unwrap().len(), 7);
    }
    #[test]
    fn connected_workspace_requires_complete_accepted_payload() {
        assert!(
            ConnectedWorkspace::from_snapshot(Some(json!({"revision":1,"tasks":[]})), 0).is_err()
        );
        let complete = json!({"revision":3,"history_fingerprint":"a".repeat(64),"projects":[],"tasks":[{"id":"task"}],"task_dependencies":[],"habits":[],"habit_occurrences":[],"metrics":[],"metric_observations":[],"schedule":[],"routines":[],"milestones":[],"strategies":[]});
        let workspace = ConnectedWorkspace::from_snapshot(Some(complete), 2).unwrap();
        assert_eq!(workspace.tasks.len(), 1);
        assert_eq!(workspace.pending_count, 2);
    }
    #[test]
    fn connected_replica_refuses_other_server() {
        let mut client = ConnectedOrbitClient::open(":memory:", "https://one.example").unwrap();
        assert!(client.replica.bind("https://two.example").is_err());
    }
}
