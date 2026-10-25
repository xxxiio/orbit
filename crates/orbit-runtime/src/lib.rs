//! Host-side Orbit application runtime.
//!
//! A runtime is hosted alongside its authoritative SQLite file. Deployment
//! topology (embedded GUI process, local daemon, self-hosted server) does not
//! change how it opens its database. Transports and authentication belong to
//! adapters outside this crate.
//!
//! Embedded and HTTP connections use the same rich application entity tables.

mod accepted_application;
mod command_engine;
pub(crate) mod default_workflow;

#[cfg(test)]
#[path = "attachment_authority_tests.rs"]
mod attachment_authority_tests;

#[cfg(test)]
mod blueprint_upgrade_tests;

use std::path::Path;

/// Host-owned authoritative SQLite command and projection implementation.
/// Both REST and MCP invoke these same atomic domain command methods.
pub mod agent_authority;
pub mod application_services;
pub mod attachment_store;
pub mod authority_milestones;
pub mod authority_routines;
pub mod authority_schedule;
pub mod authority_strategy;
pub mod blueprint_authority;
pub mod blueprint_execution;
pub mod blueprint_upgrade;
pub mod canonical;
pub mod capabilities;
pub mod capability_composition;
pub mod daily_planning;
pub mod domain_commands;
pub mod planning_actions;
pub mod planning_approval;
pub mod planning_brief;
pub mod planning_focus;
pub mod planning_handoff;
pub mod planning_scenarios;
pub mod portfolio_actions;
pub mod portfolio_capacity;
pub mod portfolio_schedule;
pub mod project_dependency_planning;
pub mod project_handoffs;
pub mod scenario_execution;
pub mod schedule_planning;
mod store;
pub mod task_dependencies;
pub mod task_dependency_preview;
pub mod task_graph;
pub mod task_impact;
pub mod task_next;
pub mod workload_insights;
pub use store::{AuthorityError, AuthorityEvent};
pub mod authority_evidence;
pub mod folder_commands;
pub mod habit_capability;
pub mod knowledge_files;
pub mod knowledge_operations;
pub mod knowledge_service;
pub mod markdown_commands;

/// Storage failures while resolving an attachment owner stay typed application
/// errors so transports can answer without exposing SQL details.
fn attachment_lookup_error(error: rusqlite::Error) -> orbit_application::ApplicationError {
    orbit_application::ApplicationError::Repository(orbit_application::RepositoryError::Other(
        format!("attachment owner lookup: {error}"),
    ))
}

/// Revision, fingerprint, canonical projects/tasks and evidence from one
/// consistent authority snapshot.
pub type AuthoritySnapshot = (
    i64,
    String,
    Vec<domain_commands::ProjectRecord>,
    Vec<domain_commands::TaskRecord>,
    authority_evidence::EvidenceSnapshot,
);

/// Host-side authoritative runtime used by HTTP and MCP adapters.
///
/// This type owns the canonical SQLite connection. Clients and transports do
/// not construct repositories or perform authoritative SQL themselves.
/// Unlike a client-side replica, every mutation executes against this host.
///
/// The host owns both the complete application repository and the canonical
/// command ledger. Projection unification remains an explicit acceptance gate.
pub struct AuthoritativeHost {
    authority: store::AuthorityStore,
    knowledge_vault: knowledge_operations::KnowledgeOperations,
    attachment_store: attachment_store::AttachmentObjectStore,
}

impl AuthoritativeHost {
    /// Validate a backup without exposing the mutable authority repository.
    pub fn validate_backup(path: &Path) -> Result<(), AuthorityError> {
        store::AuthorityStore::validate_backup(path)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, store::AuthorityError> {
        let path = path.as_ref();
        let vault_root = if path == Path::new(":memory:") {
            std::env::temp_dir().join(format!(
                "orbit-ephemeral-knowledge-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| store::AuthorityError::Invalid("clock"))?
                    .as_nanos()
            ))
        } else {
            path.with_extension("knowledge")
        };
        let attachment_root = if path == Path::new(":memory:") {
            std::env::temp_dir().join(format!(
                "orbit-ephemeral-attachments-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| store::AuthorityError::Invalid("clock"))?
                    .as_nanos()
            ))
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join("attachments")
                .join("objects")
        };
        let max_bytes = std::env::var("ORBIT_ATTACHMENT_MAX_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(orbit_domain::DEFAULT_ATTACHMENT_MAX_BYTES);
        let attachment_store =
            attachment_store::AttachmentObjectStore::open(&attachment_root, max_bytes)
                .map_err(|_| store::AuthorityError::Invalid("attachment store unavailable"))?;
        let knowledge_vault = knowledge_operations::KnowledgeOperations::open(&vault_root)
            .map_err(|_| store::AuthorityError::Invalid("knowledge vault unavailable"))?;
        // One SqliteStore instance owns both application entities and the
        // revision/command ledger. In-process and HTTP adapters share it.
        let authority = store::AuthorityStore::open(path)?;
        // Keep the complete existing application schema available to the host.
        // Ownership belongs to the same process-local authoritative service;
        // legacy client adapters may not open a second independent store.
        markdown_commands::schema(&authority)?;
        folder_commands::schema(&authority)?;
        folder_commands::create_schema(&authority)?;
        let mut host = Self {
            authority,
            knowledge_vault,
            attachment_store,
        };
        host.recover_folder_creates()?;
        host.recover_markdown_commands()?;
        host.recover_folder_commands()?;
        Ok(host)
    }

    /// Stable schema diagnostic without leaking the underlying repository to clients.
    pub fn application_schema_version(&self) -> Result<i64, orbit_application::RepositoryError> {
        self.authority.conn.schema_version()
    }

    /// Read-only application queries share the host's canonical SQLite instance.
    /// Transport adapters must not reconstruct repository ownership.
    pub fn application_queries(&self) -> application_services::ApplicationQueries<'_> {
        application_services::ApplicationQueries::new(&self.authority.conn)
    }

    /// Shared application services used by embedded and server-side adapters.
    /// Split field borrows let attachment use cases access the object store and
    /// the SQLite metadata repository in the same call.
    pub(crate) fn application_services(&mut self) -> application_services::ApplicationServices<'_> {
        application_services::ApplicationServices::new(
            &mut self.authority.conn,
            &mut self.attachment_store,
        )
    }

    /// Root directory of the managed, content-addressed attachment store.
    pub fn attachment_store_root(&self) -> &Path {
        self.attachment_store.root()
    }

    /// Attachment metadata associations for one owner in stable creation order.
    pub fn list_attachments(
        &self,
        owner: &orbit_domain::AttachmentOwner,
    ) -> Result<Vec<orbit_domain::Attachment>, orbit_application::ApplicationError> {
        orbit_application::AttachmentRepository::list_attachments(&self.authority.conn, owner)
            .map_err(orbit_application::ApplicationError::from)
    }

    /// Read one attachment's content after SHA-256 integrity verification.
    pub fn attachment_content(
        &self,
        id: &orbit_domain::AttachmentId,
    ) -> Result<(orbit_domain::Attachment, Vec<u8>), orbit_application::ApplicationError> {
        orbit_application::read_attachment_content(&self.authority.conn, &self.attachment_store, id)
    }

    /// Whether an attachment's owning entity exists in canonical storage.
    ///
    /// Attachment rows deliberately carry no foreign key because owners span
    /// several schemas (tasks, projects, blueprint work items, Markdown notes
    /// and agent jobs). Transports therefore validate the target here before
    /// accepting new metadata. Table names come from a closed match and the
    /// identifier is always a bound parameter.
    pub fn attachment_owner_exists(
        &self,
        owner: &orbit_domain::AttachmentOwner,
    ) -> Result<bool, orbit_application::ApplicationError> {
        use orbit_domain::AttachmentOwnerKind;
        let table = match owner.kind() {
            AttachmentOwnerKind::Task => "tasks",
            AttachmentOwnerKind::Project => "projects",
            AttachmentOwnerKind::WorkItem => "orbit_blueprint_work_items",
            AttachmentOwnerKind::Knowledge => "markdown_note_index",
            AttachmentOwnerKind::AgentJob => "agent_jobs",
        };
        let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)");
        self.authority
            .conn
            .query_row(&sql, [owner.id()], |row| row.get::<_, bool>(0))
            .map_err(attachment_lookup_error)
    }

    /// The project an attachment owner belongs to, when it has one.
    ///
    /// Projects resolve to themselves; tasks/work items/agent jobs use their
    /// stored `project_id`; Markdown knowledge has no project scope yet.
    /// Missing owners return `None` rather than an error.
    pub fn attachment_owner_project(
        &self,
        owner: &orbit_domain::AttachmentOwner,
    ) -> Result<Option<orbit_domain::ProjectId>, orbit_application::ApplicationError> {
        use orbit_domain::AttachmentOwnerKind;
        use rusqlite::OptionalExtension;
        let conn = &self.authority.conn;
        let project_id: Option<String> = match owner.kind() {
            AttachmentOwnerKind::Project => {
                let exists: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?1)",
                        [owner.id()],
                        |row| row.get(0),
                    )
                    .map_err(attachment_lookup_error)?;
                exists.then(|| owner.id().to_owned())
            }
            AttachmentOwnerKind::Task => conn
                .query_row(
                    "SELECT project_id FROM tasks WHERE id = ?1",
                    [owner.id()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(attachment_lookup_error)?
                .flatten(),
            AttachmentOwnerKind::WorkItem => conn
                .query_row(
                    "SELECT project_id FROM orbit_blueprint_work_items WHERE id = ?1",
                    [owner.id()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(attachment_lookup_error)?
                .flatten(),
            AttachmentOwnerKind::AgentJob => conn
                .query_row(
                    "SELECT project_id FROM agent_jobs WHERE id = ?1",
                    [owner.id()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(attachment_lookup_error)?
                .flatten(),
            AttachmentOwnerKind::Knowledge => None,
        };
        project_id
            .map(|id| {
                orbit_domain::ProjectId::new(id).map_err(orbit_application::ApplicationError::from)
            })
            .transpose()
    }

    /// Ordered acceptance criteria for one task.
    pub fn acceptance_criteria(
        &self,
        task_id: &orbit_domain::TaskId,
    ) -> Result<orbit_domain::AcceptanceCriteria, orbit_application::ApplicationError> {
        orbit_application::list_acceptance_criteria(&self.authority.conn, task_id)
    }

    /// Remove content objects no attachment references any more.
    pub fn purge_attachment_content(
        &mut self,
    ) -> Result<usize, orbit_application::ApplicationError> {
        orbit_application::purge_unreferenced_attachment_content(
            &mut self.authority.conn,
            &mut self.attachment_store,
        )
    }

    /// Execute a batch of rich application use cases within one authority-owned
    /// SQL transaction. Individual application use cases use nested SQLite
    /// savepoints; they do not commit until this enclosing transaction commits.
    /// This is the transaction seam for future command-receipt integration.
    /// A failed step rolls back the *entire* batch, including activity records.
    #[cfg(test)]
    pub(crate) fn transact_application<T>(
        &mut self,
        operation: impl FnOnce(
            &mut application_services::ApplicationServices<'_>,
        ) -> Result<T, orbit_application::ApplicationError>,
    ) -> Result<T, orbit_application::ApplicationError> {
        use orbit_application::{ApplicationError, RepositoryError};
        fn storage_error(error: rusqlite::Error) -> ApplicationError {
            ApplicationError::Repository(RepositoryError::Other(format!(
                "authority application transaction: {error}"
            )))
        }
        self.authority
            .conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(storage_error)?;
        let outcome = operation(&mut self.application_services());
        match outcome {
            Ok(value) => {
                if let Err(error) = self.authority.conn.execute_batch("COMMIT") {
                    let _ = self.authority.conn.execute_batch("ROLLBACK");
                    return Err(storage_error(error));
                }
                Ok(value)
            }
            Err(error) => {
                self.authority
                    .conn
                    .execute_batch("ROLLBACK")
                    .map_err(storage_error)?;
                Err(error)
            }
        }
    }

    /// Admit a rich application operation through the same immutable receipt,
    /// revision, semantic history, and replay engine as canonical commands.
    /// Domain adapters supply the validated semantic change that represents
    /// their corresponding application mutation. Both the rich use case and
    /// command publication execute inside a single BEGIN IMMEDIATE boundary.
    ///
    /// Note: existing plain ApplicationServices methods remain available for
    /// local-only callers; transports must adopt this envelope to obtain
    /// canonical idempotency/revision semantics.
    pub(crate) fn execute_application_command<T>(
        &mut self,
        actor: &str,
        command_id: &str,
        expected_revision: i64,
        payload: &str,
        semantic_change: orbit_sync::SemanticChange,
        apply: impl FnOnce(
            &mut application_services::ApplicationServices<'_>,
        ) -> Result<T, orbit_application::ApplicationError>,
    ) -> Result<(i64, bool, T), store::AuthorityError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        use crate::command_engine::{CommandAdmission, PendingCommand};
        use crate::store::AuthorityError;
        self.authority.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let pending = PendingCommand {
                command_id,
                actor,
                payload,
                expected_revision,
            };
            match pending.check(&self.authority.conn)? {
                CommandAdmission::Duplicate(saved) => {
                    let (revision, result): (i64, T) =
                        serde_json::from_str(&saved).map_err(|_| {
                            AuthorityError::Invalid("invalid stored rich command receipt")
                        })?;
                    Ok((revision, true, result))
                }
                CommandAdmission::New(revision) => {
                    let result = apply(&mut self.application_services())
                        .map_err(AuthorityError::Application)?;
                    let receipt = serde_json::to_string(&(revision, &result)).map_err(|_| {
                        AuthorityError::Invalid("rich receipt serialization failed")
                    })?;
                    pending.commit(&self.authority.conn, revision, semantic_change, &receipt)?;
                    Ok((revision, false, result))
                }
            }
        })();
        match result {
            Ok(value) => {
                if let Err(error) = self.authority.conn.execute_batch("COMMIT") {
                    let _ = self.authority.conn.execute_batch("ROLLBACK");
                    return Err(store::AuthorityError::Sql(error));
                }
                Ok(value)
            }
            Err(error) => {
                let _ = self.authority.conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// Replay durable empty-folder creation before note publication: a pending
    /// note may reference a folder that was created immediately before a crash.
    pub fn recover_folder_creates(&mut self) -> Result<(), store::AuthorityError> {
        for (id, command) in folder_commands::pending_creates(&self.authority)? {
            folder_commands::publish_create(&self.knowledge_vault, &command)?;
            folder_commands::accept_create(&mut self.authority, &id, &command)?;
        }
        Ok(())
    }

    pub fn execute_folder_create(
        &mut self,
        actor: &str,
        id: &str,
        command: folder_commands::FolderCreate,
    ) -> Result<folder_commands::FolderCreateReceipt, store::AuthorityError> {
        self.recover_folder_creates()?;
        self.recover_markdown_commands()?;
        self.recover_folder_commands()?;
        if let Some(receipt) =
            folder_commands::enqueue_create(&mut self.authority, actor, id, &command)?
        {
            return Ok(receipt);
        }
        folder_commands::publish_create(&self.knowledge_vault, &command)?;
        folder_commands::accept_create(&mut self.authority, id, &command)
    }

    /// Replay interrupted folder renames before allowing further commands.
    pub fn recover_folder_commands(&mut self) -> Result<(), store::AuthorityError> {
        for (id, mv) in folder_commands::pending(&self.authority)? {
            folder_commands::publish(&self.knowledge_vault, &mv)?;
            let notes = self.markdown_knowledge_index().map_err(|_| {
                store::AuthorityError::Invalid("folder recovery identity collision")
            })?;
            folder_commands::accept(&mut self.authority, &id, &mv, &notes)?;
        }
        Ok(())
    }

    /// Revisioned, idempotent folder rename with durable restart replay.
    pub fn execute_folder_move(
        &mut self,
        actor: &str,
        id: &str,
        mv: folder_commands::FolderMove,
    ) -> Result<folder_commands::FolderReceipt, store::AuthorityError> {
        self.recover_folder_creates()?;
        self.recover_markdown_commands()?;
        self.recover_folder_commands()?;
        if let Some(receipt) = folder_commands::enqueue(&mut self.authority, actor, id, &mv)? {
            return Ok(receipt);
        }
        folder_commands::publish(&self.knowledge_vault, &mv)?;
        let notes = self
            .markdown_knowledge_index()
            .map_err(|_| store::AuthorityError::Invalid("folder move identity collision"))?;
        folder_commands::accept(&mut self.authority, id, &mv, &notes)
    }

    /// Apply durable SQLite intents before accepting new knowledge commands.
    /// A failed replay prevents the host from serving a potentially partial state.
    pub fn recover_markdown_commands(&mut self) -> Result<(), store::AuthorityError> {
        for (command_id, command) in markdown_commands::pending(&self.authority)? {
            let service = knowledge_service::KnowledgeService::new(&self.knowledge_vault);
            markdown_commands::publish(&service, &command_id, &command)?;
            let notes = service.discover().map_err(|_| {
                store::AuthorityError::Invalid("markdown discovery failed during recovery")
            })?;
            let (_, published) = service
                .read_by_id(match &command {
                    markdown_commands::MarkdownCommand::Create { note_id, .. }
                    | markdown_commands::MarkdownCommand::Update { note_id, .. }
                    | markdown_commands::MarkdownCommand::Move { note_id, .. } => note_id,
                })
                .map_err(|_| store::AuthorityError::ImmutableConflict)?
                .ok_or(store::AuthorityError::ImmutableConflict)?;
            markdown_commands::accept(
                &mut self.authority,
                &command_id,
                &command,
                &notes,
                &published,
            )?;
        }
        Ok(())
    }

    /// New file-authoritative command lifecycle. A returned receipt means both
    /// the Markdown publication and SQLite acceptance completed durably.
    pub fn execute_markdown_command(
        &mut self,
        actor: &str,
        command_id: &str,
        command: markdown_commands::MarkdownCommand,
    ) -> Result<markdown_commands::MarkdownReceipt, store::AuthorityError> {
        self.recover_folder_creates()?;
        self.recover_markdown_commands()?;
        self.recover_folder_commands()?;
        if let Some(receipt) =
            markdown_commands::enqueue(&mut self.authority, actor, command_id, &command)?
        {
            return Ok(receipt);
        }
        let service = knowledge_service::KnowledgeService::new(&self.knowledge_vault);
        if let Err(error) = markdown_commands::publish(&service, command_id, &command) {
            // The caller is being told the command failed, so its note
            // reservation must not replay that rejected command on the next
            // open. Failures after publication keep the durable pending row so
            // they can be reconciled forward.
            markdown_commands::abandon_pending(&mut self.authority, command_id)?;
            return Err(error);
        }
        let notes = service
            .discover()
            .map_err(|_| store::AuthorityError::Invalid("markdown discovery failed"))?;
        {
            let (_, published) = service
                .read_by_id(match &command {
                    markdown_commands::MarkdownCommand::Create { note_id, .. }
                    | markdown_commands::MarkdownCommand::Update { note_id, .. }
                    | markdown_commands::MarkdownCommand::Move { note_id, .. } => note_id,
                })
                .map_err(|_| store::AuthorityError::ImmutableConflict)?
                .ok_or(store::AuthorityError::ImmutableConflict)?;
            markdown_commands::accept(
                &mut self.authority,
                command_id,
                &command,
                &notes,
                &published,
            )
        }
    }

    /// File-backed knowledge identity discovery. Missing and duplicate IDs
    /// fail closed rather than allowing a stale path index to decide identity.
    pub fn markdown_knowledge_index(
        &self,
    ) -> Result<
        Vec<knowledge_service::KnowledgeDocumentInfo>,
        knowledge_service::KnowledgeServiceError,
    > {
        knowledge_service::KnowledgeService::new(&self.knowledge_vault).discover()
    }

    /// Reconcile the disposable SQLite note index from physical Markdown files.
    /// Discovery completes before any SQL mutation, so duplicate/missing IDs
    /// cannot partially replace an otherwise valid index.
    pub fn reconcile_markdown_knowledge_index(
        &mut self,
    ) -> Result<Vec<knowledge_service::KnowledgeDocumentInfo>, store::AuthorityError> {
        let notes = self
            .markdown_knowledge_index()
            .map_err(|_| store::AuthorityError::Invalid("markdown discovery failed"))?;
        self.authority.reconcile_markdown_note_index(&notes)?;
        Ok(notes)
    }

    /// File-backed draft workflow. This is deliberately distinct from
    /// `knowledge_command`: it does NOT acknowledge canonical acceptance.
    /// On index failure the file is retained and can be reconciled on restart.
    pub fn stage_markdown_create(
        &mut self,
        operation_id: &str,
        note_id: &str,
        relative_path: &str,
        body: &str,
    ) -> Result<Vec<knowledge_service::KnowledgeDocumentInfo>, store::AuthorityError> {
        knowledge_service::KnowledgeService::new(&self.knowledge_vault)
            .stage_create(operation_id, note_id, relative_path, body)
            .map_err(|_| store::AuthorityError::Invalid("markdown creation failed"))?;
        self.reconcile_markdown_knowledge_index()
    }

    /// Update file bytes using an expected prior version; then rebuild the
    /// disposable path projection. No authoritative SQL receipt is emitted.
    pub fn stage_markdown_update(
        &mut self,
        operation_id: &str,
        note_id: &str,
        expected: &str,
        replacement: &str,
    ) -> Result<Vec<knowledge_service::KnowledgeDocumentInfo>, store::AuthorityError> {
        knowledge_service::KnowledgeService::new(&self.knowledge_vault)
            .stage_update_by_id(operation_id, note_id, expected, replacement)
            .map_err(|_| store::AuthorityError::Invalid("markdown update failed"))?;
        self.reconcile_markdown_knowledge_index()
    }

    /// Move by identity, preserving physical folder organization in the vault.
    /// Until a durable move intent exists this is not a canonical command.
    pub fn stage_markdown_move(
        &mut self,
        note_id: &str,
        destination: &str,
    ) -> Result<Vec<knowledge_service::KnowledgeDocumentInfo>, store::AuthorityError> {
        knowledge_service::KnowledgeService::new(&self.knowledge_vault)
            .move_by_id(note_id, destination)
            .map_err(|_| store::AuthorityError::Invalid("markdown move failed"))?;
        self.reconcile_markdown_knowledge_index()
    }

    /// Inspect the last reconciled SQLite projection (not authoritative content).
    pub fn indexed_markdown_knowledge(
        &self,
    ) -> Result<Vec<knowledge_service::KnowledgeDocumentInfo>, store::AuthorityError> {
        self.authority.markdown_note_index()
    }

    /// Stable-ID lookup from authoritative Markdown bytes, independent of folder moves.
    pub fn markdown_by_id(
        &self,
        note_id: &str,
    ) -> Result<
        Option<(knowledge_service::KnowledgeDocumentInfo, String)>,
        knowledge_service::KnowledgeServiceError,
    > {
        knowledge_service::KnowledgeService::new(&self.knowledge_vault).read_by_id(note_id)
    }

    /// File-backed search; no legacy SQL content table is consulted.
    pub fn markdown_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<
        Vec<knowledge_service::KnowledgeDocumentInfo>,
        knowledge_service::KnowledgeServiceError,
    > {
        knowledge_service::KnowledgeService::new(&self.knowledge_vault).search(query, limit)
    }

    /// Term references are resolved from physical Markdown, never from the
    /// retired SQL knowledge index. Results are navigation-friendly occurrences.
    pub fn markdown_term_locations(
        &self,
        term: &str,
        limit: usize,
    ) -> Result<
        Vec<orbit_application::KnowledgeTermOccurrence>,
        knowledge_service::KnowledgeServiceError,
    > {
        let needle = term.trim().to_lowercase();
        if needle.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut occurrences = Vec::new();
        // Search the whole vault, not just a truncated shortlist: the limit
        // applies to occurrences, not the number of candidate documents.
        for document in
            knowledge_service::KnowledgeService::new(&self.knowledge_vault).discover()?
        {
            let Some((_, markdown)) = self.markdown_by_id(&document.id)? else {
                continue;
            };
            let Ok(item_id) = orbit_domain::KnowledgeItemId::new(document.id.clone()) else {
                continue;
            };
            for (line_index, line) in markdown.lines().enumerate() {
                let folded = line.to_lowercase();
                for (byte_offset, _) in folded.match_indices(&needle) {
                    // Match Unicode-scalar positions, not UTF-8 byte offsets.
                    let column = folded[..byte_offset].chars().count() + 1;
                    occurrences.push(orbit_application::KnowledgeTermOccurrence {
                        item_id: item_id.clone(),
                        field: "body",
                        line: line_index + 1,
                        column,
                        preview: line.chars().take(160).collect(),
                    });
                    if occurrences.len() == limit {
                        return Ok(occurrences);
                    }
                }
            }
        }
        Ok(occurrences)
    }

    /// File-backed links and backlinks keyed by stable IDs, not paths.
    pub fn markdown_relations(
        &self,
        note_id: &str,
    ) -> Result<
        Option<knowledge_service::KnowledgeFileRelations>,
        knowledge_service::KnowledgeServiceError,
    > {
        knowledge_service::KnowledgeService::new(&self.knowledge_vault).relations(note_id)
    }

    /// Folder hierarchy is read from the actual Markdown directory tree.
    pub fn knowledge_tree(
        &self,
    ) -> Result<knowledge_files::KnowledgeNode, knowledge_files::VaultError> {
        self.knowledge_vault.vault().tree()
    }

    /// Read the physical Markdown document selected in the folder tree.
    /// This is filesystem-backed content, not the transitional SQL body.
    pub fn markdown_document(
        &self,
        relative_path: &str,
    ) -> Result<String, knowledge_files::VaultError> {
        self.knowledge_vault.vault().read_markdown(relative_path)
    }

    pub fn projects(
        &self,
    ) -> Result<(i64, Vec<domain_commands::ProjectRecord>), store::AuthorityError> {
        self.authority.projects()
    }

    /// Accepted-state project workload and dependency diagnostics.
    pub fn workload_insights(
        &self,
        now_ms: i64,
        horizon_ms: i64,
    ) -> Result<workload_insights::WorkloadInsights, store::AuthorityError> {
        let readiness = self.task_readiness(now_ms)?;
        let (revision, tasks) = self.tasks()?;
        if revision != readiness.revision {
            return Err(store::AuthorityError::Invalid(
                "task snapshot changed during workload analysis",
            ));
        }
        workload_insights::analyze_workload(revision, &tasks, &readiness.edges, now_ms, horizon_ms)
            .map_err(store::AuthorityError::Invalid)
    }

    /// Daily capacity allocation over the accepted task ledger.
    pub fn day_plan(
        &self,
        request: &daily_planning::DayPlanRequest,
    ) -> Result<daily_planning::DayPlan, store::AuthorityError> {
        let readiness = self.task_readiness(request.now_ms)?;
        let (task_revision, tasks) = self.tasks()?;
        if readiness.revision != task_revision {
            return Err(store::AuthorityError::Invalid(
                "task snapshot changed during planning",
            ));
        }
        daily_planning::plan_day(readiness.revision, &tasks, &readiness.edges, request)
            .map_err(store::AuthorityError::Invalid)
    }

    /// Propose non-overlapping task slots over accepted schedule intervals.
    pub fn schedule_plan(
        &self,
        request: &schedule_planning::SchedulePlanRequest,
    ) -> Result<schedule_planning::SchedulePlan, store::AuthorityError> {
        let readiness = self.task_readiness(request.starts_at_ms)?;
        let (task_revision, tasks) = self.tasks()?;
        let schedule = self.schedule()?;
        if readiness.revision != task_revision || schedule.revision != task_revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during schedule planning",
            ));
        }
        schedule_planning::plan_schedule(
            task_revision,
            schedule.revision,
            &tasks,
            &readiness.edges,
            &schedule.blocks,
            request,
        )
        .map_err(store::AuthorityError::Invalid)
    }

    /// One accepted-revision Today/Next view, shared with offline replicas.
    /// Hypothetical dependency impact, without submitting an authoritative change.
    pub fn preview_task_completion(
        &self,
        request: &task_impact::CompletionImpactRequest,
    ) -> Result<task_impact::CompletionImpact, store::AuthorityError> {
        let readiness = self.task_readiness(request.now_ms)?;
        let (revision, tasks) = self.tasks()?;
        if revision != readiness.revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during impact preview",
            ));
        }
        task_impact::preview_completion(revision, &tasks, &readiness.edges, request)
            .map_err(store::AuthorityError::Invalid)
    }

    pub fn work_brief(
        &self,
        request: &planning_brief::WorkBriefRequest,
    ) -> Result<planning_brief::WorkBrief, store::AuthorityError> {
        let readiness = self.task_readiness(request.now_ms)?;
        let (revision, tasks) = self.tasks()?;
        let schedule = self.schedule()?;
        if revision != readiness.revision || revision != schedule.revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during work brief",
            ));
        }
        planning_brief::build_work_brief(
            revision,
            &tasks,
            &readiness.edges,
            &schedule.blocks,
            request,
        )
        .map_err(store::AuthorityError::Invalid)
    }

    /// Task drilldown derived from the exact same accepted Today/Next snapshot.
    pub fn planning_focus(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_focus::FocusRequest,
    ) -> Result<planning_focus::TaskFocus, store::AuthorityError> {
        // The caller may supply a serialized brief; do not assume it reflects
        // the current host revision when constructing navigation links.
        let current = self.tasks()?.0;
        if current != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning brief revision is stale",
            ));
        }
        planning_focus::focus_task(brief, request).map_err(store::AuthorityError::Invalid)
    }

    /// Accepted project and milestone drilldown with scope-limited task links.
    pub fn planning_group(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_focus::GroupFocusRequest,
    ) -> Result<planning_focus::GroupFocus, store::AuthorityError> {
        if self.tasks()?.0 != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning brief revision is stale",
            ));
        }
        planning_focus::focus_group(brief, request).map_err(store::AuthorityError::Invalid)
    }

    /// Discover safe next steps from a revision-checked accepted task drilldown.
    pub fn planning_task_actions(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_actions::TaskActionRequest,
    ) -> Result<planning_actions::PlanningActionMenu, store::AuthorityError> {
        if self.tasks()?.0 != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning brief revision is stale",
            ));
        }
        planning_actions::task_actions(brief, request).map_err(store::AuthorityError::Invalid)
    }

    pub fn planning_group_actions(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_actions::GroupActionRequest,
    ) -> Result<planning_actions::PlanningActionMenu, store::AuthorityError> {
        if self.tasks()?.0 != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning brief revision is stale",
            ));
        }
        planning_actions::group_actions(brief, request).map_err(store::AuthorityError::Invalid)
    }

    pub fn resolve_planning_task_action(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_actions::TaskActionRequest,
        selection: &planning_actions::ActionSelection,
    ) -> Result<planning_actions::ResolvedAction, store::AuthorityError> {
        if self.tasks()?.0 != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning brief revision is stale",
            ));
        }
        planning_actions::resolve_task_action(brief, request, selection)
            .map_err(store::AuthorityError::Invalid)
    }
    pub fn resolve_planning_group_action(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_actions::GroupActionRequest,
        selection: &planning_actions::ActionSelection,
    ) -> Result<planning_actions::ResolvedAction, store::AuthorityError> {
        if self.tasks()?.0 != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning brief revision is stale",
            ));
        }
        planning_actions::resolve_group_action(brief, request, selection)
            .map_err(store::AuthorityError::Invalid)
    }

    /// Resolve a reviewed action and materialize a read-only destination from
    /// the same authoritative accepted task revision. This never writes.
    pub fn planning_task_handoff(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_actions::TaskActionRequest,
        selection: &planning_actions::ActionSelection,
    ) -> Result<planning_handoff::PlanningHandoff, store::AuthorityError> {
        if self.tasks()?.0 != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning handoff revision is stale",
            ));
        }
        planning_handoff::dispatch_task(brief, request, selection)
            .map_err(store::AuthorityError::Invalid)
    }
    pub fn planning_group_handoff(
        &self,
        brief: &planning_brief::WorkBrief,
        request: &planning_actions::GroupActionRequest,
        selection: &planning_actions::ActionSelection,
    ) -> Result<planning_handoff::PlanningHandoff, store::AuthorityError> {
        if self.tasks()?.0 != brief.revision {
            return Err(store::AuthorityError::Invalid(
                "planning handoff revision is stale",
            ));
        }
        planning_handoff::dispatch_group(brief, request, selection)
            .map_err(store::AuthorityError::Invalid)
    }

    pub fn next_actions(
        &self,
        now_ms: i64,
        limit: usize,
    ) -> Result<task_next::NextActionsReport, store::AuthorityError> {
        let readiness = self.task_readiness(now_ms)?;
        let (_, tasks) = self.tasks()?;
        task_next::next_actions(readiness.revision, &tasks, &readiness.edges, now_ms, limit)
            .map_err(store::AuthorityError::Invalid)
    }

    pub fn task_graph(
        &self,
        now_ms: i64,
    ) -> Result<task_graph::TaskGraphReport, store::AuthorityError> {
        let readiness = self.task_readiness(now_ms)?;
        let (_, tasks) = self.tasks()?;
        task_graph::analyze_task_graph(readiness.revision, &tasks, &readiness.edges, now_ms)
            .map_err(store::AuthorityError::Invalid)
    }

    /// Accepted-only project dependency diagnostics, with an optional scoped
    /// projection that suppresses foreign task details.
    pub fn project_dependency_report(
        &self,
        request: &project_dependency_planning::ProjectDependencyRequest,
    ) -> Result<project_dependency_planning::ProjectDependencyReport, store::AuthorityError> {
        let readiness = self.task_readiness(request.now_ms)?;
        let (revision, tasks) = self.tasks()?;
        if revision != readiness.revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during project analysis",
            ));
        }
        project_dependency_planning::project_dependency_report(
            revision,
            &tasks,
            &readiness.edges,
            request,
        )
        .map_err(store::AuthorityError::Invalid)
    }

    pub fn project_handoffs(
        &self,
        request: &project_handoffs::HandoffRequest,
    ) -> Result<project_handoffs::ProjectHandoffReport, store::AuthorityError> {
        let readiness = self.task_readiness(request.now_ms)?;
        let (revision, tasks) = self.tasks()?;
        if revision != readiness.revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during handoff analysis",
            ));
        }
        project_handoffs::analyze_handoffs(revision, &tasks, &readiness.edges, request)
            .map_err(store::AuthorityError::Invalid)
    }

    /// Deterministic portfolio-aware capacity proposal; never writes a reservation.
    pub fn portfolio_capacity_plan(
        &self,
        request: &portfolio_capacity::PortfolioCapacityRequest,
    ) -> Result<portfolio_capacity::PortfolioCapacityPlan, store::AuthorityError> {
        let readiness = self.task_readiness(request.now_ms)?;
        let (revision, tasks) = self.tasks()?;
        if revision != readiness.revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during portfolio allocation",
            ));
        }
        portfolio_capacity::portfolio_capacity_plan(revision, &tasks, &readiness.edges, request)
            .map_err(store::AuthorityError::Invalid)
    }

    /// Compare user-adjusted planning alternatives against one accepted revision.
    /// This operation cannot persist tasks, schedule reservations, or commands.
    pub fn compare_planning_scenarios(
        &self,
        request: &planning_scenarios::ScenarioCompareRequest,
    ) -> Result<planning_scenarios::ScenarioComparison, store::AuthorityError> {
        let readiness = self.task_readiness(request.baseline.now_ms)?;
        let (revision, tasks) = self.tasks()?;
        if revision != readiness.revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during scenario comparison",
            ));
        }
        planning_scenarios::compare_scenarios(revision, &tasks, &readiness.edges, request)
            .map_err(store::AuthorityError::Invalid)
    }

    /// Read-only portfolio recommendations from one accepted revision.
    pub fn portfolio_actions(
        &self,
        request: &portfolio_actions::PortfolioActionRequest,
    ) -> Result<portfolio_actions::PortfolioActionReport, store::AuthorityError> {
        let readiness = self.task_readiness(request.now_ms)?;
        let (revision, tasks) = self.tasks()?;
        if revision != readiness.revision {
            return Err(store::AuthorityError::Invalid(
                "accepted state advanced during portfolio planning",
            ));
        }
        portfolio_actions::recommend_portfolio_actions(revision, &tasks, &readiness.edges, request)
            .map_err(store::AuthorityError::Invalid)
    }

    pub fn task_dependency_edges(
        &self,
    ) -> Result<Vec<task_dependencies::TaskDependencyEdge>, store::AuthorityError> {
        self.authority.task_dependency_edges()
    }

    pub fn task_readiness(
        &self,
        now_ms: i64,
    ) -> Result<task_dependencies::TaskReadinessSnapshot, store::AuthorityError> {
        self.authority.task_readiness(now_ms)
    }

    pub fn tasks(&self) -> Result<(i64, Vec<domain_commands::TaskRecord>), store::AuthorityError> {
        self.authority.tasks()
    }

    /// One canonical task projection including rich details and ordered labels.
    pub fn task_record(
        &self,
        task_id: &str,
    ) -> Result<Option<domain_commands::TaskRecord>, store::AuthorityError> {
        self.authority.task_record(task_id)
    }

    pub fn project_command(
        &mut self,
        actor: &str,
        command: domain_commands::ProjectCommand,
    ) -> Result<domain_commands::ProjectResult, store::AuthorityError> {
        self.authority.authority_project_command(actor, command)
    }

    pub fn strategies(
        &self,
    ) -> Result<authority_strategy::StrategySnapshot, store::AuthorityError> {
        self.authority.authority_strategies()
    }
    pub fn strategy_command(
        &mut self,
        actor: &str,
        command: authority_strategy::StrategyCommand,
    ) -> Result<authority_strategy::StrategyResult, store::AuthorityError> {
        self.authority.authority_strategy_command(actor, command)
    }

    pub fn milestones(
        &self,
    ) -> Result<authority_milestones::MilestoneSnapshot, store::AuthorityError> {
        self.authority.authority_milestones()
    }
    pub fn milestone_command(
        &mut self,
        actor: &str,
        command: authority_milestones::MilestoneCommand,
    ) -> Result<authority_milestones::MilestoneResult, store::AuthorityError> {
        self.authority.authority_milestone_command(actor, command)
    }

    pub fn routines(&self) -> Result<authority_routines::RoutineSnapshot, store::AuthorityError> {
        self.authority.authority_routines()
    }

    pub fn routine_command(
        &mut self,
        actor: &str,
        command: authority_routines::RoutineCommand,
    ) -> Result<authority_routines::RoutineResult, store::AuthorityError> {
        self.authority.authority_routine_command(actor, command)
    }

    pub fn schedule(&self) -> Result<authority_schedule::ScheduleSnapshot, store::AuthorityError> {
        self.authority.authority_schedule()
    }

    pub fn schedule_command(
        &mut self,
        actor: &str,
        command: authority_schedule::ScheduleCommand,
    ) -> Result<authority_schedule::ScheduleResult, store::AuthorityError> {
        self.authority.authority_schedule_command(actor, command)
    }

    pub fn schedule_conflicts(
        &self,
        start: i64,
        duration: u32,
        except: Option<&str>,
    ) -> Result<Vec<authority_schedule::ScheduleConflict>, store::AuthorityError> {
        self.authority
            .authority_schedule_conflicts(start, duration, except)
    }

    pub fn task_command(
        &mut self,
        actor: &str,
        command: domain_commands::TaskCommand,
    ) -> Result<domain_commands::TaskResult, store::AuthorityError> {
        use domain_commands::{AcceptanceCriteriaAction, TaskAction, TaskResult};
        use orbit_application::{
            AcceptanceCriteriaChange, AcceptanceCriteriaCommand, UpdateTaskDetailsCommand,
        };

        // New rich variants are additive; every existing action keeps using the
        // canonical SQL command handler unchanged.
        match &command.action {
            TaskAction::SetDetails {
                task_id,
                description,
                priority,
                labels,
            } => {
                let priority = match priority {
                    Some(value) => Some(
                        orbit_domain::TaskPriority::from_str_checked(value).map_err(|_| {
                            store::AuthorityError::Invalid(
                                "invalid task priority; expected low, medium, high, or urgent",
                            )
                        })?,
                    ),
                    None => None,
                };
                let activity_id =
                    orbit_domain::ActivityId::new(format!("activity-{}", command.command_id))
                        .map_err(|_| store::AuthorityError::Invalid("invalid activity id"))?;
                let rich = UpdateTaskDetailsCommand {
                    task_id: orbit_domain::TaskId::new(task_id.clone())
                        .map_err(|_| store::AuthorityError::Invalid("invalid task id"))?,
                    description: description.clone(),
                    priority,
                    labels: labels.clone(),
                    activity_id,
                    origin: orbit_domain::ActivityOrigin::External(actor.to_owned()),
                };
                let (revision, duplicate, task_id) = self.update_task_details_accepted(
                    actor,
                    &command.command_id,
                    command.expected_revision,
                    rich,
                )?;
                let task = self
                    .authority
                    .task_record(&task_id)?
                    .ok_or(store::AuthorityError::Invalid("task does not exist"))?;
                Ok(TaskResult {
                    revision,
                    duplicate,
                    task,
                })
            }
            TaskAction::AcceptanceCriteria { task_id, change } => {
                let change = match change {
                    AcceptanceCriteriaAction::Add { criterion_id, text } => {
                        AcceptanceCriteriaChange::Add {
                            criterion_id: orbit_domain::AcceptanceCriterionId::new(
                                criterion_id.clone(),
                            )
                            .map_err(|_| {
                                store::AuthorityError::Invalid("invalid acceptance criterion id")
                            })?,
                            text: text.clone(),
                        }
                    }
                    AcceptanceCriteriaAction::SetText { criterion_id, text } => {
                        AcceptanceCriteriaChange::SetText {
                            criterion_id: orbit_domain::AcceptanceCriterionId::new(
                                criterion_id.clone(),
                            )
                            .map_err(|_| {
                                store::AuthorityError::Invalid("invalid acceptance criterion id")
                            })?,
                            text: text.clone(),
                        }
                    }
                    AcceptanceCriteriaAction::SetCompleted {
                        criterion_id,
                        completed,
                    } => AcceptanceCriteriaChange::SetCompleted {
                        criterion_id: orbit_domain::AcceptanceCriterionId::new(
                            criterion_id.clone(),
                        )
                        .map_err(|_| {
                            store::AuthorityError::Invalid("invalid acceptance criterion id")
                        })?,
                        completed: *completed,
                    },
                    AcceptanceCriteriaAction::Reorder {
                        criterion_id,
                        position,
                    } => AcceptanceCriteriaChange::Reorder {
                        criterion_id: orbit_domain::AcceptanceCriterionId::new(
                            criterion_id.clone(),
                        )
                        .map_err(|_| {
                            store::AuthorityError::Invalid("invalid acceptance criterion id")
                        })?,
                        position: *position,
                    },
                    AcceptanceCriteriaAction::Remove { criterion_id } => {
                        AcceptanceCriteriaChange::Remove {
                            criterion_id: orbit_domain::AcceptanceCriterionId::new(
                                criterion_id.clone(),
                            )
                            .map_err(|_| {
                                store::AuthorityError::Invalid("invalid acceptance criterion id")
                            })?,
                        }
                    }
                };
                let activity_id =
                    orbit_domain::ActivityId::new(format!("activity-{}", command.command_id))
                        .map_err(|_| store::AuthorityError::Invalid("invalid activity id"))?;
                let rich = AcceptanceCriteriaCommand {
                    task_id: orbit_domain::TaskId::new(task_id.clone())
                        .map_err(|_| store::AuthorityError::Invalid("invalid task id"))?,
                    change,
                    activity_id,
                    origin: orbit_domain::ActivityOrigin::External(actor.to_owned()),
                };
                let (revision, duplicate, task_id) = self.apply_acceptance_criteria_accepted(
                    actor,
                    &command.command_id,
                    command.expected_revision,
                    rich,
                )?;
                let task = self
                    .authority
                    .task_record(&task_id)?
                    .ok_or(store::AuthorityError::Invalid("task does not exist"))?;
                Ok(TaskResult {
                    revision,
                    duplicate,
                    task,
                })
            }
            _ => self.authority.authority_task_command(actor, command),
        }
    }

    pub fn evidence_command(
        &mut self,
        actor: &str,
        command: authority_evidence::EvidenceCommand,
    ) -> Result<authority_evidence::EvidenceResult, store::AuthorityError> {
        self.authority.authority_evidence_command(actor, command)
    }

    pub fn snapshot(&self) -> Result<AuthoritySnapshot, store::AuthorityError> {
        self.authority.authority_full_snapshot()
    }

    pub fn history_prefix(&self, revision: i64) -> Result<String, store::AuthorityError> {
        self.authority.authority_history_prefix(revision)
    }

    pub fn evidence(&self) -> Result<authority_evidence::EvidenceSnapshot, store::AuthorityError> {
        self.authority.authority_evidence_snapshot()
    }
}

// Device authentication belongs to the host boundary, not to HTTP/MCP adapters.
// The runtime owns both credentials and authoritative domain state in one store.
impl AuthoritativeHost {
    pub fn backup_to(&self, destination: &Path) -> Result<(), store::AuthorityError> {
        self.authority.backup_to(destination)
    }

    pub fn enroll_device(&mut self, id: &str, token: &str) -> Result<(), store::AuthorityError> {
        self.authority.enroll(id, token)
    }

    pub fn authorize_device(&self, id: &str, token: &str) -> Result<bool, store::AuthorityError> {
        self.authority.authorized(id, token)
    }

    pub fn revoke_device(&mut self, id: &str) -> Result<bool, store::AuthorityError> {
        self.authority.revoke(id)
    }

    pub fn rotate_device_token(
        &mut self,
        id: &str,
        token: &str,
    ) -> Result<bool, store::AuthorityError> {
        self.authority.rotate_token(id, token)
    }
}

#[cfg(test)]
mod host_tests {
    use super::*;

    #[test]
    fn device_credentials_are_host_owned_and_revocable() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        let first = "device-secret-one-0123456789abcdef";
        let second = "device-secret-two-fedcba9876543210";
        host.enroll_device("device-1", first).unwrap();
        assert!(host.authorize_device("device-1", first).unwrap());
        assert!(
            !host
                .authorize_device("device-1", "device-secret-wrong-0123456789")
                .unwrap()
        );
        assert!(host.rotate_device_token("device-1", second).unwrap());
        assert!(!host.authorize_device("device-1", first).unwrap());
        assert!(host.authorize_device("device-1", second).unwrap());
        assert!(host.revoke_device("device-1").unwrap());
        assert!(!host.authorize_device("device-1", second).unwrap());
    }

    #[test]
    fn markdown_index_reconciles_moves_and_rejects_ambiguous_snapshots() {
        let mut host = AuthoritativeHost::open(":memory:").unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("Research")
            .unwrap();
        host.knowledge_vault
            .vault()
            .create_markdown("Work/one.md", "---\norbit_id: note-one\n---\n# One\n")
            .unwrap();
        let first = host.reconcile_markdown_knowledge_index().unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first, host.indexed_markdown_knowledge().unwrap());
        host.knowledge_vault
            .vault()
            .move_markdown("Work/one.md", "Research/one.md")
            .unwrap();
        assert_eq!(
            host.reconcile_markdown_knowledge_index().unwrap()[0].path,
            "Research/one.md"
        );
        // An external duplicate ID must prevent any change to the prior index.
        host.knowledge_vault
            .vault()
            .create_markdown(
                "Work/duplicate.md",
                "---\norbit_id: note-one\n---\n# Duplicate\n",
            )
            .unwrap();
        assert!(host.reconcile_markdown_knowledge_index().is_err());
        assert_eq!(
            host.indexed_markdown_knowledge().unwrap()[0].path,
            "Research/one.md"
        );
        // SQLite index is metadata-only: no Markdown body should appear in its schema.
        let cols = host
            .authority
            .conn
            .prepare("PRAGMA table_info(markdown_note_index)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(cols, vec!["id", "path", "title"]);
    }

    #[test]
    fn authoritative_host_reads_one_empty_canonical_revision() {
        let host = AuthoritativeHost::open(":memory:").unwrap();
        let (revision, fingerprint, projects, tasks, evidence) = host.snapshot().unwrap();
        assert_eq!(revision, 0);
        assert!(!fingerprint.is_empty());
        assert!(projects.is_empty());
        assert!(tasks.is_empty());
        assert!(evidence.habits.is_empty());
        assert_eq!(host.projects().unwrap().0, revision);
        assert_eq!(host.tasks().unwrap().0, revision);
    }
}

#[cfg(test)]
mod markdown_workspace_workflow_tests {
    use super::*;
    #[test]
    fn file_backed_create_edit_move_and_rebuild_index() {
        let db = std::env::temp_dir().join(format!(
            "orbit-workflow-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut host = AuthoritativeHost::open(&db).unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("Research")
            .unwrap();
        host.stage_markdown_create("create-note-a", "note-a", "Work/a.md", "# One\n")
            .unwrap();
        let (note, before) = host.markdown_by_id("note-a").unwrap().unwrap();
        assert_eq!(note.path, "Work/a.md");
        let after = before.replace("One", "Two");
        host.stage_markdown_update("edit-note-a", "note-a", &before, &after)
            .unwrap();
        assert!(
            host.stage_markdown_update("edit-stale", "note-a", &before, &before)
                .is_err()
        );
        host.stage_markdown_move("note-a", "Research/a.md").unwrap();
        assert_eq!(
            host.markdown_by_id("note-a").unwrap().unwrap().0.path,
            "Research/a.md"
        );
        assert_eq!(
            host.indexed_markdown_knowledge().unwrap()[0].path,
            "Research/a.md"
        );
        assert_eq!(host.markdown_search("Two", 10).unwrap().len(), 1);
        let mut reopened = AuthoritativeHost::open(&db).unwrap();
        reopened.reconcile_markdown_knowledge_index().unwrap();
        assert_eq!(reopened.markdown_by_id("note-a").unwrap().unwrap().1, after);
        drop(reopened);
        drop(host);
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_dir_all(db.with_extension("knowledge"));
    }
}

#[cfg(test)]
mod markdown_command_acceptance_tests {
    use super::*;
    use markdown_commands::{MarkdownCommand, MarkdownReceipt};
    use std::fs;

    fn database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-markdown-command-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn create_update_reopen_and_duplicate_receipts() {
        let db = database();
        let vault = db.with_extension("knowledge");
        let mut host = AuthoritativeHost::open(&db).unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        let create = MarkdownCommand::Create {
            note_id: "note-1".into(),
            path: "Work/a.md".into(),
            body: "# First\n".into(),
        };
        let initial = host
            .execute_markdown_command("owner", "create-1", create.clone())
            .unwrap();
        assert_eq!(initial.revision, 1);
        assert_eq!(
            host.execute_markdown_command("owner", "create-1", create.clone())
                .unwrap(),
            initial
        );
        assert!(
            host.execute_markdown_command(
                "owner",
                "create-1",
                MarkdownCommand::Create {
                    note_id: "other".into(),
                    path: "Work/b.md".into(),
                    body: "different".into()
                }
            )
            .is_err()
        );
        let (_, expected) = host.markdown_by_id("note-1").unwrap().unwrap();
        let replacement = expected.replace("First", "Second");
        let update = MarkdownCommand::Update {
            note_id: "note-1".into(),
            expected,
            replacement: replacement.clone(),
        };
        let changed = host
            .execute_markdown_command("owner", "edit-1", update.clone())
            .unwrap();
        assert_eq!(changed.revision, 2);
        drop(host);
        let mut reopened = AuthoritativeHost::open(&db).unwrap();
        assert_eq!(
            reopened
                .execute_markdown_command("owner", "edit-1", update)
                .unwrap(),
            changed
        );
        assert_eq!(
            reopened.markdown_by_id("note-1").unwrap().unwrap().1,
            replacement
        );
        fs::remove_file(&db).unwrap();
        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn pending_create_is_replayed_and_accepted_at_startup() {
        let db = database();
        let vault = db.with_extension("knowledge");
        let mut host = AuthoritativeHost::open(&db).unwrap();
        let command = MarkdownCommand::Create {
            note_id: "note-recover".into(),
            path: "recover.md".into(),
            body: "# Durable\n".into(),
        };
        assert!(
            markdown_commands::enqueue(&mut host.authority, "owner", "create-recover", &command)
                .unwrap()
                .is_none()
        );
        // Mimic the crash immediately after the filesystem publication.
        let service = knowledge_service::KnowledgeService::new(&host.knowledge_vault);
        markdown_commands::publish(&service, "create-recover", &command).unwrap();
        drop(host);
        let mut reopened = AuthoritativeHost::open(&db).unwrap();
        let receipt: MarkdownReceipt = reopened
            .execute_markdown_command("owner", "create-recover", command)
            .unwrap();
        assert_eq!(receipt.revision, 1);
        assert_eq!(reopened.indexed_markdown_knowledge().unwrap().len(), 1);
        fs::remove_file(&db).unwrap();
        fs::remove_dir_all(vault).unwrap();
    }
}

#[cfg(test)]
mod markdown_move_command_acceptance_tests {
    use super::*;
    use markdown_commands::MarkdownCommand;
    use std::fs;

    fn database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-markdown-move-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn move_preserves_identity_content_and_accepted_revision_across_restart() {
        let db = database();
        let vault = db.with_extension("knowledge");
        let mut host = AuthoritativeHost::open(&db).unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("Research")
            .unwrap();
        let create = MarkdownCommand::Create {
            note_id: "note-move".into(),
            path: "Work/start.md".into(),
            body: "# Read me\n".into(),
        };
        assert_eq!(
            host.execute_markdown_command("owner", "new", create)
                .unwrap()
                .revision,
            1
        );
        let before = host.markdown_by_id("note-move").unwrap().unwrap().1;
        let move_it = MarkdownCommand::Move {
            note_id: "note-move".into(),
            from: "Work/start.md".into(),
            to: "Research/moved.md".into(),
            expected: before.clone(),
        };
        let receipt = host
            .execute_markdown_command("owner", "move", move_it.clone())
            .unwrap();
        assert_eq!(receipt.revision, 2);
        assert_eq!(
            host.markdown_by_id("note-move").unwrap().unwrap().0.path,
            "Research/moved.md"
        );
        assert_eq!(host.markdown_by_id("note-move").unwrap().unwrap().1, before);
        assert!(!vault.join("Work/start.md").exists());
        drop(host);
        let mut reopened = AuthoritativeHost::open(&db).unwrap();
        assert_eq!(
            reopened
                .execute_markdown_command("owner", "move", move_it)
                .unwrap(),
            receipt
        );
        assert_eq!(
            reopened.indexed_markdown_knowledge().unwrap()[0].path,
            "Research/moved.md"
        );
        drop(reopened);
        let _ = fs::remove_file(&db);
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn restart_completes_interrupted_hardlink_move_without_losing_note() {
        let db = database();
        let vault = db.with_extension("knowledge");
        let mut host = AuthoritativeHost::open(&db).unwrap();
        host.knowledge_vault.vault().create_folder("Work").unwrap();
        host.knowledge_vault
            .vault()
            .create_folder("Research")
            .unwrap();
        host.execute_markdown_command(
            "owner",
            "new",
            MarkdownCommand::Create {
                note_id: "note-recovery".into(),
                path: "Work/original.md".into(),
                body: "# Durable\n".into(),
            },
        )
        .unwrap();
        let expected = host.markdown_by_id("note-recovery").unwrap().unwrap().1;
        let command = MarkdownCommand::Move {
            note_id: "note-recovery".into(),
            from: "Work/original.md".into(),
            to: "Research/renamed.md".into(),
            expected,
        };
        markdown_commands::enqueue(&mut host.authority, "owner", "move-replay", &command).unwrap();
        // Simulate crash after hard-link publication but before unlinking source.
        fs::hard_link(
            vault.join("Work/original.md"),
            vault.join("Research/renamed.md"),
        )
        .unwrap();
        drop(host);
        let reopened = AuthoritativeHost::open(&db).unwrap();
        assert!(reopened.markdown_by_id("note-recovery").unwrap().is_some());
        assert!(!vault.join("Work/original.md").exists());
        assert_eq!(
            reopened.indexed_markdown_knowledge().unwrap()[0].path,
            "Research/renamed.md"
        );
        drop(reopened);
        let _ = fs::remove_file(&db);
        let _ = fs::remove_dir_all(&vault);
    }
}

pub mod agent_dispatch;
pub mod herdr_runtime;

pub mod agent_monitor;
