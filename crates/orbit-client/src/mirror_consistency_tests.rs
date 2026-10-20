//! Executable behavior contracts for offline Markdown mirror provenance.
//! These tests use real SQLite files but deliberately avoid assuming an
//! authoritative server is online or that individual point fetches are full
//! inventory refreshes.
#![cfg(test)]
use crate::authority::AuthorityReplica;
use orbit_runtime::knowledge_service::KnowledgeDocumentInfo;

fn note(id: &str, path: &str) -> (KnowledgeDocumentInfo, String) {
    (
        KnowledgeDocumentInfo {
            id: id.into(),
            path: path.into(),
            title: format!("Title {id}"),
        },
        format!("---\norbit_id: {id}\n---\n# Title {id}\n\nAccepted body"),
    )
}

#[test]
fn blank_replica_does_not_claim_complete_mirror() {
    let replica = AuthorityReplica::open(":memory:").unwrap();
    let mirror = replica.markdown_mirror_status().unwrap();
    assert!(!mirror.complete);
    assert!(!mirror.source_inventory_verified);
    assert_eq!(mirror.document_count, 0);
    assert!(mirror.inventory_digest.is_none());
    assert!(mirror.source_revision.is_none());
}

#[test]
fn direct_replacement_remains_offline_cache_without_server_proof() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica
        .replace_markdown_mirror(&[note("a", "a.md")])
        .unwrap();
    let mirror = replica.markdown_mirror_status().unwrap();
    assert!(mirror.complete);
    assert!(!mirror.source_inventory_verified);
    assert_eq!(mirror.document_count, 1);
    assert!(mirror.inventory_digest.is_none());
}

#[test]
fn individual_point_cache_revokes_full_mirror_status() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica
        .replace_markdown_mirror(&[note("a", "a.md")])
        .unwrap();
    let (info, body) = note("b", "b.md");
    replica.cache_markdown(&info, &body).unwrap();
    let mirror = replica.markdown_mirror_status().unwrap();
    assert!(!mirror.complete);
    assert_eq!(mirror.document_count, 2);
    assert!(replica.cached_markdown("a").unwrap().is_some());
    assert!(replica.cached_markdown("b").unwrap().is_some());
}

#[test]
fn failed_mirror_publish_preserves_previous_snapshot_and_provenance() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    let original = note("first", "first.md");
    replica
        .replace_markdown_mirror(std::slice::from_ref(&original))
        .unwrap();
    let before = replica.markdown_mirror_status().unwrap();
    let duplicate = note("second", "first.md");
    assert!(
        replica
            .replace_markdown_mirror(&[original.clone(), duplicate])
            .is_err()
    );
    assert_eq!(replica.markdown_mirror_status().unwrap(), before);
    assert_eq!(replica.cached_markdown("first").unwrap(), Some(original));
}

#[test]
fn empty_complete_mirror_is_distinct_from_uninitialized_cache() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    assert!(!replica.markdown_mirror_status().unwrap().complete);
    replica.replace_markdown_mirror(&[]).unwrap();
    let mirror = replica.markdown_mirror_status().unwrap();
    assert!(mirror.complete);
    assert!(!mirror.source_inventory_verified);
    assert_eq!(mirror.document_count, 0);
}

#[test]
fn mirror_provenance_survives_sqlite_reopen() {
    let file = tempfile::NamedTempFile::new().unwrap();
    {
        let mut replica = AuthorityReplica::open(file.path()).unwrap();
        replica
            .replace_markdown_mirror(&[note("one", "one.md")])
            .unwrap();
        assert!(replica.markdown_mirror_status().unwrap().complete);
    }
    let replica = AuthorityReplica::open(file.path()).unwrap();
    let mirror = replica.markdown_mirror_status().unwrap();
    assert_eq!(mirror.document_count, 1);
    assert!(mirror.complete);
    assert!(!mirror.source_inventory_verified);
}

#[test]
fn invalid_single_note_does_not_invalidate_existing_mirror() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica
        .replace_markdown_mirror(&[note("one", "one.md")])
        .unwrap();
    let before = replica.markdown_mirror_status().unwrap();
    let (info, _) = note("two", "two.md");
    assert!(
        replica
            .cache_markdown(&info, "invalid frontmatter")
            .is_err()
    );
    assert_eq!(replica.markdown_mirror_status().unwrap(), before);
}

#[test]
fn convergence_status_includes_explicit_mirror_provenance() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    let before = replica.convergence_status().unwrap();
    assert!(!before.markdown_mirror.complete);
    replica
        .replace_markdown_mirror(&[note("a", "a.md")])
        .unwrap();
    let after = replica.convergence_status().unwrap();
    assert!(after.markdown_mirror.complete);
    assert!(!after.markdown_mirror.source_inventory_verified);
    assert!(!after.server_convergence_verified);
}

/// These tests deliberately exercise actual HTTP responses and the SQLite
/// publication path. The server simulator never mutates the replica directly.
mod network {
    use super::*;
    use crate::authority::AuthorityHttp;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    fn digest(s: &str) -> String {
        format!("{:x}", Sha256::digest(s.as_bytes()))
    }

    struct Exchange {
        path: String,
        answer: Value,
    }
    fn answer(path: &str, answer: Value) -> Exchange {
        Exchange {
            path: path.into(),
            answer,
        }
    }
    fn mock(exchanges: Vec<Exchange>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for exchange in exchanges {
                let (mut connection, _) = listener.accept().unwrap();
                connection
                    .set_read_timeout(Some(Duration::from_secs(4)))
                    .unwrap();
                let mut bytes = Vec::new();
                while !bytes.windows(4).any(|data| data == b"\r\n\r\n") {
                    let mut buffer = [0u8; 8192];
                    let size = connection.read(&mut buffer).unwrap();
                    if size == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..size]);
                    assert!(bytes.len() < 65536);
                }
                let request = String::from_utf8_lossy(&bytes);
                let line = request.lines().next().unwrap_or("");
                assert!(line.contains(&exchange.path), "unexpected route {line}");
                let response = exchange.answer.to_string();
                write!(connection,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
            }
        });
        (base, handle)
    }
    fn manifest(notes: &[(KnowledgeDocumentInfo, String)]) -> Value {
        Value::Array(
            notes
                .iter()
                .map(|(note, body)| {
                    json!({
                        "note":note,"body_sha256":digest(body),
                    })
                })
                .collect(),
        )
    }
    fn doc(note: &(KnowledgeDocumentInfo, String)) -> Value {
        json!({"note":note.0,"body":note.1})
    }
    fn transport(base: &str) -> AuthorityHttp {
        AuthorityHttp::new(base, "test-token", "device-1").unwrap()
    }

    #[test]
    fn network_mirror_publishes_complete_manifest_after_two_identical_observations() {
        let a = note("a", "notes/a.md");
        let b = note("b", "notes/b.md");
        let collection = vec![a.clone(), b.clone()];
        let messages = vec![
            answer("markdown-manifest", manifest(&collection)),
            answer("markdown-by-id?id=a", doc(&a)),
            answer("markdown-by-id?id=b", doc(&b)),
            answer("markdown-manifest", manifest(&collection)),
        ];
        let (base, thread) = mock(messages);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        assert_eq!(replica.sync_markdown_mirror(&transport(&base)).unwrap(), 2);
        let state = replica.markdown_mirror_status().unwrap();
        assert!(state.complete);
        assert!(state.source_inventory_verified);
        assert_eq!(state.document_count, 2);
        assert_eq!(state.inventory_digest.as_ref().unwrap().len(), 64);
        assert_eq!(replica.cached_markdown("a").unwrap(), Some(a));
        assert_eq!(replica.cached_markdown("b").unwrap(), Some(b));
        thread.join().unwrap();
    }

    #[test]
    fn concurrent_metadata_change_preserves_old_accepted_mirror() {
        let a = note("a", "a.md");
        let moved = note("a", "folder/a.md");
        let (base, thread) = mock(vec![
            answer("markdown-manifest", manifest(std::slice::from_ref(&a))),
            answer("markdown-by-id?id=a", doc(&a)),
            answer("markdown-manifest", manifest(&[moved])),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let previous = note("old", "old.md");
        replica
            .replace_markdown_mirror(std::slice::from_ref(&previous))
            .unwrap();
        let old_status = replica.markdown_mirror_status().unwrap();
        assert!(replica.sync_markdown_mirror(&transport(&base)).is_err());
        assert_eq!(replica.markdown_mirror_status().unwrap(), old_status);
        assert_eq!(replica.cached_markdown("old").unwrap(), Some(previous));
        assert!(replica.cached_markdown("a").unwrap().is_none());
        thread.join().unwrap();
    }

    #[test]
    fn content_changed_during_download_is_detected_by_body_digest() {
        let original = note("a", "a.md");
        let updated = (
            original.0.clone(),
            original.1.replace("Accepted body", "Different body"),
        );
        let (base, thread) = mock(vec![
            answer(
                "markdown-manifest",
                manifest(std::slice::from_ref(&original)),
            ),
            answer("markdown-by-id?id=a", doc(&updated)),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        assert!(replica.sync_markdown_mirror(&transport(&base)).is_err());
        assert!(!replica.markdown_mirror_status().unwrap().complete);
        assert!(replica.cached_markdown("a").unwrap().is_none());
        thread.join().unwrap();
    }

    #[test]
    fn body_changed_between_manifest_observations_rejects_publication() {
        let a = note("a", "a.md");
        let mut updated = a.clone();
        updated.1.push_str("\nA later server edit");
        let (base, thread) = mock(vec![
            answer("markdown-manifest", manifest(std::slice::from_ref(&a))),
            answer("markdown-by-id?id=a", doc(&a)),
            answer("markdown-manifest", manifest(&[updated])),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        assert!(replica.sync_markdown_mirror(&transport(&base)).is_err());
        assert!(!replica.markdown_mirror_status().unwrap().complete);
        thread.join().unwrap();
    }

    #[test]
    fn inventory_order_changes_are_not_treated_as_file_mutations() {
        let a = note("a", "a.md");
        let b = note("b", "b.md");
        let collection = vec![a.clone(), b.clone()];
        let reverse = vec![b.clone(), a.clone()];
        let (base, thread) = mock(vec![
            answer("markdown-manifest", manifest(&reverse)),
            answer("markdown-by-id?id=a", doc(&a)),
            answer("markdown-by-id?id=b", doc(&b)),
            answer("markdown-manifest", manifest(&collection)),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        assert_eq!(replica.sync_markdown_mirror(&transport(&base)).unwrap(), 2);
        assert!(
            replica
                .markdown_mirror_status()
                .unwrap()
                .source_inventory_verified
        );
        thread.join().unwrap();
    }

    #[test]
    fn invalid_manifest_digest_is_rejected_before_any_note_download() {
        let a = note("a", "a.md");
        let (base, thread) = mock(vec![answer(
            "markdown-manifest",
            json!([
                {"note":a.0,"body_sha256":"not-a-digest"}
            ]),
        )]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        assert!(replica.sync_markdown_mirror(&transport(&base)).is_err());
        thread.join().unwrap();
    }

    #[test]
    fn duplicate_note_paths_abort_mirror_before_publication() {
        let a = note("a", "same.md");
        let b = note("b", "same.md");
        let (base, thread) = mock(vec![answer("markdown-manifest", manifest(&[a, b]))]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        assert!(replica.sync_markdown_mirror(&transport(&base)).is_err());
        thread.join().unwrap();
    }

    #[test]
    fn empty_server_vault_publishes_verifiably_complete_empty_mirror() {
        let (base, thread) = mock(vec![
            answer("markdown-manifest", json!([])),
            answer("markdown-manifest", json!([])),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .replace_markdown_mirror(&[note("stale", "stale.md")])
            .unwrap();
        assert_eq!(replica.sync_markdown_mirror(&transport(&base)).unwrap(), 0);
        assert_eq!(replica.markdown_mirror_status().unwrap().document_count, 0);
        assert!(
            replica
                .markdown_mirror_status()
                .unwrap()
                .source_inventory_verified
        );
        assert!(replica.cached_markdown("stale").unwrap().is_none());
        thread.join().unwrap();
    }
}

mod incremental_network {
    use super::*;
    use crate::authority::AuthorityHttp;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    #[derive(Clone)]
    struct Reply {
        path: String,
        body: Value,
    }
    fn serve(replies: Vec<Reply>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for expected in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(4)))
                    .unwrap();
                let mut data = Vec::new();
                while !data.windows(4).any(|v| v == b"\r\n\r\n") {
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                    assert!(data.len() < 65536);
                }
                let first = String::from_utf8_lossy(&data);
                assert!(first.lines().next().unwrap_or("").contains(&expected.path));
                let body = expected.body.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        (base, handle)
    }
    fn manifest(notes: &[(KnowledgeDocumentInfo, String)]) -> Value {
        json!(
            notes
                .iter()
                .map(|(note, body)| json!({
                    "note":note,"body_sha256":format!("{:x}",Sha256::digest(body.as_bytes()))
                }))
                .collect::<Vec<_>>()
        )
    }
    fn reply(path: &str, body: Value) -> Reply {
        Reply {
            path: path.into(),
            body,
        }
    }
    fn transport(base: &str) -> AuthorityHttp {
        AuthorityHttp::new(base, "token", "device").unwrap()
    }
    #[test]
    fn all_matching_cached_bodies_are_reused_without_document_requests() {
        let a = note("a", "a.md");
        let b = note("b", "b.md");
        let collection = vec![a.clone(), b.clone()];
        let (base, handle) = serve(vec![
            reply("markdown-manifest", manifest(&collection)),
            reply("markdown-manifest", manifest(&collection)),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.replace_markdown_mirror(&collection).unwrap();
        let report = replica
            .sync_markdown_mirror_detailed(&transport(&base))
            .unwrap();
        assert_eq!(report.total_documents, 2);
        assert_eq!(report.reused_documents, 2);
        assert_eq!(report.downloaded_documents, 0);
        assert!(
            replica
                .markdown_mirror_status()
                .unwrap()
                .source_inventory_verified
        );
        assert_eq!(replica.cached_markdown("a").unwrap(), Some(a));
        handle.join().unwrap();
    }

    #[test]
    fn only_changed_body_is_transferred_on_repeated_sync() {
        let a = note("a", "a.md");
        let b = note("b", "b.md");
        let mut changed_b = b.clone();
        changed_b.1.push_str("\nServer edit");
        let new = vec![a.clone(), changed_b.clone()];
        let (base, handle) = serve(vec![
            reply("markdown-manifest", manifest(&new)),
            reply(
                "markdown-by-id?id=b",
                json!({"note":changed_b.0,"body":changed_b.1}),
            ),
            reply("markdown-manifest", manifest(&new)),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.replace_markdown_mirror(&[a.clone(), b]).unwrap();
        let report = replica
            .sync_markdown_mirror_detailed(&transport(&base))
            .unwrap();
        assert_eq!(report.reused_documents, 1);
        assert_eq!(report.downloaded_documents, 1);
        assert_eq!(replica.cached_markdown("a").unwrap(), Some(a));
        assert_eq!(replica.cached_markdown("b").unwrap(), Some(changed_b));
        handle.join().unwrap();
    }

    #[test]
    fn server_deletion_discards_only_removed_note_on_successful_publication() {
        let original = vec![note("a", "a.md"), note("b", "b.md")];
        let new = vec![original[0].clone()];
        let (base, handle) = serve(vec![
            reply("markdown-manifest", manifest(&new)),
            reply("markdown-manifest", manifest(&new)),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.replace_markdown_mirror(&original).unwrap();
        let report = replica
            .sync_markdown_mirror_detailed(&transport(&base))
            .unwrap();
        assert_eq!(report.reused_documents, 1);
        assert_eq!(report.downloaded_documents, 0);
        assert_eq!(report.total_documents, 1);
        assert!(replica.cached_markdown("b").unwrap().is_none());
        handle.join().unwrap();
    }

    #[test]
    fn server_move_triggers_metadata_refresh_even_when_body_is_identical() {
        let old = note("a", "original/a.md");
        let mut moved = old.clone();
        moved.0.path = "new-folder/a.md".into();
        let (base, handle) = serve(vec![
            reply("markdown-manifest", manifest(std::slice::from_ref(&moved))),
            reply(
                "markdown-by-id?id=a",
                json!({"note":moved.0,"body":moved.1}),
            ),
            reply("markdown-manifest", manifest(std::slice::from_ref(&moved))),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.replace_markdown_mirror(&[old]).unwrap();
        let report = replica
            .sync_markdown_mirror_detailed(&transport(&base))
            .unwrap();
        assert_eq!(report.downloaded_documents, 1);
        assert_eq!(report.reused_documents, 0);
        assert_eq!(replica.cached_markdown("a").unwrap(), Some(moved));
        handle.join().unwrap();
    }

    #[test]
    fn failed_second_manifest_does_not_delete_removed_notes() {
        let old = vec![note("a", "a.md"), note("b", "b.md")];
        let first = vec![old[0].clone()];
        let (base, handle) = serve(vec![
            reply("markdown-manifest", manifest(&first)),
            reply("markdown-manifest", manifest(&old)),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.replace_markdown_mirror(&old).unwrap();
        let previous = replica.markdown_mirror_status().unwrap();
        assert!(
            replica
                .sync_markdown_mirror_detailed(&transport(&base))
                .is_err()
        );
        assert_eq!(replica.markdown_mirror_status().unwrap(), previous);
        assert_eq!(replica.cached_markdown("b").unwrap(), Some(old[1].clone()));
        handle.join().unwrap();
    }

    #[test]
    fn cache_corruption_is_not_reused_based_on_matching_metadata() {
        let valid = note("a", "a.md");
        let (base, handle) = serve(vec![
            reply("markdown-manifest", manifest(std::slice::from_ref(&valid))),
            reply(
                "markdown-by-id?id=a",
                json!({"note":valid.0,"body":valid.1}),
            ),
            reply("markdown-manifest", manifest(std::slice::from_ref(&valid))),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let corrupted = valid.1.replace("Accepted body", "old body");
        replica.cache_markdown(&valid.0, &corrupted).unwrap();
        let report = replica
            .sync_markdown_mirror_detailed(&transport(&base))
            .unwrap();
        assert_eq!(report.reused_documents, 0);
        assert_eq!(report.downloaded_documents, 1);
        assert_eq!(replica.cached_markdown("a").unwrap(), Some(valid));
        handle.join().unwrap();
    }
}

/// Disk-level tests exercise the schema contract directly: the metadata
/// describing a full mirror must never survive a change that invalidates it.
mod disk_integrity {
    use super::*;
    use rusqlite::{Connection, params};

    #[test]
    fn cached_inventory_status_persists_across_independent_replicas() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let first = note("one", "folder/one.md");
        {
            let mut replica = AuthorityReplica::open(file.path()).unwrap();
            replica
                .replace_markdown_mirror(std::slice::from_ref(&first))
                .unwrap();
            assert!(replica.markdown_mirror_status().unwrap().complete);
        }
        {
            let replica = AuthorityReplica::open(file.path()).unwrap();
            let status = replica.markdown_mirror_status().unwrap();
            assert_eq!(status.document_count, 1);
            assert!(status.complete);
            assert!(!status.source_inventory_verified);
            assert_eq!(replica.cached_markdown("one").unwrap(), Some(first));
        }
    }

    #[test]
    fn tampered_document_count_is_detected_without_silent_repair() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut replica = AuthorityReplica::open(file.path()).unwrap();
        replica
            .replace_markdown_mirror(&[note("one", "one.md")])
            .unwrap();
        let connection = Connection::open(file.path()).unwrap();
        connection
            .execute(
                "UPDATE markdown_mirror_metadata SET document_count=2 WHERE id=1",
                [],
            )
            .unwrap();
        assert!(replica.markdown_mirror_status().is_err());
        assert!(replica.convergence_status().is_err());
        // It remains possible to inspect the accepted data for recovery.
        assert!(replica.cached_markdown("one").unwrap().is_some());
    }

    #[test]
    fn external_deleted_cached_row_invalidates_provenance() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut replica = AuthorityReplica::open(file.path()).unwrap();
        replica
            .replace_markdown_mirror(&[note("one", "one.md"), note("two", "two.md")])
            .unwrap();
        let connection = Connection::open(file.path()).unwrap();
        connection
            .execute(
                "DELETE FROM accepted_markdown_cache WHERE note_id=?1",
                params!["two"],
            )
            .unwrap();
        assert!(replica.markdown_mirror_status().is_err());
        assert!(replica.cached_markdown("two").unwrap().is_none());
    }

    #[test]
    fn point_read_invalidates_completeness_in_same_transaction() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut replica = AuthorityReplica::open(file.path()).unwrap();
        replica
            .replace_markdown_mirror(&[note("one", "one.md")])
            .unwrap();
        let new = note("two", "two.md");
        replica.cache_markdown(&new.0, &new.1).unwrap();
        let connection = Connection::open(file.path()).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM markdown_mirror_metadata", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(replica.markdown_mirror_status().unwrap().document_count, 2);
        assert!(!replica.markdown_mirror_status().unwrap().complete);
    }

    #[test]
    fn rejected_replacement_does_not_mutate_outbox_or_mirror_metadata() {
        use crate::authority::CommandKind;
        use serde_json::json;
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .replace_markdown_mirror(&[note("one", "one.md")])
            .unwrap();
        let before = replica.markdown_mirror_status().unwrap();
        replica
            .queue(
                "pending-intent",
                CommandKind::Task,
                &json!({
                    "type":"create","task_id":"task1","title":"Test"
                }),
            )
            .unwrap();
        assert_eq!(replica.markdown_mirror_status().unwrap(), before);
        assert_eq!(replica.unresolved().unwrap().len(), 1);
    }

    #[test]
    fn fail_closed_on_malformed_inventory_digest_metadata() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut replica = AuthorityReplica::open(file.path()).unwrap();
        replica
            .replace_markdown_mirror(&[note("one", "one.md")])
            .unwrap();
        let connection = Connection::open(file.path()).unwrap();
        connection
            .execute(
                "UPDATE markdown_mirror_metadata SET inventory_digest='invalid' WHERE id=1",
                [],
            )
            .unwrap();
        assert!(replica.markdown_mirror_status().is_err());
    }
}

/// Separate replicas model disconnected devices with independent accepted
/// caches. A note edit on the authoritative server must only become visible
/// to the device that has actually downloaded and published the newer mirror.
mod two_client_contract {
    use super::*;
    use crate::authority::AuthorityHttp;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    fn reply(note: &(KnowledgeDocumentInfo, String)) -> Value {
        json!({"note":note.0,"body":note.1})
    }
    fn manifest(notes: &[(KnowledgeDocumentInfo, String)]) -> Value {
        json!(
            notes
                .iter()
                .map(|n| json!({
                    "note":n.0,"body_sha256":format!("{:x}",Sha256::digest(n.1.as_bytes()))
                }))
                .collect::<Vec<_>>()
        )
    }
    fn responses(notes: &[(KnowledgeDocumentInfo, String)]) -> Vec<(String, Value)> {
        let mut rows = vec![("markdown-manifest".into(), manifest(notes))];
        for n in notes {
            rows.push((format!("markdown-by-id?id={}", n.0.id), reply(n)));
        }
        rows.push(("markdown-manifest".into(), manifest(notes)));
        rows
    }
    fn serve(messages: Vec<(String, Value)>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for (path, value) in messages {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(4)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                    let mut chunk = [0_u8; 4096];
                    let count = stream.read(&mut chunk).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    assert!(request.len() < 65536);
                }
                let text = String::from_utf8_lossy(&request);
                assert!(text.lines().next().unwrap_or("").contains(&path));
                let body = value.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        (base, handle)
    }
    #[test]
    fn one_clients_newer_mirror_does_not_modify_other_offline_replica() {
        let original = note("a", "a.md");
        let mut edited = original.clone();
        edited.1.push_str("\nNew remote content");
        let (first_base, first_handle) = serve(responses(std::slice::from_ref(&original)));
        let mut first = AuthorityReplica::open(":memory:").unwrap();
        first
            .sync_markdown_mirror(&AuthorityHttp::new(&first_base, "token", "a").unwrap())
            .unwrap();
        first_handle.join().unwrap();
        let mut second = AuthorityReplica::open(":memory:").unwrap();
        let (second_base, second_handle) = serve(responses(std::slice::from_ref(&edited)));
        second
            .sync_markdown_mirror(&AuthorityHttp::new(&second_base, "token", "b").unwrap())
            .unwrap();
        second_handle.join().unwrap();
        assert_eq!(first.cached_markdown("a").unwrap(), Some(original));
        assert_eq!(second.cached_markdown("a").unwrap(), Some(edited));
        assert_ne!(
            first.markdown_mirror_status().unwrap().inventory_digest,
            second.markdown_mirror_status().unwrap().inventory_digest
        );
        assert!(
            first
                .markdown_mirror_status()
                .unwrap()
                .source_inventory_verified
        );
        assert!(
            second
                .markdown_mirror_status()
                .unwrap()
                .source_inventory_verified
        );
        // Both caches are accepted-server observations at different times;
        // neither attempts to turn its stored bytes into authoritative writes.
        assert_eq!(first.unresolved().unwrap().len(), 0);
        assert_eq!(second.unresolved().unwrap().len(), 0);
    }

    #[test]
    fn reconnecting_stale_client_updates_only_that_clients_cache() {
        let old = note("a", "a.md");
        let mut newest = old.clone();
        newest.1.push_str("\nupdated by another device");
        let mut one = AuthorityReplica::open(":memory:").unwrap();
        let mut two = AuthorityReplica::open(":memory:").unwrap();
        one.replace_markdown_mirror(std::slice::from_ref(&old))
            .unwrap();
        two.replace_markdown_mirror(std::slice::from_ref(&old))
            .unwrap();
        let messages = vec![
            (
                "markdown-manifest".into(),
                manifest(std::slice::from_ref(&newest)),
            ),
            ("markdown-by-id?id=a".into(), reply(&newest)),
            (
                "markdown-manifest".into(),
                manifest(std::slice::from_ref(&newest)),
            ),
        ];
        let (base, handle) = serve(messages);
        let report = one
            .sync_markdown_mirror_detailed(&AuthorityHttp::new(&base, "token", "a").unwrap())
            .unwrap();
        assert_eq!(report.downloaded_documents, 1);
        assert_eq!(report.reused_documents, 0);
        assert_eq!(one.cached_markdown("a").unwrap(), Some(newest));
        assert_eq!(two.cached_markdown("a").unwrap(), Some(old));
        assert!(
            !two.markdown_mirror_status()
                .unwrap()
                .source_inventory_verified
        );
        handle.join().unwrap();
    }
}

/// Deterministic manifest admission contracts. These are independent of the
/// network simulator and exercise malformed remote metadata directly.
mod manifest_admission {
    use super::*;
    use crate::authority::AuthorityReplica;
    use serde_json::json;

    #[test]
    fn manifest_digest_and_document_count_are_consistent_on_empty_refresh() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.replace_markdown_mirror(&[]).unwrap();
        let state = replica.markdown_mirror_status().unwrap();
        assert!(state.complete);
        assert_eq!(state.document_count, 0);
        assert!(state.inventory_digest.is_none());
        assert!(!state.source_inventory_verified);
    }

    #[test]
    fn invalid_cached_frontmatter_is_never_admitted_as_a_point_read() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        let (info, body) = note("one", "one.md");
        let invalid = body.replace("orbit_id: one", "orbit_id: other");
        assert!(replica.cache_markdown(&info, &invalid).is_err());
        assert!(replica.cached_markdown("one").unwrap().is_none());
        assert_eq!(replica.markdown_mirror_status().unwrap().document_count, 0);
    }

    #[test]
    fn conflicting_batch_intentions_cannot_modify_existing_mirror() {
        use crate::authority::CommandKind;
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .replace_markdown_mirror(&[note("one", "one.md")])
            .unwrap();
        let previous = replica.markdown_mirror_status().unwrap();
        replica
            .queue(
                "same-id",
                CommandKind::Task,
                &json!({
                    "type":"create","task_id":"task1","title":"one"
                }),
            )
            .unwrap();
        let result = replica.queue_batch(&[
            (
                "new-id".into(),
                CommandKind::Task,
                json!({
                    "type":"create","task_id":"task2","title":"two"
                }),
            ),
            (
                "same-id".into(),
                CommandKind::Evidence,
                json!({
                    "type":"record","metric_id":"m1","value":2
                }),
            ),
        ]);
        assert!(result.is_err());
        assert_eq!(replica.markdown_mirror_status().unwrap(), previous);
        assert_eq!(replica.unresolved().unwrap().len(), 1);
    }

    #[test]
    fn replacing_full_mirror_does_not_accept_pending_tasks() {
        use crate::authority::CommandKind;
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica
            .queue(
                "pending",
                CommandKind::Task,
                &json!({
                    "type":"create","task_id":"t1","title":"pending"
                }),
            )
            .unwrap();
        replica
            .replace_markdown_mirror(&[note("n", "n.md")])
            .unwrap();
        let status = replica.convergence_status().unwrap();
        assert_eq!(status.pending, 1);
        assert_eq!(status.accepted_revision, None);
        assert!(status.markdown_mirror.complete);
        assert!(!status.markdown_mirror.source_inventory_verified);
        assert!(!status.server_convergence_verified);
    }
}

#[test]
fn mirror_republication_invalidates_a_previous_point_read_without_losing_other_notes() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    let original = note("one", "one.md");
    let point = note("point", "point.md");
    replica
        .replace_markdown_mirror(std::slice::from_ref(&original))
        .unwrap();
    replica.cache_markdown(&point.0, &point.1).unwrap();
    assert!(!replica.markdown_mirror_status().unwrap().complete);
    assert!(replica.cached_markdown("point").unwrap().is_some());
    replica
        .replace_markdown_mirror(std::slice::from_ref(&original))
        .unwrap();
    assert!(replica.markdown_mirror_status().unwrap().complete);
    assert_eq!(replica.markdown_mirror_status().unwrap().document_count, 1);
    assert_eq!(replica.cached_markdown("one").unwrap(), Some(original));
    assert!(replica.cached_markdown("point").unwrap().is_none());
}

/// A Markdown mirror is published for an accepted domain revision only if the
/// authority observed before and after transfer has the same identity.
mod revision_checkpoint {
    use super::*;
    use crate::authority::AuthorityHttp;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    fn snapshot(revision: i64, fingerprint: &str) -> Value {
        let mut value = json!({"revision":revision,"history_fingerprint":fingerprint});
        for name in [
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
            value[name] = json!([]);
        }
        value
    }
    fn manifest(note: &(KnowledgeDocumentInfo, String)) -> Value {
        json!([{"note":note.0,"body_sha256":format!("{:x}",Sha256::digest(note.1.as_bytes()))}])
    }
    fn serve(steps: Vec<(&'static str, Value)>) -> (String, thread::JoinHandle<()>) {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for (route, body) in steps {
                let (mut stream, _) = socket.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(4)))
                    .unwrap();
                let mut incoming = Vec::new();
                while !incoming.windows(4).any(|window| window == b"\r\n\r\n") {
                    let mut bytes = [0u8; 1024];
                    let count = stream.read(&mut bytes).unwrap();
                    if count == 0 {
                        break;
                    }
                    incoming.extend_from_slice(&bytes[..count]);
                    assert!(incoming.len() < 65536);
                }
                assert!(
                    String::from_utf8_lossy(&incoming)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .contains(route)
                );
                let payload = body.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",payload.len(),payload).unwrap();
            }
        });
        (url, handle)
    }
    fn client(url: &str) -> AuthorityHttp {
        AuthorityHttp::new(url, "token", "device-a").unwrap()
    }
    const FINGERPRINT_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const FINGERPRINT_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn stable_checkpoint_publishes_with_explicit_source_revision() {
        let document = note("item", "notes/item.md");
        let stable = snapshot(7, FINGERPRINT_A);
        let steps = vec![
            ("authoritative/snapshot", stable.clone()),
            ("markdown-manifest", manifest(&document)),
            (
                "markdown-by-id?id=item",
                json!({"note":document.0,"body":document.1}),
            ),
            ("markdown-manifest", manifest(&document)),
            ("authoritative/snapshot", stable.clone()),
        ];
        let (url, worker) = serve(steps);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&stable).unwrap();
        let report = replica
            .sync_markdown_mirror_detailed(&client(&url))
            .unwrap();
        assert_eq!(report.downloaded_documents, 1);
        let status = replica.markdown_mirror_status().unwrap();
        assert_eq!(status.source_revision, Some(7));
        assert!(status.source_inventory_verified);
        worker.join().unwrap();
    }

    fn verified_replica() -> AuthorityReplica {
        let document = note("item", "notes/item.md");
        let stable = snapshot(7, FINGERPRINT_A);
        let (url, worker) = serve(vec![
            ("authoritative/snapshot", stable.clone()),
            ("markdown-manifest", manifest(&document)),
            (
                "markdown-by-id?id=item",
                json!({"note":document.0,"body":document.1}),
            ),
            ("markdown-manifest", manifest(&document)),
            ("authoritative/snapshot", stable.clone()),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&stable).unwrap();
        replica
            .sync_markdown_mirror_detailed(&client(&url))
            .unwrap();
        worker.join().unwrap();
        replica
    }

    #[test]
    fn verified_mirror_rejects_silent_body_corruption() {
        let replica = verified_replica();
        assert!(
            replica
                .markdown_mirror_status()
                .unwrap()
                .source_inventory_verified
        );
        replica
            .test_corrupt_cached_body("item", "---\norbit_id: item\n---\nchanged")
            .unwrap();
        assert!(replica.markdown_mirror_status().is_err());
        assert!(replica.convergence_status().is_err());
    }

    #[test]
    fn verified_mirror_rejects_corrupted_metadata() {
        let replica = verified_replica();
        replica
            .test_corrupt_cached_metadata("item", "broken json")
            .unwrap();
        assert!(replica.markdown_mirror_status().is_err());
        assert!(replica.convergence_status().is_err());
    }

    #[test]
    fn verified_mirror_rejects_identity_substitution() {
        let replica = verified_replica();
        replica
            .test_corrupt_cached_metadata(
                "item",
                &json!({
                    "id":"imposter","path":"notes/item.md","title":"Title item"
                })
                .to_string(),
            )
            .unwrap();
        assert!(replica.markdown_mirror_status().is_err());
    }

    #[test]
    fn concurrent_server_revision_change_preserves_prior_cache() {
        let document = note("item", "notes/item.md");
        let stable = snapshot(7, FINGERPRINT_A);
        let newer = snapshot(8, FINGERPRINT_B);
        let steps = vec![
            ("authoritative/snapshot", stable.clone()),
            ("markdown-manifest", manifest(&document)),
            (
                "markdown-by-id?id=item",
                json!({"note":document.0,"body":document.1}),
            ),
            ("markdown-manifest", manifest(&document)),
            ("authoritative/snapshot", newer),
        ];
        let (url, worker) = serve(steps);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&stable).unwrap();
        let old = note("old", "old.md");
        replica
            .replace_markdown_mirror(std::slice::from_ref(&old))
            .unwrap();
        let previous = replica.markdown_mirror_status().unwrap();
        assert!(
            replica
                .sync_markdown_mirror_detailed(&client(&url))
                .is_err()
        );
        assert_eq!(replica.cached_markdown("old").unwrap(), Some(old));
        assert_eq!(replica.markdown_mirror_status().unwrap(), previous);
        assert!(replica.cached_markdown("item").unwrap().is_none());
        worker.join().unwrap();
    }

    #[test]
    fn stale_replica_rejects_transfer_before_downloading_bodies() {
        let local = snapshot(4, FINGERPRINT_A);
        let remote = snapshot(5, FINGERPRINT_B);
        let (url, worker) = serve(vec![("authoritative/snapshot", remote)]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&local).unwrap();
        assert!(
            replica
                .sync_markdown_mirror_detailed(&client(&url))
                .is_err()
        );
        assert_eq!(replica.revision().unwrap(), Some(4));
        assert!(!replica.markdown_mirror_status().unwrap().complete);
        worker.join().unwrap();
    }
}
