//! Thin CLI adapter over the native Rust authoritative replica.
//! No local-only Orbit domain store or Git workspace is modified.
use crate::commands::{ReplicaAction, ReplicaKind, SyncReplicaArgs};
use orbit_client::authority::{AuthorityHttp, AuthorityReplica, CommandKind, FlushOutcome};
use serde_json::{Value, json};

pub fn invoke(args: SyncReplicaArgs) -> Result<Value, String> {
    let mut replica = AuthorityReplica::open(&args.db).map_err(format_error)?;
    replica.bind(&args.server).map_err(format_error)?;
    match args.action {
        ReplicaAction::CachedMarkdown { limit } => {
            let notes = replica
                .cached_markdown_inventory(limit)
                .map_err(format_error)?;
            Ok(
                json!({"accepted_cache":true,"mirror":replica.markdown_mirror_status().map_err(format_error)?,"notes":notes}),
            )
        }
        ReplicaAction::SearchMarkdown { query, limit } => {
            let notes = replica
                .search_cached_markdown(&query, limit)
                .map_err(format_error)?;
            Ok(
                json!({"accepted_cache":true,"mirror":replica.markdown_mirror_status().map_err(format_error)?,"notes":notes}),
            )
        }
        ReplicaAction::Snapshot => {
            Ok(json!({"accepted":replica.snapshot().map_err(format_error)?}))
        }
        ReplicaAction::Audit => {
            serde_json::to_value(replica.audit().map_err(format_error)?).map_err(|e| e.to_string())
        }
        ReplicaAction::Status => Ok(
            json!({"accepted_revision":replica.revision().map_err(format_error)?,
            "pending_or_conflicted":replica.pending_count().map_err(format_error)?}),
        ),
        ReplicaAction::WorkspaceSync => {
            serde_json::to_value(replica.workspace_sync_report().map_err(format_error)?)
                .map_err(|e| e.to_string())
        }
        ReplicaAction::SyncHealth => {
            serde_json::to_value(replica.sync_health().map_err(format_error)?)
                .map_err(|e| e.to_string())
        }
        ReplicaAction::Convergence => {
            serde_json::to_value(replica.convergence_status().map_err(format_error)?)
                .map_err(|e| e.to_string())
        }
        action @ (ReplicaAction::ObserveWorkspaceSync
        | ReplicaAction::ObserveSyncHealth
        | ReplicaAction::ObserveConvergence
        | ReplicaAction::ReviewConflictOnline { .. }) => {
            let token = args
                .token
                .as_deref()
                .ok_or("ORBIT_SYNC_HTTP_TOKEN required for online status")?;
            let device = args
                .device
                .as_deref()
                .ok_or("enrolled device required for online status")?;
            let http = AuthorityHttp::new(&args.server, token, device).map_err(format_error)?;
            match action {
                ReplicaAction::ObserveWorkspaceSync => serde_json::to_value(
                    replica
                        .observe_workspace_sync_report(&http)
                        .map_err(format_error)?,
                )
                .map_err(|e| e.to_string()),
                ReplicaAction::ObserveSyncHealth => {
                    serde_json::to_value(replica.observe_sync_health(&http).map_err(format_error)?)
                        .map_err(|e| e.to_string())
                }
                ReplicaAction::ObserveConvergence => {
                    serde_json::to_value(replica.observe_convergence(&http).map_err(format_error)?)
                        .map_err(|e| e.to_string())
                }
                ReplicaAction::ReviewConflictOnline { command_id } => serde_json::to_value(
                    replica
                        .review_conflict_online(&http, &command_id)
                        .map_err(format_error)?,
                )
                .map_err(|e| e.to_string()),
                _ => unreachable!(),
            }
        }
        ReplicaAction::ReviewConflict { command_id } => {
            serde_json::to_value(replica.conflict_review(&command_id).map_err(format_error)?)
                .map_err(|e| e.to_string())
        }
        ReplicaAction::ResolveReviewed {
            review_file,
            replacement_id,
            kind,
            action_json,
        } => {
            let raw = std::fs::read_to_string(&review_file).map_err(|e| e.to_string())?;
            let review: orbit_client::authority::ConflictReview = serde_json::from_str(&raw)
                .map_err(|e| format!("invalid reviewed conflict: {e}"))?;
            let action: Value = serde_json::from_str(&action_json)
                .map_err(|_| "action must be a JSON object".to_string())?;
            let token = args
                .token
                .as_deref()
                .ok_or("ORBIT_SYNC_HTTP_TOKEN required to verify current server history")?;
            let device = args
                .device
                .as_deref()
                .ok_or("enrolled device required for server reconciliation")?;
            let http = AuthorityHttp::new(&args.server, token, device).map_err(format_error)?;
            replica
                .replace_reviewed_conflict_online(
                    &http,
                    &review,
                    &replacement_id,
                    map_kind(kind),
                    &action,
                )
                .map_err(format_error)?;
            Ok(
                json!({"superseded":review.rejected_id,"queued":replacement_id,
                "accepted":false,"reviewed_revision":review.accepted_revision}),
            )
        }
        ReplicaAction::ResolveReviewedBatch { file } => {
            let decisions = read_resolution_batch(&file)?;
            let token = args
                .token
                .as_deref()
                .ok_or("ORBIT_SYNC_HTTP_TOKEN required for reviewed batch")?;
            let device = args
                .device
                .as_deref()
                .ok_or("enrolled device required for reviewed batch")?;
            let http = AuthorityHttp::new(&args.server, token, device).map_err(format_error)?;
            let count = replica
                .replace_reviewed_batch_online(&http, &decisions)
                .map_err(format_error)?;
            Ok(
                json!({"staged":count,"accepted":false,"atomic_local_staging":true,
                "server_acceptance_is_sequential":true}),
            )
        }
        ReplicaAction::Pending => {
            Ok(json!({"unresolved":replica.unresolved().map_err(format_error)?}))
        }
        ReplicaAction::Receipt { command_id } => Ok(json!({
            "command":replica.command_status(&command_id).map_err(format_error)?,
        })),
        ReplicaAction::Accepted { family } => {
            let snapshot = replica
                .snapshot()
                .map_err(format_error)?
                .ok_or("replica has not received an accepted snapshot; run refresh online first")?;
            let records = snapshot
                .get(family.key())
                .and_then(Value::as_array)
                .ok_or("accepted snapshot has an invalid domain projection")?;
            Ok(
                json!({"revision":snapshot["revision"],"family":family.key(),"records":records,
                "accepted":true,"pending_changes_included":false}),
            )
        }
        ReplicaAction::PreviewWorkflow { file } => {
            let workflow = crate::replica_workflow::WorkflowManifest::read(&file)?;
            serde_json::to_value(workflow.preview()).map_err(|e| e.to_string())
        }
        ReplicaAction::StageWorkflow { file } => {
            let workflow = crate::replica_workflow::WorkflowManifest::read(&file)?;
            workflow.stage(&mut replica)
        }
        ReplicaAction::Queue {
            command_id,
            kind,
            action_json,
        } => {
            let value: Value = serde_json::from_str(&action_json)
                .map_err(|_| "--action-json must be a JSON object".to_string())?;
            let kind = map_kind(kind);
            replica
                .queue(&command_id, kind, &value)
                .map_err(format_error)?;
            Ok(json!({"queued":command_id,"accepted":false}))
        }
        action @ (ReplicaAction::Refresh
        | ReplicaAction::Flush
        | ReplicaAction::SyncMarkdown
        | ReplicaAction::Reconnect
        | ReplicaAction::Converge) => {
            let token = args
                .token
                .as_deref()
                .ok_or("ORBIT_SYNC_HTTP_TOKEN is required for network requests")?;
            let device = args
                .device
                .as_deref()
                .ok_or("--device or ORBIT_AUTHORITY_DEVICE is required for network requests")?;
            let http = AuthorityHttp::new(&args.server, token, device).map_err(format_error)?;
            match action {
                ReplicaAction::Converge => {
                    serde_json::to_value(replica.converge(&http).map_err(format_error)?)
                        .map_err(|e| e.to_string())
                }
                ReplicaAction::Reconnect => {
                    serde_json::to_value(replica.reconnect(&http).map_err(format_error)?)
                        .map_err(|e| e.to_string())
                }
                ReplicaAction::SyncMarkdown => {
                    let report = replica
                        .sync_markdown_mirror_detailed(&http)
                        .map_err(format_error)?;
                    Ok(
                        json!({"refresh":report,"mirror":replica.markdown_mirror_status().map_err(format_error)?,"pending_intentions_unchanged":true}),
                    )
                }
                ReplicaAction::Refresh => {
                    let revision = replica.refresh(&http).map_err(format_error)?;
                    Ok(json!({"accepted_revision":revision,"refreshed":true}))
                }
                ReplicaAction::Flush => {
                    let result = replica.flush(&http).map_err(format_error)?;
                    match result {
                        FlushOutcome::Empty => Ok(json!({"accepted":0,"status":"empty"})),
                        FlushOutcome::Accepted(n) => {
                            Ok(json!({"accepted":n,"status":"accepted","refresh_required":true}))
                        }
                        FlushOutcome::Blocked {
                            accepted,
                            command_id,
                            reason,
                        } => Ok(
                            json!({"accepted":accepted,"status":"conflict","command_id":command_id,"reason":reason}),
                        ),
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
fn format_error(error: orbit_client::authority::AuthorityError) -> String {
    match error {
        orbit_client::authority::AuthorityError::Conflict(s)
        | orbit_client::authority::AuthorityError::Invalid(s)
        | orbit_client::authority::AuthorityError::Unauthorized(s)
        | orbit_client::authority::AuthorityError::Unavailable(s) => s,
        orbit_client::authority::AuthorityError::Transport(_) => {
            "authority server unavailable (queued changes retained)".into()
        }
        orbit_client::authority::AuthorityError::Storage(_) => {
            "authority replica storage error".into()
        }
    }
}

/// Every canonical authority command family shares the same durable ordered queue.
/// Keep the CLI in lockstep with the server dispatcher's supported families.
fn map_kind(kind: ReplicaKind) -> CommandKind {
    match kind {
        ReplicaKind::Project => CommandKind::Project,
        ReplicaKind::Task => CommandKind::Task,
        ReplicaKind::Evidence => CommandKind::Evidence,
        ReplicaKind::Schedule => CommandKind::Schedule,
        ReplicaKind::Routine => CommandKind::Routine,
        ReplicaKind::Milestone => CommandKind::Milestone,
        ReplicaKind::Strategy => CommandKind::Strategy,
        ReplicaKind::Markdown => CommandKind::Markdown,
        ReplicaKind::FolderCreate => CommandKind::FolderCreate,
        ReplicaKind::FolderMove => CommandKind::FolderMove,
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::{Cli, Command};
    use clap::Parser;
    #[test]
    fn accepted_markdown_sync_and_offline_search_have_cli_commands() {
        for args in [
            vec!["sync-markdown"],
            vec!["reconnect"],
            vec!["search-markdown", "status", "--limit", "20"],
            vec!["cached-markdown", "--limit", "100"],
        ] {
            let mut complete = vec![
                "orbit",
                "sync",
                "replica",
                "--db",
                "/tmp/orbit-test.sqlite",
                "--server",
                "http://localhost:3000",
            ];
            complete.extend(args);
            assert!(Cli::try_parse_from(complete).is_ok());
        }
    }
    #[test]
    fn every_remote_family_is_exposed_by_cli() {
        for family in [
            "project",
            "task",
            "evidence",
            "schedule",
            "routine",
            "milestone",
            "strategy",
            "markdown",
            "folder-create",
            "folder-move",
        ] {
            let cli = Cli::try_parse_from([
                "orbit",
                "sync",
                "replica",
                "--db",
                "/tmp/orbit-test.sqlite",
                "--server",
                "http://localhost:3000",
                "queue",
                "--command-id",
                "cmd-1",
                "--kind",
                family,
                "--action-json",
                r#"{"type":"create"}"#,
            ]);
            assert!(cli.is_ok(), "family {family} not supported by CLI: {cli:?}");
        }
    }
    #[test]
    fn accepted_read_family_names_are_exposed() {
        for family in [
            "projects",
            "tasks",
            "habits",
            "habit-occurrences",
            "metrics",
            "metric-observations",
            "schedule",
            "routines",
            "milestones",
            "strategies",
        ] {
            assert!(
                Cli::try_parse_from([
                    "orbit",
                    "sync",
                    "replica",
                    "--db",
                    "/tmp/orbit-test.sqlite",
                    "--server",
                    "http://localhost:3000",
                    "accepted",
                    "--family",
                    family
                ])
                .is_ok(),
                "accepted domain family {family} missing"
            );
        }
    }
    #[test]
    fn receipt_command_requires_identity() {
        assert!(
            Cli::try_parse_from([
                "orbit",
                "sync",
                "replica",
                "--db",
                "/tmp/orbit-test.sqlite",
                "--server",
                "http://localhost:3000",
                "receipt"
            ])
            .is_err()
        );
        let parsed = Cli::try_parse_from([
            "orbit",
            "sync",
            "replica",
            "--db",
            "/tmp/orbit-test.sqlite",
            "--server",
            "http://localhost:3000",
            "receipt",
            "--command-id",
            "cmd-42",
        ]);
        assert!(matches!(parsed.unwrap().command, Command::Sync(_)));
    }
}

#[cfg(test)]
mod workflow_cli_contract_tests {
    use crate::commands::Cli;
    use clap::Parser;

    fn parse(args: &[&str]) -> bool {
        let mut complete = vec![
            "orbit",
            "sync",
            "replica",
            "--db",
            "/tmp/replica.sqlite",
            "--server",
            "http://localhost:3000",
        ];
        complete.extend_from_slice(args);
        Cli::try_parse_from(complete).is_ok()
    }

    #[test]
    fn manifest_preview_is_exposed_as_local_read_only_command() {
        assert!(parse(&["preview-workflow", "--file", "plan.json"]));
        assert!(!parse(&["preview-workflow"]));
        assert!(!parse(&["preview-workflow", "--file"]));
    }

    #[test]
    fn manifest_stage_is_exposed_as_offline_command() {
        assert!(parse(&["stage-workflow", "--file", "plan.json"]));
        assert!(!parse(&["stage-workflow"]));
    }

    #[test]
    fn replica_audit_requires_no_network_token() {
        assert!(parse(&["audit"]));
        assert!(parse(&["pending"]));
        assert!(parse(&["status"]));
    }

    #[test]
    fn existing_queue_and_flush_paths_remain_available() {
        assert!(parse(&["flush"]));
        assert!(parse(&[
            "queue",
            "--command-id",
            "one",
            "--kind",
            "task",
            "--action-json",
            r#"{"type":"create"}"#
        ]));
        assert!(parse(&["receipt", "--command-id", "one"]));
    }

    #[test]
    fn manifest_command_names_are_not_confused_with_single_command_queue() {
        for action in ["preview-workflow", "stage-workflow"] {
            assert!(!parse(&[action, "--command-id", "id", "--kind", "task"]));
        }
    }
}

#[cfg(test)]
mod reconnect_cli_tests {
    use crate::commands::Cli;
    use clap::Parser;

    #[test]
    fn reconnect_is_single_explicit_online_operation() {
        let args = [
            "orbit",
            "sync",
            "replica",
            "--db",
            "/tmp/offline.sqlite",
            "--server",
            "http://localhost:3000",
            "reconnect",
        ];
        assert!(Cli::try_parse_from(args).is_ok());
    }

    #[test]
    fn reconnect_does_not_require_remote_identity_for_offline_parsing() {
        // Credentials are checked at execution time. This keeps CLI help
        // usable without a server while never permitting anonymous replay.
        assert!(
            Cli::try_parse_from([
                "orbit",
                "sync",
                "replica",
                "--db",
                "/tmp/offline.sqlite",
                "--server",
                "http://localhost:3000",
                "reconnect"
            ])
            .is_ok()
        );
    }
}

#[cfg(test)]
mod convergence_cli_contract_tests {
    use crate::commands::Cli;
    use clap::Parser;

    fn accepts(arguments: &[&str]) -> bool {
        let mut argv = vec![
            "orbit",
            "sync",
            "replica",
            "--db",
            "/tmp/client-replica.sqlite",
            "--server",
            "http://localhost:3000",
        ];
        argv.extend_from_slice(arguments);
        Cli::try_parse_from(argv).is_ok()
    }
    #[test]
    fn offline_status_does_not_require_server_credentials() {
        for command in ["convergence", "pending", "audit", "status"] {
            assert!(
                accepts(&[command]),
                "missing offline status command {command}"
            );
        }
    }
    #[test]
    fn online_convergence_is_an_explicit_network_operation() {
        for command in ["converge", "observe-convergence", "reconnect"] {
            assert!(
                accepts(&[command]),
                "missing connected sync command {command}"
            );
        }
    }
    #[test]
    fn conflict_review_offline_and_online_have_distinct_commands() {
        assert!(accepts(&["review-conflict", "--command-id", "rejected-1"]));
        assert!(accepts(&[
            "review-conflict-online",
            "--command-id",
            "rejected-1"
        ]));
        assert!(!accepts(&["review-conflict"]));
        assert!(!accepts(&["review-conflict-online"]));
    }
    #[test]
    fn reviewed_resolution_requires_explicit_review_file_and_identity() {
        let complete = [
            "resolve-reviewed",
            "--review-file",
            "review.json",
            "--replacement-id",
            "new-1",
            "--kind",
            "task",
            "--action-json",
            r#"{"type":"create","task_id":"t","title":"reviewed"}"#,
        ];
        assert!(accepts(&complete));
        assert!(!accepts(&["resolve-reviewed"]));
        assert!(!accepts(&["resolve-reviewed", "--replacement-id", "new-1"]));
        assert!(!accepts(&[
            "resolve-reviewed",
            "--review-file",
            "review.json",
            "--replacement-id",
            "new-1",
            "--kind",
            "task"
        ]));
    }
    #[test]
    fn reviewed_resolution_is_not_confused_with_legacy_replacement() {
        assert!(!accepts(&[
            "resolve-reviewed",
            "--old-command-id",
            "rejected-1",
            "--new-command-id",
            "replacement-1",
            "--kind",
            "task",
            "--action-json",
            r#"{"type":"create"}"#
        ]));
    }
    #[test]
    fn multi_domain_manifest_commands_remain_available_after_convergence() {
        assert!(accepts(&["preview-workflow", "--file", "workflow.json"]));
        assert!(accepts(&["stage-workflow", "--file", "workflow.json"]));
        assert!(accepts(&["converge"]));
    }
    #[test]
    fn unknown_server_families_remain_rejected_at_cli_boundary() {
        assert!(!accepts(&[
            "queue",
            "--command-id",
            "cmd-1",
            "--kind",
            "obsolete-relay",
            "--action-json",
            r#"{"type":"create"}"#
        ]));
        assert!(!accepts(&[
            "resolve-reviewed",
            "--review-file",
            "review.json",
            "--replacement-id",
            "new-1",
            "--kind",
            "sql",
            "--action-json",
            r#"{"type":"create"}"#
        ]));
    }
}

/// A reviewed batch is user-controlled data, not an authority to rebase.
/// Parse the whole manifest before touching the database or network.
fn read_resolution_batch(
    path: &std::path::Path,
) -> Result<Vec<orbit_client::authority::ReviewedReplacement>, String> {
    use orbit_client::authority::{ConflictReview, ReviewedReplacement};
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.len() > 1_048_576 {
        return Err("reviewed batch exceeds 1 MiB".into());
    }
    let data: Value =
        serde_json::from_str(&raw).map_err(|e| format!("invalid reviewed batch JSON: {e}"))?;
    let steps = data
        .get("resolutions")
        .and_then(Value::as_array)
        .ok_or("reviewed batch needs a resolutions array")?;
    if steps.is_empty() || steps.len() > 100 {
        return Err("reviewed batch must contain 1..=100 resolutions".into());
    }
    let mut items = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        let review: ConflictReview = serde_json::from_value(
            step.get("review")
                .cloned()
                .ok_or_else(|| format!("step {index}: missing review"))?,
        )
        .map_err(|e| format!("step {index}: invalid review: {e}"))?;
        let replacement_id = step
            .get("replacement_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("step {index}: missing replacement_id"))?
            .to_string();
        let kind = match step.get("family").and_then(Value::as_str) {
            Some("project") => CommandKind::Project,
            Some("task") => CommandKind::Task,
            Some("evidence") => CommandKind::Evidence,
            Some("schedule") => CommandKind::Schedule,
            Some("routine") => CommandKind::Routine,
            Some("milestone") => CommandKind::Milestone,
            Some("strategy") => CommandKind::Strategy,
            _ => return Err(format!("step {index}: unsupported reviewed family")),
        };
        let action = step
            .get("action")
            .cloned()
            .ok_or_else(|| format!("step {index}: missing action"))?;
        items.push(ReviewedReplacement {
            review,
            replacement_id,
            kind,
            action,
        });
    }
    Ok(items)
}

#[cfg(test)]
mod reviewed_batch_manifest_tests {
    use super::*;
    #[test]
    fn missing_or_empty_resolution_manifest_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("empty.json");
        std::fs::write(&file, r#"{"resolutions":[]}"#).unwrap();
        assert!(read_resolution_batch(&file).is_err());
        std::fs::write(&file, r#"{"resolutions":[{}]}"#).unwrap();
        assert!(read_resolution_batch(&file).is_err());
    }
    #[test]
    fn reviewed_batch_rejects_unknown_domain_family_before_network() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("invalid.json");
        let review = json!({"rejected_id":"old","kind":"task","rejected_action":{},
            "ordinal":1,"accepted_revision":1,"history_fingerprint":"a".repeat(64)});
        std::fs::write(
            &file,
            json!({"resolutions":[{"review":review,
            "replacement_id":"new","family":"other","action":{"type":"edit"}}]})
            .to_string(),
        )
        .unwrap();
        assert!(
            read_resolution_batch(&file)
                .unwrap_err()
                .contains("unsupported")
        );
    }
}

#[cfg(test)]
mod reviewed_batch_contract_tests {
    use super::*;
    fn write_manifest(value: &Value) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("reviewed-batch.json");
        std::fs::write(&file, value.to_string()).unwrap();
        (temp, file)
    }
    fn valid_review(id: &str, order: i64) -> Value {
        json!({"rejected_id":id,"kind":"task","rejected_action":{
            "type":"create","task_id":id,"title":"Old"},
            "ordinal":order,"accepted_revision":4,"history_fingerprint":"a".repeat(64)})
    }
    fn row(id: &str, new: &str, order: i64) -> Value {
        json!({"review":valid_review(id,order),"replacement_id":new,"family":"task",
            "action":{"type":"change_status","task_id":id,"status":"ready"}})
    }
    #[test]
    fn valid_reviewed_manifest_preserves_requested_order() {
        let (_temp, file) = write_manifest(&json!({"resolutions":[
            row("old-1","new-1",1),row("old-2","new-2",2)]}));
        let steps = read_resolution_batch(&file).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].review.rejected_id, "old-1");
        assert_eq!(steps[1].replacement_id, "new-2");
        assert_eq!(steps[1].review.accepted_revision, 4);
    }
    #[test]
    fn file_conflict_families_cannot_be_injected_into_domain_manifest() {
        for family in ["markdown", "folder_create", "folder_move", "other"] {
            let mut value = row("old", "new", 1);
            value["family"] = json!(family);
            let (_temp, file) = write_manifest(&json!({"resolutions":[value]}));
            assert!(read_resolution_batch(&file).is_err());
        }
    }
    #[test]
    fn missing_review_fields_reject_entire_manifest() {
        for field in [
            "rejected_id",
            "kind",
            "rejected_action",
            "ordinal",
            "accepted_revision",
            "history_fingerprint",
        ] {
            let mut value = row("old", "new", 1);
            value["review"].as_object_mut().unwrap().remove(field);
            let (_temp, file) = write_manifest(&json!({"resolutions":[value]}));
            assert!(read_resolution_batch(&file).is_err());
        }
    }
    #[test]
    fn malformed_late_step_cannot_produce_a_partial_decision_list() {
        let (_temp, file) = write_manifest(&json!({"resolutions":[
            row("old-1","new-1",1),{"review":valid_review("old-2",2),
                "replacement_id":"new-2","family":"unknown","action":{}}]}));
        assert!(read_resolution_batch(&file).unwrap_err().contains("step 1"));
    }
    #[test]
    fn huge_manifest_is_rejected_before_deserialization() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("oversized.json");
        std::fs::write(
            &path,
            format!(
                "{{\"resolutions\":[],\"padding\":\"{}\"}}",
                "x".repeat(1_048_577)
            ),
        )
        .unwrap();
        assert!(read_resolution_batch(&path).unwrap_err().contains("MiB"));
    }
    #[test]
    fn too_many_reviewed_steps_are_rejected_before_execution() {
        let rows: Vec<Value> = (0..101)
            .map(|n| row(&format!("old-{n}"), &format!("new-{n}"), n + 1))
            .collect();
        let (_temp, file) = write_manifest(&json!({"resolutions":rows}));
        assert!(read_resolution_batch(&file).is_err());
    }
    #[test]
    fn single_reviewed_step_is_accepted_for_downstream_runtime_validation() {
        let (_temp, file) = write_manifest(&json!({"resolutions":[row("old","new",1)]}));
        let parsed = read_resolution_batch(&file).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].kind, CommandKind::Task);
        assert_eq!(parsed[0].action["type"], "change_status");
    }
}

#[cfg(test)]
mod sync_health_cli_contract_tests {
    use crate::commands::{Cli, Command};
    use clap::Parser;

    fn parse(subcommand: &str) -> Result<Cli, clap::Error> {
        Cli::try_parse_from([
            "orbit",
            "sync",
            "replica",
            "--db",
            "/tmp/orbit-replica.sqlite",
            "--server",
            "http://localhost:3000",
            subcommand,
        ])
    }
    #[test]
    fn local_health_is_a_read_only_cli_subcommand() {
        assert!(parse("sync-health").is_ok());
        assert!(parse("workspace-sync").is_ok());
    }
    #[test]
    fn online_observation_has_separate_explicit_subcommands() {
        assert!(parse("observe-sync-health").is_ok());
        assert!(parse("observe-workspace-sync").is_ok());
    }
    #[test]
    fn health_commands_remain_part_of_the_canonical_sync_tree() {
        for name in [
            "sync-health",
            "workspace-sync",
            "observe-sync-health",
            "observe-workspace-sync",
        ] {
            let cli = parse(name).unwrap();
            assert!(matches!(cli.command, Command::Sync(_)));
        }
    }
    #[test]
    fn health_does_not_require_device_credentials_when_offline() {
        assert!(parse("sync-health").is_ok());
        assert!(parse("workspace-sync").is_ok());
    }
    #[test]
    fn no_implicit_online_observation_in_offline_report_name() {
        let cli = parse("workspace-sync").unwrap();
        let debug = format!("{:?}", cli.command);
        assert!(debug.contains("WorkspaceSync"));
        assert!(!debug.contains("ObserveWorkspaceSync"));
    }
    #[test]
    fn typoed_unsafe_health_operations_do_not_route_as_a_command() {
        for name in [
            "repair-workspace-sync",
            "force-converged",
            "accept-pending-as-synced",
            "rewrite-accepted-state",
        ] {
            assert!(parse(name).is_err());
        }
    }
}
