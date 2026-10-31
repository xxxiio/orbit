//! Minimal POSIX ustar archive reader and writer.
//!
//! The project builds offline and intentionally adds no archive crate, so the
//! `.orbitbak` container uses the POSIX ustar format implemented here. Only
//! regular files are written: the knowledge vault's empty directories are
//! described in the manifest and recreated on restore. Headers are
//! deterministic (uid/gid 0, mtime 0, mode 0644), which keeps archives with
//! identical content byte-identical.
//!
//! This is a container implementation, not a general-purpose tar library. It
//! rejects absolute/traversing paths and unsupported entry types instead of
//! guessing.

use std::io::{Read, Write};

use crate::error::{BackupError, Result};
use crate::util::is_safe_relative_path;

/// Fixed ustar block size.
pub const BLOCK_SIZE: usize = 512;
const NAME_LEN: usize = 100;
const PREFIX_LEN: usize = 155;
const CHECKSUM_OFFSET: usize = 148;
const CHECKSUM_LEN: usize = 8;
const TYPE_OFFSET: usize = 156;
const MAGIC_OFFSET: usize = 257;
const MAGIC: &[u8; 6] = b"ustar\0";
const VERSION: &[u8; 2] = b"00";

/// Streams regular-file entries into a ustar archive.
pub struct TarWriter<W: Write> {
    inner: W,
    finished: bool,
    archived_bytes: u64,
}

impl<W: Write> TarWriter<W> {
    /// Wrap a sink (normally a buffered file) as a tar writer.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            finished: false,
            archived_bytes: 0,
        }
    }

    /// Bytes written so far, including headers, padding and the trailer once
    /// [`TarWriter::finish`] has been called.
    pub fn archived_bytes(&self) -> u64 {
        self.archived_bytes
    }

    /// Append one regular file from a reader that yields exactly `size` bytes.
    pub fn append_file(&mut self, path: &str, size: u64, reader: &mut dyn Read) -> Result<()> {
        self.write_header(path, size)?;
        let mut copied = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        while copied < size {
            let remaining = usize::try_from((size - copied).min(buffer.len() as u64))
                .map_err(|_| BackupError::Internal("archive file size overflow".to_owned()))?;
            let read = reader
                .read(&mut buffer[..remaining])
                .map_err(|error| BackupError::io(format!("read staged file for {path}"), error))?;
            if read == 0 {
                return Err(BackupError::Integrity(format!(
                    "staged file {path} ended after {copied} of {size} bytes"
                )));
            }
            self.inner
                .write_all(&buffer[..read])
                .map_err(|error| BackupError::io("write archive content", error))?;
            copied += read as u64;
        }
        self.archived_bytes += size + padding_for(size) as u64;
        self.write_padding(size)?;
        Ok(())
    }

    /// Append one regular file from an in-memory byte slice.
    pub fn append_bytes(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let mut reader = data;
        self.append_file(path, data.len() as u64, &mut reader)
    }

    /// Write the end-of-archive trailer and flush the sink.
    pub fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Err(BackupError::Internal(
                "archive trailer written twice".to_owned(),
            ));
        }
        self.inner
            .write_all(&[0_u8; BLOCK_SIZE * 2])
            .map_err(|error| BackupError::io("write archive trailer", error))?;
        self.archived_bytes += (BLOCK_SIZE * 2) as u64;
        self.inner
            .flush()
            .map_err(|error| BackupError::io("flush archive", error))?;
        self.finished = true;
        Ok(())
    }

    fn write_header(&mut self, path: &str, size: u64) -> Result<()> {
        let (name, prefix) = split_archive_path(path)?;
        let mut header = [0_u8; BLOCK_SIZE];
        header[..name.len()].copy_from_slice(name);
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        let size_text = format!("{size:011o}\0");
        header[124..136].copy_from_slice(size_text.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN].copy_from_slice(b"        ");
        header[TYPE_OFFSET] = b'0';
        header[MAGIC_OFFSET..MAGIC_OFFSET + MAGIC.len()].copy_from_slice(MAGIC);
        header[MAGIC_OFFSET + MAGIC.len()..MAGIC_OFFSET + MAGIC.len() + VERSION.len()]
            .copy_from_slice(VERSION);
        header[265..269].copy_from_slice(b"root");
        header[297..301].copy_from_slice(b"root");
        if let Some(prefix) = prefix {
            header[PREFIX_LEN_FIELD..PREFIX_LEN_FIELD + prefix.len()].copy_from_slice(prefix);
        }
        let checksum = header.iter().map(|byte| u64::from(*byte)).sum::<u64>();
        let checksum_text = format!("{checksum:06o}\0 ");
        header[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN]
            .copy_from_slice(checksum_text.as_bytes());
        self.inner
            .write_all(&header)
            .map_err(|error| BackupError::io("write archive header", error))?;
        self.archived_bytes += BLOCK_SIZE as u64;
        Ok(())
    }

    fn write_padding(&mut self, size: u64) -> Result<()> {
        let padding = padding_for(size);
        if padding > 0 {
            let pad = [0_u8; BLOCK_SIZE];
            self.inner
                .write_all(&pad[..padding])
                .map_err(|error| BackupError::io("write archive padding", error))?;
        }
        Ok(())
    }
}

const PREFIX_LEN_FIELD: usize = 345;

fn padding_for(size: u64) -> usize {
    let remainder = (size % BLOCK_SIZE as u64) as usize;
    if remainder == 0 {
        0
    } else {
        BLOCK_SIZE - remainder
    }
}

/// Split a relative archive path into ustar `name` and optional `prefix` fields.
fn split_archive_path(path: &str) -> Result<(&[u8], Option<&[u8]>)> {
    if !is_safe_relative_path(path) {
        return Err(BackupError::UnsafePath(path.to_owned()));
    }
    if path.len() <= NAME_LEN {
        return Ok((path.as_bytes(), None));
    }
    // Prefer the last separator that keeps both fields within their limits.
    for (index, _) in path.match_indices('/').rev() {
        let prefix = &path[..index];
        let name = &path[index + 1..];
        if name.is_empty() || prefix.is_empty() {
            continue;
        }
        if name.len() <= NAME_LEN && prefix.len() <= PREFIX_LEN {
            return Ok((name.as_bytes(), Some(prefix.as_bytes())));
        }
    }
    Err(BackupError::InvalidArgument(format!(
        "archive path exceeds the ustar path limits: {path}"
    )))
}

/// One entry header read from an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarHeader {
    /// Relative slash-separated path, exactly as stored.
    pub path: String,
    /// Declared content size in bytes.
    pub size: u64,
    /// Whether the entry is a directory (written by other tools, not ours).
    pub is_directory: bool,
}

/// Sequential ustar archive reader.
pub struct TarReader<R: Read> {
    inner: R,
    finished: bool,
    position: u64,
}

impl<R: Read> TarReader<R> {
    /// Wrap a source as a tar reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            finished: false,
            position: 0,
        }
    }

    /// Read the next entry header, or `None` at the end-of-archive marker.
    pub fn next_header(&mut self) -> Result<Option<TarHeader>> {
        if self.finished {
            return Ok(None);
        }
        let mut block = [0_u8; BLOCK_SIZE];
        self.read_exact(&mut block, "entry header")?;
        self.position += BLOCK_SIZE as u64;
        if block.iter().all(|byte| *byte == 0) {
            // Consume the second trailer block when present; tolerate its
            // absence because some writers only emit one marker.
            let mut second = [0_u8; BLOCK_SIZE];
            let _ = self.inner.read_exact(&mut second);
            self.finished = true;
            return Ok(None);
        }
        if &block[MAGIC_OFFSET..MAGIC_OFFSET + MAGIC.len()] != MAGIC {
            return Err(BackupError::Archive(
                "entry is not a POSIX ustar header (bad magic)".to_owned(),
            ));
        }
        let stored_checksum = parse_octal(
            &block[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN],
            "checksum",
        )?;
        let mut checksum_header = block;
        checksum_header[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN]
            .copy_from_slice(b"        ");
        let actual_checksum = checksum_header
            .iter()
            .map(|byte| u64::from(*byte))
            .sum::<u64>();
        if stored_checksum != actual_checksum {
            return Err(BackupError::Archive(format!(
                "tar header checksum mismatch (stored {stored_checksum}, computed {actual_checksum})"
            )));
        }
        let entry_type = block[TYPE_OFFSET];
        let is_directory = match entry_type {
            b'0' | b'\0' => false,
            b'5' => true,
            other => {
                return Err(BackupError::Archive(format!(
                    "unsupported tar entry type {:?}; `.orbitbak` archives contain regular files only",
                    other as char
                )));
            }
        };
        let size = parse_octal(&block[124..136], "size")?;
        let name = field_text(&block[..NAME_LEN]);
        let prefix = field_text(&block[PREFIX_LEN_FIELD..PREFIX_LEN_FIELD + PREFIX_LEN]);
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if !is_safe_relative_path(&path) {
            return Err(BackupError::UnsafePath(path));
        }
        if is_directory && size != 0 {
            return Err(BackupError::Archive(format!(
                "directory entry {path} declares non-zero size"
            )));
        }
        self.position += size + padding_for(size) as u64;
        Ok(Some(TarHeader {
            path,
            size,
            is_directory,
        }))
    }

    /// Copy exactly the current entry's content into `writer`.
    pub fn copy_data(&mut self, size: u64, writer: &mut dyn Write) -> Result<()> {
        let mut remaining = size;
        let mut buffer = [0_u8; 64 * 1024];
        while remaining > 0 {
            let chunk = chunk_len(remaining, buffer.len())?;
            self.read_exact(&mut buffer[..chunk], "entry content")?;
            writer
                .write_all(&buffer[..chunk])
                .map_err(|error| BackupError::io("write extracted content", error))?;
            remaining -= chunk as u64;
        }
        self.skip_padding(size)
    }

    /// Hash exactly the current entry's content without buffering it.
    pub fn hash_data(&mut self, size: u64) -> Result<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        let mut remaining = size;
        let mut buffer = [0_u8; 64 * 1024];
        while remaining > 0 {
            let chunk = chunk_len(remaining, buffer.len())?;
            self.read_exact(&mut buffer[..chunk], "entry content")?;
            hasher.update(&buffer[..chunk]);
            remaining -= chunk as u64;
        }
        self.skip_padding(size)?;
        Ok(crate::util::hex_digest(hasher.finalize()))
    }

    /// Skip the current entry's content without reading it into memory.
    pub fn skip_data(&mut self, size: u64) -> Result<()> {
        let mut remaining = size;
        let mut buffer = [0_u8; 8 * 1024];
        while remaining > 0 {
            let chunk = chunk_len(remaining, buffer.len())?;
            self.read_exact(&mut buffer[..chunk], "entry content")?;
            remaining -= chunk as u64;
        }
        self.skip_padding(size)
    }

    fn skip_padding(&mut self, size: u64) -> Result<()> {
        let padding = padding_for(size);
        if padding > 0 {
            let mut buffer = [0_u8; BLOCK_SIZE];
            self.read_exact(&mut buffer[..padding], "entry padding")?;
        }
        Ok(())
    }

    fn read_exact(&mut self, buffer: &mut [u8], what: &str) -> Result<()> {
        self.inner.read_exact(buffer).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                BackupError::Archive(format!(
                    "archive is truncated while reading {what} at byte {}",
                    self.position
                ))
            } else {
                BackupError::io(format!("read archive {what}"), error)
            }
        })
    }
}

fn field_text(field: &[u8]) -> String {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn parse_octal(field: &[u8], what: &str) -> Result<u64> {
    let text = field_text(field);
    let trimmed = text.trim_matches(|character: char| character == ' ' || character == '\0');
    if trimmed.is_empty() {
        return Ok(0);
    }
    if !trimmed.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
        return Err(BackupError::Archive(format!(
            "invalid octal {what} field {:?}",
            trimmed
        )));
    }
    u64::from_str_radix(trimmed, 8)
        .map_err(|_| BackupError::Archive(format!("invalid octal {what} field {trimmed:?}")))
}

fn chunk_len(remaining: u64, max: usize) -> Result<usize> {
    usize::try_from(remaining.min(max as u64))
        .map_err(|_| BackupError::Internal("archive chunk size overflow".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_reader(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        {
            let mut writer = TarWriter::new(&mut archive);
            for (path, data) in files {
                writer.append_bytes(path, data).unwrap();
            }
            writer.finish().unwrap();
        }
        archive
    }

    fn read_all(archive: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut reader = TarReader::new(archive);
        let mut entries = Vec::new();
        while let Some(header) = reader.next_header().unwrap() {
            let mut data = Vec::new();
            reader.copy_data(header.size, &mut data).unwrap();
            entries.push((header.path, data));
        }
        entries
    }

    #[test]
    fn multiple_files_round_trip() {
        let archive = write_reader(&[
            ("manifest.json", b"{\"format_version\":1}"),
            ("database/orbit.db", b"sqlite-bytes"),
            ("knowledge/Work/one.md", b"# One"),
        ]);
        let entries = read_all(&archive);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1].0, "database/orbit.db");
        assert_eq!(entries[1].1, b"sqlite-bytes");
        assert_eq!(entries[2].0, "knowledge/Work/one.md");
        assert_eq!(entries[2].1, b"# One");
    }

    #[test]
    fn archive_is_a_multiple_of_the_block_size() {
        let archive = write_reader(&[("a.txt", b"hello")]);
        assert_eq!(archive.len() % BLOCK_SIZE, 0);
    }

    #[test]
    fn long_paths_use_the_ustar_prefix_field() {
        let long_path = format!("knowledge/{}/note.md", "a".repeat(120));
        let archive = write_reader(&[(long_path.as_str(), b"deep")]);
        let entries = read_all(&archive);
        assert_eq!(entries[0].0, long_path);
        assert_eq!(entries[0].1, b"deep");
    }

    #[test]
    fn traversal_paths_are_refused_when_writing() {
        let mut archive = Vec::new();
        let mut writer = TarWriter::new(&mut archive);
        let error = writer.append_bytes("../escape", b"x").unwrap_err();
        assert!(matches!(error, BackupError::UnsafePath(_)));
        let error = writer.append_bytes("/absolute", b"x").unwrap_err();
        assert!(matches!(error, BackupError::UnsafePath(_)));
    }

    #[test]
    fn corrupted_content_is_detected_by_the_caller_hash() {
        let archive = write_reader(&[("a.txt", b"hello")]);
        let mut tampered = archive.clone();
        // Flip a content byte after the first header.
        tampered[BLOCK_SIZE] ^= 0xff;
        let mut reader = TarReader::new(tampered.as_slice());
        let header = reader.next_header().unwrap().unwrap();
        let digest = reader.hash_data(header.size).unwrap();
        use sha2::Digest;
        assert_ne!(
            digest,
            crate::util::hex_digest(sha2::Sha256::digest(b"hello"))
        );
    }

    #[test]
    fn truncated_archive_is_reported() {
        let archive = write_reader(&[("a.txt", b"hello")]);
        let truncated = &archive[..BLOCK_SIZE + 3];
        let mut reader = TarReader::new(truncated);
        let header = reader.next_header().unwrap().unwrap();
        let error = reader.hash_data(header.size).unwrap_err();
        assert!(error.to_string().contains("truncated"));
    }

    #[test]
    fn unsupported_entry_types_are_rejected() {
        let archive = write_reader(&[("a.txt", b"hello")]);
        let mut tampered = archive.clone();
        tampered[TYPE_OFFSET] = b'L';
        // Keep the checksum valid so the rejection under test is the entry type.
        tampered[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN].copy_from_slice(b"        ");
        let checksum: u64 = tampered[..BLOCK_SIZE]
            .iter()
            .map(|byte| u64::from(*byte))
            .sum();
        let text = format!("{checksum:06o}\0 ");
        tampered[CHECKSUM_OFFSET..CHECKSUM_OFFSET + CHECKSUM_LEN].copy_from_slice(text.as_bytes());
        let mut reader = TarReader::new(tampered.as_slice());
        let error = reader.next_header().unwrap_err();
        assert!(error.to_string().contains("unsupported tar entry type"));
    }

    #[test]
    fn header_checksum_corruption_is_detected() {
        let archive = write_reader(&[("a.txt", b"hello")]);
        let mut tampered = archive.clone();
        tampered[0] = b'b';
        let mut reader = TarReader::new(tampered.as_slice());
        let error = reader.next_header().unwrap_err();
        assert!(error.to_string().contains("checksum"));
    }
}
