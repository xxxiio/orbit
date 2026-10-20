//! Accepted-server Markdown mirror and cache operations.
//! Kept separate from command queue/reconnect logic.
use super::*;

impl AuthorityReplica {
    /// A server-read Markdown document is cached only after validating its
    /// stable identity and frontmatter. This is an explicitly stale-capable,
    /// accepted-server-data cache; local unacknowledged edits are NEVER applied.
    pub fn cache_markdown(
        &mut self,
        note: &orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
        body: &str,
    ) -> Result<(), AuthorityError> {
        Self::validate_markdown_cache_record(note, body)?;
        let metadata =
            serde_json::to_string(note).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO accepted_markdown_cache(note_id,note_metadata,body) VALUES(?1,?2,?3)
            ON CONFLICT(note_id) DO UPDATE SET note_metadata=excluded.note_metadata,body=excluded.body",
            params![note.id,metadata,body])?;
        // A point read may be newer than the completed inventory. Never
        // continue advertising a complete mirror after an individual write.
        tx.execute("DELETE FROM markdown_mirror_metadata WHERE id=1", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn cached_markdown(
        &self,
        note_id: &str,
    ) -> Result<
        Option<(
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        )>,
        AuthorityError,
    > {
        let data: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT note_metadata,body FROM accepted_markdown_cache WHERE note_id=?1",
                [note_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        data.map(|(info, body)| {
            let note: orbit_runtime::knowledge_service::KnowledgeDocumentInfo =
                serde_json::from_str(&info).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            if note.id.as_str() != note_id {
                return Err(AuthorityError::Invalid("cache identity mismatch".into()));
            }
            Ok((note, body))
        })
        .transpose()
    }

    /// Search only documents explicitly fetched from, and validated against,
    /// the accepted server. This is not a search of pending Markdown writes.
    /// A result is a cached copy and may be older than the server.
    pub fn search_cached_markdown(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>, AuthorityError> {
        if query.trim().is_empty() || query.chars().count() > 256 || limit == 0 || limit > 500 {
            return Err(AuthorityError::Invalid(
                "invalid offline Markdown search".into(),
            ));
        }
        // Search literal terms; LIKE wildcard characters in the user's query
        // must not silently match arbitrary documents.
        let escaped = query
            .to_lowercase()
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_");
        let pattern = format!("%{escaped}%");
        let mut stmt = self.db.prepare(
            "SELECT note_id,note_metadata FROM accepted_markdown_cache
             WHERE lower(body) LIKE ?1 ESCAPE '!' OR lower(note_metadata) LIKE ?1 ESCAPE '!'
             ORDER BY note_id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, raw) = row?;
            let info: orbit_runtime::knowledge_service::KnowledgeDocumentInfo =
                serde_json::from_str(&raw)
                    .map_err(|error| AuthorityError::Invalid(error.to_string()))?;
            if info.id != id {
                return Err(AuthorityError::Conflict(
                    "cached Markdown identity mismatch".into(),
                ));
            }
            result.push(info);
        }
        Ok(result)
    }

    /// A byte-sensitive manifest is fetched twice, once before download and
    /// once after all bodies have been verified. Any intervening write that
    /// changes a note's content, title, location, or membership aborts the
    /// publication transaction and leaves the previous accepted cache intact.
    /// This provides a consistent observation, not a distributed lock.
    pub fn sync_markdown_mirror(&mut self, http: &AuthorityHttp) -> Result<usize, AuthorityError> {
        Ok(self.sync_markdown_mirror_detailed(http)?.total_documents)
    }

    /// Incremental transfer, complete accepted-cache publication. Reuses a
    /// note only if its metadata and actual cached bytes match the server's
    /// announced body digest. A corrupt or stale cache is never trusted just
    /// because its inventory metadata says it is complete.
    pub fn sync_markdown_mirror_detailed(
        &mut self,
        http: &AuthorityHttp,
    ) -> Result<MarkdownMirrorSyncReport, AuthorityError> {
        self.bind(&http.base)?;
        // A verified note inventory must also belong to the accepted domain
        // revision recorded in this replica. Otherwise a simultaneous write
        // can yield a mixed domain/Markdown view labelled as one checkpoint.
        // A note-only cache without a domain snapshot is still useful, but
        // cannot claim a source revision.
        let accepted = self.snapshot()?;
        let checkpoint = if let Some(ref snapshot) = accepted {
            let server = http.get("/v1/authoritative/snapshot")?;
            let local_revision = snapshot.get("revision").and_then(Value::as_i64);
            let local_fingerprint = snapshot.get("history_fingerprint").and_then(Value::as_str);
            if local_revision != server.get("revision").and_then(Value::as_i64)
                || local_fingerprint != server.get("history_fingerprint").and_then(Value::as_str)
            {
                return Err(AuthorityError::Conflict(
                    "authority advanced before Markdown transfer; refresh accepted state first"
                        .into(),
                ));
            }
            Some((
                local_revision
                    .ok_or_else(|| AuthorityError::Invalid("missing checkpoint revision".into()))?,
                local_fingerprint
                    .ok_or_else(|| {
                        AuthorityError::Invalid("missing checkpoint fingerprint".into())
                    })?
                    .to_owned(),
            ))
        } else {
            None
        };
        let first =
            Self::parse_markdown_manifest(http.get("/v1/authoritative/markdown-manifest")?)?;
        let mut downloaded = Vec::with_capacity(first.len());
        let mut reused = 0_usize;
        for entry in &first {
            let note = &entry.note;
            if let Some((local_note, local_body)) = self.cached_markdown(&note.id)?
                && local_note == *note
                && Self::markdown_body_digest(&local_body) == entry.body_sha256
            {
                // Cached accepted data is byte-identical to the signed-off
                // server manifest. Reuse it as part of the new candidate;
                // do not publish anything until the second manifest read.
                Self::validate_markdown_cache_record(&local_note, &local_body)?;
                downloaded.push((local_note, local_body));
                reused += 1;
                continue;
            }
            let response = http.get(&format!("/v1/authoritative/markdown-by-id?id={}", note.id))?;
            let fetched: orbit_runtime::knowledge_service::KnowledgeDocumentInfo =
                serde_json::from_value(
                    response.get("note").cloned().ok_or_else(|| {
                        AuthorityError::Invalid("missing Markdown metadata".into())
                    })?,
                )
                .map_err(|e| AuthorityError::Invalid(e.to_string()))?;
            if &fetched != note {
                return Err(AuthorityError::Conflict(
                    "Markdown metadata changed during download; previous mirror preserved".into(),
                ));
            }
            let body = response
                .get("body")
                .and_then(Value::as_str)
                .ok_or_else(|| AuthorityError::Invalid("missing Markdown body".into()))?;
            Self::validate_markdown_cache_record(&fetched, body)?;
            if Self::markdown_body_digest(body) != entry.body_sha256 {
                return Err(AuthorityError::Conflict(
                    "Markdown body changed during mirror download; previous mirror preserved"
                        .into(),
                ));
            }
            downloaded.push((fetched, body.to_owned()));
        }
        let last = Self::parse_markdown_manifest(http.get("/v1/authoritative/markdown-manifest")?)?;
        if first != last {
            return Err(AuthorityError::Conflict(
                "Markdown manifest changed during mirror refresh; previous mirror preserved".into(),
            ));
        }
        // Re-read accepted authority identity after the final inventory.
        // A changing domain revision invalidates the joined checkpoint even
        // if the notes themselves are unchanged. Do not publish the candidate.
        if let Some((expected_revision, expected_fingerprint)) = checkpoint.as_ref() {
            let latest = http.get("/v1/authoritative/snapshot")?;
            if latest.get("revision").and_then(Value::as_i64) != Some(*expected_revision)
                || latest.get("history_fingerprint").and_then(Value::as_str)
                    != Some(expected_fingerprint.as_str())
            {
                return Err(AuthorityError::Conflict(
                    "authority advanced during Markdown transfer; existing mirror preserved".into(),
                ));
            }
            // Detect an intra-process accepted-state refresh between the
            // network roundtrips, not just an external server write.
            if self.revision()? != Some(*expected_revision) {
                return Err(AuthorityError::Conflict(
                    "local accepted revision changed during mirror transfer".into(),
                ));
            }
        }
        let digest = Self::markdown_manifest_digest(&first);
        let revision = checkpoint.as_ref().map(|(rev, _)| *rev);
        let total_documents =
            self.publish_markdown_mirror(&downloaded, Some(digest.clone()), revision)?;
        Ok(MarkdownMirrorSyncReport {
            total_documents,
            downloaded_documents: total_documents - reused,
            reused_documents: reused,
            inventory_digest: digest,
        })
    }

    fn markdown_body_digest(body: &str) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(body.as_bytes()))
    }

    fn parse_markdown_manifest(data: Value) -> Result<Vec<MarkdownManifestEntry>, AuthorityError> {
        let mut notes: Vec<MarkdownManifestEntry> =
            serde_json::from_value(data).map_err(|e| AuthorityError::Invalid(e.to_string()))?;
        let mut ids = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        for entry in &notes {
            let note = &entry.note;
            if note.id.is_empty()
                || note.id.len() > 128
                || !note
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || note.path.is_empty()
                || !paths.insert(note.path.clone())
                || !ids.insert(note.id.clone())
                || entry.body_sha256.len() != 64
                || !entry
                    .body_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(AuthorityError::Conflict(
                    "invalid Markdown manifest identity, path or digest".into(),
                ));
            }
        }
        notes.sort_by(|a, b| a.note.id.cmp(&b.note.id));
        Ok(notes)
    }

    fn markdown_manifest_digest(notes: &[MarkdownManifestEntry]) -> String {
        use sha2::{Digest, Sha256};
        let mut bytes = Vec::new();
        for entry in notes {
            for part in [
                &entry.note.id,
                &entry.note.path,
                &entry.note.title,
                &entry.body_sha256,
            ] {
                bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
                bytes.extend_from_slice(part.as_bytes());
            }
        }
        format!("{:x}", Sha256::digest(bytes))
    }

    pub fn markdown_mirror_status(&self) -> Result<MarkdownMirrorStatus, AuthorityError> {
        let actual: i64 =
            self.db
                .query_row("SELECT COUNT(*) FROM accepted_markdown_cache", [], |r| {
                    r.get(0)
                })?;
        let saved: Option<(i64,Option<String>,Option<i64>,i64)> = self.db.query_row(
            "SELECT complete,inventory_digest,source_revision,document_count FROM markdown_mirror_metadata WHERE id=1",
            [],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).optional()?;
        let Some((complete, digest, revision, count)) = saved else {
            return Ok(MarkdownMirrorStatus {
                complete: false,
                source_inventory_verified: false,
                document_count: usize::try_from(actual)
                    .map_err(|_| AuthorityError::Invalid("negative cache count".into()))?,
                inventory_digest: None,
                source_revision: None,
            });
        };
        if count != actual || count < 0 || !matches!(complete, 0 | 1) {
            return Err(AuthorityError::Conflict(
                "Markdown mirror metadata/count mismatch".into(),
            ));
        }
        if let Some(ref value) = digest
            && (value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
        {
            return Err(AuthorityError::Conflict(
                "invalid cached Markdown manifest digest".into(),
            ));
        }
        if revision.is_some_and(|value| value < 0) {
            return Err(AuthorityError::Conflict(
                "negative Markdown mirror source revision".into(),
            ));
        }
        // A persisted "verified" flag alone is not a guarantee that SQLite
        // still holds the accepted bytes. Recalculate the original manifest
        // digest from every cached body and its metadata. This also catches
        // post-sync corruption which leaves the document count unchanged.
        if complete == 1
            && let Some(expected_digest) = digest.as_ref()
        {
            let mut cached = Vec::new();
            let mut query = self.db.prepare(
                "SELECT note_id,note_metadata,body FROM accepted_markdown_cache ORDER BY note_id",
            )?;
            let rows = query.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            let mut paths = std::collections::HashSet::new();
            for row in rows {
                let (id, metadata, body) = row?;
                let info: orbit_runtime::knowledge_service::KnowledgeDocumentInfo =
                    serde_json::from_str(&metadata).map_err(|_| {
                        AuthorityError::Conflict("corrupted cached Markdown metadata".into())
                    })?;
                if id != info.id || !paths.insert(info.path.clone()) {
                    return Err(AuthorityError::Conflict(
                        "corrupted cached Markdown identity or path".into(),
                    ));
                }
                Self::validate_markdown_cache_record(&info, &body).map_err(|_| {
                    AuthorityError::Conflict("corrupted cached Markdown content".into())
                })?;
                cached.push(MarkdownManifestEntry {
                    note: info,
                    body_sha256: Self::markdown_body_digest(&body),
                });
            }
            let actual_digest = Self::markdown_manifest_digest(&cached);
            if &actual_digest != expected_digest {
                return Err(AuthorityError::Conflict(
                    "cached Markdown bytes differ from verified manifest".into(),
                ));
            }
        }
        Ok(MarkdownMirrorStatus {
            complete: complete == 1,
            source_inventory_verified: complete == 1 && digest.is_some(),
            document_count: usize::try_from(count)
                .map_err(|_| AuthorityError::Invalid("invalid mirror count".into()))?,
            inventory_digest: digest,
            source_revision: revision,
        })
    }

    /// Simulate disk corruption in a regression test without exposing an
    /// unsafe public mutation API on the connected replica.
    #[cfg(test)]
    pub(crate) fn test_corrupt_cached_body(
        &self,
        id: &str,
        body: &str,
    ) -> Result<(), AuthorityError> {
        self.db.execute(
            "UPDATE accepted_markdown_cache SET body=?2 WHERE note_id=?1",
            params![id, body],
        )?;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn test_corrupt_cached_metadata(
        &self,
        id: &str,
        metadata: &str,
    ) -> Result<(), AuthorityError> {
        self.db.execute(
            "UPDATE accepted_markdown_cache SET note_metadata=?2 WHERE note_id=?1",
            params![id, metadata],
        )?;
        Ok(())
    }

    /// Publish a complete authoritative Markdown mirror in a single SQLite
    /// transaction. The caller must have fetched and verified EVERY document
    /// before invoking this method; no partial network refresh reaches disk.
    /// The accepted mirror is completely independent from pending commands.
    pub fn replace_markdown_mirror(
        &mut self,
        documents: &[(
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        )],
    ) -> Result<usize, AuthorityError> {
        self.publish_markdown_mirror(documents, None, None)
    }

    fn publish_markdown_mirror(
        &mut self,
        documents: &[(
            orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
            String,
        )],
        inventory_digest: Option<String>,
        source_revision: Option<i64>,
    ) -> Result<usize, AuthorityError> {
        let mut seen = std::collections::HashSet::new();
        let mut paths = std::collections::HashSet::new();
        let mut encoded = Vec::with_capacity(documents.len());
        for (note, body) in documents {
            if !seen.insert(note.id.clone()) || !paths.insert(note.path.clone()) {
                return Err(AuthorityError::Conflict(
                    "duplicate note identity/path in authoritative inventory".into(),
                ));
            }
            Self::validate_markdown_cache_record(note, body)?;
            encoded.push((
                note.id.clone(),
                serde_json::to_string(note).map_err(|e| AuthorityError::Invalid(e.to_string()))?,
                body.clone(),
            ));
        }
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM accepted_markdown_cache", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO accepted_markdown_cache(note_id,note_metadata,body) VALUES(?1,?2,?3)",
            )?;
            for (id, metadata, body) in &encoded {
                stmt.execute(params![id, metadata, body])?;
            }
        }
        tx.execute("INSERT INTO markdown_mirror_metadata(id,complete,inventory_digest,source_revision,document_count)
            VALUES(1,1,?1,?2,?3)
            ON CONFLICT(id) DO UPDATE SET complete=excluded.complete,
                inventory_digest=excluded.inventory_digest,
                source_revision=excluded.source_revision,document_count=excluded.document_count",
            params![inventory_digest,source_revision,encoded.len() as i64])?;
        tx.commit()?;
        Ok(encoded.len())
    }

    fn validate_markdown_cache_record(
        note: &orbit_runtime::knowledge_service::KnowledgeDocumentInfo,
        body: &str,
    ) -> Result<(), AuthorityError> {
        if note.id.is_empty()
            || note.id.len() > 128
            || !note
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !note.path.to_ascii_lowercase().ends_with(".md")
        {
            return Err(AuthorityError::Invalid(
                "invalid cached Markdown identity".into(),
            ));
        }
        let mut lines = body.lines();
        if lines.next() != Some("---") {
            return Err(AuthorityError::Invalid("missing frontmatter".into()));
        }
        let mut found = false;
        let mut closed = false;
        for line in lines {
            if line == "---" {
                closed = true;
                break;
            }
            if let Some(id) = line.strip_prefix("orbit_id: ") {
                if found || id != note.id {
                    return Err(AuthorityError::Invalid("cached ID mismatch".into()));
                }
                found = true;
            }
        }
        if !closed || !found {
            return Err(AuthorityError::Invalid("missing cached ID".into()));
        }
        Ok(())
    }

    /// An explicit, offline-only inventory, not a claim that the cache
    /// contains every note in the authoritative vault.
    pub fn cached_markdown_inventory(
        &self,
        limit: usize,
    ) -> Result<Vec<orbit_runtime::knowledge_service::KnowledgeDocumentInfo>, AuthorityError> {
        if limit == 0 || limit > 500 {
            return Err(AuthorityError::Invalid(
                "offline inventory limit must be 1..=500".into(),
            ));
        }
        let mut stmt = self.db.prepare(
            "SELECT note_id,note_metadata FROM accepted_markdown_cache ORDER BY note_id LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (id, raw) = row?;
            let info: orbit_runtime::knowledge_service::KnowledgeDocumentInfo =
                serde_json::from_str(&raw)
                    .map_err(|error| AuthorityError::Invalid(error.to_string()))?;
            if info.id != id {
                return Err(AuthorityError::Conflict(
                    "cached Markdown identity mismatch".into(),
                ));
            }
            result.push(info);
        }
        Ok(result)
    }
}
