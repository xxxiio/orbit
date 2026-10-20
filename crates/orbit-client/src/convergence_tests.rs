//! Behavioral contracts for client-side, snapshot-bound conflict reconciliation.
//! These tests deliberately use two independent SQLite replicas to exercise
//! cross-device divergence without pretending offline clients share pending state.
#![cfg(test)]
use crate::authority::{AuthorityError, AuthorityReplica, CommandKind};
use serde_json::{Value, json};

fn snapshot(revision: i64, marker: char) -> Value {
    json!({
        "revision":revision,
        "history_fingerprint":marker.to_string().repeat(64),
        "projects":[],"tasks":[],"habits":[],"habit_occurrences":[],
        "metrics":[],"metric_observations":[],"schedule":[],"routines":[],
        "milestones":[],"strategies":[],
    })
}
fn task(id: &str) -> Value {
    json!({"type":"create","task_id":id,"title":"reviewed"})
}
fn rejected(replica: &mut AuthorityReplica, id: &str) -> Result<(), AuthorityError> {
    replica.queue(id, CommandKind::Task, &task("t-original"))?;
    // Simulate the server's conflict disposition after a rejected write. This
    // does not modify the accepted server snapshot.
    replica.test_mark_conflicted(id)?;
    Ok(())
}

#[test]
fn status_on_new_replica_cannot_claim_server_convergence() {
    let replica = AuthorityReplica::open(":memory:").unwrap();
    let status = replica.convergence_status().unwrap();
    assert_eq!(status.accepted_revision, None);
    assert_eq!(status.pending, 0);
    assert_eq!(status.conflicts, 0);
    assert!(!status.server_convergence_verified);
}
#[test]
fn pending_intentions_do_not_increment_accepted_revision() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(7, 'a')).unwrap();
    replica
        .queue("queued-1", CommandKind::Task, &task("t1"))
        .unwrap();
    replica
        .queue("queued-2", CommandKind::Task, &task("t2"))
        .unwrap();
    let status = replica.convergence_status().unwrap();
    assert_eq!(status.accepted_revision, Some(7));
    assert_eq!(status.pending, 2);
    assert_eq!(status.conflicts, 0);
    assert!(!status.server_convergence_verified);
}
#[test]
fn review_requires_a_conflict_and_accepted_snapshot() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica
        .queue("first", CommandKind::Task, &task("a"))
        .unwrap();
    assert!(replica.conflict_review("first").is_err());
    replica.accept_snapshot(&snapshot(4, 'c')).unwrap();
    assert!(replica.conflict_review("first").is_err());
    replica.test_mark_conflicted("first").unwrap();
    let review = replica.conflict_review("first").unwrap();
    assert_eq!(review.rejected_id, "first");
    assert_eq!(review.accepted_revision, 4);
    assert_eq!(review.ordinal, 1);
}
#[test]
fn accepted_history_advance_invalidates_review_without_mutating_queue() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(1, 'a')).unwrap();
    rejected(&mut replica, "first").unwrap();
    let review = replica.conflict_review("first").unwrap();
    replica.accept_snapshot(&snapshot(2, 'b')).unwrap();
    assert!(
        replica
            .replace_reviewed_conflict(&review, "replacement", CommandKind::Task, &task("b"))
            .is_err()
    );
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "conflict"
    );
    assert!(replica.command_status("replacement").unwrap().is_none());
}
#[test]
fn changed_rejected_action_invalidates_review_without_mutation() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(1, 'a')).unwrap();
    rejected(&mut replica, "first").unwrap();
    let mut review = replica.conflict_review("first").unwrap();
    review.rejected_action = task("forged");
    assert!(
        replica
            .replace_reviewed_conflict(&review, "replacement", CommandKind::Task, &task("b"))
            .is_err()
    );
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "conflict"
    );
}
#[test]
fn revision_and_fingerprint_both_required() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(2, 'a')).unwrap();
    rejected(&mut replica, "first").unwrap();
    let mut review = replica.conflict_review("first").unwrap();
    review.history_fingerprint = "b".repeat(64);
    assert!(
        replica
            .replace_reviewed_conflict(&review, "replacement", CommandKind::Task, &task("b"))
            .is_err()
    );
    review.history_fingerprint = "a".repeat(64);
    review.accepted_revision = 3;
    assert!(
        replica
            .replace_reviewed_conflict(&review, "replacement", CommandKind::Task, &task("b"))
            .is_err()
    );
}
#[test]
fn reviewed_replacement_preserves_rejected_audit_and_global_position() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(3, 'a')).unwrap();
    rejected(&mut replica, "first").unwrap();
    replica
        .queue("later", CommandKind::Task, &task("later"))
        .unwrap();
    let review = replica.conflict_review("first").unwrap();
    replica
        .replace_reviewed_conflict(&review, "reviewed", CommandKind::Task, &task("new"))
        .unwrap();
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "superseded"
    );
    let replacement = replica.command_status("reviewed").unwrap().unwrap();
    assert_eq!(replacement["state"], "pending");
    assert_eq!(replacement["expected_revision"], 3);
    let unresolved = replica.unresolved().unwrap();
    assert_eq!(unresolved[0]["command_id"], "reviewed");
    assert_eq!(unresolved[1]["command_id"], "later");
    assert!(replica.audit().unwrap().healthy);
}
#[test]
fn previously_used_replacement_identity_is_never_recycled() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(3, 'a')).unwrap();
    rejected(&mut replica, "first").unwrap();
    replica
        .queue("used", CommandKind::Task, &task("x"))
        .unwrap();
    let review = replica.conflict_review("first").unwrap();
    assert!(
        replica
            .replace_reviewed_conflict(&review, "used", CommandKind::Task, &task("x"))
            .is_err()
    );
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "conflict"
    );
}
#[test]
fn replacement_must_keep_original_command_family() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(3, 'a')).unwrap();
    rejected(&mut replica, "first").unwrap();
    let review = replica.conflict_review("first").unwrap();
    assert!(
        replica
            .replace_reviewed_conflict(&review, "different", CommandKind::Evidence, &task("x"))
            .is_err()
    );
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "conflict"
    );
}
#[test]
fn replacement_rejects_embedded_envelope_and_malformed_identity() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(3, 'a')).unwrap();
    rejected(&mut replica, "first").unwrap();
    let review = replica.conflict_review("first").unwrap();
    for (id, action) in [
        ("bad id", task("x")),
        ("ok", json!({"command_id":"stolen","type":"create"})),
        ("also-ok", json!({"expected_revision":3,"type":"create"})),
        ("empty", json!({})),
    ] {
        assert!(
            replica
                .replace_reviewed_conflict(&review, id, CommandKind::Task, &action)
                .is_err()
        );
    }
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "conflict"
    );
}
#[test]
fn two_clients_have_independent_pending_queues_but_share_accepted_revision() {
    let mut a = AuthorityReplica::open(":memory:").unwrap();
    let mut b = AuthorityReplica::open(":memory:").unwrap();
    a.accept_snapshot(&snapshot(4, 'a')).unwrap();
    b.accept_snapshot(&snapshot(4, 'a')).unwrap();
    a.queue("client-a", CommandKind::Task, &task("a")).unwrap();
    b.queue("client-b", CommandKind::Task, &task("b")).unwrap();
    assert_eq!(a.convergence_status().unwrap().accepted_revision, Some(4));
    assert_eq!(b.convergence_status().unwrap().accepted_revision, Some(4));
    assert_eq!(a.convergence_status().unwrap().pending, 1);
    assert_eq!(b.convergence_status().unwrap().pending, 1);
    a.accept_snapshot(&snapshot(5, 'b')).unwrap();
    assert_eq!(a.convergence_status().unwrap().accepted_revision, Some(5));
    assert_eq!(b.convergence_status().unwrap().accepted_revision, Some(4));
    assert_eq!(b.convergence_status().unwrap().pending, 1);
}
#[test]
fn offline_snapshot_status_is_never_marked_live_verified() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(100, 'f')).unwrap();
    assert!(
        !replica
            .convergence_status()
            .unwrap()
            .server_convergence_verified
    );
}

mod network_contract {
    use super::*;
    use crate::authority::AuthorityHttp;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    struct Response {
        expected_path: &'static str,
        body: Value,
        status: u16,
    }
    fn serve(responses: Vec<Response>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0_u8; 16384];
                let mut request = Vec::new();
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let bytes = socket.read(&mut buffer).unwrap();
                    if bytes == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..bytes]);
                    assert!(request.len() < 65536, "unexpected oversized test request");
                }
                let start = String::from_utf8_lossy(&request);
                let first = start.lines().next().unwrap_or("");
                assert!(
                    first.contains(response.expected_path),
                    "unexpected HTTP request {first}"
                );
                let body = response.body.to_string();
                let status = if response.status == 200 {
                    "200 OK"
                } else {
                    "409 Conflict"
                };
                let raw = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(raw.as_bytes()).unwrap();
                socket.flush().unwrap();
            }
        });
        (address, handle)
    }
    fn response(path: &'static str, body: Value) -> Response {
        Response {
            expected_path: path,
            body,
            status: 200,
        }
    }
    fn http(base: &str) -> AuthorityHttp {
        AuthorityHttp::new(base, "test-token", "test-device").unwrap()
    }

    #[test]
    fn observed_server_revision_newer_than_replica_is_not_converged() {
        let server_snapshot = snapshot(6, 'b');
        let (base, handle) = serve(vec![response(
            "/v1/authoritative/snapshot",
            server_snapshot,
        )]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(5, 'a')).unwrap();
        replica
            .queue("offline-command", CommandKind::Task, &task("t"))
            .unwrap();
        let observation = replica.observe_convergence(&http(&base)).unwrap();
        assert_eq!(observation.server_revision, 6);
        assert_eq!(observation.local.accepted_revision, Some(5));
        assert_eq!(observation.local.pending, 1);
        assert!(!observation.accepted_snapshot_matches);
        assert!(!observation.clean_outbox);
        assert!(!observation.converged_when_observed);
        // Observation is read only: no accidental acceptance.
        assert_eq!(replica.revision().unwrap(), Some(5));
        assert_eq!(
            replica.command_status("offline-command").unwrap().unwrap()["state"],
            "pending"
        );
        handle.join().unwrap();
    }
    #[test]
    fn equal_revision_with_changed_fingerprint_never_counts_as_convergence() {
        let (base, handle) = serve(vec![response(
            "/v1/authoritative/snapshot",
            snapshot(5, 'b'),
        )]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(5, 'a')).unwrap();
        let observation = replica.observe_convergence(&http(&base)).unwrap();
        assert!(!observation.accepted_snapshot_matches);
        assert!(!observation.converged_when_observed);
        assert!(observation.clean_outbox);
        handle.join().unwrap();
    }
    #[test]
    fn matching_revision_and_empty_outbox_is_converged_only_at_observation() {
        let (base, handle) = serve(vec![response(
            "/v1/authoritative/snapshot",
            snapshot(5, 'a'),
        )]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(5, 'a')).unwrap();
        let observation = replica.observe_convergence(&http(&base)).unwrap();
        assert!(observation.accepted_snapshot_matches);
        assert!(observation.clean_outbox);
        assert!(observation.converged_when_observed);
        assert!(!observation.local.server_convergence_verified);
        handle.join().unwrap();
    }
    #[test]
    fn remote_review_stale_after_second_client_writes_is_rejected() {
        let (base, handle) = serve(vec![
            response("/v1/authoritative/snapshot", snapshot(5, 'a')),
            response("/v1/authoritative/snapshot", snapshot(6, 'b')),
            response(
                "/v1/authoritative/history-prefix?revision=5",
                json!({
                "revision":5,"history_fingerprint":"a".repeat(64)}),
            ),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(5, 'a')).unwrap();
        rejected(&mut replica, "rejected").unwrap();
        let transport = http(&base);
        let review = replica
            .review_conflict_online(&transport, "rejected")
            .unwrap();
        assert_eq!(review.accepted_revision, 5);
        assert!(
            replica
                .replace_reviewed_conflict_online(
                    &transport,
                    &review,
                    "replacement",
                    CommandKind::Task,
                    &task("revised"),
                )
                .is_err()
        );
        assert_eq!(replica.revision().unwrap(), Some(6));
        assert_eq!(
            replica.command_status("rejected").unwrap().unwrap()["state"],
            "conflict"
        );
        assert!(replica.command_status("replacement").unwrap().is_none());
        handle.join().unwrap();
    }
    #[test]
    fn online_review_and_replacement_freeze_the_reviewed_revision() {
        let (base, handle) = serve(vec![
            response("/v1/authoritative/snapshot", snapshot(9, 'a')),
            response("/v1/authoritative/snapshot", snapshot(9, 'a')),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(9, 'a')).unwrap();
        rejected(&mut replica, "conflict").unwrap();
        let transport = http(&base);
        let review = replica
            .review_conflict_online(&transport, "conflict")
            .unwrap();
        replica
            .replace_reviewed_conflict_online(
                &transport,
                &review,
                "new-action",
                CommandKind::Task,
                &task("new"),
            )
            .unwrap();
        assert_eq!(
            replica.command_status("new-action").unwrap().unwrap()["expected_revision"],
            9
        );
        assert_eq!(
            replica.command_status("conflict").unwrap().unwrap()["state"],
            "superseded"
        );
        handle.join().unwrap();
    }
    #[test]
    fn two_replicas_observe_same_server_without_sharing_pending_intentions() {
        let accepted = snapshot(10, 'a');
        let (base, handle) = serve(vec![
            response("/v1/authoritative/snapshot", accepted.clone()),
            response("/v1/authoritative/snapshot", accepted),
        ]);
        let mut a = AuthorityReplica::open(":memory:").unwrap();
        let mut b = AuthorityReplica::open(":memory:").unwrap();
        a.accept_snapshot(&snapshot(10, 'a')).unwrap();
        b.accept_snapshot(&snapshot(10, 'a')).unwrap();
        a.queue("a-work", CommandKind::Task, &task("a")).unwrap();
        let observed_a = a.observe_convergence(&http(&base)).unwrap();
        let observed_b = b.observe_convergence(&http(&base)).unwrap();
        assert!(!observed_a.converged_when_observed);
        assert!(observed_b.converged_when_observed);
        assert_eq!(observed_a.local.pending, 1);
        assert_eq!(observed_b.local.pending, 0);
        handle.join().unwrap();
    }
}

mod cycle_contract {
    use super::*;
    use crate::authority::AuthorityHttp;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    struct Reply(&'static str, Value);
    fn server(replies: Vec<Reply>) -> (String, thread::JoinHandle<()>) {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", socket.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for Reply(path, body) in replies {
                let (mut stream, _) = socket.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut buf = [0_u8; 8192];
                let mut request = Vec::new();
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                    if request.windows(4).any(|v| v == b"\r\n\r\n") {
                        break;
                    }
                    assert!(request.len() < 32768);
                }
                let text = String::from_utf8_lossy(&request);
                assert!(
                    text.lines().next().unwrap_or("").contains(path),
                    "expected {path}, got request {}",
                    text.lines().next().unwrap_or("")
                );
                let body = body.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                stream.flush().unwrap();
            }
        });
        (base, handle)
    }
    fn connection(server: &str) -> AuthorityHttp {
        AuthorityHttp::new(server, "token", "device-1").unwrap()
    }

    #[test]
    fn clean_reconnect_with_no_local_intentions_is_observed_converged() {
        let (base, server_thread) = server(vec![
            // Reconnect: refresh the accepted projection.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // Markdown mirror: join the transfer to the accepted checkpoint.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            // The checkpoint must still hold after the transfer.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // Final point-in-time observation.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(8, 'a')).unwrap();
        let result = replica.converge(&connection(&base)).unwrap();
        assert_eq!(result.reconnect.accepted_commands, 0);
        assert_eq!(result.reconnect.markdown_documents, Some(0));
        assert_eq!(result.reconnect.unresolved_commands, 0);
        assert!(result.observation.converged_when_observed);
        assert!(result.fully_synchronized_at_observation);
        server_thread.join().unwrap();
    }
    #[test]
    fn second_client_writes_between_refresh_and_observation() {
        let (base, server_thread) = server(vec![
            // Reconnect: refresh the accepted projection.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // Markdown mirror: join the transfer to the accepted checkpoint.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            // The checkpoint must still hold after the transfer.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // A second client advanced history before the final observation.
            Reply("/v1/authoritative/snapshot", snapshot(9, 'b')),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(8, 'a')).unwrap();
        let result = replica.converge(&connection(&base)).unwrap();
        assert_eq!(result.reconnect.accepted_revision, Some(8));
        assert_eq!(result.observation.server_revision, 9);
        assert!(!result.observation.accepted_snapshot_matches);
        assert!(!result.fully_synchronized_at_observation);
        assert_eq!(replica.revision().unwrap(), Some(8));
        server_thread.join().unwrap();
    }
    #[test]
    fn blocked_outbox_keeps_conflict_even_if_accepted_snapshot_is_fresh() {
        let (base, server_thread) = server(vec![
            // Reconnect: refresh the accepted projection.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // Markdown mirror: join the transfer to the accepted checkpoint.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            // The checkpoint must still hold after the transfer.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // Final point-in-time observation.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(8, 'a')).unwrap();
        rejected(&mut replica, "rejected-intent").unwrap();
        let result = replica.converge(&connection(&base)).unwrap();
        assert_eq!(
            result.reconnect.blocked_command.as_deref(),
            Some("rejected-intent")
        );
        assert_eq!(result.reconnect.unresolved_commands, 1);
        assert!(!result.observation.clean_outbox);
        assert!(!result.fully_synchronized_at_observation);
        assert_eq!(
            replica.command_status("rejected-intent").unwrap().unwrap()["state"],
            "conflict"
        );
        server_thread.join().unwrap();
    }
    #[test]
    fn markdown_mirror_failure_cannot_be_reported_as_full_convergence() {
        let (base, server_thread) = server(vec![
            // Reconnect: refresh the accepted projection.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // Markdown mirror: read the accepted checkpoint first.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
            // The malformed inventory aborts the mirror refresh.
            Reply(
                "/v1/authoritative/markdown-manifest",
                json!({"invalid":"inventory"}),
            ),
            // Observation still compares the accepted domain projection.
            Reply("/v1/authoritative/snapshot", snapshot(8, 'a')),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(8, 'a')).unwrap();
        let result = replica.converge(&connection(&base)).unwrap();
        assert!(result.observation.converged_when_observed);
        assert!(result.reconnect.mirror_error.is_some());
        assert!(!result.fully_synchronized_at_observation);
        server_thread.join().unwrap();
    }
    #[test]
    fn staged_task_can_be_accepted_then_observed_from_server() {
        let hash_a = "a".repeat(64);
        let (base, server_thread) = server(vec![
            // Flush freezes the expected revision before posting the command.
            Reply("/v1/authoritative/snapshot", snapshot(5, 'a')),
            Reply(
                "/v1/authoritative/task-commands",
                json!({
                    "revision":6,"duplicate":false,"task":{"id":"task-1"}
                }),
            ),
            // Reconnect: refresh the accepted projection with a prefix proof.
            Reply("/v1/authoritative/snapshot", snapshot(6, 'b')),
            Reply(
                "/v1/authoritative/history-prefix?revision=5",
                json!({
                    "revision":5,"history_fingerprint":hash_a
                }),
            ),
            // Markdown mirror: join the transfer to the accepted checkpoint.
            Reply("/v1/authoritative/snapshot", snapshot(6, 'b')),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            Reply("/v1/authoritative/markdown-manifest", json!([])),
            // The checkpoint must still hold after the transfer.
            Reply("/v1/authoritative/snapshot", snapshot(6, 'b')),
            // Final point-in-time observation.
            Reply("/v1/authoritative/snapshot", snapshot(6, 'b')),
        ]);
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(5, 'a')).unwrap();
        replica
            .queue("intent-1", CommandKind::Task, &task("task-1"))
            .unwrap();
        let result = replica.converge(&connection(&base)).unwrap();
        assert_eq!(result.reconnect.accepted_commands, 1);
        assert_eq!(result.reconnect.accepted_revision, Some(6));
        assert_eq!(result.reconnect.unresolved_commands, 0);
        assert!(result.fully_synchronized_at_observation);
        assert_eq!(
            replica.accepted_receipt("intent-1").unwrap().unwrap()["revision"],
            6
        );
        assert_eq!(
            replica.command_status("intent-1").unwrap().unwrap()["state"],
            "accepted"
        );
        server_thread.join().unwrap();
    }
}

mod revision_guard_contract {
    use super::*;
    #[test]
    fn stale_review_cannot_supersede_a_command_even_when_retry_id_is_valid() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(11, 'a')).unwrap();
        rejected(&mut replica, "reject").unwrap();
        let review = replica.conflict_review("reject").unwrap();
        replica.accept_snapshot(&snapshot(12, 'b')).unwrap();
        for replacement in ["new-a", "new-b", "new-c"] {
            assert!(
                replica
                    .replace_reviewed_conflict(
                        &review,
                        replacement,
                        CommandKind::Task,
                        &task("changed")
                    )
                    .is_err()
            );
            assert!(replica.command_status(replacement).unwrap().is_none());
        }
        assert_eq!(
            replica.command_status("reject").unwrap().unwrap()["state"],
            "conflict"
        );
    }
    #[test]
    fn a_second_review_after_refresh_succeeds_with_new_frozen_revision() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(11, 'a')).unwrap();
        rejected(&mut replica, "reject").unwrap();
        let previous = replica.conflict_review("reject").unwrap();
        replica.accept_snapshot(&snapshot(12, 'b')).unwrap();
        assert!(
            replica
                .replace_reviewed_conflict(&previous, "wrong", CommandKind::Task, &task("changed"))
                .is_err()
        );
        let latest = replica.conflict_review("reject").unwrap();
        assert_eq!(latest.accepted_revision, 12);
        replica
            .replace_reviewed_conflict(&latest, "new", CommandKind::Task, &task("changed"))
            .unwrap();
        assert_eq!(
            replica.command_status("new").unwrap().unwrap()["expected_revision"],
            12
        );
    }
    #[test]
    fn repeated_review_token_is_not_reusable_after_a_successful_replacement() {
        let mut replica = AuthorityReplica::open(":memory:").unwrap();
        replica.accept_snapshot(&snapshot(8, 'a')).unwrap();
        rejected(&mut replica, "reject").unwrap();
        let review = replica.conflict_review("reject").unwrap();
        replica
            .replace_reviewed_conflict(&review, "one", CommandKind::Task, &task("one"))
            .unwrap();
        assert!(
            replica
                .replace_reviewed_conflict(&review, "two", CommandKind::Task, &task("two"))
                .is_err()
        );
        assert!(replica.command_status("two").unwrap().is_none());
        assert_eq!(
            replica.command_status("reject").unwrap().unwrap()["state"],
            "superseded"
        );
    }
}

#[test]
fn domain_snapshot_review_does_not_pretend_to_protect_markdown_revisions() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&snapshot(4, 'a')).unwrap();
    replica
        .queue(
            "markdown-conflict",
            CommandKind::Markdown,
            &json!({"type":"create","note_id":"n","path":"n.md"}),
        )
        .unwrap();
    replica.test_mark_conflicted("markdown-conflict").unwrap();
    let review = replica.conflict_review("markdown-conflict").unwrap();
    assert!(
        replica
            .replace_reviewed_conflict(
                &review,
                "unsafe-replacement",
                CommandKind::Markdown,
                &json!({"type":"create","note_id":"n","path":"n.md"})
            )
            .is_err()
    );
    assert_eq!(
        replica
            .command_status("markdown-conflict")
            .unwrap()
            .unwrap()["state"],
        "conflict"
    );
    assert!(
        replica
            .command_status("unsafe-replacement")
            .unwrap()
            .is_none()
    );
}
