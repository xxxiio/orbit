//! Local/connected backend for the one project-centric TUI shell.
//!
//! The renderer uses one view model for both modes; the shell only branches for
//! capability/availability labels. Connected rich mutations return the last
//! accepted DTO while the exact queued command identity is followed through the
//! outbox. The shell reports `accepted`, `pending` or `conflict` for that
//! command; a missing identity or state fails closed as pending, never success.

use orbit_client::OrbitClient;
use orbit_client::authority::{CommandKind, ConflictReview};
use orbit_client::unified::{AuthorityConnection, CommandDisposition};
use orbit_client::{
    AcceptanceCriterionDto, ActivityDto, AttachmentDto, ClientError, ClientErrorCode,
    ConfigureProjectStrategyRequest, CreateKnowledgeRequest, CreateProjectRequest,
    CreateTaskRequest, HabitDto, MetricTargetDto, PendingAttachmentDto, ProjectDto,
    ProjectEvidenceBindingDto, ProjectWorkflowDto, RecordHabitOccurrenceRequest,
    SetProjectStatusRequest, SetTaskPlanningRequest, SetTaskStatusRequest, TaskDto,
    TaskPlanProposalDto, WorkflowDescriptorDto,
};
use orbit_runtime::domain_commands::{ProjectCommand, TaskAction, TaskCommand, TaskRecord};
use orbit_runtime::planning_approval::{ApprovedScheduleReceipt, ApprovedScheduleRequest};
use orbit_runtime::planning_scenarios::{ScenarioCompareRequest, ScenarioComparison};
use orbit_runtime::portfolio_actions::{self, PortfolioActionReport, PortfolioActionRequest};
use orbit_runtime::scenario_execution::{ScenarioExecutionRequest, ScenarioExecutionReview};
use orbit_runtime::task_dependencies::TaskDependencyEdge;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Test-only counter of backend pending-count reads. The frame renderer must
/// read the cached count on [`crate::app::App`] instead of asking the client
/// every frame, so the perf tests can prove render performs no backend read.
///
/// The counter is thread-local so concurrently running tests cannot observe
/// each other's reads.
#[cfg(test)]
pub(crate) mod diagnostics {
    use std::cell::Cell;

    thread_local! {
        static PENDING_COUNT_READS: Cell<u64> = const { Cell::new(0) };
        static DETAIL_READS: Cell<u64> = const { Cell::new(0) };
        static CAPABILITY_READS: Cell<u64> = const { Cell::new(0) };
    }

    pub(crate) fn pending_count_reads() -> u64 {
        PENDING_COUNT_READS.with(Cell::get)
    }

    pub(crate) fn reset_pending_count_reads() {
        PENDING_COUNT_READS.with(|reads| reads.set(0));
    }

    pub(crate) fn record_pending_count_read() {
        PENDING_COUNT_READS.with(|reads| reads.set(reads.get() + 1));
    }

    /// Detail-pane backend reads (`task_detail`, `acceptance_criteria`,
    /// `list_attachments`). Rendering must read the cached model only, so the
    /// render tests assert this counter never moves during a frame.
    pub(crate) fn detail_reads() -> u64 {
        DETAIL_READS.with(Cell::get)
    }

    pub(crate) fn reset_detail_reads() {
        DETAIL_READS.with(|reads| reads.set(0));
    }

    pub(crate) fn record_detail_read() {
        DETAIL_READS.with(|reads| reads.set(reads.get() + 1));
    }

    /// Capability offer/status/history reads. These are cached on [`App`] by
    /// input lifecycle events, so a painted frame must never move this counter.
    pub(crate) fn capability_reads() -> u64 {
        CAPABILITY_READS.with(Cell::get)
    }

    pub(crate) fn reset_capability_reads() {
        CAPABILITY_READS.with(|reads| reads.set(0));
    }

    pub(crate) fn record_capability_read() {
        CAPABILITY_READS.with(|reads| reads.set(reads.get() + 1));
    }
}

fn next_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "{prefix}-{}-{nanos}-{}",
        std::process::id(),
        ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// A fresh durable command identity for a queued intention. Keeping the
/// generator here means the shell and the backend never mint competing ids.
pub(crate) fn next_command_id() -> String {
    next_id("command")
}

/// Human text for a unified connection error. The typed error stays intact at
/// the client boundary; this is only the message a TUI row can display.
fn describe_connection_error(error: &orbit_client::unified::ClientConnectionError) -> String {
    use orbit_client::unified::ClientConnectionError;
    match error {
        ClientConnectionError::Local(error) => error.message.clone(),
        ClientConnectionError::Unavailable(message) => message.clone(),
        ClientConnectionError::Unsupported(message) => (*message).to_owned(),
        ClientConnectionError::Serialization(error) => error.to_string(),
        other => format!("{other:?}"),
    }
}

fn unavailable(message: impl Into<String>) -> ClientError {
    ClientError {
        code: ClientErrorCode::Unavailable,
        message: message.into(),
    }
}

fn mapped(error: impl std::fmt::Debug) -> ClientError {
    unavailable(format!("{error:?}"))
}

/// The connected console is the only accepted-list surface; local deployments
/// use the shared shell views instead.
pub(crate) const CONNECTED_LIST_REQUIRES_CONNECTED: &str =
    "accepted listing requires a connected deployment; the local authority has no console reads";

/// The honest connected-mode disposition for every capability action surface.
/// Connected parity is tracked separately and must never be simulated by
/// queueing a command the connected transport cannot execute.
pub const CONNECTED_CAPABILITY_REFUSED: &str = "Capability actions require the local authoritative workspace; connected parity is tracked as M25 EC-004.";

/// Decode one accepted projection collection into the authority's own record
/// type. A row this binary cannot decode fails closed instead of being dropped.
fn decode_accepted_rows<T: serde::de::DeserializeOwned>(
    rows: Vec<serde_json::Value>,
    collection: &str,
) -> Result<Vec<T>, ClientError> {
    rows.into_iter()
        .map(|row| {
            serde_json::from_value(row).map_err(|error| {
                unavailable(format!(
                    "accepted {collection} row could not be decoded: {error}"
                ))
            })
        })
        .collect()
}

/// Map a connected-connection error into the shared typed client error so the
/// shell has one failure contract. Local facade errors keep their code.
fn connected_error(error: orbit_client::unified::ClientConnectionError) -> ClientError {
    use orbit_client::unified::ClientConnectionError;
    match error {
        ClientConnectionError::Local(error) => error,
        ClientConnectionError::Unavailable(message) => unavailable(message),
        other => mapped(other),
    }
}

/// Per-command outcome for the mutation the shell just submitted.
///
/// Aggregate outbox depth cannot tell "my command is pending" from "someone
/// else queued earlier", so connected rich mutations follow the exact command
/// identity recorded by the client. A missing identity or state fails closed as
/// [`MutationOutcome::Pending`] with an explicit `unavailable` state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MutationOutcome {
    Accepted,
    Pending {
        command_id: Option<String>,
        state: String,
    },
    Conflict {
        command_id: String,
        state: String,
        reason: String,
    },
}

impl MutationOutcome {
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// Anything that is not a committed acceptance keeps the pending signal on.
    pub fn needs_attention(&self) -> bool {
        !self.is_accepted()
    }
}

/// One row the pending-command inspector can show without exposing UI types.
///
/// A row is durable local intent only: `[pending]` or `[conflict]`. It is never
/// an accepted workspace record, even after a refresh.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCommand {
    /// Command identity, or the attachment id for a staged upload.
    pub id: String,
    /// Command family (`task`, `schedule`, `attachment`, ...).
    pub kind: String,
    /// Durable outbox state (`pending` or `conflict`).
    pub status: String,
    /// Permanent rejection that needs explicit reconciliation.
    pub conflict: bool,
    /// Short human summary derived from the canonical action payload.
    pub summary: String,
    /// Canonical action payload, or staged-upload metadata.
    pub payload: String,
}

/// A short, stable one-line summary of a queued canonical action. This is
/// presentation text only; it never claims acceptance.
fn pending_summary(kind: &str, action: &serde_json::Value) -> String {
    let verb = action
        .get("type")
        .or_else(|| action.get("kind"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("command");
    let identity = [
        "project_id",
        "task_id",
        "block_id",
        "routine_id",
        "milestone_id",
        "habit_id",
        "metric_id",
        "note_id",
        "occurrence_id",
        "path",
    ]
    .iter()
    .find_map(|key| action.get(*key).and_then(serde_json::Value::as_str));
    let caption = action
        .get("title")
        .or_else(|| action.get("name"))
        .or_else(|| action.get("objective"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    match (identity, caption.is_empty()) {
        (Some(id), false) => format!("{kind} {verb} {id} · {caption}"),
        (Some(id), true) => format!("{kind} {verb} {id}"),
        (None, false) => format!("{kind} {verb} · {caption}"),
        (None, true) => format!("{kind} {verb}"),
    }
}

/// One accepted Markdown note, rendered for a read-only console line.
fn describe_note(note: &orbit_runtime::knowledge_service::KnowledgeDocumentInfo) -> String {
    format!("{} — {} ({})", note.title, note.path, note.id)
}

/// Append one accepted note body, truncating only for display. A truncation is
/// labelled so a body never looks complete when it is not.
fn push_note_body(lines: &mut Vec<String>, body: &str) {
    const LIMIT: usize = 200;
    let body_lines: Vec<&str> = body.lines().collect();
    for line in body_lines.iter().take(LIMIT) {
        lines.push((*line).to_owned());
    }
    if body_lines.len() > LIMIT {
        lines.push(format!(
            "… {} more line(s) omitted",
            body_lines.len() - LIMIT
        ));
    }
}

/// Render one note's provenance and body. `provenance` must distinguish
/// accepted server state from the labelled offline mirror.
fn push_note_read(
    lines: &mut Vec<String>,
    provenance: &str,
    note: &orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
    body: &str,
) {
    lines.push(format!("{provenance} note: {}", note.title));
    lines.push(format!("id: {} • path: {}", note.id, note.path));
    lines.push(String::new());
    push_note_body(lines, body);
}

/// One-line replica health shared by `sync.status` (cached) and `sync.observe`
/// (live comparison). The two commands differ only in which report they read.
fn format_sync_report(report: &orbit_client::sync_health::WorkspaceSyncReport) -> String {
    let health = &report.health;
    let blocked = report
        .domains
        .blocking_head()
        .map(|(id, family, _)| format!(" first={family}:{id}"))
        .unwrap_or_default();
    format!(
        "Sync {:?} | accepted {:?} | live {:?} | pending {} | conflicts {}{} | mirror {} | next {:?}",
        health.primary,
        health.accepted_revision,
        health.observed_server_revision,
        health.pending_count,
        health.conflict_count,
        blocked,
        if health.markdown_mirror_complete {
            "complete"
        } else {
            "incomplete"
        },
        health.actions
    )
}

/// The accepted snapshot's revision, which every read-only planning analysis
/// and every approval is fenced against.
fn snapshot_revision(snapshot: &serde_json::Value) -> Result<i64, String> {
    snapshot
        .get("revision")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| "Accepted snapshot is missing its revision".to_owned())
}

fn outcome_from_disposition(
    connection: &AuthorityConnection,
    disposition: &CommandDisposition,
) -> MutationOutcome {
    match disposition {
        CommandDisposition::Pending => MutationOutcome::Pending {
            command_id: connection.last_queued_command_id(),
            state: "pending".into(),
        },
        _ => MutationOutcome::Accepted,
    }
}

/// Translate one durable command state into an outcome. The caller must only
/// pass a state read for the command this mutation queued.
fn outcome_from_state(
    command_id: Option<String>,
    state: Option<&str>,
    conflict_reason: Option<String>,
) -> MutationOutcome {
    match state {
        Some("accepted") => MutationOutcome::Accepted,
        Some("conflict") => MutationOutcome::Conflict {
            command_id: command_id.unwrap_or_default(),
            state: "conflict".into(),
            reason: conflict_reason
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| {
                    "command conflict requires explicit reconciliation; inspect the outbox".into()
                }),
        },
        // `pending`, `superseded` or an unexpected state is never success.
        Some(state) => MutationOutcome::Pending {
            command_id,
            state: state.to_owned(),
        },
        None => MutationOutcome::Pending {
            command_id,
            state: "unavailable".into(),
        },
    }
}

/// Read the conflict explanation recorded on the outbox row, if any.
fn conflict_reason(connection: &AuthorityConnection, command_id: &str) -> Option<String> {
    connection
        .command_status(command_id)
        .ok()
        .flatten()
        .and_then(|status| {
            status
                .pointer("/receipt/error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
}

/// Read the durable state of one command identity.
fn command_outcome(connection: &AuthorityConnection, command_id: &str) -> MutationOutcome {
    match connection.command_state(command_id) {
        Ok(state) => {
            let reason = (state.as_deref() == Some("conflict"))
                .then(|| conflict_reason(connection, command_id))
                .flatten();
            outcome_from_state(Some(command_id.to_owned()), state.as_deref(), reason)
        }
        Err(_) => MutationOutcome::Pending {
            command_id: Some(command_id.to_owned()),
            state: "unavailable".into(),
        },
    }
}

/// Follow the command queued by one rich mutation.
///
/// `before` is the last identity seen before the call. A new identity is the
/// only honest signal that this call queued something; if none appeared, the
/// outcome fails closed rather than treating "no visible growth" as success.
fn read_rich_outcome(connection: &AuthorityConnection, before: Option<String>) -> MutationOutcome {
    if matches!(connection, AuthorityConnection::Embedded(_)) {
        // Embedded authority commits synchronously and keeps no outbox.
        return MutationOutcome::Accepted;
    }
    let after = connection.last_queued_command_id();
    let Some(command_id) = after else {
        return MutationOutcome::Pending {
            command_id: None,
            state: "unavailable".into(),
        };
    };
    if Some(&command_id) == before.as_ref() {
        return MutationOutcome::Pending {
            command_id: Some(command_id),
            state: "unavailable".into(),
        };
    }
    command_outcome(connection, &command_id)
}

/// Process-scoped directory for connected attachment exports. The first call
/// creates it; later calls reuse the same path inside this process.
fn connected_export_dir() -> Result<std::path::PathBuf, ClientError> {
    static DIRECTORY: OnceLock<std::path::PathBuf> = OnceLock::new();
    if let Some(path) = DIRECTORY.get() {
        return Ok(path.clone());
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-attachments-{}-{nanos}",
        std::process::id()
    ));
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).map_err(|error| {
        unavailable(format!(
            "cannot create connected attachment export directory {}: {error}",
            path.display()
        ))
    })?;
    Ok(DIRECTORY.get_or_init(|| path).clone())
}

pub enum Backend {
    Local(OrbitClient),
    Connected {
        connection: AuthorityConnection,
        /// Outcome of the most recent connected mutation, consumed by the shell
        /// through [`Backend::take_outcome`].
        pending: Option<MutationOutcome>,
    },
}

impl Backend {
    pub fn open_connected(replica: &Path, server: &str) -> Result<Self, ClientError> {
        let token = std::env::var("ORBIT_AUTHORITY_TOKEN").ok();
        let device = std::env::var("ORBIT_AUTHORITY_DEVICE").ok();
        let identity = match (token.as_deref(), device.as_deref()) {
            (Some(token), Some(device)) if !token.is_empty() && !device.is_empty() => {
                Some((device, token))
            }
            _ => None,
        };
        Self::open_connected_with_identity(replica, server, identity)
    }

    pub fn open_connected_with_identity(
        replica: &Path,
        server: &str,
        identity: Option<(&str, &str)>,
    ) -> Result<Self, ClientError> {
        let connection = AuthorityConnection::remote(replica, server, identity)
            .map_err(|error| unavailable(format!("connected client unavailable: {error:?}")))?;
        Ok(Self::Connected {
            connection,
            pending: None,
        })
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    /// Returns and clears the outcome of the last connected mutation.
    pub fn take_outcome(&mut self) -> Option<MutationOutcome> {
        match self {
            Self::Local(_) => None,
            Self::Connected { pending, .. } => pending.take(),
        }
    }

    /// True when the last mutation is queued, conflicted, or its state could
    /// not be established. Never inferred from aggregate outbox depth.
    pub fn take_pending(&mut self) -> bool {
        self.take_outcome()
            .is_some_and(|outcome| outcome.needs_attention())
    }

    pub fn refresh_accepted(&mut self) -> Result<(), ClientError> {
        match self {
            Self::Local(_) => Ok(()),
            Self::Connected { connection, .. } => connection.refresh().map(|_| ()).map_err(mapped),
        }
    }

    pub fn flush_pending(&mut self) -> Result<(String, bool), ClientError> {
        match self {
            Self::Local(_) => Ok(("Local authority has no outbox".into(), false)),
            Self::Connected { connection, .. } => {
                use orbit_client::authority::FlushOutcome;
                let (message, blocked) = match connection.flush().map_err(mapped)? {
                    FlushOutcome::Empty => ("No pending commands".to_owned(), false),
                    FlushOutcome::Accepted(count) => {
                        (format!("Server accepted {count} pending command(s)"), false)
                    }
                    FlushOutcome::Blocked {
                        accepted,
                        command_id,
                        reason,
                    } => (
                        format!(
                            "Flush blocked after accepting {accepted} command(s): {command_id} was rejected. {reason}. Inspect the outbox with `!` and resolve it with `resolve <command-id> <command>` in the connected console (or the CLI)."
                        ),
                        true,
                    ),
                };
                if let Err(error) = connection.refresh() {
                    return Ok((
                        format!("{message}; accepted-state refresh failed: {error:?}"),
                        true,
                    ));
                }
                Ok((message, blocked))
            }
        }
    }

    /// Pending work the user can see, including staged attachment uploads.
    /// The client owns this sum; the TUI must not re-derive it from outbox
    /// lengths, which omit attachment staging.
    pub fn pending_count(&self) -> usize {
        #[cfg(test)]
        diagnostics::record_pending_count_read();
        match self {
            Self::Local(_) => 0,
            Self::Connected { connection, .. } => connection
                .pending_command_count()
                .map(|count| count.max(0) as usize)
                .unwrap_or_else(|_| {
                    connection
                        .unresolved_commands()
                        .map(|rows| rows.len())
                        .unwrap_or(0)
                }),
        }
    }

    /// Every durable but unaccepted intention: ordered outbox commands plus
    /// staged attachment uploads, in their own rows. Never accepted data.
    pub fn pending_commands(&self) -> Result<Vec<PendingCommand>, String> {
        let Self::Connected { connection, .. } = self else {
            return Err(
                "pending command inspection requires a connected replica; the local authority commits synchronously"
                    .into(),
            );
        };
        let mut rows = Vec::new();
        for row in connection
            .unresolved_commands()
            .map_err(|error| describe_connection_error(&error))?
        {
            let id = row
                .get("command_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            let kind = row
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("command")
                .to_owned();
            let status = row
                .get("state")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("pending")
                .to_owned();
            let action = row
                .get("action")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            rows.push(PendingCommand {
                id: id.to_owned(),
                conflict: status == "conflict",
                summary: pending_summary(&kind, &action),
                payload: action.to_string(),
                kind,
                status,
            });
        }
        for upload in connection
            .pending_attachment_uploads()
            .map_err(|error| describe_connection_error(&error))?
        {
            let attachment = &upload.attachment;
            rows.push(PendingCommand {
                id: attachment.id.clone(),
                kind: "attachment".into(),
                conflict: upload.state == "conflict",
                status: upload.state.clone(),
                summary: format!(
                    "upload {} → {}:{}",
                    attachment.filename, attachment.owner_kind, attachment.owner_id
                ),
                payload: serde_json::json!({
                    "filename": attachment.filename,
                    "content_type": attachment.content_type,
                    "size_bytes": attachment.size_bytes,
                    "owner_kind": attachment.owner_kind,
                    "owner_id": attachment.owner_id,
                    "checksum": attachment.checksum,
                })
                .to_string(),
            });
        }
        Ok(rows)
    }

    /// Read-only inspection of one durable intention. Reports the recorded
    /// state, last error, conflict marker and any accepted receipt. A missing
    /// identity is stated explicitly instead of being treated as accepted.
    pub fn pending_command_detail(&self, id: &str) -> Result<String, String> {
        let Self::Connected { connection, .. } = self else {
            return Err(
                "pending command detail requires a connected replica; the local authority commits synchronously"
                    .into(),
            );
        };
        let mut lines = Vec::new();
        match connection
            .command_status(id)
            .map_err(|error| describe_connection_error(&error))?
        {
            Some(status) => {
                let kind = status
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("command");
                let state = status
                    .get("state")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown");
                lines.push(format!("command: {id}"));
                lines.push(format!("kind: {kind}"));
                lines.push(format!("state: {state}"));
                lines.push("attempts: not recorded by this replica".into());
                if let Some(revision) = status.get("expected_revision").and_then(|v| v.as_i64()) {
                    lines.push(format!("expected revision: {revision}"));
                }
                if let Some(action) = status.get("action") {
                    lines.push(format!("local intent: {action}"));
                }
                if let Some(reason) = status
                    .pointer("/receipt/error")
                    .and_then(serde_json::Value::as_str)
                {
                    lines.push(format!("last error: {reason}"));
                }
                if state == "conflict" {
                    lines.push(
                        "conflict: yes — the authority permanently rejected this command; it stays queued until you explicitly submit a reviewed replacement"
                            .into(),
                    );
                }
                match connection.accepted_receipt(id) {
                    Ok(Some(receipt)) => lines.push(format!("accepted receipt: {receipt}")),
                    Ok(None) => lines.push("accepted receipt: none (not accepted)".into()),
                    Err(error) => lines.push(format!(
                        "accepted receipt: unavailable ({})",
                        describe_connection_error(&error)
                    )),
                }
            }
            None => {
                let staged = connection
                    .pending_attachment_uploads()
                    .map_err(|error| describe_connection_error(&error))?
                    .into_iter()
                    .find(|upload| upload.attachment.id == id);
                match staged {
                    Some(upload) => {
                        let attachment = upload.attachment;
                        lines.push(format!("staged upload: {}", attachment.id));
                        lines.push(format!(
                            "owner: {}:{}",
                            attachment.owner_kind, attachment.owner_id
                        ));
                        lines.push(format!("filename: {}", attachment.filename));
                        lines.push(format!("content type: {}", attachment.content_type));
                        lines.push(format!("size: {} bytes", attachment.size_bytes));
                        lines.push(format!("checksum: {}", attachment.checksum));
                        lines.push(format!("state: {}", upload.state));
                        lines.push(match upload.last_error {
                            Some(error) => format!("last error: {error}"),
                            None => "last error: none recorded".into(),
                        });
                        lines.push(
                            "This content is staged locally only; it was never accepted by the server."
                                .into(),
                        );
                    }
                    None => {
                        lines.push(format!("No durable command or staged upload with id {id}."));
                        lines.push(
                            "A missing identity is not acceptance; refresh the accepted snapshot to check for a receipt."
                                .into(),
                        );
                    }
                }
            }
        }
        Ok(lines.join("\n"))
    }

    /// Queue one user-reviewed intention. The returned identity is durable
    /// local intent: it is pending, not accepted.
    pub fn queue_connected_intent(
        &mut self,
        description: &str,
        kind: CommandKind,
        action: &serde_json::Value,
    ) -> Result<String, String> {
        let Self::Connected {
            connection,
            pending,
        } = self
        else {
            return Err("queueing connected intentions requires a connected replica".into());
        };
        let command_id = next_command_id();
        connection
            .queue_intent(&command_id, kind, action)
            .map_err(|error| {
                format!(
                    "Unable to queue {description}: {}",
                    describe_connection_error(&error)
                )
            })?;
        *pending = Some(MutationOutcome::Pending {
            command_id: Some(command_id.clone()),
            state: "pending".into(),
        });
        Ok(command_id)
    }

    /// Stage a reviewed batch atomically in the replica outbox. Returns how
    /// many commands were newly staged; none are accepted.
    pub fn queue_connected_batch(
        &mut self,
        intents: &[(String, CommandKind, serde_json::Value)],
    ) -> Result<usize, String> {
        let Self::Connected {
            connection,
            pending,
        } = self
        else {
            return Err("queueing connected intentions requires a connected replica".into());
        };
        let inserted = connection.queue_intent_batch(intents).map_err(|error| {
            format!(
                "Workflow staging rejected: {}",
                describe_connection_error(&error)
            )
        })?;
        *pending = Some(MutationOutcome::Pending {
            command_id: intents.first().map(|(id, _, _)| id.clone()),
            state: "pending".into(),
        });
        Ok(inserted)
    }

    /// Review a rejected command against accepted history. Live review is
    /// preferred; the replica's stored review is used when offline. Either way
    /// the returned token is snapshot-bound and must not be auto-rebased.
    pub fn review_connected_conflict(&mut self, id: &str) -> Result<ConflictReview, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("conflict review requires a connected replica".into());
        };
        match connection.review_conflict_online(id) {
            Ok(review) => Ok(review),
            Err(live) => connection.conflict_review(id).map_err(|replica| {
                format!(
                    "conflict review unavailable: live review failed ({}); replica review failed ({})",
                    describe_connection_error(&live),
                    describe_connection_error(&replica)
                )
            }),
        }
    }

    /// Replace one reviewed conflict with a fresh pending command. The review
    /// token is mandatory; nothing is queued without it and the rejected audit
    /// row is retained.
    pub fn resolve_connected_conflict(
        &mut self,
        review: &ConflictReview,
        replacement_id: &str,
        kind: CommandKind,
        action: &serde_json::Value,
    ) -> Result<CommandDisposition, String> {
        let Self::Connected {
            connection,
            pending,
        } = self
        else {
            return Err("conflict resolution requires a connected replica".into());
        };
        let disposition = connection
            .resolve_reviewed_conflict(review, replacement_id, kind, action)
            .map_err(|error| {
                format!(
                    "conflict replacement rejected: {}",
                    describe_connection_error(&error)
                )
            })?;
        *pending = Some(match disposition {
            CommandDisposition::Pending => MutationOutcome::Pending {
                command_id: Some(replacement_id.to_owned()),
                state: "pending".into(),
            },
            _ => MutationOutcome::Accepted,
        });
        Ok(disposition)
    }

    /// Read-only replica sync report for connected deployments. Offline state
    /// is never reported as synchronized with an unobserved live server.
    pub fn connected_sync_report(&self) -> Result<String, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("sync status requires a connected replica".into());
        };
        let report = connection
            .workspace_sync_report()
            .map_err(|error| describe_connection_error(&error))?;
        Ok(format_sync_report(&report))
    }

    /// Compare the cached replica report with a live authority observation.
    /// The same domains are compared; only the live observation can report
    /// whether the server has advanced since this replica last refreshed.
    pub fn connected_observe_sync_report(&mut self) -> Result<String, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("sync observation requires a connected replica".into());
        };
        let report = connection
            .observe_workspace_sync_report()
            .map_err(|error| {
                format!(
                    "Live sync observation unavailable (use sync.status for the cached report): {}",
                    describe_connection_error(&error)
                )
            })?;
        Ok(format_sync_report(&report))
    }

    /// Search accepted server Markdown; when the server is unreachable, fall
    /// back to the local accepted mirror and label it as cached and stale.
    pub fn connected_notes_search(&self, query: &str) -> Result<Vec<String>, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("note search requires a connected replica".into());
        };
        if query.trim().is_empty() || query.len() > 512 {
            return Err("note.search requires a nonempty query of at most 512 characters".into());
        }
        let mut lines = Vec::new();
        match connection.markdown_search(query, 100) {
            Ok(notes) => {
                lines.push(format!(
                    "{} accepted server note(s) for {query:?}; pending edits excluded",
                    notes.len()
                ));
                lines.extend(notes.iter().map(describe_note));
            }
            Err(live) => {
                let notes = connection
                    .search_cached_markdown(query, 100)
                    .map_err(|cached| {
                        format!(
                            "note search unavailable: live search failed ({}); cached search failed ({})",
                            describe_connection_error(&live),
                            describe_connection_error(&cached)
                        )
                    })?;
                lines.push(format!(
                    "OFFLINE: {} cached accepted note(s) for {query:?}; the mirror may be stale and excludes pending edits",
                    notes.len()
                ));
                lines.extend(notes.iter().map(describe_note));
            }
        }
        Ok(lines)
    }

    /// Refresh the local accepted Markdown mirror from the server.
    pub fn connected_sync_notes(&mut self) -> Result<String, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("note synchronization requires a connected replica".into());
        };
        let count = connection
            .sync_markdown_mirror()
            .map_err(|error| describe_connection_error(&error))?;
        Ok(format!(
            "{count} server-accepted Markdown note(s) mirrored atomically; deleted and moved notes reconciled; pending edits untouched"
        ))
    }

    /// Every note in the local accepted mirror without contacting the server.
    pub fn connected_cached_notes(&self) -> Result<Vec<String>, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("cached note browsing requires a connected replica".into());
        };
        let notes = connection
            .cached_markdown_inventory(500)
            .map_err(|error| describe_connection_error(&error))?;
        let mut lines = vec![format!(
            "{} cached accepted Markdown note(s); the offline mirror may be stale",
            notes.len()
        )];
        lines.extend(notes.iter().map(describe_note));
        Ok(lines)
    }

    /// Read one accepted Markdown body by immutable note id or vault-relative
    /// path. A connected note read is accepted server state; when the server is
    /// unreachable only the labelled accepted mirror is used, and a missing
    /// mirror copy fails instead of inventing content.
    pub fn connected_note_show(&self, target: &str) -> Result<Vec<String>, String> {
        let target = target.trim();
        if target.is_empty() || target.len() > 1024 {
            return Err("note.show requires a note ID or a vault-relative .md path".into());
        }
        let Self::Connected { connection, .. } = self else {
            return Err("note.show requires a connected replica".into());
        };
        let is_path = target.contains('/') || target.to_ascii_lowercase().ends_with(".md");
        if is_path
            && (target.contains('\\')
                || target.contains('\0')
                || target.starts_with('/')
                || target
                    .split('/')
                    .any(|part| part.is_empty() || part.starts_with('.')))
        {
            return Err(format!("invalid vault-relative note path: {target:?}"));
        }
        let mut lines = Vec::new();
        if is_path {
            match connection.markdown_document(target) {
                Ok(body) => match connection.cached_markdown_inventory(500) {
                    Ok(notes) => {
                        let info = notes.into_iter().find(|note| note.path == target);
                        match info {
                            Some(note) => {
                                push_note_read(&mut lines, "Accepted server", &note, &body)
                            }
                            None => {
                                lines.push(format!("Accepted server Markdown note {target}"));
                                lines.push(String::new());
                                push_note_body(&mut lines, &body);
                            }
                        }
                    }
                    Err(_) => {
                        lines.push(format!("Accepted server Markdown note {target}"));
                        lines.push(String::new());
                        push_note_body(&mut lines, &body);
                    }
                },
                Err(live) => {
                    let cached = connection
                        .cached_markdown_inventory(500)
                        .ok()
                        .and_then(|notes| notes.into_iter().find(|note| note.path == target));
                    match cached {
                        Some(note) => match connection.cached_markdown_by_id(&note.id) {
                            Ok(Some((info, body))) => {
                                push_note_read(&mut lines, "OFFLINE cached", &info, &body)
                            }
                            _ => {
                                return Err(format!(
                                    "note.show unavailable: live read failed ({}); no cached copy exists",
                                    describe_connection_error(&live)
                                ));
                            }
                        },
                        None => {
                            return Err(format!(
                                "note.show unavailable: live read failed ({}); no cached copy exists",
                                describe_connection_error(&live)
                            ));
                        }
                    }
                }
            }
        } else {
            match connection.markdown_by_id(target) {
                Ok(Some((note, body))) => {
                    push_note_read(&mut lines, "Accepted server", &note, &body)
                }
                Ok(None) => {
                    return Err(format!("No accepted Markdown note with id {target}"));
                }
                Err(live) => match connection.cached_markdown_by_id(target) {
                    Ok(Some((note, body))) => {
                        push_note_read(&mut lines, "OFFLINE cached", &note, &body)
                    }
                    _ => {
                        return Err(format!(
                            "note.show unavailable: live read failed ({}); no cached copy exists",
                            describe_connection_error(&live)
                        ));
                    }
                },
            }
        }
        Ok(lines)
    }

    /// The accepted snapshot revision. Read-only planning analyses and any
    /// approval are fenced against this exact value.
    pub fn accepted_revision(&self) -> Result<i64, String> {
        let snapshot = self.connected_snapshot()?;
        snapshot_revision(&snapshot)
    }

    /// Fail closed when the replica has never observed accepted state.
    ///
    /// Accepted-list calls must share the same message as `plan.*`: an empty
    /// projection with no snapshot means "nothing has been accepted here yet",
    /// not "the authority is empty".
    fn require_accepted_snapshot(&self) -> Result<(), ClientError> {
        self.accepted_revision().map(|_| ()).map_err(unavailable)
    }

    fn connected_snapshot(&self) -> Result<serde_json::Value, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("accepted planning state requires a connected replica".into());
        };
        connection
            .snapshot()
            .map_err(|error| describe_connection_error(&error))?
            .ok_or_else(|| "No accepted snapshot yet; refresh while online first".to_owned())
    }

    /// The one accepted task graph every read-only planning analysis uses.
    fn accepted_task_graph(
        &self,
    ) -> Result<(i64, Vec<TaskRecord>, Vec<TaskDependencyEdge>), String> {
        let snapshot = self.connected_snapshot()?;
        let revision = snapshot_revision(&snapshot)?;
        let tasks: Vec<TaskRecord> = serde_json::from_value(
            snapshot
                .get("tasks")
                .cloned()
                .ok_or_else(|| "Accepted snapshot missing tasks".to_owned())?,
        )
        .map_err(|error| format!("Accepted snapshot tasks unavailable: {error}"))?;
        let edges: Vec<TaskDependencyEdge> = serde_json::from_value(
            snapshot
                .get("task_dependencies")
                .cloned()
                .ok_or_else(|| "Accepted snapshot missing task dependencies".to_owned())?,
        )
        .map_err(|error| format!("Accepted snapshot dependencies unavailable: {error}"))?;
        Ok((revision, tasks, edges))
    }

    /// Read-only project-aware next actions over the accepted snapshot.
    pub fn connected_portfolio(
        &self,
        request: &PortfolioActionRequest,
    ) -> Result<PortfolioActionReport, String> {
        let (revision, tasks, edges) = self.accepted_task_graph()?;
        portfolio_actions::recommend_portfolio_actions(revision, &tasks, &edges, request)
            .map_err(str::to_owned)
    }

    /// Read-only what-if comparison over the accepted snapshot. The request is
    /// revision-bound; a changed revision fails instead of comparing stale data.
    pub fn connected_scenario_compare(
        &self,
        request: &ScenarioCompareRequest,
    ) -> Result<ScenarioComparison, String> {
        let (revision, tasks, edges) = self.accepted_task_graph()?;
        if request.expected_revision != revision {
            return Err(
                "Accepted revision changed: compare again against the current accepted state"
                    .into(),
            );
        }
        orbit_runtime::planning_scenarios::compare_scenarios(revision, &tasks, &edges, request)
            .map_err(str::to_owned)
    }

    /// Read-only, online reconstruction of a scenario as exact calendar slots.
    /// The returned review is snapshot-bound and must be approved separately.
    pub fn connected_scenario_review(
        &self,
        request: &ScenarioExecutionRequest,
    ) -> Result<ScenarioExecutionReview, String> {
        let revision = self.accepted_revision()?;
        let Self::Connected { connection, .. } = self else {
            return Err("scenario review requires a connected replica".into());
        };
        let review = connection
            .review_scenario_execution(request)
            .map_err(|error| {
                format!(
                    "Scenario review requires live authority: {}",
                    describe_connection_error(&error)
                )
            })?;
        // Fail closed: a review that is not read-only, is fenced to another
        // revision, describes another command, or omits reviewed slots cannot
        // be approved.
        if !review.read_only
            || review.review.expected_revision != revision
            || review.review.command_id != request.command_id
            || review.review.scheduled_slots.len() != review.plan.items.len()
        {
            return Err("Authoritative scenario review contract mismatch".into());
        }
        Ok(review)
    }

    /// Apply exactly one reviewed and explicitly approved schedule. This is a
    /// synchronous authoritative write, never a queued intention; a rejection
    /// requires a fresh review.
    pub fn connected_apply_approved_schedule(
        &mut self,
        request: &ApprovedScheduleRequest,
    ) -> Result<ApprovedScheduleReceipt, String> {
        let Self::Connected { connection, .. } = self else {
            return Err("approving a schedule requires a connected replica".into());
        };
        connection
            .apply_approved_scenario_schedule(request)
            .map_err(|error| {
                format!(
                    "Plan execution rejected: {}",
                    describe_connection_error(&error)
                )
            })
    }

    fn record_disposition(&mut self, disposition: &CommandDisposition) {
        if let Self::Connected {
            connection,
            pending,
        } = self
        {
            *pending = Some(outcome_from_disposition(connection, disposition));
        }
    }

    /// Follow the exact command a rich mutation queued. `before` is the last
    /// identity observed before the call, so a concurrent queue cannot be
    /// mistaken for this mutation.
    fn record_rich_outcome(&mut self, before: Option<String>) {
        let Self::Connected {
            connection,
            pending,
        } = self
        else {
            return;
        };
        *pending = Some(read_rich_outcome(connection, before));
    }

    pub fn workspace_view_data(&self) -> Result<orbit_ui::WorkspaceData, orbit_ui::ViewDataError> {
        let mut data = match self {
            Self::Local(client) => client.workspace_view_data()?,
            Self::Connected { connection, .. } => connection.workspace_view_data()?,
        };
        data.pending_commands = self.pending_count();
        Ok(data)
    }

    #[allow(dead_code)]
    pub fn project_view_data(
        &self,
        project_id: &str,
        observed_at_ms: i64,
    ) -> Result<orbit_ui::ProjectData, orbit_ui::ViewDataError> {
        let mut data = match self {
            Self::Local(client) => client.project_view_data(project_id, observed_at_ms)?,
            Self::Connected { connection, .. } => connection.project_view_data(project_id)?,
        };
        data.pending_commands = self.pending_count();
        Ok(data)
    }

    pub fn resolve_project_views(
        &self,
        project_id: &str,
        registry: &orbit_ui::ComponentRegistry,
        observed_at_ms: i64,
    ) -> Result<(orbit_ui::ProjectData, Vec<orbit_ui::ResolvedView>, bool), orbit_ui::ViewDataError>
    {
        match self {
            Self::Local(client) => {
                client.resolve_project_views(project_id, registry, observed_at_ms)
            }
            Self::Connected { connection, .. } => {
                connection.resolve_connected_project_views(project_id, registry)
            }
        }
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_projects(),
            Self::Connected { .. } => Ok(self
                .workspace_view_data()
                .map_err(|error| unavailable(error.to_string()))?
                .projects
                .into_iter()
                .map(|project| ProjectDto {
                    id: project.id,
                    name: project.name,
                    status: project.status,
                    mode: project.mode,
                    objective: project.objective,
                    progress_signals: Vec::new(),
                    success_criteria: Vec::new(),
                    completion_ready: project.completion_ready,
                })
                .collect()),
        }
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_tasks(),
            Self::Connected { .. } => Ok(self
                .workspace_view_data()
                .map_err(|error| unavailable(error.to_string()))?
                .tasks
                .into_iter()
                .map(|task| TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task.id,
                    title: task.title,
                    status: task.status,
                    project_id: task.project_id,
                    milestone_id: task.milestone_id,
                    estimated_minutes: task.estimated_minutes,
                    earliest_start_at_ms: task.earliest_start_at_ms,
                    deadline_at_ms: task.deadline_at_ms,
                })
                .collect()),
        }
    }

    /// Accepted routine compositions for the read-only console listing.
    ///
    /// Connected deployments only: the console is the connected read surface,
    /// and a local authority has no snapshot to list. Rows are the same typed
    /// records the authority serves, so the console cannot invent a shape.
    pub fn accepted_routines(
        &self,
    ) -> Result<Vec<orbit_runtime::authority_routines::RoutineRecord>, ClientError> {
        match self {
            Self::Local(_) => Err(unavailable(CONNECTED_LIST_REQUIRES_CONNECTED)),
            Self::Connected { connection, .. } => {
                self.require_accepted_snapshot()?;
                connection
                    .routines()
                    .map_err(connected_error)
                    .and_then(|rows| decode_accepted_rows(rows, "routines"))
            }
        }
    }

    /// Accepted project strategies for the read-only console listing.
    pub fn accepted_strategies(
        &self,
    ) -> Result<Vec<orbit_runtime::authority_strategy::StrategyRecord>, ClientError> {
        match self {
            Self::Local(_) => Err(unavailable(CONNECTED_LIST_REQUIRES_CONNECTED)),
            Self::Connected { connection, .. } => {
                self.require_accepted_snapshot()?;
                connection
                    .strategies()
                    .map_err(connected_error)
                    .and_then(|rows| decode_accepted_rows(rows, "strategies"))
            }
        }
    }

    /// Accepted milestones for the read-only console listing.
    pub fn accepted_milestones(
        &self,
    ) -> Result<Vec<orbit_runtime::authority_milestones::MilestoneRecord>, ClientError> {
        match self {
            Self::Local(_) => Err(unavailable(CONNECTED_LIST_REQUIRES_CONNECTED)),
            Self::Connected { connection, .. } => {
                self.require_accepted_snapshot()?;
                connection
                    .milestones()
                    .map_err(connected_error)
                    .and_then(|rows| decode_accepted_rows(rows, "milestones"))
            }
        }
    }

    /// Accepted schedule blocks for the read-only console listing.
    pub fn accepted_schedule(
        &self,
    ) -> Result<Vec<orbit_runtime::authority_schedule::ScheduleRecord>, ClientError> {
        match self {
            Self::Local(_) => Err(unavailable(CONNECTED_LIST_REQUIRES_CONNECTED)),
            Self::Connected { connection, .. } => {
                self.require_accepted_snapshot()?;
                connection
                    .schedule()
                    .map_err(connected_error)
                    .and_then(|rows| decode_accepted_rows(rows, "schedule"))
            }
        }
    }

    pub fn agent_job_detail(
        &self,
        job_id: &str,
    ) -> Result<Option<orbit_client::view_data::AgentJobDetail>, ClientError> {
        match self {
            Self::Local(client) => client.agent_job_detail(job_id),
            Self::Connected { .. } => Ok(None),
        }
    }

    pub fn list_knowledge(&self) -> Result<Vec<orbit_client::KnowledgeItemDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_knowledge(),
            Self::Connected { .. } => Err(unavailable(
                "connected mode note browsing uses the connected console (`:`): note.search, note.cached or note.show",
            )),
        }
    }

    pub fn task_allowed_statuses(&self, status: &str) -> Vec<String> {
        orbit_client::view_data::allowed_task_statuses(status)
    }

    pub fn create_project(
        &mut self,
        request: CreateProjectRequest,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.create_project(request),
            Self::Connected { connection, .. } => {
                let id = request.id.unwrap_or_else(|| next_id("project"));
                let revision = connection
                    .workspace_view_data()
                    .ok()
                    .and_then(|data| data.revision)
                    .unwrap_or(0);
                let disposition = connection
                    .create_project(ProjectCommand {
                        workflow_id: None,
                        command_id: next_id("command"),
                        expected_revision: revision,
                        project_id: id.clone(),
                        name: request.name.clone(),
                    })
                    .map_err(mapped)?;
                self.record_disposition(&disposition);
                Ok(ProjectDto {
                    id,
                    name: request.name,
                    status: "proposed".into(),
                    mode: "standard".into(),
                    objective: None,
                    progress_signals: Vec::new(),
                    success_criteria: Vec::new(),
                    completion_ready: false,
                })
            }
        }
    }

    pub fn create_task(&mut self, request: CreateTaskRequest) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.create_task(request),
            Self::Connected {
                connection,
                pending,
            } => {
                let id = request.id.unwrap_or_else(|| next_id("task"));
                let milestone_id = request.milestone_id.clone();
                let revision = connection
                    .workspace_view_data()
                    .ok()
                    .and_then(|data| data.revision)
                    .unwrap_or(0);
                let disposition = connection
                    .task_command(TaskCommand {
                        command_id: next_id("command"),
                        expected_revision: revision,
                        action: TaskAction::Create {
                            task_id: id.clone(),
                            title: request.title.clone(),
                            project_id: request.project_id.clone(),
                        },
                    })
                    .map_err(mapped)?;
                *pending = Some(outcome_from_disposition(connection, &disposition));
                if let Some(milestone_id) = milestone_id.clone() {
                    let disposition = connection
                        .task_command(TaskCommand {
                            command_id: next_id("command"),
                            expected_revision: revision + 1,
                            action: TaskAction::AssignMilestone {
                                task_id: id.clone(),
                                milestone_id: Some(milestone_id),
                            },
                        })
                        .map_err(mapped)?;
                    *pending = Some(outcome_from_disposition(connection, &disposition));
                }
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id,
                    title: request.title,
                    status: "inbox".into(),
                    project_id: request.project_id,
                    milestone_id,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    fn connected_task_command(&mut self, action: TaskAction) -> Result<(), ClientError> {
        match self {
            Self::Local(_) => Err(unavailable("local task command path")),
            Self::Connected { connection, .. } => {
                let revision = connection
                    .workspace_view_data()
                    .ok()
                    .and_then(|data| data.revision)
                    .unwrap_or(0);
                let disposition = connection
                    .task_command(TaskCommand {
                        command_id: next_id("command"),
                        expected_revision: revision,
                        action,
                    })
                    .map_err(mapped)?;
                self.record_disposition(&disposition);
                Ok(())
            }
        }
    }

    pub fn set_task_status(
        &mut self,
        request: SetTaskStatusRequest,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.set_task_status(request),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::ChangeStatus {
                    task_id: request.id.clone(),
                    status: request.status.clone(),
                })?;
                let title = self
                    .workspace_view_data()
                    .ok()
                    .and_then(|workspace| {
                        workspace
                            .tasks
                            .into_iter()
                            .find(|task| task.id == request.id)
                            .map(|task| task.title)
                    })
                    .unwrap_or_else(|| "Task".into());
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: request.id,
                    title,
                    status: request.status,
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn rename_task(&mut self, id: String, title: String) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.rename_task(id, title),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::Rename {
                    task_id: id.clone(),
                    title,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn set_task_planning(
        &mut self,
        request: SetTaskPlanningRequest,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.set_task_planning(request),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::SetPlanning {
                    task_id: request.id.clone(),
                    estimated_minutes: request.estimated_minutes.map(i64::from),
                    earliest_start_at_ms: request.earliest_start_at_ms,
                    deadline_at_ms: request.deadline_at_ms,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: request.id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: request.estimated_minutes,
                    earliest_start_at_ms: request.earliest_start_at_ms,
                    deadline_at_ms: request.deadline_at_ms,
                })
            }
        }
    }

    pub fn add_task_prerequisite(
        &mut self,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.add_task_prerequisite(task_id, prerequisite_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::AddPrerequisite {
                    task_id: task_id.clone(),
                    prerequisite_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn remove_task_prerequisite(
        &mut self,
        task_id: String,
        prerequisite_id: String,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.remove_task_prerequisite(task_id, prerequisite_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::RemovePrerequisite {
                    task_id: task_id.clone(),
                    prerequisite_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn assign_task_project(
        &mut self,
        task_id: String,
        project_id: Option<String>,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.assign_task_project(task_id, project_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::AssignProject {
                    task_id: task_id.clone(),
                    project_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn assign_task_milestone(
        &mut self,
        task_id: String,
        milestone_id: Option<String>,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => client.assign_task_milestone(task_id, milestone_id),
            Self::Connected { .. } => {
                self.connected_task_command(TaskAction::AssignMilestone {
                    task_id: task_id.clone(),
                    milestone_id,
                })?;
                Ok(TaskDto {
                    description: String::new(),
                    priority: None,
                    labels: Vec::new(),
                    id: task_id,
                    title: String::new(),
                    status: String::new(),
                    project_id: None,
                    milestone_id: None,
                    estimated_minutes: None,
                    earliest_start_at_ms: None,
                    deadline_at_ms: None,
                })
            }
        }
    }

    pub fn configure_project_strategy(
        &mut self,
        request: ConfigureProjectStrategyRequest,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.configure_project_strategy(request),
            Self::Connected { .. } => Err(unavailable(
                "project strategy editing requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn set_project_status(
        &mut self,
        request: SetProjectStatusRequest,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.set_project_status(request),
            Self::Connected { .. } => Err(unavailable(
                "project status editing requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn list_project_evidence(
        &self,
        project_id: String,
    ) -> Result<Vec<ProjectEvidenceBindingDto>, ClientError> {
        match self {
            Self::Local(client) => client.list_project_evidence(project_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not project evidence bindings yet",
            )),
        }
    }

    pub fn change_project_evidence(
        &mut self,
        project_id: String,
        kind: String,
        evidence_id: String,
        bind: bool,
    ) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => {
                client.change_project_evidence(project_id, kind, evidence_id, bind)
            }
            Self::Connected { .. } => Err(unavailable(
                "evidence editing requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn set_metric_target(&mut self, target: MetricTargetDto) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => client.set_metric_target(target),
            Self::Connected { .. } => Err(unavailable(
                "metric targets require the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn record_metric_observation(
        &mut self,
        metric_id: String,
        observed_at_ms: i64,
        value: i64,
    ) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => {
                client.record_metric_observation(metric_id, observed_at_ms, value)
            }
            Self::Connected { .. } => Err(unavailable(
                "metric observations require the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn create_knowledge(
        &mut self,
        request: CreateKnowledgeRequest,
    ) -> Result<orbit_client::KnowledgeItemDto, ClientError> {
        match self {
            Self::Local(client) => client.create_knowledge(request),
            Self::Connected { .. } => Err(unavailable(
                "note creation requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn record_habit_occurrence(
        &mut self,
        request: RecordHabitOccurrenceRequest,
    ) -> Result<orbit_client::HabitOccurrenceDto, ClientError> {
        match self {
            Self::Local(client) => client.record_habit_occurrence(request),
            Self::Connected { .. } => Err(unavailable(
                "habit recording requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    #[allow(dead_code)]
    pub fn create_habit(
        &mut self,
        request: orbit_client::CreateHabitRequest,
    ) -> Result<HabitDto, ClientError> {
        match self {
            Self::Local(client) => client.create_habit(request),
            Self::Connected { .. } => Err(unavailable(
                "habit creation requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    #[allow(dead_code)]
    pub fn habit_history(
        &self,
        habit_id: String,
    ) -> Result<Vec<orbit_client::HabitOccurrenceDto>, ClientError> {
        match self {
            Self::Local(client) => client.habit_history(habit_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode habit history uses the connected console (`:`): habit.create or habit.record",
            )),
        }
    }

    pub fn install_bundled_blueprint(
        &mut self,
        id: &str,
    ) -> Result<orbit_blueprint::Blueprint, ClientError> {
        match self {
            Self::Local(client) => client.install_bundled_blueprint(id),
            Self::Connected { .. } => Err(unavailable(
                "blueprint installation requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn bind_project_blueprint(
        &mut self,
        project_id: &str,
        blueprint_id: &str,
        version: u32,
    ) -> Result<orbit_blueprint::BlueprintBinding, ClientError> {
        match self {
            Self::Local(client) => client.bind_project_blueprint(project_id, blueprint_id, version),
            Self::Connected { .. } => Err(unavailable(
                "blueprint binding requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn list_blueprint_versions(&self) -> Result<Vec<(String, u32)>, ClientError> {
        match self {
            Self::Local(client) => client.list_blueprint_versions(),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not expose published blueprint versions yet",
            )),
        }
    }

    pub fn project_blueprint_definition(
        &self,
        project_id: &str,
    ) -> Result<Option<orbit_blueprint::Blueprint>, ClientError> {
        match self {
            Self::Local(client) => client.project_blueprint_definition(project_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not project pinned blueprint definitions yet",
            )),
        }
    }

    /// Raw pinned binding; the shell prefers `project_workflow` for labels and
    /// keeps this for connected/legacy diagnostics.
    #[allow(dead_code)]
    pub fn project_blueprint_binding(
        &self,
        project_id: &str,
    ) -> Result<Option<orbit_blueprint::BlueprintBinding>, ClientError> {
        match self {
            Self::Local(client) => client.project_blueprint_binding(project_id),
            Self::Connected { .. } => Ok(None),
        }
    }

    pub fn list_blueprint_work_items(
        &self,
        project_id: &str,
    ) -> Result<Vec<orbit_blueprint::BlueprintWorkItem>, ClientError> {
        match self {
            Self::Local(client) => client.list_blueprint_work_items(project_id),
            Self::Connected { .. } => Err(unavailable(
                "connected mode does not project blueprint work items yet",
            )),
        }
    }

    pub fn create_blueprint_work_item(
        &mut self,
        project_id: &str,
        kind: &str,
        properties: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, ClientError> {
        match self {
            Self::Local(client) => client.create_blueprint_work_item(project_id, kind, properties),
            Self::Connected { .. } => Err(unavailable(
                "work-item creation requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn update_blueprint_work_item(
        &mut self,
        id: &str,
        expected_revision: u64,
        changes: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, ClientError> {
        match self {
            Self::Local(client) => {
                client.update_blueprint_work_item(id, expected_revision, changes)
            }
            Self::Connected { .. } => Err(unavailable(
                "work-item editing requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    pub fn transition_blueprint_work_item(
        &mut self,
        id: &str,
        to: &str,
        expected_revision: u64,
    ) -> Result<orbit_blueprint::BlueprintWorkItem, ClientError> {
        match self {
            Self::Local(client) => client.transition_blueprint_work_item(id, to, expected_revision),
            Self::Connected { .. } => Err(unavailable(
                "workflow transitions require the connected console (`:`, then Connected commands)",
            )),
        }
    }

    /// Capability installation is local-authority setup. It is exposed through
    /// the same facade the CLI uses; connected transports must not queue it.
    #[allow(dead_code)] // Exercised by the M30 capability acceptance tests.
    pub fn install_trusted_capability(&mut self, capability_id: &str) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => client.install_trusted_capability(capability_id),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    /// Explicit project-scoped capability grant.
    #[allow(dead_code)] // Exercised by the M30 capability acceptance tests.
    pub fn authorize_trusted_capability(
        &mut self,
        capability_id: &str,
        project_id: &str,
    ) -> Result<(), ClientError> {
        match self {
            Self::Local(client) => client.authorize_trusted_capability(capability_id, project_id),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    /// Publish a blueprint manifest against the installed capabilities.
    #[allow(dead_code)] // Exercised by the M30 capability acceptance tests.
    pub fn publish_blueprint_manifest(
        &mut self,
        manifest_json: &str,
    ) -> Result<orbit_blueprint::Blueprint, ClientError> {
        match self {
            Self::Local(client) => client.publish_blueprint_manifest(manifest_json),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    /// Capability actions are exposed by the local authoritative host only.
    /// Connected mode refuses with the tracked M25 EC-004 gap rather than
    /// queueing an intention that could never execute.
    pub fn capability_action_offers(
        &self,
        item_id: &str,
    ) -> Result<Vec<orbit_client::OrbitCapabilityOffer>, ClientError> {
        #[cfg(test)]
        diagnostics::record_capability_read();
        match self {
            Self::Local(client) => client.capability_action_offers(item_id),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    #[allow(dead_code)] // Part of the required M30 client/backend surface.
    pub fn capability_status(
        &self,
        project_id: Option<&str>,
    ) -> Result<Vec<orbit_client::OrbitCapabilityStatus>, ClientError> {
        #[cfg(test)]
        diagnostics::record_capability_read();
        match self {
            Self::Local(client) => client.capability_status(project_id),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    /// Record an explicit pending request. Never dispatches.
    pub fn request_capability_action(
        &mut self,
        request: orbit_client::OrbitCapabilityRequest,
    ) -> Result<orbit_client::OrbitCapabilityAction, ClientError> {
        match self {
            Self::Local(client) => client.request_capability_action(request),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    /// Claim and execute one recorded request exactly once.
    pub fn execute_capability_action(
        &mut self,
        command_id: &str,
    ) -> Result<orbit_client::OrbitCapabilityReceipt, ClientError> {
        match self {
            Self::Local(client) => client.execute_capability_action(command_id),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    #[allow(dead_code)] // Part of the required M30 client/backend surface.
    pub fn capability_action_state(
        &self,
        command_id: &str,
    ) -> Result<Option<orbit_client::OrbitCapabilityActionState>, ClientError> {
        #[cfg(test)]
        diagnostics::record_capability_read();
        match self {
            Self::Local(client) => client.capability_action_state(command_id),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    pub fn capability_action_history(
        &self,
        item_id: &str,
        limit: usize,
    ) -> Result<Vec<orbit_client::OrbitCapabilityAction>, ClientError> {
        #[cfg(test)]
        diagnostics::record_capability_read();
        match self {
            Self::Local(client) => client.capability_action_history(item_id, limit),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    /// Explicitly settle an uncertain outcome. Never re-executes.
    pub fn reconcile_capability_action(
        &mut self,
        command_id: &str,
        succeed: bool,
    ) -> Result<orbit_client::OrbitCapabilityReceipt, ClientError> {
        match self {
            Self::Local(client) => client.reconcile_capability_action(command_id, succeed),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    /// Durable effect count for the built-in reference capability. Local-only;
    /// used by the M30 acceptance tests to prove no note was appended.
    #[allow(dead_code)]
    pub fn reference_work_item_note_count(&self, item_id: &str) -> Result<usize, ClientError> {
        match self {
            Self::Local(client) => client.reference_work_item_note_count(item_id),
            Self::Connected { .. } => Err(unavailable(CONNECTED_CAPABILITY_REFUSED)),
        }
    }

    pub fn work_brief(
        &self,
        request: &orbit_runtime::planning_brief::WorkBriefRequest,
    ) -> Result<orbit_runtime::planning_brief::WorkBrief, ClientError> {
        match self {
            Self::Local(client) => client.work_brief(request),
            Self::Connected { connection, .. } => connection.work_brief(request).map_err(mapped),
        }
    }

    pub fn planning_focus(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::FocusRequest,
    ) -> Result<orbit_runtime::planning_focus::TaskFocus, ClientError> {
        match self {
            Self::Local(client) => client.planning_focus(brief, request),
            Self::Connected { connection, .. } => {
                connection.planning_focus(brief, request).map_err(mapped)
            }
        }
    }

    pub fn planning_group(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_focus::GroupFocusRequest,
    ) -> Result<orbit_runtime::planning_focus::GroupFocus, ClientError> {
        match self {
            Self::Local(client) => client.planning_group(brief, request),
            Self::Connected { connection, .. } => {
                connection.planning_group(brief, request).map_err(mapped)
            }
        }
    }

    pub fn planning_task_actions(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
    ) -> Result<orbit_runtime::planning_actions::PlanningActionMenu, ClientError> {
        match self {
            Self::Local(client) => client.planning_task_actions(brief, request),
            Self::Connected { connection, .. } => connection
                .planning_task_actions(brief, request)
                .map_err(mapped),
        }
    }

    pub fn planning_task_handoff(
        &self,
        brief: &orbit_runtime::planning_brief::WorkBrief,
        request: &orbit_runtime::planning_actions::TaskActionRequest,
        selection: &orbit_runtime::planning_actions::ActionSelection,
    ) -> Result<orbit_runtime::planning_handoff::PlanningHandoff, ClientError> {
        match self {
            Self::Local(client) => client.planning_task_handoff(brief, request, selection),
            Self::Connected { connection, .. } => connection
                .planning_task_handoff(brief, request, selection)
                .map_err(mapped),
        }
    }

    pub fn preview_task_dependency(
        &self,
        request: &orbit_runtime::task_dependency_preview::DependencyPreviewRequest,
    ) -> Result<orbit_runtime::task_dependency_preview::DependencyPreview, ClientError> {
        match self {
            Self::Local(client) => client.preview_task_dependency(request),
            Self::Connected { connection, .. } => {
                connection.preview_task_dependency(request).map_err(mapped)
            }
        }
    }

    pub fn schedule_plan(
        &self,
        request: &orbit_runtime::schedule_planning::SchedulePlanRequest,
    ) -> Result<orbit_runtime::schedule_planning::SchedulePlan, ClientError> {
        match self {
            Self::Local(client) => client.schedule_plan(request),
            Self::Connected { connection, .. } => connection.schedule_plan(request).map_err(mapped),
        }
    }

    pub fn completion_impact(
        &self,
        request: &orbit_runtime::task_impact::CompletionImpactRequest,
    ) -> Result<orbit_runtime::task_impact::CompletionImpact, ClientError> {
        match self {
            Self::Local(client) => client.completion_impact(request),
            Self::Connected { connection, .. } => {
                connection.preview_task_completion(request).map_err(mapped)
            }
        }
    }

    pub fn preview_task_plan(
        &self,
        _start_ms: i64,
        _end_ms: i64,
        _policy: orbit_client::PlanningPolicyDto,
    ) -> Result<TaskPlanProposalDto, ClientError> {
        match self {
            Self::Local(client) => client.preview_task_plan(_start_ms, _end_ms, _policy),
            Self::Connected { .. } => Err(unavailable(
                "task-plan preview requires the local authoritative planner",
            )),
        }
    }

    pub fn apply_task_plan(
        &mut self,
        _plan: TaskPlanProposalDto,
    ) -> Result<Vec<orbit_client::ScheduleBlockDto>, ClientError> {
        match self {
            Self::Local(client) => client.apply_task_plan(_plan),
            Self::Connected { .. } => Err(unavailable(
                "task-plan application requires the connected console (`:`, then Connected commands)",
            )),
        }
    }

    /// One task with its rich details.
    ///
    /// Local mode reads the authoritative projection; connected mode reads the
    /// authoritative task-detail route so the details editor sees description,
    /// priority and labels instead of the thin accepted snapshot.
    pub fn task_detail(&self, id: &str) -> Result<TaskDto, ClientError> {
        #[cfg(test)]
        diagnostics::record_detail_read();
        match self {
            Self::Local(client) => client.task_detail(id),
            Self::Connected { connection, .. } => {
                connection.task_detail(id).map_err(connected_error)
            }
        }
    }

    /// Rich task details (description, priority, labels).
    ///
    /// Connected mode queues the canonical `SetDetails` command and returns the
    /// currently accepted projection, so the caller must consult `take_outcome`
    /// before reporting success.
    pub fn update_task_details(
        &mut self,
        task_id: String,
        description: String,
        priority: Option<String>,
        labels: Vec<String>,
    ) -> Result<TaskDto, ClientError> {
        match self {
            Self::Local(client) => {
                client.update_task_details(task_id, description, priority, labels)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .update_task_details(task_id, description, priority, labels)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn acceptance_criteria(
        &self,
        task_id: &str,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        #[cfg(test)]
        diagnostics::record_detail_read();
        match self {
            Self::Local(client) => client.acceptance_criteria(task_id),
            Self::Connected { connection, .. } => connection
                .acceptance_criteria(task_id)
                .map_err(connected_error),
        }
    }

    pub fn add_acceptance_criterion(
        &mut self,
        task_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => client.add_acceptance_criterion(task_id, text),
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .add_acceptance_criterion(task_id, text)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn set_acceptance_criterion_text(
        &mut self,
        task_id: String,
        criterion_id: String,
        text: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => {
                client.set_acceptance_criterion_text(task_id, criterion_id, text)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .set_acceptance_criterion_text(task_id, criterion_id, text)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn set_acceptance_criterion_completed(
        &mut self,
        task_id: String,
        criterion_id: String,
        completed: bool,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => {
                client.set_acceptance_criterion_completed(task_id, criterion_id, completed)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .set_acceptance_criterion_completed(task_id, criterion_id, completed)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn move_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
        position: usize,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => {
                client.move_acceptance_criterion(task_id, criterion_id, position)
            }
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .move_acceptance_criterion(task_id, criterion_id, position)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn remove_acceptance_criterion(
        &mut self,
        task_id: String,
        criterion_id: String,
    ) -> Result<Vec<AcceptanceCriterionDto>, ClientError> {
        match self {
            Self::Local(client) => client.remove_acceptance_criterion(task_id, criterion_id),
            Self::Connected { connection, .. } => {
                let before = connection.last_queued_command_id();
                let result = connection
                    .remove_acceptance_criterion(task_id, criterion_id)
                    .map_err(connected_error);
                if result.is_ok() {
                    self.record_rich_outcome(before);
                }
                result
            }
        }
    }

    pub fn task_activity(&self, task_id: &str) -> Result<Vec<ActivityDto>, ClientError> {
        match self {
            Self::Local(client) => client.task_activity(task_id),
            Self::Connected { connection, .. } => {
                connection.task_activity(task_id).map_err(connected_error)
            }
        }
    }

    pub fn list_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<Vec<AttachmentDto>, ClientError> {
        #[cfg(test)]
        diagnostics::record_detail_read();
        match self {
            Self::Local(client) => client.list_attachments(owner_kind, owner_id),
            Self::Connected { connection, .. } => connection
                .list_attachments(owner_kind, owner_id)
                .map_err(connected_error),
        }
    }

    /// Unacknowledged uploads for one owner. Local mode has no staging queue,
    /// so it always returns an empty list; connected mode reads the replica.
    pub fn pending_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<Vec<PendingAttachmentDto>, ClientError> {
        match self {
            Self::Local(_) => Ok(Vec::new()),
            Self::Connected { connection, .. } => connection
                .staged_attachments(owner_kind, owner_id)
                .map_err(connected_error),
        }
    }

    /// Number of staged uploads the server has not accepted yet.
    pub fn pending_attachment_count(&self) -> Result<i64, ClientError> {
        match self {
            Self::Local(_) => Ok(0),
            Self::Connected { connection, .. } => connection
                .pending_attachment_upload_count()
                .map_err(connected_error),
        }
    }

    /// One staged upload by attachment id, across owners.
    ///
    /// Preview uses this to label bytes that still live in local staging as
    /// pending instead of implying they are accepted server content.
    pub fn pending_attachment(
        &self,
        attachment_id: &str,
    ) -> Result<Option<PendingAttachmentDto>, ClientError> {
        match self {
            Self::Local(_) => Ok(None),
            Self::Connected { connection, .. } => Ok(connection
                .pending_attachment_uploads()
                .map_err(connected_error)?
                .into_iter()
                .find(|pending| pending.attachment.id == attachment_id)),
        }
    }

    /// Durable state of one queued command, with its conflict reason when the
    /// outbox recorded a permanent rejection.
    #[allow(dead_code)]
    pub fn command_outcome(&self, command_id: &str) -> MutationOutcome {
        match self {
            Self::Local(_) => MutationOutcome::Accepted,
            Self::Connected { connection, .. } => command_outcome(connection, command_id),
        }
    }

    /// Identity of the most recent command this connected process queued.
    #[allow(dead_code)]
    pub fn last_queued_command_id(&self) -> Option<String> {
        match self {
            Self::Local(_) => None,
            Self::Connected { connection, .. } => connection.last_queued_command_id(),
        }
    }

    pub fn import_attachment(
        &mut self,
        owner_kind: String,
        owner_id: String,
        path: String,
        description: Option<String>,
    ) -> Result<AttachmentDto, ClientError> {
        match self {
            Self::Local(client) => {
                client.import_attachment(owner_kind, owner_id, path, description)
            }
            Self::Connected { connection, .. } => connection
                .import_attachment(owner_kind, owner_id, path, description)
                .map_err(connected_error),
        }
    }

    pub fn attachment_content(&self, id: &str) -> Result<(AttachmentDto, Vec<u8>), ClientError> {
        match self {
            Self::Local(client) => client.attachment_content(id),
            Self::Connected { connection, .. } => {
                connection.attachment_content(id).map_err(connected_error)
            }
        }
    }

    pub fn export_attachment(
        &self,
        id: &str,
        destination: String,
    ) -> Result<AttachmentDto, ClientError> {
        match self {
            Self::Local(client) => client.export_attachment(id, destination),
            Self::Connected { connection, .. } => connection
                .export_attachment(id, destination)
                .map_err(connected_error),
        }
    }

    pub fn remove_attachment(&mut self, id: String) -> Result<bool, ClientError> {
        match self {
            Self::Local(client) => client.remove_attachment(id),
            Self::Connected { connection, .. } => {
                connection.remove_attachment(id).map_err(connected_error)
            }
        }
    }

    pub fn purge_attachment_content(&mut self) -> Result<usize, ClientError> {
        match self {
            Self::Local(client) => client.purge_attachment_content(),
            Self::Connected { connection, .. } => connection
                .purge_attachment_content()
                .map_err(connected_error),
        }
    }

    /// Workflows available offline. The descriptors are compiled into the
    /// binary, so this works in both deployment modes.
    pub fn bundled_workflows(&self) -> Vec<WorkflowDescriptorDto> {
        match self {
            Self::Local(client) => client.bundled_workflows(),
            Self::Connected { .. } => orbit_blueprint::bundled::BUNDLED_BLUEPRINTS
                .iter()
                .map(|descriptor| WorkflowDescriptorDto {
                    id: descriptor.id.to_owned(),
                    name: descriptor.name.to_owned(),
                    description: descriptor.description.to_owned(),
                    version: descriptor.version,
                    bundled: true,
                    is_default: descriptor.id == "basic",
                })
                .collect(),
        }
    }

    pub fn apply_workflow(
        &mut self,
        project_id: String,
        workflow_id: String,
        version: Option<u32>,
    ) -> Result<ProjectDto, ClientError> {
        match self {
            Self::Local(client) => client.apply_workflow(project_id, workflow_id, version),
            Self::Connected { .. } => Err(unavailable(
                "changing a project workflow requires the local authoritative workspace",
            )),
        }
    }

    pub fn project_workflow(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectWorkflowDto>, ClientError> {
        match self {
            Self::Local(client) => client.project_workflow(project_id),
            Self::Connected { .. } => Ok(None),
        }
    }

    /// Directory used for explicit attachment exports and viewer handoff.
    ///
    /// Connected mode has no server filesystem access, so exports use a
    /// process-scoped private temp directory. It is created with 0700 on Unix
    /// and never reused across processes.
    pub fn attachment_export_dir(&self) -> Result<std::path::PathBuf, ClientError> {
        match self {
            Self::Local(client) => Ok(client.attachment_store_root().join("exports")),
            Self::Connected { .. } => connected_export_dir(),
        }
    }

    /// Explicitly unpin a project's workflow (falls back to generated views).
    #[allow(dead_code)]
    pub fn remove_project_workflow(&mut self, project_id: String) -> Result<bool, ClientError> {
        match self {
            Self::Local(client) => client.remove_project_workflow(project_id),
            Self::Connected { .. } => Err(unavailable(
                "changing a project workflow requires the local authoritative workspace",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_state_mapping_never_promotes_unknown_state_to_success() {
        assert_eq!(
            outcome_from_state(Some("cmd".into()), Some("accepted"), None),
            MutationOutcome::Accepted
        );
        assert_eq!(
            outcome_from_state(Some("cmd".into()), Some("pending"), None),
            MutationOutcome::Pending {
                command_id: Some("cmd".into()),
                state: "pending".into()
            }
        );
        assert!(matches!(
            outcome_from_state(
                Some("cmd".into()),
                Some("conflict"),
                Some("stale revision".into())
            ),
            MutationOutcome::Conflict { ref reason, .. } if reason == "stale revision"
        ));
        // Missing state and unexpected states fail closed as pending.
        assert_eq!(
            outcome_from_state(Some("cmd".into()), None, None),
            MutationOutcome::Pending {
                command_id: Some("cmd".into()),
                state: "unavailable".into()
            }
        );
        assert_eq!(
            outcome_from_state(Some("cmd".into()), Some("superseded"), None),
            MutationOutcome::Pending {
                command_id: Some("cmd".into()),
                state: "superseded".into()
            }
        );
    }

    #[test]
    fn connected_export_directory_is_process_scoped_and_private() {
        let first = super::connected_export_dir().unwrap();
        let second = super::connected_export_dir().unwrap();
        assert_eq!(first, second, "one directory per process");
        assert!(first.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&first).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "export directory must be private");
        }
    }
}
