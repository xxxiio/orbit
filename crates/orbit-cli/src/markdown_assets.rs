//! Portable, content-addressed vault attachments. The user-visible note is
//! always the authoritative reference; SQLite never stores attachment bodies.
use super::markdown_cli::{authority_response, knowledge_error};
use super::*;
use orbit_runtime::markdown_commands::MarkdownCommand;
use sha2::{Digest, Sha256};
use std::io::Write;

const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Subcommand)]
pub enum AssetCommand {
    /// Store a verified immutable attachment and link it into an existing note.
    Add {
        id: String,
        file: PathBuf,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        command_id: Option<String>,
    },
    /// List physical attachment links from the Markdown body.
    List { id: String },
    /// Verify every referenced attachment against its immutable digest.
    Verify { id: String },
    /// Remove a note link while preserving shared immutable content.
    Unlink {
        id: String,
        asset: String,
        #[arg(long)]
        command_id: Option<String>,
    },
}
fn asset_error(message: impl Into<String>) -> CliError {
    CliError::Usage(message.into())
}
pub(super) fn asset_root(db: &Path) -> PathBuf {
    db.with_extension("knowledge").join(".orbit-assets")
}
pub(super) fn safe_basename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && name != "."
        && name != ".."
        && !name.starts_with('.')
}
pub(super) fn is_content_name(name: &str) -> bool {
    safe_basename(name)
        && (name.len() == 64 || (name.len() > 65 && name.as_bytes()[64] == b'.'))
        && name.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit)
        && name[64..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.')
}
fn content_name(bytes: &[u8], original: &Path) -> Result<String, CliError> {
    let name = original
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| asset_error("non-UTF8 file name"))?;
    if !safe_basename(name) {
        return Err(asset_error("unsafe attachment filename"));
    }
    let extension = original
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension.len() > 12 || !extension.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(asset_error("unsupported extension"));
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    Ok(if extension.is_empty() {
        digest
    } else {
        format!("{digest}.{extension}")
    })
}
pub(super) fn validate_asset_file(file: &Path) -> Result<Vec<u8>, CliError> {
    let metadata = fs::symlink_metadata(file).map_err(|e| CliError::Io(e.to_string()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_ASSET_BYTES
    {
        return Err(asset_error(
            "attachment must be a regular non-symlink file <= 64 MiB",
        ));
    }
    let bytes = fs::read(file).map_err(|e| CliError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(asset_error("attachment too large"));
    }
    Ok(bytes)
}
fn ensure_assets(root: &Path) -> Result<(), CliError> {
    if root.exists() {
        let kind = fs::symlink_metadata(root).map_err(|e| CliError::Io(e.to_string()))?;
        if !kind.is_dir() || kind.file_type().is_symlink() {
            return Err(asset_error("unsafe asset directory"));
        }
    } else {
        fs::create_dir(root).map_err(|e| CliError::Io(e.to_string()))?;
    }
    Ok(())
}
/// Publish an attachment using a synchronized private staging file and an
/// atomic no-clobber hard link. Readers never observe partially written asset
/// bytes; a crash before the note receipt may leave a harmless orphaned blob.
fn publish_asset(root: &Path, name: &str, bytes: &[u8]) -> Result<(), CliError> {
    if !is_content_name(name) {
        return Err(asset_error("unsafe content-addressed asset name"));
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    if !name.starts_with(&digest) {
        return Err(asset_error("attachment name does not match content"));
    }
    ensure_assets(root)?;
    let dest = root.join(name);
    // Validate symlinks too: `exists` returns false for dangling symlinks.
    match fs::symlink_metadata(&dest) {
        Ok(_) => {
            if validate_asset_file(&dest)? == bytes {
                return Ok(());
            }
            return Err(asset_error("immutable asset digest collision"));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(CliError::Io(e.to_string())),
    }
    let mut staged = None;
    for i in 0..32u32 {
        let candidate = root.join(format!(
            ".orbit-asset-stage-{}-{}-{i}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| CliError::Io(e.to_string()))?
                .as_nanos()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut f) => {
                if let Err(e) = f.write_all(bytes).and_then(|_| f.sync_all()) {
                    let _ = fs::remove_file(&candidate);
                    return Err(CliError::Io(e.to_string()));
                }
                staged = Some(candidate);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(CliError::Io(e.to_string())),
        }
    }
    let stage = staged.ok_or_else(|| asset_error("could not allocate attachment staging file"))?;
    let result = (|| -> Result<(), CliError> {
        match fs::hard_link(&stage, &dest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if validate_asset_file(&dest)? != bytes {
                    return Err(asset_error("immutable asset collision"));
                }
            }
            Err(e) => return Err(CliError::Io(e.to_string())),
        }
        fs::File::open(root)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| CliError::Io(e.to_string()))?;
        Ok(())
    })();
    let _ = fs::remove_file(&stage);
    result
}
pub(super) fn linked_assets(body: &str) -> Vec<String> {
    let mut found = std::collections::BTreeSet::new();
    for section in body.split("/.orbit-assets/").skip(1) {
        let filename: String = section
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            .collect();
        if safe_basename(&filename)
            && filename.len() >= 64
            && filename.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit)
        {
            found.insert(filename);
        }
    }
    found.into_iter().collect()
}
pub fn run_asset_command(
    host: &mut orbit_runtime::AuthoritativeHost,
    db: &Path,
    cmd: AssetCommand,
) -> Result<Response, CliError> {
    match cmd {
        AssetCommand::List { id } => {
            let (_, body) = host
                .markdown_by_id(&id)
                .map_err(knowledge_error)?
                .ok_or_else(|| asset_error("unknown note identity"))?;
            authority_response(serde_json::json!({"id":id,"assets":linked_assets(&body)}))
        }
        AssetCommand::Verify { id } => {
            let (_, body) = host
                .markdown_by_id(&id)
                .map_err(knowledge_error)?
                .ok_or_else(|| asset_error("unknown note identity"))?;
            let mut verified = Vec::new();
            for name in linked_assets(&body) {
                let bytes = validate_asset_file(&asset_root(db).join(&name))?;
                let hash = format!("{:x}", Sha256::digest(&bytes));
                if !name.starts_with(&hash) {
                    return Err(asset_error(format!("asset hash mismatch: {name}")));
                }
                verified.push(name);
            }
            authority_response(serde_json::json!({"id":id,"verified":verified}))
        }
        AssetCommand::Unlink {
            id,
            asset,
            command_id,
        } => {
            if !safe_basename(&asset) {
                return Err(asset_error("invalid asset name"));
            }
            let (_, before) = host
                .markdown_by_id(&id)
                .map_err(knowledge_error)?
                .ok_or_else(|| asset_error("unknown note identity"))?;
            if !linked_assets(&before).contains(&asset) {
                return Err(asset_error("asset is not linked to this note"));
            }
            // Remove the exact Markdown link, not arbitrary matching prose.
            let mut after = String::new();
            for line in before.lines() {
                let is_reference = line.trim().starts_with('[')
                    && line.contains(&format!("](/.orbit-assets/{asset})"));
                if !is_reference {
                    after.push_str(line);
                    after.push('\n');
                }
            }
            if after == before {
                return Err(asset_error(
                    "linked asset did not occur as a standalone reference",
                ));
            }
            let receipt = host
                .execute_markdown_command(
                    "orbit-cli",
                    &command_id.unwrap_or_else(|| generate_id("command")),
                    MarkdownCommand::Update {
                        note_id: id,
                        expected: before,
                        replacement: after,
                    },
                )
                .map_err(knowledge_error)?;
            authority_response(serde_json::json!({"asset":asset,"receipt":receipt}))
        }
        AssetCommand::Add {
            id,
            file,
            label,
            command_id,
        } => {
            let (_, before) = host
                .markdown_by_id(&id)
                .map_err(knowledge_error)?
                .ok_or_else(|| asset_error("unknown note identity"))?;
            let bytes = validate_asset_file(&file)?;
            let name = content_name(&bytes, &file)?;
            let text = label.unwrap_or_else(|| {
                file.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("attachment")
                    .to_string()
            });
            if text.is_empty()
                || text.len() > 200
                || text.chars().any(|c| matches!(c, '\n' | '\r' | ']' | '['))
            {
                return Err(asset_error("invalid attachment label"));
            }
            let link = format!("[{}](/.orbit-assets/{})", text, name);
            if before.contains(&link) {
                // Verify the bytes even for a previously linked asset.
                if validate_asset_file(&asset_root(db).join(&name))? != bytes {
                    return Err(asset_error("linked asset bytes mismatch"));
                }
                return authority_response(
                    serde_json::json!({"id":id,"asset":name,"changed":false}),
                );
            }
            publish_asset(&asset_root(db), &name, &bytes)?;
            let mut replacement = before.clone();
            if !replacement.ends_with('\n') {
                replacement.push('\n');
            }
            replacement.push('\n');
            replacement.push_str(&link);
            replacement.push('\n');
            let receipt = host
                .execute_markdown_command(
                    "orbit-cli",
                    &command_id.unwrap_or_else(|| generate_id("command")),
                    MarkdownCommand::Update {
                        note_id: id,
                        expected: before,
                        replacement,
                    },
                )
                .map_err(knowledge_error)?;
            authority_response(serde_json::json!({"asset":name,"receipt":receipt}))
        }
    }
}
/// Verify all attachment references before any portable export is published.
/// A package must never be presented as complete with missing blobs.
pub(super) fn preflight_export_references(db: &Path, bodies: &[&str]) -> Result<(), CliError> {
    let mut names = std::collections::BTreeSet::new();
    for body in bodies {
        names.extend(linked_assets(body));
    }
    let root = asset_root(db);
    for name in names {
        if !is_content_name(&name) {
            return Err(asset_error("unsafe referenced attachment name"));
        }
        let bytes = validate_asset_file(&root.join(&name))?;
        if !name.starts_with(&format!("{:x}", Sha256::digest(&bytes))) {
            return Err(asset_error(format!(
                "corrupt attachment blocks export: {name}"
            )));
        }
    }
    Ok(())
}

/// Export only assets actually referenced by authoritative Markdown notes, not
/// arbitrary files left behind by interrupted attachment publication.
pub fn export_asset_directory(
    host: &orbit_runtime::AuthoritativeHost,
    db: &Path,
    destination: &Path,
) -> Result<usize, CliError> {
    let root = asset_root(db);
    if !root.exists() {
        return Ok(0);
    }
    ensure_assets(&root)?;
    let mut references = std::collections::BTreeSet::new();
    for note in host.markdown_knowledge_index().map_err(knowledge_error)? {
        let (_, body) = host
            .markdown_by_id(&note.id)
            .map_err(knowledge_error)?
            .ok_or_else(|| asset_error("indexed Markdown note disappeared"))?;
        references.extend(linked_assets(&body));
    }
    // Verify the complete export set before writing a single output file.
    let mut verified = Vec::new();
    for name in references {
        let bytes = validate_asset_file(&root.join(&name))?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if !name.starts_with(&hash) {
            return Err(asset_error("referenced asset digest mismatch"));
        }
        verified.push((name, bytes));
    }
    let count = verified.len();
    if !verified.is_empty() {
        let folder = destination.join(".orbit-assets");
        fs::create_dir_all(&folder).map_err(|e| CliError::Io(e.to_string()))?;
        for (name, bytes) in verified {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(folder.join(name))
                .map_err(|e| CliError::Io(e.to_string()))?;
            output
                .write_all(&bytes)
                .and_then(|_| output.sync_all())
                .map_err(|e| CliError::Io(e.to_string()))?;
        }
        fs::File::open(&folder)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| CliError::Io(e.to_string()))?;
    }
    Ok(count)
}
/// Reject untrusted directories and verify content digests before copying. This
/// is a resumable import: Markdown and attachment publication are not one tx.
/// Inspect an import's entire asset set without writing to the vault.
/// This must run before any Markdown or folder publication.
pub(super) fn preflight_import_assets(
    source: &Path,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>, CliError> {
    let from = source.join(".orbit-assets");
    match fs::symlink_metadata(&from) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(CliError::Io(e.to_string())),
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(asset_error("unsafe import asset directory"));
        }
        Ok(_) => {}
    }
    let mut verified = std::collections::BTreeMap::new();
    for entry in fs::read_dir(&from).map_err(|e| CliError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| CliError::Io(e.to_string()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| asset_error("non-UTF8 asset name"))?;
        if !is_content_name(&name) {
            return Err(asset_error("invalid imported asset name"));
        }
        let bytes = validate_asset_file(&entry.path())?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if !name.starts_with(&hash) {
            return Err(asset_error("imported asset hash mismatch"));
        }
        verified.insert(name, bytes);
    }
    Ok(verified)
}

/// An existing valid asset in this vault can satisfy a Markdown reference even
/// if a portable import does not carry a duplicate blob. Reject missing or
/// corrupted references before publishing any of the incoming notes.
pub(super) fn preflight_import_references(
    db: &Path,
    notes: &[String],
    incoming: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<(), CliError> {
    let mut needed = std::collections::BTreeSet::new();
    for note in notes {
        needed.extend(linked_assets(note));
    }
    for name in needed {
        if !is_content_name(&name) {
            return Err(asset_error("invalid asset reference in imported note"));
        }
        if incoming.contains_key(&name) {
            continue;
        }
        let path = asset_root(db).join(&name);
        let bytes = validate_asset_file(&path)
            .map_err(|_| asset_error(format!("unavailable referenced attachment: {name}")))?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if !name.starts_with(&hash) {
            return Err(asset_error(format!(
                "corrupt referenced attachment: {name}"
            )));
        }
    }
    Ok(())
}

/// Publish a previously verified batch. A resumed import can safely retry.
pub(super) fn publish_import_assets(
    db: &Path,
    verified: std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<usize, CliError> {
    let count = verified.len();
    for (name, bytes) in verified {
        publish_asset(&asset_root(db), &name, &bytes)?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_asset_names() {
        assert!(!safe_basename("../secret"));
        assert!(!safe_basename(".hidden"));
        assert!(!safe_basename("some path.png"));
    }
    #[test]
    fn atomic_publication_never_overwrites_an_existing_asset() {
        let dir = std::env::temp_dir().join(format!(
            "orbit-asset-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let digest = format!("{:x}", Sha256::digest(b"test"));
        publish_asset(&dir, &format!("{digest}.txt"), b"test").unwrap();
        publish_asset(&dir, &format!("{digest}.txt"), b"test").unwrap();
        assert_eq!(
            fs::read(dir.join(format!("{digest}.txt"))).unwrap(),
            b"test"
        );
        assert!(publish_asset(&dir, &format!("{digest}.txt"), b"altered").is_err());
        assert!(fs::read_dir(&dir).unwrap().all(|x| {
            !x.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".orbit-asset-stage-")
        }));
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn preflight_rejects_corrupted_package_without_destination_mutation() {
        let root = std::env::temp_dir().join(format!(
            "orbit-import-preflight-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let assets = source.join(".orbit-assets");
        fs::create_dir_all(&assets).unwrap();
        let invalid_name = format!("{}.txt", "a".repeat(64));
        fs::write(assets.join(&invalid_name), b"wrong bytes").unwrap();
        assert!(preflight_import_assets(&source).is_err());
        assert!(!root.join("destination.knowledge").exists());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn preflight_checks_missing_note_asset_refs() {
        let root = std::env::temp_dir().join(format!(
            "orbit-import-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let reference = format!("[Attachment](/.orbit-assets/{}.pdf)", "a".repeat(64));
        let notes = vec![reference];
        assert!(preflight_import_references(&root, &notes, &Default::default()).is_err());
        assert!(!asset_root(&root).exists());
    }
    #[test]
    fn preflight_can_resolve_valid_content_from_import_package() {
        let root = std::env::temp_dir().join(format!(
            "orbit-import-valid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let content = b"portable attachment";
        let name = format!("{:x}.txt", Sha256::digest(content));
        let mut assets = std::collections::BTreeMap::new();
        assets.insert(name.clone(), content.to_vec());
        let notes = vec![format!("[Attachment](/.orbit-assets/{name})")];
        preflight_import_references(&root, &notes, &assets).unwrap();
        assert!(!asset_root(&root).exists());
    }
    #[test]
    fn detects_embedded_links() {
        let hash = "a".repeat(64);
        let body = format!("[file](/.orbit-assets/{hash}.pdf)");
        assert_eq!(linked_assets(&body), vec![format!("{hash}.pdf")]);
    }
}
