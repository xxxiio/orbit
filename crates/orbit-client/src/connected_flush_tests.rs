//! Connected outbox flush, per-command state and offline-first detail reads.
//!
//! Every server interaction here is a scripted loopback socket: the test asserts
//! the exact method/path of each request, so a command that must not be resent
//! fails the test if any unexpected request arrives.
use crate::authority::{AuthorityError, FlushOutcome};
use crate::unified::{AuthorityConnection, ClientConnectionError};
use orbit_runtime::domain_commands::{TaskAction, TaskCommand};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Seen {
    method: String,
    target: String,
    body: Vec<u8>,
}

enum Respond {
    Json { status: u16, body: Value },
}

struct Expect {
    method: &'static str,
    path: String,
    respond: Respond,
}

fn get(path: &str, body: Value) -> Expect {
    Expect {
        method: "GET",
        path: path.to_owned(),
        respond: Respond::Json { status: 200, body },
    }
}

fn post(path: &str, body: Value) -> Expect {
    Expect {
        method: "POST",
        path: path.to_owned(),
        respond: Respond::Json { status: 200, body },
    }
}

fn post_status(path: &str, status: u16, body: Value) -> Expect {
    Expect {
        method: "POST",
        path: path.to_owned(),
        respond: Respond::Json { status, body },
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("local addr").port()
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn read_request(socket: &mut TcpStream) -> Seen {
    let mut buffer = [0_u8; 8192];
    let mut raw = Vec::new();
    while !raw.windows(4).any(|part| part == b"\r\n\r\n") {
        let read = socket.read(&mut buffer).expect("read headers");
        assert!(read > 0, "connection closed before request headers");
        raw.extend_from_slice(&buffer[..read]);
        assert!(raw.len() < 128 * 1024, "oversized request header");
    }
    let split = raw
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .expect("header terminator")
        + 4;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let lower = head.to_ascii_lowercase();
    let content_length: usize = lower
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = raw[split..].to_vec();
    while body.len() < content_length {
        let read = socket.read(&mut buffer).expect("read body");
        assert!(read > 0, "connection closed before request body");
        body.extend_from_slice(&buffer[..read]);
    }
    body.truncate(content_length);
    assert!(
        lower.contains("authorization: bearer test-token"),
        "missing bearer token in {head}"
    );
    assert!(
        lower.contains("x-orbit-device-id: device-1"),
        "missing device header in {head}"
    );
    let mut parts = head.lines().next().unwrap_or_default().split_whitespace();
    Seen {
        method: parts.next().unwrap_or_default().to_owned(),
        target: parts.next().unwrap_or_default().to_owned(),
        body,
    }
}

fn accept_with_deadline(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    listener
        .set_nonblocking(true)
        .expect("non-blocking listener");
    loop {
        match listener.accept() {
            Ok((socket, _)) => {
                listener.set_nonblocking(false).expect("blocking listener");
                return socket;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "timed out waiting for a request");
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    }
}

fn serve(port: u16, script: Vec<Expect>) -> thread::JoinHandle<Vec<Seen>> {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind scripted server");
    thread::spawn(move || {
        let mut seen = Vec::new();
        for expect in script {
            let mut socket = accept_with_deadline(&listener);
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("socket timeout");
            let request = read_request(&mut socket);
            assert_eq!(request.method, expect.method, "unexpected method");
            assert!(
                request.target.contains(&expect.path),
                "unexpected target {} (wanted {})",
                request.target,
                expect.path
            );
            seen.push(request);
            let Respond::Json { status, body } = expect.respond;
            let raw = body.to_string();
            let reason = match status {
                200 => "200 OK",
                400 => "400 Bad Request",
                404 => "404 Not Found",
                409 => "409 Conflict",
                _ => "500 Internal Server Error",
            };
            let response = format!(
                "HTTP/1.1 {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{raw}",
                raw.len()
            );
            socket.write_all(response.as_bytes()).expect("write reply");
        }
        seen
    })
}

/// Observe a quiet window: any request at all is recorded and answered with an
/// error, so a "nothing was resent" assertion cannot pass by accident.
fn serve_quiet(port: u16, window: Duration) -> thread::JoinHandle<Vec<Seen>> {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind quiet server");
    thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("non-blocking listener");
        let deadline = Instant::now() + window;
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    socket
                        .set_nonblocking(false)
                        .expect("blocking accepted socket");
                    seen.push(read_request(&mut socket));
                    let _ = socket.write_all(
                        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("quiet accept failed: {error}"),
            }
        }
        seen
    })
}

fn remote(port: u16) -> AuthorityConnection {
    AuthorityConnection::remote(
        ":memory:",
        &base_url(port),
        Some(("device-1", "test-token")),
    )
    .expect("open remote")
}

fn offline_remote() -> AuthorityConnection {
    AuthorityConnection::remote(":memory:", "https://example.org", None).expect("open offline")
}

fn task_value(description: &str) -> Value {
    json!({
        "id": "task-1",
        "title": "Task 1",
        "status": "planned",
        "project_id": "project-1",
        "milestone_id": null,
        "estimated_minutes": 30,
        "earliest_start_at_ms": null,
        "deadline_at_ms": null,
        "description": description,
        "priority": "low",
        "labels": ["api"],
    })
}

fn task_receipt(revision: i64) -> Value {
    json!({"revision": revision, "duplicate": false, "task": task_value("server body")})
}

fn accept_snapshot(remote: &mut AuthorityConnection, revision: i64, tasks: Vec<Value>) {
    let AuthorityConnection::Remote { replica, .. } = remote else {
        panic!("expected a remote connection");
    };
    replica
        .accept_snapshot_for_testing(&json!({
            "revision": revision,
            "history_fingerprint": "a".repeat(64),
            "projects": [],
            "tasks": tasks,
            "task_dependencies": [],
            "habits": [],
            "habit_occurrences": [],
            "metrics": [],
            "metric_observations": [],
            "schedule": [],
            "routines": [],
            "milestones": [],
            "strategies": [],
        }))
        .expect("accept snapshot");
}

fn set_details(command_id: &str, task_id: &str) -> TaskCommand {
    TaskCommand {
        command_id: command_id.to_owned(),
        expected_revision: 0,
        action: TaskAction::SetDetails {
            task_id: task_id.to_owned(),
            description: "pending body".into(),
            priority: Some("high".into()),
            labels: vec!["ui".into()],
        },
    }
}

/// Defect 1: a 400 is a domain rejection, not an outage. It must mark only the
/// rejected row conflicted and must never wedge later commands.
#[test]
fn http_400_marks_one_conflict_and_later_commands_still_flush() {
    let port = free_port();
    let mut remote = remote(port);
    remote.task_command(set_details("cmd-1", "task-1")).unwrap();
    remote.task_command(set_details("cmd-2", "task-2")).unwrap();
    assert_eq!(remote.pending_command_count().unwrap(), 2);

    let first = serve(
        port,
        vec![
            get("/v1/authoritative/snapshot", json!({"revision": 3})),
            post_status(
                "/v1/authoritative/task-commands",
                400,
                json!({"error": "task task-1 does not accept rich details"}),
            ),
            get("/v1/authoritative/snapshot", json!({"revision": 3})),
            post(
                "/v1/authoritative/task-commands",
                json!({
                    "revision": 4,
                    "duplicate": false,
                    "task": {"id": "task-2", "title": "Task 2", "status": "planned"},
                }),
            ),
        ],
    );
    match remote.flush() {
        Ok(FlushOutcome::Blocked {
            accepted, reason, ..
        }) => {
            assert_eq!(
                accepted, 1,
                "the later row was still accepted; only row 1 is blocked"
            );
            assert!(
                reason.contains("does not accept rich details"),
                "the server rejection reason must be reported: {reason}"
            );
        }
        other => panic!("expected a blocked outcome, got {other:?}"),
    }
    let seen = first.join().expect("server thread");
    assert_eq!(seen.len(), 4, "row 2 must be attempted after row 1 failed");
    assert!(seen[1].target.contains("task-commands"));

    assert_eq!(
        remote.command_state("cmd-1").unwrap().as_deref(),
        Some("conflict"),
        "a domain rejection is permanent"
    );
    assert_eq!(
        remote.command_state("cmd-2").unwrap().as_deref(),
        Some("accepted"),
        "a later row must not be blocked by an earlier rejection"
    );

    // A command queued after the conflict must still be submitted, and the
    // permanently rejected row must never be resent.
    remote.task_command(set_details("cmd-3", "task-3")).unwrap();
    let second = serve(
        port,
        vec![
            get("/v1/authoritative/snapshot", json!({"revision": 4})),
            post(
                "/v1/authoritative/task-commands",
                json!({
                    "revision": 5,
                    "duplicate": false,
                    "task": {"id": "task-3", "title": "Task 3", "status": "planned"},
                }),
            ),
        ],
    );
    match remote.flush() {
        Ok(FlushOutcome::Blocked {
            accepted, reason, ..
        }) => {
            assert_eq!(accepted, 1, "the new row still flushed");
            assert!(reason.contains("conflict") || reason.contains("reconciliation"));
        }
        other => panic!("expected a blocked outcome, got {other:?}"),
    }
    let seen = second.join().expect("server thread");
    assert_eq!(seen.len(), 2);
    assert!(
        seen[1].target.contains("task-commands"),
        "only the remaining row is sent"
    );
    let resent = String::from_utf8_lossy(&seen[1].body);
    assert!(
        resent.contains("cmd-3"),
        "the remaining row is cmd-3: {resent}"
    );
    assert!(
        !resent.contains("cmd-1"),
        "a permanent rejection is never resent"
    );

    // Nothing pending remains; flushing again performs no network I/O.
    let quiet = serve_quiet(port, Duration::from_millis(200));
    assert!(matches!(
        remote.flush(),
        Ok(FlushOutcome::Blocked { accepted: 0, .. })
    ));
    assert!(
        quiet.join().expect("quiet server").is_empty(),
        "a conflict-only queue must not transmit"
    );
}

/// Defect 2: the accepted read must happen before the queue. A failed read
/// leaves the outbox untouched so the caller can retry safely.
#[test]
fn failed_accepted_read_never_queues_a_rich_mutation() {
    let mut remote = remote(free_port());
    accept_snapshot(&mut remote, 4, vec![]);

    let error = remote
        .update_task_details(
            "task-1".into(),
            "new body".into(),
            Some("high".into()),
            vec!["ui".into()],
        )
        .expect_err("an unreachable accepted read must fail");
    assert!(
        matches!(
            error,
            ClientConnectionError::Remote(AuthorityError::Transport(_))
                | ClientConnectionError::Remote(AuthorityError::Unavailable(_))
        ),
        "unexpected error {error:?}"
    );
    assert_eq!(remote.pending_command_count().unwrap(), 0);
    assert!(remote.unresolved_commands().unwrap().is_empty());
    assert!(remote.last_queued_command_id().is_none());

    let error = remote
        .add_acceptance_criterion("task-1".into(), "Works offline".into())
        .expect_err("an unreachable criteria read must fail");
    assert!(
        matches!(error, ClientConnectionError::Remote(_)),
        "unexpected error {error:?}"
    );
    assert_eq!(remote.pending_command_count().unwrap(), 0);
    assert!(remote.unresolved_commands().unwrap().is_empty());
    assert!(remote.last_queued_command_id().is_none());
}

/// Defect 2: when the read succeeds the queued mutation returns the value the
/// server had *before* the queue, never the pending value.
#[test]
fn successful_queue_returns_the_pre_read_accepted_value() {
    let mut remote = offline_remote();
    accept_snapshot(&mut remote, 7, vec![task_value("old body")]);

    let returned = remote
        .update_task_details(
            "task-1".into(),
            "pending body".into(),
            Some("HIGH".into()),
            vec![" ui ".into(), "ui".into()],
        )
        .expect("queue details offline");
    assert_eq!(returned.description, "old body");
    assert_eq!(returned.priority.as_deref(), Some("low"));
    assert_eq!(returned.labels, vec!["api".to_string()]);
    assert_eq!(remote.pending_command_count().unwrap(), 1);

    let pending = remote.unresolved_commands().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["action"]["description"], "pending body");
    assert_eq!(
        pending[0]["action"]["priority"], "high",
        "priority is canonicalised before queueing"
    );
    assert_eq!(
        pending[0]["action"]["labels"],
        json!(["ui"]),
        "labels are trimmed and de-duplicated by the domain"
    );
}

/// Defect 3: a frontend can follow one specific command identity.
#[test]
fn queued_command_state_moves_from_pending_to_accepted() {
    let port = free_port();
    let mut remote = remote(port);
    remote
        .task_command(set_details("details-1", "task-1"))
        .unwrap();
    assert_eq!(
        remote.last_queued_command_id().as_deref(),
        Some("details-1")
    );
    assert_eq!(
        remote.command_state("details-1").unwrap().as_deref(),
        Some("pending")
    );
    assert_eq!(
        remote.command_state("never-queued").unwrap(),
        None,
        "an unknown identity is not reported as pending"
    );
    assert_eq!(remote.pending_command_count().unwrap(), 1);

    let server = serve(
        port,
        vec![
            get("/v1/authoritative/snapshot", json!({"revision": 3})),
            post("/v1/authoritative/task-commands", task_receipt(4)),
        ],
    );
    assert!(matches!(remote.flush(), Ok(FlushOutcome::Accepted(1))));
    server.join().expect("server thread");
    assert_eq!(
        remote.command_state("details-1").unwrap().as_deref(),
        Some("accepted")
    );
    assert_eq!(remote.pending_command_count().unwrap(), 0);
}

/// Defect 4: validation happens before queueing, with actionable input errors.
#[test]
fn invalid_details_and_criterion_text_produce_no_outbox_entry() {
    let mut remote = offline_remote();
    accept_snapshot(&mut remote, 2, vec![task_value("old body")]);

    let invalid_priority = remote
        .update_task_details(
            "task-1".into(),
            "body".into(),
            Some("critical".into()),
            Vec::new(),
        )
        .expect_err("unknown priority is rejected");
    assert!(
        matches!(
            invalid_priority,
            ClientConnectionError::Local(ref error)
                if error.code == crate::ClientErrorCode::InvalidInput
                    && error.message.contains("low, medium, high, urgent")
        ),
        "unexpected error {invalid_priority:?}"
    );

    let invalid_label = remote
        .update_task_details(
            "task-1".into(),
            "body".into(),
            None,
            vec!["bad\nlabel".into()],
        )
        .expect_err("control characters in a label are rejected");
    assert!(
        matches!(
            invalid_label,
            ClientConnectionError::Local(ref error)
                if error.code == crate::ClientErrorCode::InvalidInput
        ),
        "unexpected error {invalid_label:?}"
    );

    let blank_criterion = remote
        .add_acceptance_criterion("task-1".into(), "   ".into())
        .expect_err("blank criterion text is rejected");
    assert!(
        matches!(
            blank_criterion,
            ClientConnectionError::Local(ref error)
                if error.code == crate::ClientErrorCode::InvalidInput
                    && error.message.contains("acceptance criterion")
        ),
        "unexpected error {blank_criterion:?}"
    );

    let long_criterion = remote
        .set_acceptance_criterion_text("task-1".into(), "criterion-1".into(), "x".repeat(2_001))
        .expect_err("over-long criterion text is rejected");
    assert!(matches!(
        long_criterion,
        ClientConnectionError::Local(ref error)
            if error.code == crate::ClientErrorCode::InvalidInput
    ));

    assert_eq!(
        remote.pending_command_count().unwrap(),
        0,
        "invalid input must never reach the durable outbox"
    );
    assert!(remote.unresolved_commands().unwrap().is_empty());
    assert!(remote.last_queued_command_id().is_none());
}

/// Defect 5: the accepted snapshot is the first source for task detail, so
/// offline connected detail editing can still load accepted values.
#[test]
fn task_detail_reads_the_accepted_snapshot_before_the_route() {
    let mut remote = remote(free_port());
    accept_snapshot(&mut remote, 5, vec![task_value("accepted body")]);

    let detail = remote
        .task_detail("task-1")
        .expect("accepted snapshot detail without a server");
    assert_eq!(detail.description, "accepted body");
    assert_eq!(detail.priority.as_deref(), Some("low"));
    assert_eq!(detail.labels, vec!["api".to_string()]);
    assert_eq!(detail.estimated_minutes, Some(30));

    // A task the snapshot does not contain still uses the authoritative route,
    // and an unreachable server is an explicit typed failure, never a fabricated
    // detail projection.
    assert!(matches!(
        remote.task_detail("task-absent"),
        Err(ClientConnectionError::Remote(_))
    ));
}

/// Defect 1: a 404 for an outbox route is permanent too. The same request can
/// never succeed, so the row must not wedge the queue forever.
#[test]
fn http_404_for_an_outbox_route_is_a_permanent_conflict() {
    let port = free_port();
    let mut remote = remote(port);
    remote
        .task_command(set_details("cmd-missing", "task-1"))
        .unwrap();
    let server = serve(
        port,
        vec![
            get("/v1/authoritative/snapshot", json!({"revision": 2})),
            post_status(
                "/v1/authoritative/task-commands",
                404,
                json!({"error": "route retired"}),
            ),
        ],
    );
    match remote.flush() {
        Ok(FlushOutcome::Blocked {
            accepted, reason, ..
        }) => {
            assert_eq!(accepted, 0);
            assert!(reason.contains("not found"), "{reason}");
        }
        other => panic!("expected a blocked outcome, got {other:?}"),
    }
    server.join().expect("server thread");
    assert_eq!(
        remote.command_state("cmd-missing").unwrap().as_deref(),
        Some("conflict")
    );
    assert_eq!(remote.pending_command_count().unwrap(), 1);
}

/// Defect 3: the aggregate pending count includes durable attachment uploads,
/// so a frontend cannot mistake a staged file for a settled workspace.
#[test]
fn pending_count_includes_queued_commands_and_attachment_uploads() {
    let directory = tempfile::tempdir().expect("tempdir");
    let source = directory.path().join("note.txt");
    std::fs::write(&source, b"pending bytes").expect("write source");
    let mut remote = offline_remote();
    remote.task_command(set_details("cmd-1", "task-1")).unwrap();
    assert_eq!(remote.pending_command_count().unwrap(), 1);
    remote
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("stage attachment offline");
    assert_eq!(
        remote.pending_command_count().unwrap(),
        2,
        "a staged upload is a durable pending intention too"
    );
    assert_eq!(
        remote.command_state("cmd-1").unwrap().as_deref(),
        Some("pending")
    );
    assert_eq!(remote.last_queued_command_id().as_deref(), Some("cmd-1"));
}
