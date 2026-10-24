//! Offline-safe reviewed authority workflow manifests.
//!
//! The CLI never applies a manifest to the authoritative domain store: it
//! validates and stages canonical intentions in the replica's ordered outbox.
//! `preview` is read-only; `stage` is one SQLite transaction. Submission is
//! intentionally separate and requires enrolled-device authentication.
use orbit_client::authority::{AuthorityError, AuthorityReplica, CommandKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, fs, path::Path};

const MAX_WORKFLOW_BYTES: u64 = 2 * 1024 * 1024;
const MAX_COMMANDS: usize = 100;
const MAX_ACTION_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowManifest {
    pub version: u32,
    pub name: String,
    pub commands: Vec<ManifestCommand>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCommand {
    pub command_id: String,
    pub kind: ManifestKind,
    pub action: Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestKind {
    Project,
    Task,
    Evidence,
    Schedule,
    Routine,
    Milestone,
    Strategy,
    Markdown,
    FolderCreate,
    FolderMove,
}

impl ManifestKind {
    pub fn into_kind(self) -> CommandKind {
        match self {
            Self::Project => CommandKind::Project,
            Self::Task => CommandKind::Task,
            Self::Evidence => CommandKind::Evidence,
            Self::Schedule => CommandKind::Schedule,
            Self::Routine => CommandKind::Routine,
            Self::Milestone => CommandKind::Milestone,
            Self::Strategy => CommandKind::Strategy,
            Self::Markdown => CommandKind::Markdown,
            Self::FolderCreate => CommandKind::FolderCreate,
            Self::FolderMove => CommandKind::FolderMove,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Task => "task",
            Self::Evidence => "evidence",
            Self::Schedule => "schedule",
            Self::Routine => "routine",
            Self::Milestone => "milestone",
            Self::Strategy => "strategy",
            Self::Markdown => "markdown",
            Self::FolderCreate => "folder_create",
            Self::FolderMove => "folder_move",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct WorkflowPreview {
    pub name: String,
    pub version: u32,
    pub command_count: usize,
    pub domains: Vec<DomainCount>,
    pub commands: Vec<CommandPreview>,
    pub accepted: bool,
    pub staged: bool,
}

#[derive(Debug, Serialize)]
pub struct DomainCount {
    pub family: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct CommandPreview {
    pub index: usize,
    pub command_id: String,
    pub kind: String,
    pub summary: String,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn summary(kind: ManifestKind, action: &Value) -> String {
    // The preview deliberately never reveals Markdown bodies, credential-like
    // values or arbitrary action fields. It's suitable for terminal output.
    let keys: &[&str] = match kind {
        ManifestKind::Project => &["project_id", "name"],
        ManifestKind::Task => &["type", "task_id", "project_id"],
        ManifestKind::Evidence => &["type", "habit_id", "metric_id"],
        ManifestKind::Schedule => &["type", "block_id", "task_id"],
        ManifestKind::Routine => &["type", "routine_id"],
        ManifestKind::Milestone => &["type", "milestone_id", "project_id"],
        ManifestKind::Strategy => &["type", "project_id"],
        ManifestKind::Markdown => &["type", "note_id", "path"],
        ManifestKind::FolderCreate => &["path"],
        ManifestKind::FolderMove => &["from", "to"],
    };
    keys.iter()
        .filter_map(|key| {
            action
                .get(key)
                .and_then(Value::as_str)
                .map(|value| format!("{key}={}", value.chars().take(80).collect::<String>()))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl WorkflowManifest {
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata =
            fs::metadata(path).map_err(|e| format!("cannot inspect workflow manifest: {e}"))?;
        if !metadata.is_file() || metadata.len() > MAX_WORKFLOW_BYTES {
            return Err("workflow must be a regular JSON file of at most 2 MiB".into());
        }
        let input = fs::read(path).map_err(|e| format!("cannot read workflow: {e}"))?;
        if input.len() as u64 > MAX_WORKFLOW_BYTES {
            return Err("workflow exceeds the 2 MiB limit".into());
        }
        let manifest: Self =
            serde_json::from_slice(&input).map_err(|e| format!("invalid workflow JSON: {e}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported workflow version; expected 1".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return Err("workflow name must be 1..=200 bytes".into());
        }
        if self.commands.is_empty() || self.commands.len() > MAX_COMMANDS {
            return Err(format!("workflow must contain 1..={MAX_COMMANDS} commands"));
        }
        let mut ids = HashSet::new();
        for (index, command) in self.commands.iter().enumerate() {
            if !valid_id(&command.command_id) {
                return Err(format!("command {index}: invalid command_id"));
            }
            if !ids.insert(command.command_id.as_str()) {
                return Err(format!("command {index}: duplicated command_id"));
            }
            let Some(action) = command.action.as_object() else {
                return Err(format!("command {index}: action must be an object"));
            };
            if action.is_empty() {
                return Err(format!("command {index}: action must not be empty"));
            }
            if action.contains_key("command_id") || action.contains_key("expected_revision") {
                return Err(format!(
                    "command {index}: envelope fields belong to the outbox"
                ));
            }
            let bytes =
                serde_json::to_vec(&command.action).map_err(|e| format!("command {index}: {e}"))?;
            if bytes.len() > MAX_ACTION_BYTES {
                return Err(format!("command {index}: action exceeds 64 KiB"));
            }
            self.validate_action_shape(index, command)?;
        }
        Ok(())
    }

    fn validate_action_shape(&self, index: usize, command: &ManifestCommand) -> Result<(), String> {
        let object = command.action.as_object().expect("checked by validate");
        // This is a structural preflight, NOT an attempt to reproduce server
        // business rules. Domain validation remains authoritative on the host.
        let required = match command.kind {
            ManifestKind::Project => &["project_id", "name"][..],
            ManifestKind::Markdown => &["kind", "note_id"][..],
            ManifestKind::FolderCreate => &["path"][..],
            ManifestKind::FolderMove => &["from", "to"][..],
            _ => &["type"][..],
        };
        for key in required {
            if !object
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|v| !v.trim().is_empty())
            {
                return Err(format!("command {index}: missing nonempty {key}"));
            }
        }
        if matches!(
            command.kind,
            ManifestKind::FolderCreate | ManifestKind::FolderMove
        ) {
            for key in required {
                let path = object[*key].as_str().expect("checked above");
                if path.starts_with('/')
                    || path.contains('\\')
                    || path
                        .split('/')
                        .any(|p| p.is_empty() || p == "." || p == "..")
                {
                    return Err(format!("command {index}: unsafe vault-relative {key}"));
                }
            }
        }
        Ok(())
    }

    pub fn preview(&self) -> WorkflowPreview {
        let mut counts = std::collections::BTreeMap::new();
        let commands = self
            .commands
            .iter()
            .enumerate()
            .map(|(index, command)| {
                *counts
                    .entry(command.kind.label().to_string())
                    .or_insert(0usize) += 1;
                CommandPreview {
                    index,
                    command_id: command.command_id.clone(),
                    kind: command.kind.label().into(),
                    summary: summary(command.kind, &command.action),
                }
            })
            .collect();
        WorkflowPreview {
            name: self.name.clone(),
            version: self.version,
            command_count: self.commands.len(),
            domains: counts
                .into_iter()
                .map(|(family, count)| DomainCount { family, count })
                .collect(),
            commands,
            accepted: false,
            staged: false,
        }
    }

    pub fn stage(&self, replica: &mut AuthorityReplica) -> Result<Value, String> {
        self.validate()?;
        let commands: Vec<_> = self
            .commands
            .iter()
            .map(|command| {
                (
                    command.command_id.clone(),
                    command.kind.into_kind(),
                    command.action.clone(),
                )
            })
            .collect();
        let inserted = replica
            .queue_batch(&commands)
            .map_err(|error| match error {
                AuthorityError::Conflict(reason)
                | AuthorityError::Invalid(reason)
                | AuthorityError::Unauthorized(reason)
                | AuthorityError::Unavailable(reason) => reason,
                AuthorityError::Storage(_) => "replica database write failed".into(),
                AuthorityError::Transport(_) => {
                    "unexpected network call during offline staging".into()
                }
            })?;
        Ok(json!({
            "name": self.name,
            "total": self.commands.len(),
            "staged": inserted,
            "already_staged": self.commands.len() - inserted,
            "accepted": false,
            "remote_execution_atomic": false,
            "next_step": "sync replica flush",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(id: &str, kind: ManifestKind, action: Value) -> ManifestCommand {
        ManifestCommand {
            command_id: id.into(),
            kind,
            action,
        }
    }
    fn valid() -> WorkflowManifest {
        WorkflowManifest {
            version: 1,
            name: "Create project and plan".into(),
            commands: vec![
                command(
                    "project-1",
                    ManifestKind::Project,
                    json!({"project_id":"p", "name":"Plan"}),
                ),
                command(
                    "task-1",
                    ManifestKind::Task,
                    json!({"type":"create", "task_id":"t", "title":"Next"}),
                ),
                command(
                    "block-1",
                    ManifestKind::Schedule,
                    json!({"type":"create", "block_id":"b"}),
                ),
            ],
        }
    }
    #[test]
    fn all_command_families_have_stable_names() {
        for (kind, name) in [
            (ManifestKind::Project, "project"),
            (ManifestKind::Task, "task"),
            (ManifestKind::Evidence, "evidence"),
            (ManifestKind::Schedule, "schedule"),
            (ManifestKind::Routine, "routine"),
            (ManifestKind::Milestone, "milestone"),
            (ManifestKind::Strategy, "strategy"),
            (ManifestKind::Markdown, "markdown"),
            (ManifestKind::FolderCreate, "folder_create"),
            (ManifestKind::FolderMove, "folder_move"),
        ] {
            assert_eq!(kind.label(), name);
        }
    }
    #[test]
    fn valid_cross_domain_plan_is_previewable() {
        let manifest = valid();
        manifest.validate().unwrap();
        let preview = manifest.preview();
        assert_eq!(preview.command_count, 3);
        assert!(!preview.accepted);
        assert!(!preview.staged);
        assert_eq!(preview.commands[0].command_id, "project-1");
    }
    #[test]
    fn duplicate_identity_is_rejected() {
        let mut manifest = valid();
        manifest.commands[1].command_id = "project-1".into();
        assert!(manifest.validate().unwrap_err().contains("duplicated"));
    }
    #[test]
    fn malformed_envelope_is_rejected() {
        let mut manifest = valid();
        manifest.commands[1].action["expected_revision"] = json!(10);
        assert!(manifest.validate().is_err());
    }
    #[test]
    fn empty_or_oversized_workflows_fail_before_database_open() {
        let mut manifest = valid();
        manifest.commands.clear();
        assert!(manifest.validate().is_err());
        for i in 0..101 {
            manifest.commands.push(command(
                &format!("c{i}"),
                ManifestKind::Task,
                json!({"type":"create"}),
            ));
        }
        assert!(manifest.validate().is_err());
    }
    #[test]
    fn traversal_and_absolute_folder_paths_rejected() {
        for path in ["../outside", "/absolute", "a/../b", "a//b", "a\\b"] {
            let manifest = WorkflowManifest {
                version: 1,
                name: "Files".into(),
                commands: vec![command(
                    "folder-1",
                    ManifestKind::FolderCreate,
                    json!({"path":path}),
                )],
            };
            assert!(manifest.validate().is_err(), "{path}");
        }
    }
    #[test]
    fn preview_redacts_arbitrary_fields() {
        let manifest = WorkflowManifest {
            version: 1,
            name: "Safe preview".into(),
            commands: vec![command(
                "markdown-1",
                ManifestKind::Markdown,
                json!({"kind":"Update","note_id":"note","replacement":"TOP SECRET"}),
            )],
        };
        manifest.validate().unwrap();
        let serialized = serde_json::to_string(&manifest.preview()).unwrap();
        assert!(!serialized.contains("TOP SECRET"));
        assert!(serialized.contains("note_id=note"));
    }
    #[test]
    fn unsupported_versions_rejected() {
        let mut manifest = valid();
        manifest.version = 2;
        assert!(manifest.validate().is_err());
    }
    #[test]
    fn invalid_ids_rejected() {
        for bad in ["", "has spaces", "with.dot", "an/slash"] {
            let mut manifest = valid();
            manifest.commands[0].command_id = bad.into();
            assert!(manifest.validate().is_err());
        }
    }
    #[test]
    fn every_family_is_serializable_in_manifest() {
        for family in [
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
        ] {
            let json = format!(
                r#"{{"version":1,"name":"x","commands":[{{"command_id":"id","kind":"{family}","action":{{"type":"create","kind":"Create","note_id":"note","path":"a","project_id":"p","name":"Project","from":"a","to":"b"}}}}]}}"#
            );
            let parsed: WorkflowManifest = serde_json::from_str(&json).unwrap();
            assert!(parsed.validate().is_ok(), "{family}");
        }
    }
}

#[cfg(test)]
mod workflow_acceptance {
    use super::*;
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Scratch {
        path: PathBuf,
    }
    impl Scratch {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "orbit-manifest-{}-{nanos}.sqlite",
                std::process::id()
            ));
            Self { path }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let _ = fs::remove_file(format!("{}{}", self.path.display(), suffix));
            }
        }
    }
    fn manifest(entries: &[(&str, ManifestKind, Value)]) -> WorkflowManifest {
        WorkflowManifest {
            version: 1,
            name: "New workspace".into(),
            commands: entries
                .iter()
                .map(|(id, kind, action)| ManifestCommand {
                    command_id: (*id).into(),
                    kind: *kind,
                    action: action.clone(),
                })
                .collect(),
        }
    }
    fn sample() -> WorkflowManifest {
        manifest(&[
            (
                "create-project",
                ManifestKind::Project,
                json!({"project_id":"p1","name":"Research"}),
            ),
            (
                "create-task",
                ManifestKind::Task,
                json!({"type":"create","task_id":"t1","title":"Collect evidence","project_id":"p1"}),
            ),
            (
                "set-metric",
                ManifestKind::Evidence,
                json!({"type":"set_metric_target","metric_id":"quality","baseline":0,"target":10,"direction":"increase"}),
            ),
            (
                "schedule",
                ManifestKind::Schedule,
                json!({"type":"create","block_id":"block1","title":"Research","starts_at_ms":1000,"duration_minutes":45,"flexibility":"movable","target_kind":"task","target_id":"t1"}),
            ),
        ])
    }
    #[test]
    fn manifest_can_be_staged_offline_in_one_transaction() {
        let dir = Scratch::new();
        let mut replica = AuthorityReplica::open(&dir.path).unwrap();
        let result = sample().stage(&mut replica).unwrap();
        assert_eq!(result["staged"], 4);
        assert_eq!(result["accepted"], false);
        assert_eq!(replica.pending_count().unwrap(), 4);
        let unresolved = replica.unresolved().unwrap();
        assert_eq!(unresolved.len(), 4);
        assert_eq!(unresolved[0]["command_id"], "create-project");
        assert_eq!(unresolved[3]["command_id"], "schedule");
        assert!(replica.snapshot().unwrap().is_none());
    }
    #[test]
    fn idempotent_repeated_manifest_does_not_add_more_commands() {
        let dir = Scratch::new();
        let mut replica = AuthorityReplica::open(&dir.path).unwrap();
        let input = sample();
        input.stage(&mut replica).unwrap();
        let repeat = input.stage(&mut replica).unwrap();
        assert_eq!(repeat["staged"], 0);
        assert_eq!(repeat["already_staged"], 4);
        assert_eq!(replica.pending_count().unwrap(), 4);
    }
    #[test]
    fn conflicting_identity_rolls_back_all_new_staged_commands() {
        let dir = Scratch::new();
        let mut replica = AuthorityReplica::open(&dir.path).unwrap();
        sample().stage(&mut replica).unwrap();
        let conflict = manifest(&[
            (
                "new-command",
                ManifestKind::Task,
                json!({"type":"create","task_id":"new","title":"New"}),
            ),
            (
                "create-project",
                ManifestKind::Project,
                json!({"project_id":"different","name":"Different"}),
            ),
        ]);
        assert!(conflict.stage(&mut replica).is_err());
        assert!(replica.command_status("new-command").unwrap().is_none());
        assert_eq!(replica.pending_count().unwrap(), 4);
    }
    #[test]
    fn existing_identity_with_different_family_is_not_a_retry() {
        let dir = Scratch::new();
        let mut replica = AuthorityReplica::open(&dir.path).unwrap();
        sample().stage(&mut replica).unwrap();
        let altered = manifest(&[(
            "create-project",
            ManifestKind::Task,
            json!({"type":"create","task_id":"p1","title":"Not a project"}),
        )]);
        assert!(altered.stage(&mut replica).is_err());
        assert_eq!(replica.pending_count().unwrap(), 4);
    }
    #[test]
    fn malformed_manifest_never_touches_replica() {
        let dir = Scratch::new();
        let mut replica = AuthorityReplica::open(&dir.path).unwrap();
        let bad = manifest(&[
            (
                "first",
                ManifestKind::Project,
                json!({"project_id":"p","name":"Plan"}),
            ),
            (
                "second",
                ManifestKind::Task,
                json!({"type":"create","expected_revision":0}),
            ),
        ]);
        assert!(bad.stage(&mut replica).is_err());
        assert_eq!(replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn accepted_state_is_never_synthesized_from_offline_queue() {
        let dir = Scratch::new();
        let mut replica = AuthorityReplica::open(&dir.path).unwrap();
        sample().stage(&mut replica).unwrap();
        assert!(replica.snapshot().unwrap().is_none());
        for command in ["create-project", "create-task", "set-metric", "schedule"] {
            let status = replica.command_status(command).unwrap().unwrap();
            assert_eq!(status["state"], "pending");
            assert!(status["receipt"].is_null());
        }
    }
    #[test]
    fn offline_workflow_persists_across_replica_restarts() {
        let dir = Scratch::new();
        {
            let mut replica = AuthorityReplica::open(&dir.path).unwrap();
            sample().stage(&mut replica).unwrap();
        }
        let replica = AuthorityReplica::open(&dir.path).unwrap();
        assert_eq!(replica.pending_count().unwrap(), 4);
        assert!(replica.snapshot().unwrap().is_none());
        assert!(replica.audit().unwrap().healthy);
    }
    #[test]
    fn preview_is_side_effect_free() {
        let dir = Scratch::new();
        let replica = AuthorityReplica::open(&dir.path).unwrap();
        let preview = sample().preview();
        assert_eq!(preview.command_count, 4);
        assert_eq!(preview.domains.len(), 4);
        assert_eq!(replica.pending_count().unwrap(), 0);
    }
    #[test]
    fn previews_never_claim_server_acceptance() {
        let preview = sample().preview();
        assert!(!preview.accepted);
        assert!(!preview.staged);
        assert!(
            !serde_json::to_string(&preview)
                .unwrap()
                .contains("accepted_revision")
        );
    }
    #[test]
    fn read_validates_and_loads_regular_json_file() {
        let dir = Scratch::new();
        let path = dir.path.with_extension("json");
        fs::write(&path, serde_json::to_vec(&sample()).unwrap()).unwrap();
        let read = WorkflowManifest::read(&path).unwrap();
        assert_eq!(read.commands.len(), 4);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn read_rejects_unknown_manifest_fields() {
        let dir = Scratch::new();
        let path = dir.path.with_extension("json");
        fs::write(
            &path,
            r#"{"version":1,"name":"x","unexpected":true,"commands":[]}"#,
        )
        .unwrap();
        assert!(WorkflowManifest::read(&path).is_err());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn read_rejects_unknown_command_fields() {
        let dir = Scratch::new();
        let path = dir.path.with_extension("json");
        fs::write(&path,r#"{"version":1,"name":"x","commands":[{"command_id":"one","kind":"task","action":{"type":"create"},"extra":1}]}"#).unwrap();
        assert!(WorkflowManifest::read(&path).is_err());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn action_payloads_over_64_kib_are_rejected() {
        let input = manifest(&[(
            "oversized",
            ManifestKind::Task,
            json!({"type":"create","title":"x".repeat(70_000)}),
        )]);
        assert!(input.validate().unwrap_err().contains("64 KiB"));
    }
    #[test]
    fn missing_required_fields_are_rejected() {
        for (kind, action) in [
            (ManifestKind::Project, json!({"name":"Project"})),
            (ManifestKind::Project, json!({"project_id":"id"})),
            (ManifestKind::Task, json!({"task_id":"id"})),
            (ManifestKind::Evidence, json!({"metric_id":"metric"})),
            (ManifestKind::FolderCreate, json!({"irrelevant":"x"})),
            (ManifestKind::FolderMove, json!({"from":"old"})),
            (ManifestKind::Markdown, json!({"kind":"Create"})),
        ] {
            assert!(manifest(&[("one", kind, action)]).validate().is_err());
        }
    }
}
