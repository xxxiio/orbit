//! Managed, content-addressed attachment object store.
//!
//! Content bytes live outside SQLite and are addressed by their lowercase
//! SHA-256 digest. The caller-supplied display filename is never used to build
//! a path. Objects are staged and then atomically renamed into place so a crash
//! can only leave an unreferenced object for later garbage collection.
use std::fmt::Write as _;
use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use orbit_application::{AttachmentContentStore, AttachmentStoreError, StoredAttachmentContent};
use sha2::{Digest, Sha256};

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Filesystem-backed attachment content store.
///
/// Layout:
/// - `<root>/<first two digest chars>/<digest>` for immutable objects
/// - `<root>/.staging/<unique>` for in-progress writes
pub struct AttachmentObjectStore {
    root: PathBuf,
    max_bytes: u64,
}

impl AttachmentObjectStore {
    pub fn open(root: impl Into<PathBuf>, max_bytes: u64) -> Result<Self, AttachmentStoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(io_error)?;
        let staging = root.join(".staging");
        fs::create_dir_all(&staging).map_err(io_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
            fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
        }
        Ok(Self { root, max_bytes })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn object_path(&self, object: &str) -> Result<PathBuf, AttachmentStoreError> {
        validate_object_id(object)?;
        Ok(self.root.join(&object[..2]).join(object))
    }
}

impl AttachmentContentStore for AttachmentObjectStore {
    fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    fn store(&mut self, content: &[u8]) -> Result<StoredAttachmentContent, AttachmentStoreError> {
        if content.len() as u64 > self.max_bytes {
            return Err(AttachmentStoreError::TooLarge {
                max_bytes: self.max_bytes,
            });
        }
        let object = hex_digest(content);
        let path = self.object_path(&object)?;
        let stored = StoredAttachmentContent {
            object: object.clone(),
            checksum: object,
            size_bytes: content.len() as u64,
            created_at_ms: now_ms(),
        };

        // Content addressing makes an existing object with the same digest a
        // successful deduplicated write. Never overwrite it.
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                verify_existing_object(&metadata, content.len() as u64)?;
                return Ok(stored);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }

        let bucket = path
            .parent()
            .ok_or_else(|| AttachmentStoreError::Io("invalid object path".to_owned()))?;
        fs::create_dir_all(bucket).map_err(io_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(bucket, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
        }

        let staging = self.root.join(".staging").join(staging_name());
        let write_result = write_staging(&staging, content);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&staging);
            return Err(io_error(error));
        }

        // Re-check after staging so a concurrent writer's identical object is
        // never overwritten by the rename below.
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                let verification = verify_existing_object(&metadata, content.len() as u64);
                let _ = fs::remove_file(&staging);
                verification?;
                return Ok(stored);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(io_error(error));
            }
        }

        // The staging file and destination share a filesystem, so rename is
        // atomic. A concurrently created identical object is harmless.
        if let Err(error) = fs::rename(&staging, &path) {
            let _ = fs::remove_file(&staging);
            return Err(io_error(error));
        }
        Ok(stored)
    }

    fn read(&self, object: &str) -> Result<Vec<u8>, AttachmentStoreError> {
        let path = self.object_path(object)?;
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AttachmentStoreError::Io(
                "object path is not a regular file".to_owned(),
            ));
        }
        let content = fs::read(&path).map_err(io_error)?;
        if content.len() as u64 != metadata.len() {
            return Err(AttachmentStoreError::Io("object size mismatch".to_owned()));
        }
        if hex_digest(&content) != object {
            return Err(AttachmentStoreError::Io(
                "object checksum mismatch".to_owned(),
            ));
        }
        Ok(content)
    }

    fn contains(&self, object: &str) -> Result<bool, AttachmentStoreError> {
        let path = self.object_path(object)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => Ok(metadata.is_file() && !metadata.file_type().is_symlink()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(io_error(error)),
        }
    }

    fn remove(&mut self, object: &str) -> Result<bool, AttachmentStoreError> {
        let path = self.object_path(object)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Ok(false);
                }
                fs::remove_file(&path).map_err(io_error)?;
                Ok(true)
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(io_error(error)),
        }
    }
}

fn write_staging(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(content)?;
    file.sync_all()
}

fn verify_existing_object(
    metadata: &fs::Metadata,
    expected_size: u64,
) -> Result<(), AttachmentStoreError> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AttachmentStoreError::Io(
            "object path is not a regular file".to_owned(),
        ));
    }
    if metadata.len() != expected_size {
        return Err(AttachmentStoreError::Io("object size mismatch".to_owned()));
    }
    Ok(())
}

fn validate_object_id(object: &str) -> Result<(), AttachmentStoreError> {
    if object.len() == 64
        && object
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(AttachmentStoreError::Io("invalid object id".to_owned()))
    }
}

fn hex_digest(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    let mut value = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn staging_name() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn io_error(error: std::io::Error) -> AttachmentStoreError {
    AttachmentStoreError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn scratch_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "orbit-attachment-store-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn is_invalid_object_id(error: AttachmentStoreError) -> bool {
        matches!(error, AttachmentStoreError::Io(detail) if detail == "invalid object id")
    }

    #[test]
    fn store_and_read_roundtrip_is_content_addressed() {
        let root = scratch_root("roundtrip");
        let mut store = AttachmentObjectStore::open(&root, 1024).unwrap();
        let stored = store.store(b"hello attachment").unwrap();
        assert_eq!(stored.size_bytes, 16);
        assert_eq!(stored.object, stored.checksum);
        assert_eq!(stored.object.len(), 64);
        assert!(store.contains(&stored.object).unwrap());
        assert_eq!(store.read(&stored.object).unwrap(), b"hello attachment");
        assert_eq!(
            store.root().join(&stored.object[..2]).join(&stored.object),
            root.join(&stored.object[..2]).join(&stored.object)
        );
        assert!(
            root.join(&stored.object[..2])
                .join(&stored.object)
                .is_file()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn identical_content_is_deduplicated() {
        let root = scratch_root("dedup");
        let mut store = AttachmentObjectStore::open(&root, 1024).unwrap();
        let first = store.store(b"same bytes").unwrap();
        let second = store.store(b"same bytes").unwrap();
        assert_eq!(first.object, second.object);
        let objects = fs::read_dir(root.join(&first.object[..2]))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(objects.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oversize_content_is_rejected_before_writing() {
        let root = scratch_root("oversize");
        let mut store = AttachmentObjectStore::open(&root, 4).unwrap();
        let error = store.store(b"12345").unwrap_err();
        assert_eq!(error, AttachmentStoreError::TooLarge { max_bytes: 4 });
        assert!(!root.join(".staging").read_dir().unwrap().any(|_| true));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_object_ids_are_rejected_on_every_operation() {
        let root = scratch_root("invalid");
        let mut store = AttachmentObjectStore::open(&root, 1024).unwrap();
        for object in ["", "abc", "../escape", &"A".repeat(64), &"g".repeat(64)] {
            assert!(is_invalid_object_id(store.read(object).unwrap_err()));
            assert!(is_invalid_object_id(store.contains(object).unwrap_err()));
            assert!(is_invalid_object_id(store.remove(object).unwrap_err()));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_objects_are_never_read_or_removed() {
        use std::os::unix::fs::symlink;

        let root = scratch_root("symlink");
        let mut store = AttachmentObjectStore::open(&root, 1024).unwrap();
        let stored = store.store(b"real content").unwrap();
        let object_path = root.join(&stored.object[..2]).join(&stored.object);
        let outside = root.join("outside.txt");
        fs::write(&outside, b"real content").unwrap();
        fs::remove_file(&object_path).unwrap();
        symlink(&outside, &object_path).unwrap();

        assert!(store.read(&stored.object).is_err());
        assert!(!store.contains(&stored.object).unwrap());
        assert!(!store.remove(&stored.object).unwrap());
        assert!(
            object_path
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remove_deletes_only_the_regular_object() {
        let root = scratch_root("remove");
        let mut store = AttachmentObjectStore::open(&root, 1024).unwrap();
        let stored = store.store(b"temporary").unwrap();
        assert!(store.remove(&stored.object).unwrap());
        assert!(!store.contains(&stored.object).unwrap());
        assert!(store.read(&stored.object).is_err());
        assert!(!store.remove(&stored.object).unwrap());
        let _ = fs::remove_dir_all(root);
    }
}
