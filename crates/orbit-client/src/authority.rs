//! Native connected-mode data boundary. The server is authoritative; local
//! SQLite holds only acknowledged snapshots and durable pending intentions.
//! This module intentionally does not mutate the local-only Orbit domain store.
use reqwest::blocking::Client;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

#[path = "authority/http.rs"]
mod http;
#[path = "authority/markdown_mirror.rs"]
mod markdown_mirror;
pub use http::AuthorityHttp;
use http::validate_url;

type CommandStatusRow = (String, String, String, Option<i64>, String, Option<String>);

#[derive(Debug)]
pub enum AuthorityError {
    Storage(rusqlite::Error),
    Transport(reqwest::Error),
    Invalid(String),
    Conflict(String),
    Unavailable(String),
}
impl From<rusqlite::Error> for AuthorityError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Storage(e)
    }
}
impl From<reqwest::Error> for AuthorityError {
    fn from(e: reqwest::Error) -> Self {
        Self::Transport(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
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
    Agent,
    Blueprint,
}
impl CommandKind {
    fn as_str(self) -> &'static str {
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
            Self::Agent => "agent",
            Self::Blueprint => "blueprint",
        }
    }
    fn endpoint(self) -> &'static str {
        match self {
            Self::Project => "project-commands",
            Self::Task => "task-commands",
            Self::Evidence => "evidence-commands",
            Self::Schedule => "schedule-commands",
            Self::Routine => "routine-commands",
            Self::Milestone => "milestone-commands",
            Self::Strategy => "strategy-commands",
            Self::Markdown => "markdown-commands",
            Self::FolderCreate => "folder-creates",
            Self::FolderMove => "folder-moves",
            Self::Agent => "agent-jobs",
            Self::Blueprint => "blueprints",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FlushOutcome {
    Empty,
    Accepted(usize),
    Blocked {
        accepted: usize,
        command_id: String,
        reason: String,
    },
}

/// Every connected mutation, irrespective of client (CLI, TUI, MCP) or
/// batching, passes this same admission policy. The server remains the
/// authority for domain validity; this policy protects the local outbox.
fn validate_intention(id: &str, action: &Value) -> Result<(), AuthorityError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !action.is_object()
        || action.as_object().is_some_and(|fields| fields.is_empty())
        || action.get("command_id").is_some()
        || action.get("expected_revision").is_some()
    {
        return Err(AuthorityError::Invalid(
            "invalid outbox command identity or action".into(),
        ));
    }
    Ok(())
}

/// One coherent reconnect attempt. No field implies that pending commands are
/// accepted: `accepted_revision` refers only to a downloaded server snapshot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ReconnectReport {
    pub revision_before: Option<i64>,
    pub accepted_revision: Option<i64>,
    pub accepted_commands: usize,
    pub unresolved_commands: usize,
    pub blocked_command: Option<String>,
    pub block_reason: Option<String>,
    pub markdown_documents: Option<usize>,
    pub mirror_error: Option<String>,
}

/// Conflict review is a snapshot-bound token, not permission to auto-rebase.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ConflictReview {
    pub rejected_id: String,
    pub kind: String,
    pub rejected_action: Value,
    pub ordinal: i64,
    pub accepted_revision: i64,
    pub history_fingerprint: String,
}

/// One explicit decision approved against a particular accepted snapshot.
/// These are staged atomically, but the server still accepts commands in
/// deterministic global order and may reject a later command.
#[derive(Debug, Clone)]
pub struct ReviewedReplacement {
    pub review: ConflictReview,
    pub replacement_id: String,
    pub kind: CommandKind,
    pub action: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConvergenceStatus {
    pub accepted_revision: Option<i64>,
    pub history_fingerprint: Option<String>,
    pub pending: usize,
    pub conflicts: usize,
    pub accepted: usize,
    pub superseded: usize,
    pub cached_markdown: usize,
    pub markdown_mirror: MarkdownMirrorStatus,
    pub outbox_healthy: bool,
    pub first_unresolved: Option<String>,
    pub server_convergence_verified: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct MarkdownManifestEntry {
    note: orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
    body_sha256: String,
}

/// Network-efficient full-mirror publication. Only unchanged, validated
/// accepted documents are reused; all other bodies are transferred again.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MarkdownMirrorSyncReport {
    pub total_documents: usize,
    pub downloaded_documents: usize,
    pub reused_documents: usize,
    pub inventory_digest: String,
}

/// Persisted provenance for the accepted Markdown cache. `complete` is false
/// for individually fetched notes, and `source_inventory_verified` does not
/// claim the filesystem has not changed since the final inventory read.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct MarkdownMirrorStatus {
    pub complete: bool,
    pub source_inventory_verified: bool,
    pub document_count: usize,
    pub inventory_digest: Option<String>,
    pub source_revision: Option<i64>,
}

/// A point-in-time server observation. This never asserts that another client
/// cannot write immediately after the response. It distinguishes genuine
/// agreement from an offline cache that merely has no pending intentions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConvergenceObservation {
    pub server_revision: i64,
    pub server_history_fingerprint: String,
    pub local: ConvergenceStatus,
    pub accepted_snapshot_matches: bool,
    pub clean_outbox: bool,
    pub converged_when_observed: bool,
}

/// One complete connected-client synchronization attempt. A cycle can be
/// partially successful: server acceptance, snapshot refresh, Markdown mirror
/// refresh, and final observed convergence have independent outcomes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConvergenceCycle {
    pub reconnect: ReconnectReport,
    pub observation: ConvergenceObservation,
    pub fully_synchronized_at_observation: bool,
}

pub struct AuthorityReplica {
    db: Connection,
}
impl AuthorityReplica {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AuthorityError> {
        let db = Connection::open(path)?;
        // Never initialize the connected replica over a local-authoritative
        // workspace. Both layouts are SQLite but have incompatible ownership.
        // Inspect before CREATE TABLE, so a mistaken path cannot contaminate
        // an existing local-only database with replica metadata.
        let local_authority: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name IN (             'authority_identity','authority_history','authority_commands','domain_command_log','projects','tasks'))",
            [], |row| row.get(0),
        )?;
        if local_authority {
            return Err(AuthorityError::Conflict(
                "this SQLite file belongs to a local authoritative workspace, not a connected replica".into()));
        }
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS authority_settings (id INTEGER PRIMARY KEY CHECK(id=1), url TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS authority_snapshot (id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL CHECK(revision>=0), fingerprint TEXT NOT NULL, payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS authority_outbox (command_id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('project','task','evidence','schedule','routine','milestone','strategy','markdown','folder_create','folder_move','agent','blueprint')), action TEXT NOT NULL, expected_revision INTEGER, state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','conflict','superseded')), receipt TEXT, ordinal INTEGER NOT NULL UNIQUE);")?;
        // Greenfield schema: do not silently retain an older SQL-knowledge
        // outbox whose endpoints were retired. The user may initialize a fresh
        // replica, but an existing incompatible outbox fails closed.
        let ddl: String = db.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='authority_outbox'",
            [],
            |r| r.get(0),
        )?;
        if ddl.contains("'knowledge_create'")
            || ddl.contains("'knowledge_update'")
            || !ddl.contains("'markdown'")
            || !ddl.contains("'folder_create'")
            || !ddl.contains("'folder_move'")
            || !ddl.contains("'superseded'")
            || !ddl.contains("'schedule'")
            || !ddl.contains("'routine'")
            || !ddl.contains("'milestone'")
            || !ddl.contains("'strategy'")
            || !ddl.contains("'agent'")
            || !ddl.contains("'blueprint'")
        {
            return Err(AuthorityError::Invalid(
                "incompatible old outbox schema: initialize fresh replica".into(),
            ));
        }
        // Greenfield cutover: the old secondary Markdown queue had no global
        // ordering relative to task/project writes. Never use both queues.
        let old_markdown_queue: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='markdown_outbox')",
            [], |r|r.get(0))?;
        if old_markdown_queue {
            return Err(AuthorityError::Invalid(
                "split Markdown outbox unsupported: initialize fresh replica".into(),
            ));
        }
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS accepted_markdown_cache (
            note_id TEXT PRIMARY KEY NOT NULL,
            note_metadata TEXT NOT NULL,
            body TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS markdown_mirror_metadata (
            id INTEGER PRIMARY KEY CHECK(id=1),
            complete INTEGER NOT NULL CHECK(complete IN (0,1)),
            inventory_digest TEXT,
            source_revision INTEGER,
            document_count INTEGER NOT NULL CHECK(document_count>=0)
        );",
        )?;
        Ok(Self { db })
    }
    /// The canonical server URL is pinned to this replica on first connection.
    pub fn bind(&mut self, url: &str) -> Result<(), AuthorityError> {
        validate_url(url)?;
        let existing: Option<String> = self
            .db
            .query_row("SELECT url FROM authority_settings WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        if existing.as_deref().is_some_and(|old| old != url) {
            return Err(AuthorityError::Conflict(
                "replica is bound to a different server".into(),
            ));
        }
        self.db.execute(
            "INSERT OR IGNORE INTO authority_settings(id,url) VALUES(1,?1)",
            [url],
        )?;
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Option<Value>, AuthorityError> {
        let data: Option<(i64, String, String)> = self
            .db
            .query_row(
                "SELECT revision,fingerprint,payload FROM authority_snapshot WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        data.map(|(revision, fingerprint, raw)| {
            let value: Value =
                serde_json::from_str(&raw).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            if value.get("revision").and_then(Value::as_i64) != Some(revision)
                || value.get("history_fingerprint").and_then(Value::as_str)
                    != Some(fingerprint.as_str())
            {
                return Err(AuthorityError::Conflict(
                    "accepted snapshot metadata is inconsistent".into(),
                ));
            }
            for key in [
                "projects",
                "tasks",
                "habits",
                "habit_occurrences",
                "metrics",
                "metric_observations",
                "schedule",
                "routines",
                "milestones",
                "strategies",
            ] {
                if !value.get(key).is_some_and(Value::is_array) {
                    return Err(AuthorityError::Invalid(format!(
                        "accepted snapshot missing {key}"
                    )));
                }
            }
            Ok(value)
        })
        .transpose()
    }
    pub fn revision(&self) -> Result<Option<i64>, AuthorityError> {
        Ok(self
            .db
            .query_row(
                "SELECT revision FROM authority_snapshot WHERE id=1",
                [],
                |r| r.get(0),
            )
            .optional()?)
    }
    /// Atomic six-family refresh. Nothing pending is acknowledged by refresh.
    pub fn accept_snapshot(&mut self, data: &Value) -> Result<(), AuthorityError> {
        let revision = data
            .get("revision")
            .and_then(Value::as_i64)
            .filter(|v| *v >= 0)
            .ok_or_else(|| AuthorityError::Invalid("invalid server revision".into()))?;
        let fingerprint = data
            .get("history_fingerprint")
            .and_then(Value::as_str)
            .filter(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
            .ok_or_else(|| AuthorityError::Invalid("invalid history fingerprint".into()))?;
        for key in [
            "projects",
            "tasks",
            "habits",
            "habit_occurrences",
            "metrics",
            "metric_observations",
            "schedule",
            "routines",
            "milestones",
            "strategies",
        ] {
            if !data.get(key).is_some_and(Value::is_array) {
                return Err(AuthorityError::Invalid(format!("missing array: {key}")));
            }
        }
        // Read the existing projection, not just its revision metadata. A server
        // must never rewrite accepted state at an unchanged history revision.
        let previous = self.snapshot()?;
        if let Some(old) = previous {
            let old_rev = old["revision"]
                .as_i64()
                .ok_or_else(|| AuthorityError::Invalid("cached revision missing".into()))?;
            let old_fingerprint = old["history_fingerprint"]
                .as_str()
                .ok_or_else(|| AuthorityError::Invalid("cached fingerprint missing".into()))?;
            if revision < old_rev
                || (revision == old_rev && (fingerprint != old_fingerprint || &old != data))
            {
                return Err(AuthorityError::Conflict("server history or accepted projection differs from cached snapshot; manual recovery required".into()));
            }
        }
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO authority_snapshot(id,revision,fingerprint,payload) VALUES(1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,fingerprint=excluded.fingerprint,payload=excluded.payload",params![revision,fingerprint,data.to_string()])?;
        tx.commit()?;
        Ok(())
    }
    /// Network refresh verifies that the new history extends the accepted
    /// snapshot, rather than trusting only increasing revision numbers.
    /// The prefix proof and snapshot are distinct reads; if the server advances
    /// between them, immutable history still makes the proof valid.
    pub fn refresh(&mut self, http: &AuthorityHttp) -> Result<i64, AuthorityError> {
        self.bind(&http.base)?;
        let snapshot = http.get("/v1/authoritative/snapshot")?;
        if let Some((old_rev, old_fp)) = self
            .db
            .query_row(
                "SELECT revision,fingerprint FROM authority_snapshot WHERE id=1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            let new_rev = snapshot
                .get("revision")
                .and_then(Value::as_i64)
                .ok_or_else(|| AuthorityError::Invalid("server lacks revision".into()))?;
            if new_rev > old_rev {
                let proof = http.get(&format!(
                    "/v1/authoritative/history-prefix?revision={old_rev}"
                ))?;
                if proof.get("revision").and_then(Value::as_i64) != Some(old_rev)
                    || proof.get("history_fingerprint").and_then(Value::as_str)
                        != Some(old_fp.as_str())
                {
                    return Err(AuthorityError::Conflict(
                        "server history does not extend the accepted replica".into(),
                    ));
                }
            }
        }
        self.accept_snapshot(&snapshot)?;
        self.revision()?.ok_or_else(|| {
            AuthorityError::Invalid("snapshot revision missing after refresh".into())
        })
    }
    /// Reconnect a complete workspace: replay the oldest pending command
    /// first, then refresh the accepted projection, then refresh the Markdown
    /// mirror. A conflict does not prevent observing newer accepted state.
    /// A failed mirror refresh retains the previous mirror and is explicitly
    /// reported separately from a successful domain snapshot refresh.
    ///
    /// Transport and snapshot-integrity failures return errors. The durable
    /// outbox and previously accepted snapshot remain available for recovery.
    pub fn reconnect(&mut self, http: &AuthorityHttp) -> Result<ReconnectReport, AuthorityError> {
        let revision_before = self.revision()?;
        let outcome = self.flush(http)?;
        let (accepted_commands, blocked_command, block_reason) = match outcome {
            FlushOutcome::Empty => (0, None, None),
            FlushOutcome::Accepted(count) => (count, None, None),
            FlushOutcome::Blocked {
                accepted,
                command_id,
                reason,
            } => (accepted, Some(command_id), Some(reason)),
        };
        // Always refresh after a partial server commit. Even when a command
        // blocks, accepted remote state is relevant to explicit reconciliation.
        let accepted_revision = Some(self.refresh(http)?);
        let (markdown_documents, mirror_error) = match self.sync_markdown_mirror(http) {
            Ok(count) => (Some(count), None),
            Err(error) => (
                None,
                Some(match error {
                    AuthorityError::Conflict(reason)
                    | AuthorityError::Invalid(reason)
                    | AuthorityError::Unavailable(reason) => reason,
                    AuthorityError::Transport(_) => "Markdown server unavailable".into(),
                    AuthorityError::Storage(_) => "Markdown mirror storage error".into(),
                }),
            ),
        };
        Ok(ReconnectReport {
            revision_before,
            accepted_revision,
            accepted_commands,
            unresolved_commands: self.unresolved()?.len(),
            blocked_command,
            block_reason,
            markdown_documents,
            mirror_error,
        })
    }

    /// Durably queue a domain intention exactly once. Repeating the same
    /// command ID and logical action is a no-op (including after acceptance).
    /// Reusing the ID for a different intent always conflicts. This is the
    /// local half of the server's immutable-receipt idempotency contract.
    pub fn queue(
        &mut self,
        id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<(), AuthorityError> {
        validate_intention(id, action)?;
        let tx = self.db.transaction()?;
        let previous: Option<(String, String)> = tx
            .query_row(
                "SELECT kind,action FROM authority_outbox WHERE command_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old_kind, old_json)) = previous {
            let old_action: Value = serde_json::from_str(&old_json)
                .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            if old_kind == kind.as_str() && &old_action == action {
                return Ok(());
            }
            return Err(AuthorityError::Conflict(
                "command ID already used for another intention".into(),
            ));
        }
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ordinal),0)+1 FROM authority_outbox",
            [],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO authority_outbox(command_id,kind,action,ordinal) VALUES(?1,?2,?3,?4)",
            params![id, kind.as_str(), action.to_string(), next],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Queue an external agent command in the same globally ordered durable
    /// replica outbox as domain mutations. The action freezes its HTTP route
    /// and full wire payload; no server-side shell execution happens here.
    /// External commands do not consume domain history revisions.
    pub fn queue_agent_transport(
        &mut self,
        id: &str,
        route: &str,
        body: &Value,
    ) -> Result<(), AuthorityError> {
        const ROUTES: &[&str] = &[
            "agent-jobs/start",
            "agent-jobs/resume",
            "agent-jobs/continue",
            "agent-jobs/handoffs",
            "agent-jobs/review",
        ];
        if !ROUTES.contains(&route) || !body.is_object() {
            return Err(AuthorityError::Invalid(
                "unsupported agent transport command".into(),
            ));
        }
        // Wire payloads can contain a command_id; wrap them as data so
        // validate_intention only inspects the canonical outbox envelope.
        self.queue(id, CommandKind::Agent, &json!({"route":route,"body":body}))
    }

    /// Stage a blueprint mutation in the canonical global outbox. The route is
    /// frozen at enqueue time; replay never reconstructs mutable input files.
    pub fn queue_blueprint_transport(
        &mut self,
        id: &str,
        route: &str,
        body: &Value,
    ) -> Result<(), AuthorityError> {
        const ROUTES: &[&str] = &[
            "blueprints",
            "blueprints/install",
            "blueprints/bind",
            "blueprints/items",
            "blueprints/transition",
            "blueprints/properties",
            "blueprints/template",
        ];
        if !ROUTES.contains(&route) || !body.is_object() {
            return Err(AuthorityError::Invalid(
                "unsupported blueprint mutation".into(),
            ));
        }
        self.queue(
            id,
            CommandKind::Blueprint,
            &json!({"route":route,"body":body}),
        )
    }

    pub fn blueprint_pending(&self) -> Result<Value, AuthorityError> {
        let mut stmt=self.db.prepare("SELECT command_id,action,state FROM authority_outbox WHERE kind='blueprint' AND state IN ('pending','conflict') ORDER BY ordinal")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let (id, raw, state) = row?;
            let intent: Value =
                serde_json::from_str(&raw).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            entries.push(json!({"id":id,"route":intent.get("route"),"state":state}));
        }
        Ok(json!({"accepted":false,"queued":entries.len(),"commands":entries}))
    }

    pub fn agent_pending(&self) -> Result<Value, AuthorityError> {
        let mut stmt=self.db.prepare("SELECT command_id,action,state FROM authority_outbox WHERE kind='agent' AND state IN ('pending','conflict') ORDER BY ordinal")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let (id, raw, state) = row?;
            let action: Value =
                serde_json::from_str(&raw).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            entries.push(json!({"id":id,"path":action.get("route"),"state":state}));
        }
        Ok(json!({"accepted":false,"queued":entries.len(),"commands":entries}))
    }

    /// Queue a complete user-reviewed workflow as a single SQLite transaction.
    /// All IDs and actions are preflighted before any write. This guarantees
    /// that a local storage failure cannot leave a partially staged workflow.
    /// The authority still accepts commands separately: this is NOT a promise
    /// of atomic remote execution.
    pub fn queue_batch(
        &mut self,
        commands: &[(String, CommandKind, Value)],
    ) -> Result<usize, AuthorityError> {
        if commands.is_empty() || commands.len() > 100 {
            return Err(AuthorityError::Invalid(
                "workflow must have 1..=100 commands".into(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for (id, _, action) in commands {
            validate_intention(id, action)?;
            if !ids.insert(id.as_str()) {
                return Err(AuthorityError::Invalid(
                    "duplicate workflow command identity".into(),
                ));
            }
        }
        let tx = self.db.transaction()?;
        let mut ordinal: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ordinal),0) FROM authority_outbox",
            [],
            |row| row.get(0),
        )?;
        let mut inserted = 0;
        for (id, kind, action) in commands {
            let previous: Option<(String, String)> = tx
                .query_row(
                    "SELECT kind,action FROM authority_outbox WHERE command_id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((previous_kind, previous_json)) = previous {
                let previous_action: Value = serde_json::from_str(&previous_json)
                    .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
                if previous_kind == kind.as_str() && previous_action == *action {
                    continue;
                }
                return Err(AuthorityError::Conflict(format!(
                    "workflow command ID {id} already belongs to another intention"
                )));
            }
            ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| AuthorityError::Invalid("outbox sequence exhausted".into()))?;
            tx.execute(
                "INSERT INTO authority_outbox(command_id,kind,action,ordinal) VALUES(?1,?2,?3,?4)",
                params![id, kind.as_str(), action.to_string(), ordinal],
            )?;
            inserted += 1;
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Read-only consistency report for the single canonical outbox.
    /// Neither accepted snapshots nor pending intentions are modified.
    pub fn audit(&self) -> Result<crate::outbox_audit::OutboxAudit, AuthorityError> {
        crate::outbox_audit::audit(&self.db)
    }

    /// Acknowledgements are durable and queryable across process restarts.
    /// None means the intention has not received a validated acceptance receipt.
    /// Do not infer success from the absence of a pending outbox row.
    pub fn accepted_receipt(&self, id: &str) -> Result<Option<Value>, AuthorityError> {
        let raw: Option<String> = self
            .db
            .query_row(
                "SELECT receipt FROM authority_outbox WHERE command_id=?1 AND state='accepted'",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        raw.map(|data| {
            serde_json::from_str(&data).map_err(|e| AuthorityError::Invalid(e.to_string()))
        })
        .transpose()
    }

    /// One logical queue contains all supported command families. Expose
    /// the complete audit trail, not only unresolved intentions, so callers
    /// can distinguish accepted, pending, conflicted and superseded commands.
    pub fn command_status(&self, id: &str) -> Result<Option<Value>, AuthorityError> {
        let row: Option<CommandStatusRow> = self.db.query_row(
            "SELECT kind,action,state,expected_revision,command_id,receipt FROM authority_outbox WHERE command_id=?1",[id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        row.map(
            |(kind, action, state, expected_revision, command_id, receipt)| {
                let action: Value = serde_json::from_str(&action)
                    .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
                let receipt: Option<Value> = receipt
                    .map(|data| {
                        serde_json::from_str(&data)
                            .map_err(|e| AuthorityError::Invalid(e.to_string()))
                    })
                    .transpose()?;
                Ok(json!({"command_id":command_id,"kind":kind,"action":action,
                "state":state,"expected_revision":expected_revision,"receipt":receipt}))
            },
        )
        .transpose()
    }
    /// All commands share one durable global ordinal, including Markdown
    /// writes. A rejected command cannot be overtaken by other domains.
    pub fn queue_markdown(
        &mut self,
        command_id: &str,
        command: &orbit_runtime::markdown_commands::MarkdownCommand,
    ) -> Result<(), AuthorityError> {
        let valid = !command_id.is_empty()
            && command_id.len() <= 128
            && !command_id.starts_with("update-")
            && command_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !valid {
            return Err(AuthorityError::Invalid(
                "invalid Markdown command ID".into(),
            ));
        }
        let payload =
            serde_json::to_value(command).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let previous: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT kind,action FROM authority_outbox WHERE command_id=?1",
                [command_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((kind, old)) = previous {
            let serialized = payload.to_string();
            if kind == "markdown" && old == serialized {
                return Ok(());
            }
            return Err(AuthorityError::Conflict(
                "command ID already used for a different command".into(),
            ));
        }
        self.queue(command_id, CommandKind::Markdown, &payload)
    }

    fn queue_file_operation(
        &mut self,
        command_id: &str,
        kind: CommandKind,
        payload: &Value,
    ) -> Result<(), AuthorityError> {
        let valid = !command_id.is_empty()
            && command_id.len() <= 128
            && command_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !valid || !payload.is_object() {
            return Err(AuthorityError::Invalid(
                "invalid file-operation command".into(),
            ));
        }
        let previous: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT kind,action FROM authority_outbox WHERE command_id=?1",
                [command_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((k, old)) = previous {
            let serialized = payload.to_string();
            if k == kind.as_str() && old == serialized {
                return Ok(());
            }
            return Err(AuthorityError::Conflict("command ID already used".into()));
        }
        self.queue(command_id, kind, payload)
    }
    pub fn queue_folder_create(
        &mut self,
        command_id: &str,
        command: &orbit_runtime::folder_commands::FolderCreate,
    ) -> Result<(), AuthorityError> {
        self.queue_file_operation(
            command_id,
            CommandKind::FolderCreate,
            &serde_json::to_value(command).map_err(|e| AuthorityError::Invalid(e.to_string()))?,
        )
    }
    pub fn queue_folder_move(
        &mut self,
        command_id: &str,
        command: &orbit_runtime::folder_commands::FolderMove,
    ) -> Result<(), AuthorityError> {
        self.queue_file_operation(
            command_id,
            CommandKind::FolderMove,
            &serde_json::to_value(command).map_err(|e| AuthorityError::Invalid(e.to_string()))?,
        )
    }

    pub fn markdown_unresolved(&self) -> Result<Vec<Value>, AuthorityError> {
        let mut stmt = self.db.prepare(
            "SELECT command_id,action,state FROM authority_outbox WHERE kind='markdown' AND state IN ('pending','conflict') ORDER BY ordinal")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, payload, state) = row?;
            let command: Value = serde_json::from_str(&payload)
                .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            Ok(json!({"command_id":id,"command":command,"state":state}))
        })
        .collect()
    }

    /// This is the global ordered flush; it deliberately includes other
    /// pending domain commands instead of skipping older intents.
    pub fn flush_markdown(&mut self, http: &AuthorityHttp) -> Result<FlushOutcome, AuthorityError> {
        self.flush(http)
    }

    pub fn pending_count(&self) -> Result<i64, AuthorityError> {
        Ok(self.db.query_row(
            "SELECT COUNT(*) FROM authority_outbox WHERE state IN ('pending','conflict')",
            [],
            |r| r.get(0),
        )?)
    }
    /// Show all unresolved intentions; receipts remain available for audit.
    pub fn unresolved(&self) -> Result<Vec<Value>, AuthorityError> {
        let mut stmt=self.db.prepare("SELECT command_id,kind,action,expected_revision,state FROM authority_outbox WHERE state IN ('pending','conflict') ORDER BY ordinal")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, kind, raw, rev, state) = row?;
            let action: Value =
                serde_json::from_str(&raw).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            result.push(json!({"command_id":id,"kind":kind,"action":action,"expected_revision":rev,"state":state}));
        }
        Ok(result)
    }
    /// Complete the local retry/refresh/mirror workflow, then observe the
    /// authority again. A successful refresh by itself does not mean that
    /// another client has not committed newer data or that conflicts vanished.
    /// The observation read is intentionally the final network interaction.
    pub fn converge(&mut self, http: &AuthorityHttp) -> Result<ConvergenceCycle, AuthorityError> {
        let reconnect = self.reconnect(http)?;
        let observation = self.observe_convergence(http)?;
        let fully_synchronized_at_observation = observation.converged_when_observed
            && reconnect.blocked_command.is_none()
            && reconnect.mirror_error.is_none()
            && reconnect.unresolved_commands == 0
            && observation.local.markdown_mirror.source_inventory_verified
            && observation.local.markdown_mirror.source_revision
                == Some(observation.server_revision);
        Ok(ConvergenceCycle {
            reconnect,
            observation,
            fully_synchronized_at_observation,
        })
    }

    /// Compare the server's accepted state with the local accepted cache,
    /// without refreshing the cache or changing a pending intention. A shared
    /// server revision AND fingerprint is required; equal revision alone does
    /// not prove convergence. This is a point-in-time observation, not a lock.
    pub fn observe_convergence(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<ConvergenceObservation, AuthorityError> {
        self.bind(&http.base)?;
        let response = http.get("/v1/authoritative/snapshot")?;
        let revision = response
            .get("revision")
            .and_then(Value::as_i64)
            .filter(|v| *v >= 0)
            .ok_or_else(|| AuthorityError::Invalid("server snapshot missing revision".into()))?;
        let fingerprint = response
            .get("history_fingerprint")
            .and_then(Value::as_str)
            .filter(|v| {
                v.len() == 64
                    && v.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
            .ok_or_else(|| AuthorityError::Invalid("server fingerprint invalid".into()))?
            .to_owned();
        let local = self.convergence_status()?;
        let accepted_snapshot_matches = local.accepted_revision == Some(revision)
            && local.history_fingerprint.as_deref() == Some(fingerprint.as_str());
        let clean_outbox = local.pending == 0 && local.conflicts == 0 && local.outbox_healthy;
        Ok(ConvergenceObservation {
            server_revision: revision,
            server_history_fingerprint: fingerprint,
            local,
            accepted_snapshot_matches,
            clean_outbox,
            converged_when_observed: accepted_snapshot_matches && clean_outbox,
        })
    }

    /// Report an actionable offline condition without contacting the server.
    pub fn sync_health(&self) -> Result<crate::sync_health::SyncHealth, AuthorityError> {
        Ok(crate::sync_health::SyncHealth::offline(
            &self.convergence_status()?,
        ))
    }

    /// An observed health report is valid only at the instant of observation.
    pub fn observe_sync_health(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::sync_health::SyncHealth, AuthorityError> {
        Ok(crate::sync_health::SyncHealth::observed(
            &self.observe_convergence(http)?,
        ))
    }

    /// Actionable, read-only cross-domain queue and accepted-state report.
    pub fn workspace_sync_report(
        &self,
    ) -> Result<crate::sync_health::WorkspaceSyncReport, AuthorityError> {
        crate::sync_health::WorkspaceSyncReport::offline(&self.db, &self.convergence_status()?)
    }

    /// A point-in-time online report with the same domain queue structure.
    pub fn observe_workspace_sync_report(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<crate::sync_health::WorkspaceSyncReport, AuthorityError> {
        let observation = self.observe_convergence(http)?;
        crate::sync_health::WorkspaceSyncReport::observed(&self.db, &observation)
    }

    /// Review after refreshing the accepted server state. If another client
    /// advanced the history since the last offline sync, the user sees the
    /// new authoritative snapshot rather than reviewing stale data.
    pub fn review_conflict_online(
        &mut self,
        http: &AuthorityHttp,
        id: &str,
    ) -> Result<ConflictReview, AuthorityError> {
        self.refresh(http)?;
        self.conflict_review(id)
    }

    /// Stable description of the exact authoritative state against which a
    /// conflict was reviewed. The caller must show this to the user before
    /// proposing a replacement, especially after another device has written.
    #[cfg(test)]
    pub(crate) fn test_mark_conflicted(&self, id: &str) -> Result<(), AuthorityError> {
        self.db.execute(
            "UPDATE authority_outbox SET state='conflict' WHERE command_id=?1",
            [id],
        )?;
        Ok(())
    }

    pub fn conflict_review(&self, command_id: &str) -> Result<ConflictReview, AuthorityError> {
        let snapshot = self.snapshot()?.ok_or_else(|| {
            AuthorityError::Conflict(
                "no accepted snapshot; refresh before reviewing a conflict".into(),
            )
        })?;
        let (kind, raw, state, ordinal): (String, String, String, i64) = self
            .db
            .query_row(
                "SELECT kind,action,state,ordinal FROM authority_outbox WHERE command_id=?1",
                [command_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| AuthorityError::Invalid("command not found".into()))?;
        if state != "conflict" {
            return Err(AuthorityError::Conflict("command is not conflicted".into()));
        }
        let action: Value =
            serde_json::from_str(&raw).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        if !action.is_object() {
            return Err(AuthorityError::Invalid("invalid conflicted action".into()));
        }
        Ok(ConflictReview {
            rejected_id: command_id.to_owned(),
            kind,
            rejected_action: action,
            ordinal,
            accepted_revision: snapshot["revision"]
                .as_i64()
                .ok_or_else(|| AuthorityError::Invalid("invalid accepted revision".into()))?,
            history_fingerprint: snapshot["history_fingerprint"]
                .as_str()
                .ok_or_else(|| AuthorityError::Invalid("invalid accepted fingerprint".into()))?
                .to_owned(),
        })
    }

    /// Only commit a reviewed resolution against the *same* accepted history
    /// the user saw. No automatic rebase is safe for arbitrary domain actions.
    /// The check and replacement are in one SQLite transaction. For connected
    /// clients use replace_reviewed_conflict_online, which checks the server
    /// again before changing the durable local queue.
    pub fn replace_reviewed_conflict(
        &mut self,
        review: &ConflictReview,
        new_id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<(), AuthorityError> {
        validate_intention(new_id, action)?;
        // File commands have their own content revisions and receipt shape.
        // They do not use the domain expected_revision envelope. A domain
        // snapshot-bound review therefore cannot safely authorize them.
        if matches!(
            kind,
            CommandKind::Markdown
                | CommandKind::FolderCreate
                | CommandKind::FolderMove
                | CommandKind::Agent
                | CommandKind::Blueprint
        ) {
            return Err(AuthorityError::Invalid(
                "file conflict requires content-specific revision review".into(),
            ));
        }
        if review.rejected_id == new_id || review.kind != kind.as_str() {
            return Err(AuthorityError::Conflict(
                "replacement must have a fresh ID and unchanged family".into(),
            ));
        }
        let tx = self.db.transaction()?;
        let (revision, fingerprint): (i64, String) = tx
            .query_row(
                "SELECT revision,fingerprint FROM authority_snapshot WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AuthorityError::Conflict("accepted snapshot missing".into()))?;
        if revision != review.accepted_revision || fingerprint != review.history_fingerprint {
            return Err(AuthorityError::Conflict(
                "accepted history changed; review again".into(),
            ));
        }
        let previous: (String, String, String, i64) = tx
            .query_row(
                "SELECT state,kind,action,ordinal FROM authority_outbox WHERE command_id=?1",
                [&review.rejected_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| AuthorityError::Conflict("rejected command no longer exists".into()))?;
        let parsed: Value = serde_json::from_str(&previous.2)
            .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        if previous.0 != "conflict"
            || previous.1 != review.kind
            || previous.3 != review.ordinal
            || parsed != review.rejected_action
        {
            return Err(AuthorityError::Conflict(
                "rejected command changed; review again".into(),
            ));
        }
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM authority_outbox WHERE command_id=?1)",
            [new_id],
            |row| row.get(0),
        )?;
        if used {
            return Err(AuthorityError::Conflict(
                "replacement ID already used".into(),
            ));
        }
        let end: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ordinal),0) FROM authority_outbox",
            [],
            |row| row.get(0),
        )?;
        let end = end
            .checked_add(1)
            .ok_or_else(|| AuthorityError::Invalid("outbox ordinal exhausted".into()))?;
        tx.execute(
            "UPDATE authority_outbox SET state='superseded',ordinal=?1 WHERE command_id=?2",
            params![end, review.rejected_id],
        )?;
        // Freeze the review's authoritative revision, not the revision at
        // eventual submission. If another device writes after review, the
        // server must reject this replacement instead of silently rebasing.
        tx.execute("INSERT INTO authority_outbox(command_id,kind,action,ordinal,expected_revision) VALUES(?1,?2,?3,?4,?5)",
            params![new_id,kind.as_str(),action.to_string(),review.ordinal,review.accepted_revision])?;
        tx.commit()?;
        Ok(())
    }

    /// Stage multiple independent conflict resolutions in one transaction.
    /// Every resolution is bound to the SAME server-accepted history. Never
    /// move an original audit row until every review and replacement passes.
    /// Neither batch staging nor its commit constitutes server acceptance.
    pub fn replace_reviewed_batch(
        &mut self,
        decisions: &[ReviewedReplacement],
    ) -> Result<usize, AuthorityError> {
        if decisions.is_empty() || decisions.len() > 100 {
            return Err(AuthorityError::Invalid(
                "reviewed resolution batch requires 1..=100 decisions".into(),
            ));
        }
        let mut rejected_ids = std::collections::HashSet::new();
        let mut replacement_ids = std::collections::HashSet::new();
        for decision in decisions {
            validate_intention(&decision.replacement_id, &decision.action)?;
            if matches!(
                decision.kind,
                CommandKind::Markdown
                    | CommandKind::FolderCreate
                    | CommandKind::FolderMove
                    | CommandKind::Agent
                    | CommandKind::Blueprint
            ) {
                return Err(AuthorityError::Invalid(
                    "file conflicts require content-specific revision review".into(),
                ));
            }
            if !rejected_ids.insert(decision.review.rejected_id.as_str())
                || !replacement_ids.insert(decision.replacement_id.as_str())
            {
                return Err(AuthorityError::Invalid(
                    "duplicate rejected or replacement command".into(),
                ));
            }
            if decision.review.rejected_id == decision.replacement_id
                || decision.review.kind != decision.kind.as_str()
            {
                return Err(AuthorityError::Conflict(
                    "replacement must have a fresh identity and unchanged command family".into(),
                ));
            }
        }
        if replacement_ids.iter().any(|id| rejected_ids.contains(id)) {
            return Err(AuthorityError::Conflict(
                "a replacement cannot reuse another rejected command ID".into(),
            ));
        }
        let tx = self.db.transaction()?;
        let (revision, fingerprint): (i64, String) = tx
            .query_row(
                "SELECT revision,fingerprint FROM authority_snapshot WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AuthorityError::Conflict("accepted snapshot missing".into()))?;
        if decisions.iter().any(|d| {
            d.review.accepted_revision != revision || d.review.history_fingerprint != fingerprint
        }) {
            return Err(AuthorityError::Conflict(
                "server history changed since review; review all decisions again".into(),
            ));
        }
        let mut old_ordinals = Vec::with_capacity(decisions.len());
        for decision in decisions {
            let existing: (String, String, String, i64) = tx
                .query_row(
                    "SELECT state,kind,action,ordinal FROM authority_outbox WHERE command_id=?1",
                    [&decision.review.rejected_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?
                .ok_or_else(|| AuthorityError::Conflict("rejected command missing".into()))?;
            let old_action: Value = serde_json::from_str(&existing.2)
                .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            if existing.0 != "conflict"
                || existing.1 != decision.review.kind
                || existing.3 != decision.review.ordinal
                || old_action != decision.review.rejected_action
            {
                return Err(AuthorityError::Conflict(
                    "rejected command changed since review".into(),
                ));
            }
            let used: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM authority_outbox WHERE command_id=?1)",
                [&decision.replacement_id],
                |r| r.get(0),
            )?;
            if used {
                return Err(AuthorityError::Conflict(
                    "replacement ID already used".into(),
                ));
            }
            old_ordinals.push(existing.3);
        }
        let mut end: i64 = tx.query_row(
            "SELECT COALESCE(MAX(ordinal),0) FROM authority_outbox",
            [],
            |r| r.get(0),
        )?;
        for (decision, old_ordinal) in decisions.iter().zip(old_ordinals) {
            end = end
                .checked_add(1)
                .ok_or_else(|| AuthorityError::Invalid("outbox sequence exhausted".into()))?;
            tx.execute(
                "UPDATE authority_outbox SET state='superseded',ordinal=?1 WHERE command_id=?2",
                params![end, decision.review.rejected_id],
            )?;
            tx.execute("INSERT INTO authority_outbox(command_id,kind,action,ordinal,expected_revision) VALUES(?1,?2,?3,?4,?5)",
                params![decision.replacement_id,decision.kind.as_str(),decision.action.to_string(),
                    old_ordinal,decision.review.accepted_revision])?;
        }
        tx.commit()?;
        Ok(decisions.len())
    }

    /// One network refresh serves as the consistency barrier for the whole
    /// reviewed batch, preventing the first resolution from changing the
    /// accepted context of subsequent decisions while locally staging them.
    pub fn replace_reviewed_batch_online(
        &mut self,
        http: &AuthorityHttp,
        decisions: &[ReviewedReplacement],
    ) -> Result<usize, AuthorityError> {
        self.refresh(http)?;
        self.replace_reviewed_batch(decisions)
    }

    /// Re-observe the real server before applying the reviewed local
    /// replacement. This catches changes accepted by another connected device
    /// since review, without ever discarding the original rejected command.
    pub fn replace_reviewed_conflict_online(
        &mut self,
        http: &AuthorityHttp,
        review: &ConflictReview,
        new_id: &str,
        kind: CommandKind,
        action: &Value,
    ) -> Result<(), AuthorityError> {
        self.refresh(http)?;
        self.replace_reviewed_conflict(review, new_id, kind, action)
    }

    /// A read-only synchronization summary that never confuses local pending
    /// intentions with accepted server state. Offline reports are explicitly
    /// marked as not yet converged with the live authority.
    pub fn convergence_status(&self) -> Result<ConvergenceStatus, AuthorityError> {
        let snapshot = self.snapshot()?;
        let mut pending = 0_usize;
        let mut conflicts = 0_usize;
        let mut accepted = 0_usize;
        let mut superseded = 0_usize;
        let mut stmt = self
            .db
            .prepare("SELECT state,COUNT(*) FROM authority_outbox GROUP BY state")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (state, count) = row?;
            let count = usize::try_from(count)
                .map_err(|_| AuthorityError::Invalid("invalid state count".into()))?;
            match state.as_str() {
                "pending" => pending = count,
                "conflict" => conflicts = count,
                "accepted" => accepted = count,
                "superseded" => superseded = count,
                _ => return Err(AuthorityError::Invalid("unknown outbox state".into())),
            }
        }
        let cached_markdown: i64 =
            self.db
                .query_row("SELECT COUNT(*) FROM accepted_markdown_cache", [], |r| {
                    r.get(0)
                })?;
        let audit = self.audit()?;
        Ok(ConvergenceStatus {
            accepted_revision: snapshot.as_ref().and_then(|s| s["revision"].as_i64()),
            history_fingerprint: snapshot
                .as_ref()
                .and_then(|s| s["history_fingerprint"].as_str())
                .map(str::to_owned),
            pending,
            conflicts,
            accepted,
            superseded,
            cached_markdown: usize::try_from(cached_markdown)
                .map_err(|_| AuthorityError::Invalid("invalid markdown count".into()))?,
            markdown_mirror: self.markdown_mirror_status()?,
            outbox_healthy: audit.healthy,
            first_unresolved: audit.first_unresolved,
            // Offline status alone cannot prove up-to-date server convergence.
            server_convergence_verified: false,
        })
    }

    /// A command envelope freezes its revision BEFORE any network submission.
    /// This permits safe retries after the server commits but the reply is lost.
    fn next_envelope(
        &mut self,
        revision: i64,
    ) -> Result<Option<(String, CommandKind, Value)>, AuthorityError> {
        let row:Option<(String,String,String,Option<i64>)> = self.db.query_row("SELECT command_id,kind,action,expected_revision FROM authority_outbox WHERE state='pending' ORDER BY ordinal LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let Some((id, kind, raw, frozen)) = row else {
            return Ok(None);
        };
        let kind = match kind.as_str() {
            "project" => CommandKind::Project,
            "task" => CommandKind::Task,
            "evidence" => CommandKind::Evidence,
            "schedule" => CommandKind::Schedule,
            "routine" => CommandKind::Routine,
            "milestone" => CommandKind::Milestone,
            "strategy" => CommandKind::Strategy,
            _ => return Err(AuthorityError::Invalid("unknown command kind".into())),
        };
        let mut action: Value =
            serde_json::from_str(&raw).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let revision = frozen.unwrap_or(revision);
        if frozen.is_none() {
            self.db.execute(
                "UPDATE authority_outbox SET expected_revision=?1 WHERE command_id=?2",
                params![revision, id],
            )?;
        }
        if matches!(kind, CommandKind::Project) {
            // Project commands are flat by server contract, unlike task/evidence actions.
            let obj = action
                .as_object_mut()
                .ok_or_else(|| AuthorityError::Invalid("invalid queued project".into()))?;
            obj.insert("command_id".into(), json!(id));
            obj.insert("expected_revision".into(), json!(revision));
        } else {
            let mut envelope =
                json!({"command_id":id,"expected_revision":revision,"action":action});
            std::mem::swap(&mut action, &mut envelope);
        }
        Ok(Some((id, kind, action)))
    }
    pub fn flush(&mut self, http: &AuthorityHttp) -> Result<FlushOutcome, AuthorityError> {
        // The connected outbox is the sole source of pending mutations.
        // Refuse to transmit if persisted ordering or receipts are corrupt.
        // Auditing never rewrites records and cannot silently repair history.
        let audit = self.audit()?;
        if !audit.healthy {
            return Err(AuthorityError::Conflict(format!(
                "outbox integrity failed ({} issue(s)); inspect with replica audit",
                audit.issues.len()
            )));
        }
        self.bind(&http.base)?;
        let mut accepted = 0;
        loop {
            // Select the globally first unresolved intent, including a
            // conflict. Earlier Markdown writes cannot be bypassed by tasks.
            let first:Option<(String,String,String,String)>=self.db.query_row(
                "SELECT command_id,kind,action,state FROM authority_outbox WHERE state IN ('pending','conflict') ORDER BY ordinal LIMIT 1",
                [],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            let Some((id, kind, raw, state)) = first else {
                break;
            };
            if state == "conflict" {
                return Ok(FlushOutcome::Blocked {
                    accepted,
                    command_id: id,
                    reason: "command conflict requires explicit reconciliation".into(),
                });
            }
            if kind == "agent" || kind == "blueprint" {
                let intent: Value = serde_json::from_str(&raw)
                    .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
                let route = intent.get("route").and_then(Value::as_str).ok_or_else(|| {
                    AuthorityError::Invalid("transport intention missing route".into())
                })?;
                let body = intent
                    .get("body")
                    .filter(|x| x.is_object())
                    .ok_or_else(|| {
                        AuthorityError::Invalid("transport intention missing payload".into())
                    })?;
                const AGENT_ROUTES: &[&str] = &[
                    "agent-jobs/start",
                    "agent-jobs/resume",
                    "agent-jobs/continue",
                    "agent-jobs/handoffs",
                    "agent-jobs/review",
                ];
                const BLUEPRINT_ROUTES: &[&str] = &[
                    "blueprints",
                    "blueprints/install",
                    "blueprints/bind",
                    "blueprints/items",
                    "blueprints/transition",
                    "blueprints/properties",
                    "blueprints/template",
                ];
                if !(if kind == "agent" {
                    AGENT_ROUTES
                } else {
                    BLUEPRINT_ROUTES
                })
                .contains(&route)
                {
                    return Err(AuthorityError::Invalid("unknown transport route".into()));
                }
                let response = if kind == "blueprint" {
                    http.post(
                        "/v1/authoritative/blueprints/commands",
                        &json!({"command_id":id,"route":route,"body":body}),
                    )
                } else {
                    http.post(&format!("/v1/authoritative/{route}"), body)
                };
                match response {
                    Ok(receipt) => {
                        if kind == "blueprint"
                            && (receipt.get("command_id").and_then(Value::as_str)
                                != Some(id.as_str())
                                || receipt.get("response").is_none())
                        {
                            return Err(AuthorityError::Invalid(
                                "invalid blueprint command acceptance receipt".into(),
                            ));
                        }
                        // Only an authenticated successful JSON response is
                        // accepted. Store the original outbox identity in its
                        // local receipt for independent integrity auditing.
                        self.db.execute("UPDATE authority_outbox SET state='accepted',receipt=?1 WHERE command_id=?2",
                            params![json!({"command_id":id,"response":receipt}).to_string(),id])?;
                        accepted += 1;
                    }
                    Err(AuthorityError::Conflict(reason)) => {
                        self.db.execute(
                            "UPDATE authority_outbox SET state='conflict' WHERE command_id=?1",
                            [&id],
                        )?;
                        return Ok(FlushOutcome::Blocked {
                            accepted,
                            command_id: id,
                            reason,
                        });
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            if matches!(kind.as_str(), "markdown" | "folder_create" | "folder_move") {
                let command: Value = serde_json::from_str(&raw)
                    .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
                let endpoint = match kind.as_str() {
                    "markdown" => "/v1/authoritative/markdown-commands",
                    "folder_create" => "/v1/authoritative/folder-creates",
                    "folder_move" => "/v1/authoritative/folder-moves",
                    _ => return Err(AuthorityError::Invalid("invalid file command kind".into())),
                };
                match http.post(endpoint, &json!({"command_id":id,"command":command})) {
                    Ok(receipt) => {
                        if receipt.get("command_id").and_then(Value::as_str) != Some(id.as_str()) {
                            return Err(AuthorityError::Invalid(
                                "invalid file command receipt ID".into(),
                            ));
                        }
                        match kind.as_str() {
                            "markdown"
                                if receipt.get("note_id").and_then(Value::as_str)
                                    != command.get("note_id").and_then(Value::as_str)
                                    || receipt
                                        .get("revision")
                                        .and_then(Value::as_i64)
                                        .is_none_or(|r| r < 1) =>
                            {
                                return Err(AuthorityError::Invalid(
                                    "invalid Markdown receipt".into(),
                                ));
                            }
                            "folder_create" if receipt.get("path") != command.get("path") => {
                                return Err(AuthorityError::Invalid(
                                    "invalid folder creation receipt".into(),
                                ));
                            }
                            "folder_move"
                                if receipt.get("from") != command.get("from")
                                    || receipt.get("to") != command.get("to") =>
                            {
                                return Err(AuthorityError::Invalid(
                                    "invalid folder move receipt".into(),
                                ));
                            }
                            _ => {}
                        }
                        self.db.execute("UPDATE authority_outbox SET state='accepted',receipt=?1 WHERE command_id=?2",
                            params![receipt.to_string(),id])?;
                        accepted += 1;
                    }
                    Err(AuthorityError::Conflict(reason)) => {
                        self.db.execute(
                            "UPDATE authority_outbox SET state='conflict' WHERE command_id=?1",
                            [&id],
                        )?;
                        return Ok(FlushOutcome::Blocked {
                            accepted,
                            command_id: id,
                            reason,
                        });
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            // Ordinary domain commands freeze their expected revision on first
            // submission. Retry with the same envelope if the reply is lost.
            let revision = http
                .get("/v1/authoritative/snapshot")?
                .get("revision")
                .and_then(Value::as_i64)
                .ok_or_else(|| AuthorityError::Invalid("server lacks revision".into()))?;
            let Some((next_id, kind, action)) = self.next_envelope(revision)? else {
                break;
            };
            if next_id != id {
                return Err(AuthorityError::Invalid("global queue order changed".into()));
            }
            match http.post(&format!("/v1/authoritative/{}", kind.endpoint()), &action) {
                Ok(receipt) => {
                    // Never mark a queued command accepted on an unrelated or
                    // malformed success response. Keep the frozen envelope
                    // pending so the same idempotent request can be retried.
                    let accepted_revision = receipt
                        .get("revision")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| {
                            AuthorityError::Invalid("command receipt lacks revision".into())
                        })?;
                    let expected_revision = action
                        .get("expected_revision")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| {
                            AuthorityError::Invalid("submitted command lacks revision".into())
                        })?;
                    if accepted_revision != expected_revision + 1
                        || !receipt.get("duplicate").is_some_and(Value::is_boolean)
                    {
                        return Err(AuthorityError::Invalid(
                            "invalid authoritative command receipt".into(),
                        ));
                    }
                    // Project/task receipts include the accepted entity. A
                    // successful HTTP status alone cannot acknowledge an
                    // unrelated record, even if its revision looks valid.
                    match kind {
                        CommandKind::Project => {
                            if receipt.pointer("/project/id").and_then(Value::as_str)
                                != action.get("project_id").and_then(Value::as_str)
                            {
                                return Err(AuthorityError::Invalid(
                                    "project receipt identity mismatch".into(),
                                ));
                            }
                        }
                        CommandKind::Task => {
                            if receipt.pointer("/task/id").and_then(Value::as_str)
                                != action.pointer("/action/task_id").and_then(Value::as_str)
                            {
                                return Err(AuthorityError::Invalid(
                                    "task receipt identity mismatch".into(),
                                ));
                            }
                        }
                        CommandKind::Schedule => {
                            let expected =
                                action.pointer("/action/block_id").and_then(Value::as_str);
                            let received = receipt.pointer("/block/id").and_then(Value::as_str);
                            if action.pointer("/action/type").and_then(Value::as_str)
                                == Some("delete")
                            {
                                if !receipt.get("block").is_some_and(Value::is_null) {
                                    return Err(AuthorityError::Invalid(
                                        "delete receipt must contain null block".into(),
                                    ));
                                }
                            } else if expected != received || expected.is_none() {
                                return Err(AuthorityError::Invalid(
                                    "schedule receipt identity mismatch".into(),
                                ));
                            }
                        }
                        CommandKind::Milestone => {
                            let expected = action
                                .pointer("/action/milestone_id")
                                .and_then(Value::as_str);
                            let received = receipt.pointer("/milestone/id").and_then(Value::as_str);
                            if action.pointer("/action/type").and_then(Value::as_str)
                                == Some("delete")
                            {
                                if !receipt.get("milestone").is_some_and(Value::is_null) {
                                    return Err(AuthorityError::Invalid(
                                        "milestone delete receipt must be null".into(),
                                    ));
                                }
                            } else if expected.is_none() || expected != received {
                                return Err(AuthorityError::Invalid(
                                    "milestone receipt identity mismatch".into(),
                                ));
                            }
                        }
                        CommandKind::Routine => {
                            let expected =
                                action.pointer("/action/routine_id").and_then(Value::as_str);
                            let received = receipt.pointer("/routine/id").and_then(Value::as_str);
                            if action.pointer("/action/type").and_then(Value::as_str)
                                == Some("delete")
                            {
                                if !receipt.get("routine").is_some_and(Value::is_null) {
                                    return Err(AuthorityError::Invalid(
                                        "routine delete receipt must be null".into(),
                                    ));
                                }
                            } else if expected.is_none() || expected != received {
                                return Err(AuthorityError::Invalid(
                                    "routine receipt identity mismatch".into(),
                                ));
                            }
                        }
                        CommandKind::Strategy => {
                            let expected =
                                action.pointer("/action/project_id").and_then(Value::as_str);
                            let received = receipt
                                .pointer("/strategy/project_id")
                                .and_then(Value::as_str);
                            if expected.is_none() || expected != received {
                                return Err(AuthorityError::Invalid(
                                    "strategy receipt project identity mismatch".into(),
                                ));
                            }
                        }
                        CommandKind::Evidence => {}
                        _ => {
                            return Err(AuthorityError::Invalid(
                                "unexpected domain command type".into(),
                            ));
                        }
                    }
                    self.db.execute("UPDATE authority_outbox SET state='accepted',receipt=?1 WHERE command_id=?2",
                        params![receipt.to_string(),id])?;
                    accepted += 1;
                }
                Err(AuthorityError::Conflict(reason)) => {
                    self.db.execute(
                        "UPDATE authority_outbox SET state='conflict' WHERE command_id=?1",
                        [&id],
                    )?;
                    return Ok(FlushOutcome::Blocked {
                        accepted,
                        command_id: id,
                        reason,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(if accepted == 0 {
            FlushOutcome::Empty
        } else {
            FlushOutcome::Accepted(accepted)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(rev: i64, fingerprint: &str) -> Value {
        json!({"revision":rev,"history_fingerprint":fingerprint,"projects":[],"tasks":[],"habits":[],"habit_occurrences":[],"metrics":[],"metric_observations":[],"schedule":[],"routines":[],"milestones":[],"strategies":[]})
    }
    #[test]
    fn agent_intentions_share_domain_global_order_and_reject_reused_ids() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "first-task",
                CommandKind::Task,
                &json!({"type":"create","task_id":"a"}),
            )
            .unwrap();
        let body = json!({"job_id":"agent-1","task_id":"a","at_ms":42});
        replica
            .queue_agent_transport("agent-1", "agent-jobs/start", &body)
            .unwrap();
        replica
            .queue_agent_transport("agent-1", "agent-jobs/start", &body)
            .unwrap();
        assert!(matches!(
            replica.queue_agent_transport(
                "agent-1",
                "agent-jobs/start",
                &json!({"job_id":"agent-1","task_id":"b"})
            ),
            Err(AuthorityError::Conflict(_))
        ));
        assert!(
            replica
                .queue_agent_transport("evil", "../admin", &body)
                .is_err()
        );
        let unresolved = replica.unresolved().unwrap();
        assert_eq!(unresolved.len(), 2);
        assert_eq!(unresolved[0]["command_id"], "first-task");
        assert_eq!(unresolved[1]["command_id"], "agent-1");
        assert_eq!(replica.agent_pending().unwrap()["queued"], 1);
        assert!(replica.audit().unwrap().healthy);
    }

    #[test]
    fn cache_rejects_incomplete_and_divergent_history_without_overwrite() {
        let mut replica = AuthorityReplica {
            db: Connection::open_in_memory().unwrap(),
        };
        replica.db.execute_batch("CREATE TABLE authority_snapshot(id INTEGER PRIMARY KEY,revision INTEGER,fingerprint TEXT,payload TEXT)").unwrap();
        let hash = "a".repeat(64);
        replica.accept_snapshot(&sample(2, &hash)).unwrap();
        assert!(replica.accept_snapshot(&sample(1, &hash)).is_err());
        assert!(
            replica
                .accept_snapshot(&sample(2, &"b".repeat(64)))
                .is_err()
        );
        assert!(replica.accept_snapshot(&json!({"revision":3})).is_err());
        assert_eq!(replica.revision().unwrap(), Some(2));
    }
    #[test]
    fn equal_revision_cannot_rewrite_accepted_projection() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let hash = "c".repeat(64);
        let original = sample(4, &hash);
        replica.accept_snapshot(&original).unwrap();
        // Object-key order is irrelevant, but the accepted collection is immutable.
        replica.accept_snapshot(&json!({"tasks":[],"projects":[],"revision":4,
            "history_fingerprint":hash,"habits":[],"habit_occurrences":[],
            "metrics":[],"metric_observations":[],"schedule":[],"routines":[],"milestones":[],"strategies":[]})).unwrap();
        let mut rewritten = original.clone();
        rewritten["projects"] = json!([{"id":"injected"}]);
        assert!(matches!(
            replica.accept_snapshot(&rewritten),
            Err(AuthorityError::Conflict(_))
        ));
        assert_eq!(replica.snapshot().unwrap(), Some(original));
    }
    #[test]
    fn malformed_persisted_snapshot_fails_closed() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let original = sample(3, &"d".repeat(64));
        replica.accept_snapshot(&original).unwrap();
        replica
            .db
            .execute("UPDATE authority_snapshot SET revision=5 WHERE id=1", [])
            .unwrap();
        assert!(matches!(
            replica.snapshot(),
            Err(AuthorityError::Conflict(_))
        ));
        assert!(
            replica
                .accept_snapshot(&sample(6, &"e".repeat(64)))
                .is_err()
        );
    }
    #[test]
    fn domain_retries_preserve_one_ordinal_and_reject_payload_collisions() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let action = json!({"type":"create","task_id":"task-a","title":"Original"});
        replica.queue("same", CommandKind::Task, &action).unwrap();
        replica
            .queue(
                "same",
                CommandKind::Task,
                &json!({"title":"Original","task_id":"task-a","type":"create"}),
            )
            .unwrap();
        assert_eq!(replica.pending_count().unwrap(), 1);
        assert_eq!(replica.unresolved().unwrap().len(), 1);
        assert!(matches!(
            replica.queue(
                "same",
                CommandKind::Task,
                &json!({"type":"create","task_id":"task-a","title":"Changed"})
            ),
            Err(AuthorityError::Conflict(_))
        ));
        assert!(matches!(
            replica.queue("same", CommandKind::Evidence, &action),
            Err(AuthorityError::Conflict(_))
        ));
        let (_, _, first) = replica.next_envelope(7).unwrap().unwrap();
        assert_eq!(first["expected_revision"], 7);
        replica.queue("same", CommandKind::Task, &action).unwrap();
        let (_, _, second) = replica.next_envelope(500).unwrap().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn accepted_receipts_are_durable_and_requeue_never_replays() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replica.db");
        let action = json!({"type":"set_metric_target","metric_id":"m","baseline":0,"target":10,"direction":"increase"});
        {
            let mut replica = AuthorityReplica::open(&path).unwrap();
            replica
                .queue("accepted-1", CommandKind::Evidence, &action)
                .unwrap();
            assert!(replica.accepted_receipt("accepted-1").unwrap().is_none());
            replica.db.execute("UPDATE authority_outbox SET state='accepted',receipt=?1 WHERE command_id='accepted-1'",
                [json!({"revision":3,"duplicate":false}).to_string()]).unwrap();
        }
        let mut replica = AuthorityReplica::open(&path).unwrap();
        assert_eq!(
            replica.accepted_receipt("accepted-1").unwrap().unwrap()["revision"],
            3
        );
        replica
            .queue("accepted-1", CommandKind::Evidence, &action)
            .unwrap();
        assert_eq!(replica.pending_count().unwrap(), 0);
        assert_eq!(
            replica.command_status("accepted-1").unwrap().unwrap()["state"],
            "accepted"
        );
        assert_eq!(
            replica.command_status("accepted-1").unwrap().unwrap()["receipt"]["revision"],
            3
        );
        assert!(matches!(
            replica.queue(
                "accepted-1",
                CommandKind::Evidence,
                &json!({"type":"different"})
            ),
            Err(AuthorityError::Conflict(_))
        ));
        assert!(replica.command_status("missing").unwrap().is_none());
    }

    #[test]
    fn conflicted_requeue_does_not_clear_conflict_or_change_frozen_revision() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let payload = json!({"type":"create_habit","habit_id":"h","name":"Walk","cadence":"daily"});
        replica
            .queue("conflicted", CommandKind::Evidence, &payload)
            .unwrap();
        replica.next_envelope(3).unwrap();
        replica
            .db
            .execute(
                "UPDATE authority_outbox SET state='conflict' WHERE command_id='conflicted'",
                [],
            )
            .unwrap();
        replica
            .queue("conflicted", CommandKind::Evidence, &payload)
            .unwrap();
        let status = replica.command_status("conflicted").unwrap().unwrap();
        assert_eq!(status["state"], "conflict");
        assert_eq!(status["expected_revision"], 3);
        assert_eq!(replica.pending_count().unwrap(), 1);
    }

    #[test]
    fn native_queue_project_payload_is_flat_and_task_payload_is_nested() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "p-1",
                CommandKind::Project,
                &json!({"project_id":"p","name":"Personal"}),
            )
            .unwrap();
        let (_, _, payload) = replica.next_envelope(0).unwrap().unwrap();
        assert_eq!(payload["project_id"], "p");
        assert_eq!(payload["command_id"], "p-1");
        assert!(payload.get("action").is_none());
        replica
            .db
            .execute(
                "UPDATE authority_outbox SET state='accepted' WHERE command_id='p-1'",
                [],
            )
            .unwrap();
        replica
            .queue(
                "t-1",
                CommandKind::Task,
                &json!({"type":"create","task_id":"t","title":"Task"}),
            )
            .unwrap();
        let (_, _, payload) = replica.next_envelope(1).unwrap().unwrap();
        assert_eq!(payload["action"]["type"], "create");
        assert_eq!(payload["expected_revision"], 1);
    }

    #[test]
    fn conflict_requires_snapshot_bound_review_and_preserves_receipt() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "first",
                CommandKind::Task,
                &json!({"type":"create","task_id":"t","title":"old"}),
            )
            .unwrap();
        replica.db.execute("UPDATE authority_outbox SET state='conflict',expected_revision=2 WHERE command_id='first'",[]).unwrap();
        assert!(replica.conflict_review("first").is_err());
        assert_eq!(
            replica.command_status("first").unwrap().unwrap()["state"],
            "conflict"
        );
        replica
            .accept_snapshot(&sample(3, &"a".repeat(64)))
            .unwrap();
        let review = replica.conflict_review("first").unwrap();
        replica
            .replace_reviewed_conflict(
                &review,
                "second",
                CommandKind::Task,
                &json!({"type":"create","task_id":"t","title":"new"}),
            )
            .unwrap();
        assert_eq!(replica.unresolved().unwrap()[0]["command_id"], "second");
        assert_eq!(
            replica.command_status("first").unwrap().unwrap()["state"],
            "superseded"
        );
        let (id, _, payload) = replica.next_envelope(9).unwrap().unwrap();
        assert_eq!(id, "second");
        assert_eq!(payload["expected_revision"], 3);
    }

    #[test]
    fn reviewed_replacement_retains_original_queue_position() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "a",
                CommandKind::Task,
                &json!({"type":"create","task_id":"a","title":"old"}),
            )
            .unwrap();
        replica
            .queue(
                "b",
                CommandKind::Task,
                &json!({"type":"create","task_id":"b","title":"later"}),
            )
            .unwrap();
        replica
            .db
            .execute(
                "UPDATE authority_outbox SET state='conflict' WHERE command_id='a'",
                [],
            )
            .unwrap();
        replica
            .accept_snapshot(&sample(5, &"a".repeat(64)))
            .unwrap();
        let review = replica.conflict_review("a").unwrap();
        replica
            .replace_reviewed_conflict(
                &review,
                "a-reviewed",
                CommandKind::Task,
                &json!({"type":"create","task_id":"a","title":"reviewed"}),
            )
            .unwrap();
        let ids: Vec<String> = replica
            .unresolved()
            .unwrap()
            .iter()
            .map(|row| row["command_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids, vec!["a-reviewed", "b"]);
        assert_eq!(replica.next_envelope(5).unwrap().unwrap().0, "a-reviewed");
    }
    #[test]
    fn rejects_insecure_or_cross_server_configuration() {
        assert!(validate_url("http://example.com").is_err());
        assert!(validate_url("https://user:password@example.com").is_err());
        assert!(validate_url("http://localhost:8787").is_ok());
    }
}

#[cfg(test)]
mod device_header_tests {
    use super::*;
    #[test]
    fn remote_transport_requires_an_enrolled_device_identity() {
        assert!(AuthorityHttp::new("https://orbit.example", "token", "").is_err());
        assert!(AuthorityHttp::new("https://orbit.example", "token", "bad\nheader").is_err());
        assert!(AuthorityHttp::new("https://orbit.example", "token", "desktop-1").is_ok());
        assert!(AuthorityHttp::new("http://orbit.example", "token", "desktop-1").is_err());
    }
}

#[cfg(test)]
mod markdown_outbox_contract_tests {
    use super::*;
    use orbit_runtime::markdown_commands::MarkdownCommand;

    #[test]
    fn markdown_task_and_project_writes_share_one_global_order() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "project",
                CommandKind::Project,
                &json!({"project_id":"p","name":"P"}),
            )
            .unwrap();
        replica
            .queue_markdown(
                "note",
                &MarkdownCommand::Create {
                    note_id: "n".into(),
                    path: "n.md".into(),
                    body: "# N".into(),
                },
            )
            .unwrap();
        replica
            .queue(
                "task",
                CommandKind::Task,
                &json!({"type":"create","task_id":"t","title":"T"}),
            )
            .unwrap();
        let order: Vec<String> = {
            let mut stmt = replica
                .db
                .prepare("SELECT command_id FROM authority_outbox ORDER BY ordinal")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(order, vec!["project", "note", "task"]);
        assert_eq!(replica.pending_count().unwrap(), 3);
        assert_eq!(replica.markdown_unresolved().unwrap().len(), 1);
    }
    #[test]
    fn markdown_outbox_is_durable_idempotent_and_conflicts_are_visible() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replica.sqlite");
        let cmd = MarkdownCommand::Create {
            note_id: "note-a".into(),
            path: "Work/a.md".into(),
            body: "# A".into(),
        };
        {
            let mut replica = AuthorityReplica::open(&path).unwrap();
            replica.queue_markdown("create-a", &cmd).unwrap();
            replica.queue_markdown("create-a", &cmd).unwrap();
            let changed = MarkdownCommand::Create {
                note_id: "note-a".into(),
                path: "Work/a.md".into(),
                body: "# B".into(),
            };
            assert!(matches!(
                replica.queue_markdown("create-a", &changed),
                Err(AuthorityError::Conflict(_))
            ));
            assert!(replica.queue_markdown("update-bad", &cmd).is_err());
        }
        let replica = AuthorityReplica::open(&path).unwrap();
        let unresolved = replica.markdown_unresolved().unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0]["command_id"], "create-a");
        assert_eq!(unresolved[0]["state"], "pending");
        assert!(replica.snapshot().unwrap().is_none());
    }
}

#[cfg(test)]
mod folder_outbox_acceptance_tests {
    use super::*;
    use orbit_runtime::folder_commands::{FolderCreate, FolderMove};

    #[test]
    fn folder_writes_share_global_order_and_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("folder-replica.sqlite");
        let create = FolderCreate {
            path: "Work".into(),
        };
        let rename = FolderMove {
            from: "Work".into(),
            to: "Projects".into(),
        };
        {
            let mut replica = AuthorityReplica::open(&path).unwrap();
            replica.queue_folder_create("mkdir-work", &create).unwrap();
            replica
                .queue_markdown(
                    "create-note",
                    &orbit_runtime::markdown_commands::MarkdownCommand::Create {
                        note_id: "n".into(),
                        path: "Work/n.md".into(),
                        body: "# N".into(),
                    },
                )
                .unwrap();
            replica.queue_folder_move("rename-work", &rename).unwrap();
            replica.queue_folder_create("mkdir-work", &create).unwrap();
            assert!(matches!(
                replica.queue_folder_create(
                    "mkdir-work",
                    &FolderCreate {
                        path: "Other".into()
                    }
                ),
                Err(AuthorityError::Conflict(_))
            ));
            assert_eq!(replica.pending_count().unwrap(), 3);
        }
        let replica = AuthorityReplica::open(&path).unwrap();
        let unresolved = replica.unresolved().unwrap();
        let ids: Vec<&str> = unresolved
            .iter()
            .map(|r| r["command_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["mkdir-work", "create-note", "rename-work"]);
        assert!(replica.snapshot().unwrap().is_none());
    }

    #[test]
    fn folder_conflict_cannot_be_rebased_using_domain_snapshot() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue_folder_create(
                "first",
                &FolderCreate {
                    path: "Work".into(),
                },
            )
            .unwrap();
        replica
            .queue_folder_move(
                "second",
                &FolderMove {
                    from: "Work".into(),
                    to: "Research".into(),
                },
            )
            .unwrap();
        replica
            .db
            .execute(
                "UPDATE authority_outbox SET state='conflict' WHERE command_id='first'",
                [],
            )
            .unwrap();
        replica.accept_snapshot(&json!({"revision":1,"history_fingerprint":"a".repeat(64),"projects":[],"tasks":[],"habits":[],"habit_occurrences":[],"metrics":[],"metric_observations":[],"schedule":[],"routines":[],"milestones":[],"strategies":[]})).unwrap();
        let review = replica.conflict_review("first").unwrap();
        assert!(
            replica
                .replace_reviewed_conflict(
                    &review,
                    "first-reviewed",
                    CommandKind::FolderCreate,
                    &json!({"path":"Work"})
                )
                .is_err()
        );
        assert_eq!(replica.unresolved().unwrap()[0]["command_id"], "first");
        assert_eq!(replica.unresolved().unwrap()[1]["command_id"], "second");
    }
}

#[cfg(test)]
mod replacement_contract_tests {
    use super::*;
    #[test]
    fn replacement_rejects_cross_family_and_reused_identity() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "first",
                CommandKind::Task,
                &json!({"type":"change_status","task_id":"t","status":"ready"}),
            )
            .unwrap();
        replica
            .queue(
                "second",
                CommandKind::Task,
                &json!({"type":"change_status","task_id":"u","status":"ready"}),
            )
            .unwrap();
        replica
            .db
            .execute(
                "UPDATE authority_outbox SET state='conflict' WHERE command_id='first'",
                [],
            )
            .unwrap();
        replica.accept_snapshot(&json!({"revision":4,"history_fingerprint":"b".repeat(64),"projects":[],"tasks":[],"habits":[],"habit_occurrences":[],"metrics":[],"metric_observations":[],"schedule":[],"routines":[],"milestones":[],"strategies":[]})).unwrap();
        let review = replica.conflict_review("first").unwrap();
        let action = json!({"type":"change_status","task_id":"t","status":"done"});
        assert!(
            replica
                .replace_reviewed_conflict(&review, "replacement", CommandKind::Project, &action)
                .is_err()
        );
        assert!(
            replica
                .replace_reviewed_conflict(&review, "second", CommandKind::Task, &action)
                .is_err()
        );
        assert_eq!(
            replica.command_status("first").unwrap().unwrap()["state"],
            "conflict"
        );
        replica
            .replace_reviewed_conflict(&review, "replacement", CommandKind::Task, &action)
            .unwrap();
        assert_eq!(
            replica.command_status("first").unwrap().unwrap()["state"],
            "superseded"
        );
        let unresolved = replica.unresolved().unwrap();
        assert_eq!(unresolved.len(), 2);
        assert_eq!(unresolved[0]["command_id"], "replacement");
        assert_eq!(unresolved[1]["command_id"], "second");
        assert_eq!(replica.accepted_receipt("replacement").unwrap(), None);
    }
}

#[cfg(test)]
mod transactional_workflow_staging_tests {
    use super::*;

    fn workflow() -> Vec<(String, CommandKind, Value)> {
        vec![
            (
                "p".into(),
                CommandKind::Project,
                json!({"project_id":"p","name":"Project"}),
            ),
            (
                "t".into(),
                CommandKind::Task,
                json!({"type":"create","task_id":"t","project_id":"p","title":"Task"}),
            ),
            (
                "s".into(),
                CommandKind::Schedule,
                json!({"type":"create","block_id":"b"}),
            ),
        ]
    }

    #[test]
    fn batch_is_durable_ordered_and_idempotent() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let commands = workflow();
        assert_eq!(replica.queue_batch(&commands).unwrap(), 3);
        assert_eq!(replica.queue_batch(&commands).unwrap(), 0);
        let rows = replica.unresolved().unwrap();
        let ids: Vec<_> = rows
            .iter()
            .map(|r| r["command_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["p", "t", "s"]);
        assert!(replica.snapshot().unwrap().is_none());
    }

    #[test]
    fn collision_at_end_rolls_back_entire_batch() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue("s", CommandKind::Routine, &json!({"type":"create"}))
            .unwrap();
        assert!(matches!(
            replica.queue_batch(&workflow()),
            Err(AuthorityError::Conflict(_))
        ));
        assert_eq!(replica.pending_count().unwrap(), 1);
        assert_eq!(replica.command_status("p").unwrap(), None);
        assert_eq!(replica.command_status("t").unwrap(), None);
    }

    #[test]
    fn invalid_or_duplicate_late_step_never_stages_prefix() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let mut commands = workflow();
        commands[2].0 = "p".into();
        assert!(replica.queue_batch(&commands).is_err());
        assert_eq!(replica.pending_count().unwrap(), 0);
        commands[2].0 = "s".into();
        commands[2].2 = json!({"expected_revision":7});
        assert!(replica.queue_batch(&commands).is_err());
        assert_eq!(replica.pending_count().unwrap(), 0);
    }

    #[test]
    fn preexisting_identical_prefix_does_not_change_order() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let commands = workflow();
        replica
            .queue(&commands[0].0, commands[0].1, &commands[0].2)
            .unwrap();
        assert_eq!(replica.queue_batch(&commands).unwrap(), 2);
        let ids: Vec<_> = replica
            .unresolved()
            .unwrap()
            .iter()
            .map(|r| r["command_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids, vec!["p", "t", "s"]);
    }
}

#[cfg(test)]
mod database_mode_isolation_tests {
    use super::*;

    #[test]
    fn replica_refuses_local_authoritative_database_without_adding_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite");
        rusqlite::Connection::open(&path).unwrap().execute_batch(
            "CREATE TABLE authority_identity (id INTEGER PRIMARY KEY, generation TEXT NOT NULL);"
        ).unwrap();
        assert!(matches!(
            AuthorityReplica::open(&path),
            Err(AuthorityError::Conflict(_))
        ));
        let inspection = rusqlite::Connection::open(&path).unwrap();
        let outbox: i64 = inspection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='authority_outbox'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(outbox, 0, "a rejected replica open must not create tables");
    }

    #[test]
    fn replica_refuses_local_domain_tables_without_command_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.sqlite");
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE projects (id TEXT PRIMARY KEY);")
            .unwrap();
        assert!(matches!(
            AuthorityReplica::open(&path),
            Err(AuthorityError::Conflict(_))
        ));
    }
}

#[cfg(test)]
mod offline_accepted_markdown_tests {
    use super::*;
    use orbit_runtime::knowledge_service::KnowledgeDocumentInfo;

    fn note(id: &str, title: &str) -> KnowledgeDocumentInfo {
        KnowledgeDocumentInfo {
            id: id.to_owned(),
            title: title.to_owned(),
            path: format!("notes/{id}.md"),
        }
    }

    fn body(id: &str, content: &str) -> String {
        format!("---\norbit_id: {id}\n---\n{content}\n")
    }

    #[test]
    fn offline_search_is_limited_to_accepted_fetched_notes() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .cache_markdown(
                &note("first", "Planning"),
                &body("first", "Alpha milestone"),
            )
            .unwrap();
        replica
            .cache_markdown(
                &note("second", "Research"),
                &body("second", "Beta planning"),
            )
            .unwrap();
        replica
            .queue(
                "pending-note",
                CommandKind::Markdown,
                &json!({"type":"create","note_id":"not-accepted","content":"planning"}),
            )
            .unwrap();
        let found = replica.search_cached_markdown("planning", 10).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(replica.cached_markdown_inventory(10).unwrap().len(), 2);
        assert_eq!(replica.pending_count().unwrap(), 1);
        assert!(replica.cached_markdown("not-accepted").unwrap().is_none());
    }

    #[test]
    fn offline_search_treats_sql_metacharacters_literally() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        // Every cached body carries `orbit_id` frontmatter, so a bare `_`
        // legitimately matches both notes. Give note a a sequence that would
        // match `important_` only if `_` were a single-character wildcard.
        replica
            .cache_markdown(
                &note("a", "Ordinary"),
                &body("a", "some content importantX"),
            )
            .unwrap();
        replica
            .cache_markdown(&note("b", "Percent"), &body("b", "100% done ! important_"))
            .unwrap();
        for term in ["%", "!"] {
            let results = replica.search_cached_markdown(term, 10).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "b");
        }
        // `_` is matched literally, not as a one-character LIKE wildcard.
        let results = replica.search_cached_markdown("important_", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "b");
    }

    #[test]
    fn offline_cache_survives_process_restart_without_promoting_outbox() {
        let directory = tempfile::tempdir().unwrap();
        let db = directory.path().join("replica.sqlite");
        {
            let mut replica = AuthorityReplica::open(&db).unwrap();
            replica
                .cache_markdown(
                    &note("stored", "Stored"),
                    &body("stored", "offline reference"),
                )
                .unwrap();
            replica
                .queue(
                    "pending",
                    CommandKind::Task,
                    &json!({"type":"create","task_id":"task","title":"Not accepted"}),
                )
                .unwrap();
        }
        let replica = AuthorityReplica::open(&db).unwrap();
        assert_eq!(
            replica.search_cached_markdown("offline", 10).unwrap().len(),
            1
        );
        assert!(replica.snapshot().unwrap().is_none());
        assert_eq!(replica.pending_count().unwrap(), 1);
    }

    #[test]
    fn offline_search_rejects_malformed_queries_and_cache_records() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        assert!(replica.search_cached_markdown("  ", 10).is_err());
        assert!(replica.search_cached_markdown("search", 0).is_err());
        assert!(replica.cached_markdown_inventory(501).is_err());
        replica
            .cache_markdown(&note("known", "Valid"), &body("known", "needle"))
            .unwrap();
        replica
            .db
            .execute(
                "UPDATE accepted_markdown_cache SET note_metadata=?1 WHERE note_id='known'",
                [serde_json::to_string(&note("other", "Bad")).unwrap()],
            )
            .unwrap();
        assert!(matches!(
            replica.search_cached_markdown("needle", 5),
            Err(AuthorityError::Conflict(_))
        ));
        assert!(replica.cached_markdown_inventory(5).is_err());
    }
}

#[cfg(test)]
mod canonical_outbox_admission_tests {
    use super::*;

    #[test]
    fn single_and_batch_command_admission_have_identical_rules() {
        let mut single = AuthorityReplica::open(":memory:").unwrap();
        let mut batch = AuthorityReplica::open(":memory:").unwrap();
        for (id, action) in [
            ("invalid id", json!({"type":"create"})),
            ("", json!({"type":"create"})),
            ("valid", json!({})),
            ("valid", json!([1, 2, 3])),
            ("valid", json!({"type":"create","command_id":"nested"})),
            ("valid", json!({"type":"create","expected_revision":0})),
        ] {
            assert!(single.queue(id, CommandKind::Task, &action).is_err());
            assert!(
                batch
                    .queue_batch(&[(id.into(), CommandKind::Task, action)])
                    .is_err()
            );
        }
        assert_eq!(single.pending_count().unwrap(), 0);
        assert_eq!(batch.pending_count().unwrap(), 0);
    }

    #[test]
    fn invalid_late_intention_rolls_back_entire_workflow() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let commands = vec![
            (
                "good".to_string(),
                CommandKind::Project,
                json!({"project_id":"project","name":"Project"}),
            ),
            (
                "bad id".to_string(),
                CommandKind::Task,
                json!({"type":"create","task_id":"task"}),
            ),
        ];
        assert!(replica.queue_batch(&commands).is_err());
        assert!(replica.unresolved().unwrap().is_empty());
        assert!(replica.command_status("good").unwrap().is_none());
    }

    #[test]
    fn healthy_outbox_passes_preflight_without_network() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "valid",
                CommandKind::Project,
                &json!({"project_id":"project","name":"Project"}),
            )
            .unwrap();
        assert!(replica.audit().unwrap().healthy);
    }

    #[test]
    fn tampered_outbox_is_detectable_before_submission() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "valid",
                CommandKind::Project,
                &json!({"project_id":"project","name":"Project"}),
            )
            .unwrap();
        replica
            .db
            .execute(
                "UPDATE authority_outbox SET action='[]' WHERE command_id='valid'",
                [],
            )
            .unwrap();
        let report = replica.audit().unwrap();
        assert!(!report.healthy);
        assert!(!report.issues.is_empty());
    }
}

#[cfg(test)]
mod full_markdown_mirror_tests {
    use super::*;
    use orbit_runtime::knowledge_service::KnowledgeDocumentInfo;

    fn doc(id: &str, path: &str, value: &str) -> (KnowledgeDocumentInfo, String) {
        (
            KnowledgeDocumentInfo {
                id: id.into(),
                path: path.into(),
                title: value.into(),
            },
            format!("---\norbit_id: {id}\n---\n# {value}\n"),
        )
    }

    #[test]
    fn a_complete_refresh_replaces_removed_notes_and_moves() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let original = vec![doc("a", "old/a.md", "one"), doc("b", "b.md", "two")];
        assert_eq!(replica.replace_markdown_mirror(&original).unwrap(), 2);
        assert_eq!(replica.cached_markdown_inventory(10).unwrap().len(), 2);
        let updated = vec![doc("a", "new/a.md", "updated")];
        assert_eq!(replica.replace_markdown_mirror(&updated).unwrap(), 1);
        assert!(replica.cached_markdown("b").unwrap().is_none());
        let (note, body) = replica.cached_markdown("a").unwrap().unwrap();
        assert_eq!(note.path, "new/a.md");
        assert!(body.contains("updated"));
    }

    #[test]
    fn malformed_full_refresh_preserves_last_complete_mirror() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let good = doc("original", "original.md", "old");
        replica
            .replace_markdown_mirror(std::slice::from_ref(&good))
            .unwrap();
        let mut broken = doc("new", "new.md", "new");
        broken.1 = "---\norbit_id: wrong\n---\n".into();
        assert!(
            replica
                .replace_markdown_mirror(&[good.clone(), broken])
                .is_err()
        );
        assert_eq!(replica.cached_markdown_inventory(10).unwrap(), vec![good.0]);
    }

    #[test]
    fn duplicate_id_or_path_does_not_replace_cache() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let good = doc("old", "old.md", "old");
        replica
            .replace_markdown_mirror(std::slice::from_ref(&good))
            .unwrap();
        let a = doc("x", "same.md", "first");
        let b = doc("y", "same.md", "second");
        assert!(matches!(
            replica.replace_markdown_mirror(&[a.clone(), b]),
            Err(AuthorityError::Conflict(_))
        ));
        assert!(replica.replace_markdown_mirror(&[a.clone(), a]).is_err());
        assert_eq!(replica.cached_markdown_inventory(10).unwrap(), vec![good.0]);
    }

    #[test]
    fn pending_writes_survive_cache_replacement() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "task-1",
                CommandKind::Task,
                &json!({"type":"create","task_id":"t1"}),
            )
            .unwrap();
        replica
            .replace_markdown_mirror(&[doc("a", "a.md", "a")])
            .unwrap();
        replica.replace_markdown_mirror(&[]).unwrap();
        assert_eq!(replica.pending_count().unwrap(), 1);
        assert_eq!(replica.cached_markdown_inventory(10).unwrap().len(), 0);
    }

    #[test]
    fn complete_mirror_is_persistent_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replica.db");
        {
            let mut replica = AuthorityReplica::open(&path).unwrap();
            replica
                .replace_markdown_mirror(&[doc("persisted", "notes/p.md", "saved")])
                .unwrap();
        }
        let replica = AuthorityReplica::open(&path).unwrap();
        assert_eq!(replica.cached_markdown_inventory(10).unwrap().len(), 1);
        assert!(replica.cached_markdown("persisted").unwrap().is_some());
    }
}

#[cfg(test)]
mod reconnect_report_tests {
    use super::{AuthorityReplica, CommandKind, ReconnectReport};
    use serde_json::json;
    #[test]
    fn reconnect_report_never_conflates_accepted_and_unresolved() {
        let report = ReconnectReport {
            revision_before: Some(3),
            accepted_revision: Some(5),
            accepted_commands: 2,
            unresolved_commands: 1,
            blocked_command: Some("later-command".into()),
            block_reason: Some("revision conflict".into()),
            markdown_documents: None,
            mirror_error: Some("inventory changed".into()),
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["accepted_commands"], 2);
        assert_eq!(json["unresolved_commands"], 1);
        assert_eq!(json["blocked_command"], "later-command");
        assert!(json["markdown_documents"].is_null());
        assert_eq!(
            serde_json::from_value::<ReconnectReport>(json).unwrap(),
            report
        );
    }
    #[test]
    fn blueprint_intentions_share_canonical_outbox_and_reject_conflicting_replay() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let action = json!({"id":"dev","version":1});
        replica
            .queue("domain-1", CommandKind::Task, &json!({"type":"create"}))
            .unwrap();
        replica
            .queue_blueprint_transport("bp-1", "blueprints/install", &action)
            .unwrap();
        replica
            .queue_blueprint_transport("bp-1", "blueprints/install", &action)
            .unwrap();
        assert!(
            replica
                .queue_blueprint_transport("bp-1", "blueprints/bind", &action)
                .is_err()
        );
        assert!(
            replica
                .queue_blueprint_transport("evil", "../private", &action)
                .is_err()
        );
        assert_eq!(replica.pending_count().unwrap(), 2);
        let pending = replica.blueprint_pending().unwrap();
        assert_eq!(pending["queued"], 1);
        assert_eq!(pending["commands"][0]["id"], "bp-1");
    }
}

#[cfg(test)]
#[path = "conflict_batch_tests.rs"]
mod conflict_batch_tests;
