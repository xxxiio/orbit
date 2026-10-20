//! Physical Markdown and folder commands. All writes use durable authority receipts.
use super::markdown_assets::{AssetCommand, export_asset_directory, run_asset_command};
use super::markdown_metadata::{
    NoteKind, NoteMetadataOperation, NoteStatus, inspect_notes, mutate_note,
};
use super::*;
use orbit_runtime::{
    folder_commands::{FolderCreate, FolderMove},
    markdown_commands::MarkdownCommand,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Args)]
pub struct NoteCommand {
    #[command(subcommand)]
    pub command: NoteSubcommand,
}
#[derive(Debug, Subcommand)]
pub enum NoteSubcommand {
    /// Publish a new Markdown file with an immutable orbit_id.
    Create {
        path: String,
        #[arg(long)]
        id: String,
        #[arg(long, conflicts_with = "file")]
        body: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Read current authoritative Markdown bytes by stable identity.
    Read { id: String },
    /// Replace bytes, checking that the saved document hasn't changed.
    Update {
        id: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        expected_file: Option<PathBuf>,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Move a document to a new directory/filename without changing identity.
    Move {
        id: String,
        destination: String,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        expected_file: Option<PathBuf>,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Set a Markdown note classification in YAML frontmatter.
    Kind {
        id: String,
        kind: NoteKind,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Set an authoritative note lifecycle status in YAML frontmatter.
    Status {
        id: String,
        status: NoteStatus,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Add an explicit stable-ID link to another note.
    Link {
        id: String,
        target: String,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Remove explicit stable-ID links, including labeled links.
    Unlink {
        id: String,
        target: String,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Manage immutable content-addressed attachments linked from a note.
    Asset {
        #[command(subcommand)]
        command: AssetCommand,
    },
    /// Explicit typed references to projects, tasks, and habits.
    Reference {
        id: String,
        target: String,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Remove one typed reference.
    Unreference {
        id: String,
        target: String,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// Find notes that reference an external project, task, or habit identity.
    ReferenceBacklinks { target: String },
    /// Find all notes that link an immutable attachment by digest filename.
    AssetBacklinks { asset: String },
    /// Audit missing, corrupt, and unreferenced vault asset files without deletion.
    AssetAudit,
    /// Enumerate stable identities and current physical paths.
    List,
    /// Filter notes by YAML kind/status, using only Markdown bytes.
    Catalog {
        #[arg(long)]
        kind: Option<NoteKind>,
        #[arg(long)]
        status: Option<NoteStatus>,
    },
    /// Search Markdown file content.
    Search {
        query: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Import a directory of Markdown files into an existing vault, preserving stable IDs.
    ImportDir {
        source: PathBuf,
        #[arg(long)]
        into: Option<String>,
    },
    /// Export the complete Markdown vault to a new directory without modifying originals.
    ExportDir { destination: PathBuf },
    /// Inspect references and backlinks for a stable note identity.
    Relations { id: String },
    /// Read a Markdown file by vault-relative path.
    Path { path: String },
}
#[derive(Debug, Args)]
pub struct FolderCommand {
    #[command(subcommand)]
    pub command: FolderSubcommand,
}
#[derive(Debug, Subcommand)]
pub enum FolderSubcommand {
    Tree,
    Create {
        path: String,
        #[arg(long)]
        command_id: Option<String>,
    },
    Move {
        from: String,
        to: String,
        #[arg(long)]
        command_id: Option<String>,
    },
}

pub(crate) fn authority_response<T: Serialize>(value: T) -> Result<Response, CliError> {
    let value = serde_json::to_value(value).map_err(|e| CliError::Serialization(e.to_string()))?;
    Ok(Response::Authority { result: value })
}
pub(crate) fn knowledge_error<E: std::fmt::Debug>(e: E) -> CliError {
    CliError::Serialization(format!("Markdown vault: {e:?}"))
}
fn command_id(id: Option<String>) -> String {
    id.unwrap_or_else(|| generate_id("command"))
}

pub fn run_note_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    db: &Path,
    command: NoteSubcommand,
) -> Result<Response, CliError> {
    match command {
        NoteSubcommand::Create {
            path,
            id,
            body,
            file,
            command_id: cid,
        } => {
            let body = match (body, file) {
                (Some(text), None) => text,
                (None, Some(file)) => {
                    fs::read_to_string(file).map_err(|e| CliError::Io(e.to_string()))?
                }
                (None, None) => {
                    return Err(CliError::Usage(
                        "note create requires --body or --file".into(),
                    ));
                }
                (Some(_), Some(_)) => {
                    return Err(CliError::Usage("use either --body or --file".into()));
                }
            };
            authority_response(
                host.execute_markdown_command(
                    "orbit-cli",
                    &command_id(cid),
                    MarkdownCommand::Create {
                        note_id: id,
                        path,
                        body,
                    },
                )
                .map_err(knowledge_error)?,
            )
        }
        NoteSubcommand::Read { id } => {
            let result = host
                .markdown_by_id(&id)
                .map_err(knowledge_error)?
                .ok_or_else(|| CliError::Usage(format!("Markdown note not found: {id}")))?;
            authority_response(serde_json::json!({"note":result.0,"body":result.1}))
        }
        NoteSubcommand::Update {
            id,
            file,
            expected_file,
            command_id: cid,
        } => {
            let expected = if let Some(file) = expected_file {
                fs::read_to_string(file).map_err(|e| CliError::Io(e.to_string()))?
            } else {
                host.markdown_by_id(&id)
                    .map_err(knowledge_error)?
                    .ok_or_else(|| CliError::Usage(format!("Markdown note not found: {id}")))?
                    .1
            };
            let replacement = fs::read_to_string(file).map_err(|e| CliError::Io(e.to_string()))?;
            authority_response(
                host.execute_markdown_command(
                    "orbit-cli",
                    &command_id(cid),
                    MarkdownCommand::Update {
                        note_id: id,
                        expected,
                        replacement,
                    },
                )
                .map_err(knowledge_error)?,
            )
        }
        NoteSubcommand::Move {
            id,
            destination,
            from,
            expected_file,
            command_id: cid,
        } => {
            let current = if from.is_none() || expected_file.is_none() {
                Some(
                    host.markdown_by_id(&id)
                        .map_err(knowledge_error)?
                        .ok_or_else(|| CliError::Usage(format!("Markdown note not found: {id}")))?,
                )
            } else {
                None
            };
            let from = from.unwrap_or_else(|| {
                current
                    .as_ref()
                    .expect("current path present")
                    .0
                    .path
                    .clone()
            });
            let expected = if let Some(file) = expected_file {
                fs::read_to_string(file).map_err(|e| CliError::Io(e.to_string()))?
            } else {
                current.expect("current body present").1
            };
            authority_response(
                host.execute_markdown_command(
                    "orbit-cli",
                    &command_id(cid),
                    MarkdownCommand::Move {
                        note_id: id,
                        from,
                        to: destination,
                        expected,
                    },
                )
                .map_err(knowledge_error)?,
            )
        }
        NoteSubcommand::Kind {
            id,
            kind,
            command_id,
        } => mutate_note(host, id, NoteMetadataOperation::Kind(kind), command_id),
        NoteSubcommand::Status {
            id,
            status,
            command_id,
        } => mutate_note(host, id, NoteMetadataOperation::Status(status), command_id),
        NoteSubcommand::Link {
            id,
            target,
            command_id,
        } => mutate_note(host, id, NoteMetadataOperation::AddLink(target), command_id),
        NoteSubcommand::Unlink {
            id,
            target,
            command_id,
        } => mutate_note(
            host,
            id,
            NoteMetadataOperation::RemoveLink(target),
            command_id,
        ),
        NoteSubcommand::Asset { command } => run_asset_command(host, db, command),
        NoteSubcommand::Reference {
            id,
            target,
            command_id,
        } => mutate_note(
            host,
            id,
            NoteMetadataOperation::AddTypedReference(target),
            command_id,
        ),
        NoteSubcommand::Unreference {
            id,
            target,
            command_id,
        } => mutate_note(
            host,
            id,
            NoteMetadataOperation::RemoveTypedReference(target),
            command_id,
        ),
        NoteSubcommand::ReferenceBacklinks { target } => {
            super::markdown_discovery::reference_backlinks(host, &target)
        }
        NoteSubcommand::AssetBacklinks { asset } => {
            super::markdown_discovery::asset_backlinks(host, &asset)
        }
        NoteSubcommand::AssetAudit => super::markdown_discovery::audit_assets(host, db),
        NoteSubcommand::List => {
            authority_response(host.markdown_knowledge_index().map_err(knowledge_error)?)
        }
        NoteSubcommand::Catalog { kind, status } => inspect_notes(host, kind, status),
        NoteSubcommand::Search { query, limit } => {
            if query.trim().is_empty() || limit == 0 || limit > 1000 {
                return Err(CliError::Usage(
                    "query must not be blank and limit must be 1..1000".into(),
                ));
            }
            authority_response(
                host.markdown_search(&query, limit)
                    .map_err(knowledge_error)?,
            )
        }
        NoteSubcommand::ImportDir { source, into } => {
            import_dir(host, db, &source, into.as_deref())
        }
        NoteSubcommand::ExportDir { destination } => export_dir(host, db, &destination),
        NoteSubcommand::Relations { id } => {
            authority_response(host.markdown_relations(&id).map_err(knowledge_error)?)
        }
        NoteSubcommand::Path { path } => authority_response(
            serde_json::json!({"path":path,"body":host.markdown_document(&path).map_err(knowledge_error)?}),
        ),
    }
}

pub fn run_folder_host(
    host: &mut orbit_runtime::AuthoritativeHost,
    command: FolderSubcommand,
) -> Result<Response, CliError> {
    match command {
        FolderSubcommand::Tree => {
            authority_response(host.knowledge_tree().map_err(knowledge_error)?)
        }
        FolderSubcommand::Create {
            path,
            command_id: cid,
        } => authority_response(
            host.execute_folder_create("orbit-cli", &command_id(cid), FolderCreate { path })
                .map_err(knowledge_error)?,
        ),
        FolderSubcommand::Move {
            from,
            to,
            command_id: cid,
        } => authority_response(
            host.execute_folder_move("orbit-cli", &command_id(cid), FolderMove { from, to })
                .map_err(knowledge_error)?,
        ),
    }
}

/// Stable relative-path hashing keeps imports replayable across processes and
/// ensures that the command ID is tied to the exact published document bytes.
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn importer_id(value: &str) -> String {
    format!("note-{}", &digest(value)[..24])
}

fn safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.starts_with('.')
        && !segment.contains('\\')
        && !segment.contains('\0')
}
fn safe_relative_note(path: &str) -> bool {
    path.len() <= 1024 && path.ends_with(".md") && path.split('/').all(safe_segment)
}
fn valid_note_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Reject symlinks, hidden paths, and non-Markdown inputs rather than importing
/// files outside an explicitly selected source directory.
fn discover_import_files(root: &Path) -> Result<BTreeMap<String, PathBuf>, CliError> {
    if !root.is_dir()
        || fs::symlink_metadata(root)
            .map_err(|e| CliError::Io(e.to_string()))?
            .file_type()
            .is_symlink()
    {
        return Err(CliError::Usage(
            "import source must be a real directory".into(),
        ));
    }
    let mut results = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| CliError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| CliError::Io(e.to_string()))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| CliError::Usage("non-UTF8 import path".into()))?;
            if !safe_segment(&name) {
                continue;
            }
            let kind = entry.file_type().map_err(|e| CliError::Io(e.to_string()))?;
            if kind.is_symlink() {
                return Err(CliError::Usage(format!("symlink in import source: {name}")));
            }
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                stack.push((entry.path(), relative));
            } else if kind.is_file() && name.ends_with(".md") {
                if !safe_relative_note(&relative) {
                    return Err(CliError::Usage(format!("invalid note path: {relative}")));
                }
                results.insert(relative, entry.path());
            }
        }
    }
    Ok(results)
}

fn existing_id(body: &str) -> Result<Option<String>, CliError> {
    if !body.starts_with("---\n") {
        return Ok(None);
    }
    let mut found = None;
    let mut closed = false;
    for line in body.lines().skip(1) {
        if line == "---" {
            closed = true;
            break;
        }
        if let Some(id) = line.strip_prefix("orbit_id: ") {
            if found.is_some() || !valid_note_id(id) {
                return Err(CliError::Usage(
                    "invalid or duplicated orbit_id in import source".into(),
                ));
            }
            found = Some(id.to_string());
        }
    }
    if !closed || found.is_none() {
        return Err(CliError::Usage(
            "Markdown frontmatter must contain one valid orbit_id".into(),
        ));
    }
    Ok(found)
}

fn folders_for(paths: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let mut folders = BTreeSet::new();
    for path in paths {
        let mut current = String::new();
        let parts: Vec<_> = path.split('/').collect();
        for item in &parts[..parts.len().saturating_sub(1)] {
            current = if current.is_empty() {
                (*item).to_string()
            } else {
                format!("{current}/{item}")
            };
            folders.insert(current.clone());
        }
    }
    folders
}

/// Import uses the same durable per-note receipts as regular note creation.
/// A directory is a resumable batch, not one atomic transaction. Plan and
/// identity collision checks run before mutating folders or notes.
fn import_dir(
    host: &mut orbit_runtime::AuthoritativeHost,
    db: &Path,
    source: &Path,
    into: Option<&str>,
) -> Result<Response, CliError> {
    let prefix = into.unwrap_or("");
    if !prefix.is_empty() && !prefix.split('/').all(safe_segment) {
        return Err(CliError::Usage("invalid destination folder".into()));
    }
    let discovered = discover_import_files(source)?;
    if discovered.is_empty() {
        return Err(CliError::Usage("no Markdown notes found in source".into()));
    }
    let existing = host.markdown_knowledge_index().map_err(knowledge_error)?;
    let by_id: BTreeMap<_, _> = existing
        .iter()
        .map(|item| (item.id.as_str(), item.path.as_str()))
        .collect();
    let by_path: BTreeMap<_, _> = existing
        .iter()
        .map(|item| (item.path.as_str(), item.id.as_str()))
        .collect();
    let mut plan = Vec::new();
    let mut imported_ids = BTreeMap::new();
    for (relative, source_path) in discovered {
        let target = if prefix.is_empty() {
            relative
        } else {
            format!("{prefix}/{relative}")
        };
        if !safe_relative_note(&target) {
            return Err(CliError::Usage(format!(
                "invalid destination path: {target}"
            )));
        }
        let bytes = fs::read_to_string(source_path).map_err(|e| CliError::Io(e.to_string()))?;
        let id = existing_id(&bytes)?.unwrap_or_else(|| importer_id(&target));
        if let Some(old) = imported_ids.insert(id.clone(), target.clone()) {
            return Err(CliError::Usage(format!(
                "duplicate import identity {id}: {old} and {target}"
            )));
        }
        if let Some(old_path) = by_id.get(id.as_str())
            && *old_path != target
        {
            return Err(CliError::Usage(format!(
                "identity {id} already exists at {old_path}"
            )));
        }
        if let Some(old_id) = by_path.get(target.as_str())
            && *old_id != id
        {
            return Err(CliError::Usage(format!(
                "destination {target} already belongs to {old_id}"
            )));
        }
        let published = if bytes.starts_with("---\n") {
            bytes
        } else {
            format!("---\norbit_id: {id}\n---\n{bytes}")
        };
        let command_id = format!(
            "import-{}",
            &digest(&format!("{target}\n{published}"))[..48]
        );
        plan.push((id, target, published, command_id));
    }
    // A corrupt or missing attachment in the incoming package is a preflight
    // failure: it must not leave already-created notes or directories behind.
    let incoming_assets = super::markdown_assets::preflight_import_assets(source)?;
    let import_bodies: Vec<String> = plan.iter().map(|(_, _, body, _)| body.clone()).collect();
    super::markdown_assets::preflight_import_references(db, &import_bodies, &incoming_assets)?;
    // Preflight conflicting *existing* documents before publication; a retry
    // with the same bytes remains legal (receipts are still checked below).
    for (id, target, body, _) in &plan {
        if by_id.contains_key(id.as_str()) {
            let current = host
                .markdown_by_id(id)
                .map_err(knowledge_error)?
                .ok_or_else(|| {
                    CliError::Usage(format!("missing document for indexed identity {id}"))
                })?;
            if current.0.path != *target || current.1 != *body {
                return Err(CliError::Usage(format!(
                    "import would overwrite modified note {id}"
                )));
            }
        }
    }
    // Publish verified immutable bytes *before* any Markdown reference becomes
    // visible. A crash can leave recoverable orphan blobs, never a committed
    // note whose attachment has not yet been installed.
    let asset_count = super::markdown_assets::publish_import_assets(db, incoming_assets)?;
    let mut dirs = folders_for(plan.iter().map(|(_, target, _, _)| target.clone()));
    let tree = host.knowledge_tree().map_err(knowledge_error)?;
    fn collect(node: &orbit_runtime::knowledge_files::KnowledgeNode, acc: &mut BTreeSet<String>) {
        if let orbit_runtime::knowledge_files::KnowledgeNode::Folder { path, children, .. } = node {
            acc.insert(path.clone());
            for child in children {
                collect(child, acc);
            }
        }
    }
    let mut existing_folders = BTreeSet::new();
    collect(&tree, &mut existing_folders);
    // Parent-first order; the sorted path set is naturally lexicographic.
    for folder in std::mem::take(&mut dirs) {
        if !existing_folders.contains(&folder) {
            let command_id = format!("import-folder-{}", &digest(&folder)[..40]);
            host.execute_folder_create(
                "orbit-cli",
                &command_id,
                FolderCreate {
                    path: folder.clone(),
                },
            )
            .map_err(knowledge_error)?;
            existing_folders.insert(folder);
        }
    }
    let count = plan.len();
    let mut receipts = Vec::with_capacity(count);
    for (id, path, body, cid) in plan {
        receipts.push(
            host.execute_markdown_command(
                "orbit-cli",
                &cid,
                MarkdownCommand::Create {
                    note_id: id,
                    path,
                    body,
                },
            )
            .map_err(knowledge_error)?,
        );
    }
    authority_response(
        serde_json::json!({"imported":count,"assets":asset_count,"receipts":receipts}),
    )
}

/// Export never overwrites an existing directory. The resulting tree is a
/// portable, human-readable Markdown backup, not a SQLite replication stream.
/// Fully materialize and verify the export input before creating the destination.
/// A failed read, unsafe path or missing/corrupt attachment must never leave a
/// directory that looks like a finished portable knowledge export.
fn prepare_export(
    host: &orbit_runtime::AuthoritativeHost,
    db: &Path,
) -> Result<Vec<(String, String)>, CliError> {
    let notes = host.markdown_knowledge_index().map_err(knowledge_error)?;
    let mut prepared = Vec::with_capacity(notes.len());
    let mut seen = BTreeSet::new();
    for note in notes {
        if !safe_relative_note(&note.path) || !seen.insert(note.path.clone()) {
            return Err(CliError::Usage(
                "unsafe or duplicate indexed Markdown path".into(),
            ));
        }
        let body = host
            .markdown_document(&note.path)
            .map_err(knowledge_error)?;
        // Check the identity against the file, not merely the cached index.
        if existing_id(&body)?.as_deref() != Some(note.id.as_str()) {
            return Err(CliError::Usage(format!(
                "identity mismatch in {}",
                note.path
            )));
        }
        prepared.push((note.path, body));
    }
    // Content-addressed assets must be validated *before* making an output
    // directory visible. Keep the verified bytes alongside the source notes.
    super::markdown_assets::preflight_export_references(
        db,
        &prepared
            .iter()
            .map(|(_, body)| body.as_str())
            .collect::<Vec<_>>(),
    )?;
    Ok(prepared)
}

/// Export is staged as a sibling directory. On success a single rename makes
/// the complete portable package visible; on failure the staging tree is
/// removed and no partial export is offered to the user.
fn export_dir(
    host: &orbit_runtime::AuthoritativeHost,
    db: &Path,
    destination: &Path,
) -> Result<Response, CliError> {
    let prepared = prepare_export(host, db)?;
    if fs::symlink_metadata(destination).is_ok() {
        return Err(CliError::Usage(
            "export destination must not already exist".into(),
        ));
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = None;
    for serial in 0..32u32 {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| CliError::Io(e.to_string()))?
            .as_nanos();
        let candidate = parent.join(format!(".orbit-export-{}-{stamp}-{serial}", process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                staged = Some(candidate);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(CliError::Io(e.to_string())),
        }
    }
    let staged =
        staged.ok_or_else(|| CliError::Usage("unable to allocate export staging folder".into()))?;
    let published = (|| -> Result<usize, CliError> {
        for (path, body) in &prepared {
            let file = staged.join(path);
            fs::create_dir_all(file.parent().expect("relative note has parent"))
                .map_err(|e| CliError::Io(e.to_string()))?;
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file)
                .map_err(|e| CliError::Io(e.to_string()))?;
            use std::io::Write;
            out.write_all(body.as_bytes())
                .and_then(|_| out.sync_all())
                .map_err(|e| CliError::Io(e.to_string()))?;
        }
        let assets = export_asset_directory(host, db, &staged)?;
        if fs::symlink_metadata(destination).is_ok() {
            return Err(CliError::Usage(
                "export destination appeared during export".into(),
            ));
        }
        fs::File::open(&staged)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| CliError::Io(e.to_string()))?;
        fs::rename(&staged, destination).map_err(|e| CliError::Io(e.to_string()))?;
        // The export is already visible after rename. A parent-directory
        // fsync failure must not report an incomplete export and prompt a
        // destructive retry over successfully published files.
        let _ = fs::File::open(parent).and_then(|dir| dir.sync_all());
        Ok(assets)
    })();
    match published {
        Ok(assets) => authority_response(
            serde_json::json!({"exported":prepared.len(),"assets":assets,"directory":destination.display().to_string()}),
        ),
        Err(error) => {
            let _ = fs::remove_dir_all(&staged);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp_workspace() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("orbit-note-cli-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root.join("workspace.db")
    }
    fn execute(db: &Path, args: &[&str]) -> Result<serde_json::Value, CliError> {
        let mut raw = vec![
            "orbit".to_owned(),
            "--db".to_owned(),
            db.display().to_string(),
            "--json".to_owned(),
        ];
        raw.extend(args.iter().map(|x| x.to_string()));
        let cli = Cli::try_parse_from(raw).map_err(|e| CliError::Usage(e.to_string()))?;
        let response = run(cli)?;
        serde_json::from_str(&response).map_err(|e| CliError::Serialization(e.to_string()))
    }
    #[test]
    fn notes_and_folders_survive_process_reopens_and_idempotent_create() {
        let db = temp_workspace();
        let db_ref = db.as_path();
        execute(
            db_ref,
            &["folder", "create", "Work", "--command-id", "folder-first"],
        )
        .unwrap();
        let first = execute(
            db_ref,
            &[
                "note",
                "create",
                "Work/alpha.md",
                "--id",
                "alpha",
                "--body",
                "# Alpha\n\n[[beta]]",
                "--command-id",
                "note-first",
            ],
        )
        .unwrap();
        let retry = execute(
            db_ref,
            &[
                "note",
                "create",
                "Work/alpha.md",
                "--id",
                "alpha",
                "--body",
                "# Alpha\n\n[[beta]]",
                "--command-id",
                "note-first",
            ],
        )
        .unwrap();
        assert_eq!(first, retry);
        let loaded = execute(db_ref, &["note", "read", "alpha"]).unwrap();
        assert!(
            loaded["result"]["body"]
                .as_str()
                .unwrap()
                .contains("orbit_id: alpha")
        );
        assert!(
            loaded["result"]["body"]
                .as_str()
                .unwrap()
                .contains("[[beta]]")
        );
        execute(
            db_ref,
            &[
                "folder",
                "move",
                "Work",
                "Archived",
                "--command-id",
                "folder-move",
            ],
        )
        .unwrap();
        let after = execute(db_ref, &["note", "read", "alpha"]).unwrap();
        assert_eq!(after["result"]["note"]["path"], "Archived/alpha.md");
        assert!(
            execute(
                db_ref,
                &[
                    "note",
                    "create",
                    "Archived/bad.md",
                    "--id",
                    "alpha",
                    "--body",
                    "duplicate"
                ]
            )
            .is_err()
        );
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn updates_use_explicit_expected_bytes_and_reject_conflicts() {
        let db = temp_workspace();
        let parent = db.parent().unwrap();
        let expected = parent.join("previous.md");
        let replacement = parent.join("next.md");
        execute(
            &db,
            &[
                "note",
                "create",
                "sample.md",
                "--id",
                "sample",
                "--body",
                "initial",
                "--command-id",
                "create-sample",
            ],
        )
        .unwrap();
        fs::write(&expected, "---\norbit_id: sample\n---\ninitial").unwrap();
        fs::write(&replacement, "---\norbit_id: sample\n---\nupdated").unwrap();
        let expected_path = expected.to_str().unwrap();
        let replacement_path = replacement.to_str().unwrap();
        let args = [
            "note",
            "update",
            "sample",
            "--file",
            replacement_path,
            "--expected-file",
            expected_path,
            "--command-id",
            "edit-once",
        ];
        let receipt = execute(&db, &args).unwrap();
        assert_eq!(receipt, execute(&db, &args).unwrap());
        fs::write(&replacement, "---\norbit_id: sample\n---\nother").unwrap();
        assert!(execute(&db, &args).is_err());
        assert!(
            execute(
                &db,
                &[
                    "note",
                    "update",
                    "sample",
                    "--file",
                    replacement_path,
                    "--expected-file",
                    expected_path,
                    "--command-id",
                    "different-command"
                ]
            )
            .is_err()
        );
        // A rejected conflicting command must not keep a pending reservation:
        // the next update without --expected-file reads the current accepted
        // bytes and proceeds normally.
        assert!(
            execute(
                &db,
                &["note", "update", "sample", "--file", replacement_path]
            )
            .is_ok()
        );
        let _ = fs::remove_dir_all(parent);
    }
    #[test]
    fn typed_references_and_immutable_attachments_survive_reopen() {
        let db = temp_workspace();
        let source = db.parent().unwrap().join("drawing.png");
        fs::write(&source, b"PNG demo fixture").unwrap();
        execute(
            &db,
            &[
                "note", "create", "work.md", "--id", "work", "--body", "# Work",
            ],
        )
        .unwrap();
        execute(
            &db,
            &[
                "note",
                "reference",
                "work",
                "project:demo",
                "--command-id",
                "reference-demo",
            ],
        )
        .unwrap();
        let before = execute(&db, &["note", "read", "work"]).unwrap();
        assert!(
            before["result"]["body"]
                .as_str()
                .unwrap()
                .contains("orbit://project:demo")
        );
        execute(
            &db,
            &[
                "note",
                "asset",
                "add",
                "work",
                source.to_str().unwrap(),
                "--label",
                "Diagram",
            ],
        )
        .unwrap();
        let assets = execute(&db, &["note", "asset", "list", "work"]).unwrap();
        let asset = assets["result"]["assets"][0].as_str().unwrap().to_string();
        assert!(asset.ends_with(".png"));
        let verified = execute(&db, &["note", "asset", "verify", "work"]).unwrap();
        assert_eq!(verified["result"]["verified"][0], asset);
        let export = db.parent().unwrap().join("export");
        execute(&db, &["note", "export-dir", export.to_str().unwrap()]).unwrap();
        assert!(export.join(".orbit-assets").join(&asset).is_file());
        execute(&db, &["note", "asset", "unlink", "work", &asset]).unwrap();
        let after = execute(&db, &["note", "asset", "list", "work"]).unwrap();
        assert!(after["result"]["assets"].as_array().unwrap().is_empty());
        execute(&db, &["note", "unreference", "work", "project:demo"]).unwrap();
        assert!(
            !execute(&db, &["note", "read", "work"]).unwrap()["result"]["body"]
                .as_str()
                .unwrap()
                .contains("orbit://project:demo")
        );
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn export_rejects_missing_attachment_before_creating_destination() {
        let db = temp_workspace();
        let missing = format!("{}.txt", "a".repeat(64));
        execute(
            &db,
            &[
                "note",
                "create",
                "note.md",
                "--id",
                "broken",
                "--body",
                &format!("[Missing](/.orbit-assets/{missing})"),
            ],
        )
        .unwrap();
        let destination = db.parent().unwrap().join("portable");
        assert!(execute(&db, &["note", "export-dir", destination.to_str().unwrap()]).is_err());
        assert!(!destination.exists());
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn export_rejects_corrupted_attachment_without_partial_folder() {
        let db = temp_workspace();
        let content = b"valid bytes";
        let asset = format!("{:x}.txt", Sha256::digest(content));
        execute(
            &db,
            &[
                "note",
                "create",
                "note.md",
                "--id",
                "corrupt",
                "--body",
                &format!("[File](/.orbit-assets/{asset})"),
            ],
        )
        .unwrap();
        let root = super::markdown_assets::asset_root(&db);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(&asset), b"corrupted bytes").unwrap();
        let destination = db.parent().unwrap().join("portable");
        assert!(execute(&db, &["note", "export-dir", destination.to_str().unwrap()]).is_err());
        assert!(!destination.exists());
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn imported_attachments_verify_hashes() {
        let db = temp_workspace();
        let folder = db.parent().unwrap().join("input");
        fs::create_dir_all(folder.join(".orbit-assets")).unwrap();
        let bytes = b"hello asset";
        let asset = format!("{:x}.txt", Sha256::digest(bytes));
        fs::write(folder.join(".orbit-assets").join(&asset), bytes).unwrap();
        fs::write(
            folder.join("a.md"),
            format!("---\norbit_id: imported-a\n---\n# A\n[document](/.orbit-assets/{asset})\n"),
        )
        .unwrap();
        execute(&db, &["note", "import-dir", folder.to_str().unwrap()]).unwrap();
        assert!(execute(&db, &["note", "asset", "verify", "imported-a"]).is_ok());
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn corrupt_import_assets_fail_before_creating_markdown_or_folders() {
        let db = temp_workspace();
        let folder = db.parent().unwrap().join("broken-import");
        fs::create_dir_all(folder.join(".orbit-assets")).unwrap();
        let broken = format!("{}.txt", "a".repeat(64));
        fs::write(
            folder.join(".orbit-assets").join(&broken),
            b"incorrect bytes",
        )
        .unwrap();
        fs::write(
            folder.join("good.md"),
            "---\norbit_id: valid-note\n---\n# Valid\n",
        )
        .unwrap();
        assert!(
            execute(
                &db,
                &[
                    "note",
                    "import-dir",
                    folder.to_str().unwrap(),
                    "--into",
                    "Incoming"
                ]
            )
            .is_err()
        );
        let notes = execute(&db, &["note", "list"]).unwrap();
        assert!(notes["result"].as_array().unwrap().is_empty());
        assert!(!db.with_extension("knowledge").join("Incoming").exists());
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn unavailable_import_references_fail_before_creating_notes() {
        let db = temp_workspace();
        let folder = db.parent().unwrap().join("missing-import-asset");
        fs::create_dir_all(&folder).unwrap();
        let missing = format!("{}.pdf", "a".repeat(64));
        fs::write(
            folder.join("note.md"),
            format!("---\norbit_id: reference-note\n---\n[Missing](/.orbit-assets/{missing})\n"),
        )
        .unwrap();
        assert!(execute(&db, &["note", "import-dir", folder.to_str().unwrap()]).is_err());
        assert!(
            execute(&db, &["note", "list"]).unwrap()["result"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn note_search_and_relations_use_physical_documents() {
        let db = temp_workspace();
        execute(
            &db,
            &[
                "note",
                "create",
                "a.md",
                "--id",
                "note-a",
                "--body",
                "# Alpha\n[[note-b]]",
            ],
        )
        .unwrap();
        execute(
            &db,
            &[
                "note",
                "create",
                "b.md",
                "--id",
                "note-b",
                "--body",
                "# Beta, snowflake mango",
            ],
        )
        .unwrap();
        let result = execute(&db, &["note", "search", "snowflake"]).unwrap();
        assert!(
            result["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["id"] == "note-b")
        );
        let links = execute(&db, &["note", "relations", "note-b"]).unwrap();
        assert!(
            links["result"]["backlinks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id == "note-a")
        );
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn import_export_round_trip_preserves_markdown_and_identity() {
        let db = temp_workspace();
        let parent = db.parent().unwrap();
        let source = parent.join("incoming");
        fs::create_dir_all(source.join("docs")).unwrap();
        fs::write(source.join("docs/one.md"), "# First\n").unwrap();
        fs::write(
            source.join("docs/two.md"),
            "---\norbit_id: persistent-two\n---\n# Second\n",
        )
        .unwrap();
        let source_str = source.to_str().unwrap();
        let result = execute(
            &db,
            &["note", "import-dir", source_str, "--into", "Imported"],
        )
        .unwrap();
        assert_eq!(result["result"]["imported"], 2);
        let retry = execute(
            &db,
            &["note", "import-dir", source_str, "--into", "Imported"],
        )
        .unwrap();
        assert_eq!(result, retry);
        let note = execute(&db, &["note", "read", "persistent-two"]).unwrap();
        assert_eq!(note["result"]["note"]["path"], "Imported/docs/two.md");
        let exported = parent.join("exported");
        let exported_str = exported.to_str().unwrap();
        let result = execute(&db, &["note", "export-dir", exported_str]).unwrap();
        assert_eq!(result["result"]["exported"], 2);
        assert_eq!(
            fs::read_to_string(exported.join("Imported/docs/two.md")).unwrap(),
            "---\norbit_id: persistent-two\n---\n# Second\n"
        );
        assert!(
            fs::read_to_string(exported.join("Imported/docs/one.md"))
                .unwrap()
                .contains("orbit_id: note-")
        );
        assert!(execute(&db, &["note", "export-dir", exported_str]).is_err());
        let _ = fs::remove_dir_all(parent);
    }
    #[test]
    fn metadata_and_links_are_persisted_as_markdown_and_rebuild_backlinks() {
        let db = temp_workspace();
        execute(
            &db,
            &[
                "note",
                "create",
                "decisions/a.md",
                "--id",
                "a",
                "--body",
                "# Decision",
            ],
        )
        .err();
        execute(
            &db,
            &[
                "note",
                "create",
                "a.md",
                "--id",
                "a",
                "--body",
                "# Decision",
            ],
        )
        .unwrap();
        execute(
            &db,
            &[
                "note",
                "create",
                "b.md",
                "--id",
                "b",
                "--body",
                "# Reference",
            ],
        )
        .unwrap();
        execute(&db, &["note", "kind", "a", "decision"]).unwrap();
        execute(&db, &["note", "status", "a", "archived"]).unwrap();
        execute(&db, &["note", "link", "a", "b"]).unwrap();
        let catalog = execute(
            &db,
            &[
                "note", "catalog", "--kind", "decision", "--status", "archived",
            ],
        )
        .unwrap();
        assert_eq!(catalog["result"].as_array().unwrap().len(), 1);
        let read = execute(&db, &["note", "read", "a"]).unwrap();
        let body = read["result"]["body"].as_str().unwrap();
        assert!(body.contains("orbit_kind: decision"));
        assert!(body.contains("orbit_status: archived"));
        assert!(body.contains("[[b]]"));
        let links = execute(&db, &["note", "relations", "b"]).unwrap();
        assert_eq!(links["result"]["backlinks"][0], "a");
        assert!(execute(&db, &["note", "link", "a", "missing"]).is_err());
        execute(&db, &["note", "unlink", "a", "b"]).unwrap();
        let links = execute(&db, &["note", "relations", "b"]).unwrap();
        assert!(links["result"]["backlinks"].as_array().unwrap().is_empty());
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
    #[test]
    fn import_rejects_duplicate_ids_before_modifying_vault() {
        let db = temp_workspace();
        let source = db.parent().unwrap().join("duplicates");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.md"), "---\norbit_id: repeated\n---\n# A").unwrap();
        fs::write(source.join("b.md"), "---\norbit_id: repeated\n---\n# B").unwrap();
        assert!(execute(&db, &["note", "import-dir", source.to_str().unwrap()]).is_err());
        assert!(
            execute(&db, &["note", "list"]).unwrap()["result"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let _ = fs::remove_dir_all(db.parent().unwrap());
    }
}
