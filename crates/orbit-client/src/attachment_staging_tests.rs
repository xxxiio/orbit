//! Connected-mode attachment staging, retry, integrity and accepted reads.
//!
//! These tests exercise a real HTTP socket: the scripted server asserts the
//! exact method/path/body of every request, so a duplicate flush that sends
//! anything at all fails the test.
use crate::ClientErrorCode;
use crate::unified::{AuthorityConnection, ClientConnectionError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "orbit-client-attachments-{label}-{}-{}",
            process::id(),
            SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create scratch directory");
        Self { path }
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, bytes).expect("write scratch file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Reserve a loopback port so the "offline" phase can run before the server
/// starts listening on exactly the URL the replica is bound to.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("local addr").port()
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

#[derive(Clone)]
struct SeenRequest {
    method: String,
    target: String,
    body: Vec<u8>,
}

enum Responder {
    Json {
        status: u16,
        body: Value,
    },
    Content {
        bytes: Vec<u8>,
        checksum: String,
    },
    /// Accept the request, then close the socket without answering: the
    /// client observes a transport failure after the server may have
    /// committed (lost acknowledgement).
    Drop,
}

struct Expect {
    method: &'static str,
    path_contains: String,
    respond: Responder,
}

fn expect_json(method: &'static str, path: &str, value: Value) -> Expect {
    Expect {
        method,
        path_contains: path.to_owned(),
        respond: Responder::Json {
            status: 200,
            body: value,
        },
    }
}

fn expect_status(method: &'static str, path: &str, status: u16, value: Value) -> Expect {
    Expect {
        method,
        path_contains: path.to_owned(),
        respond: Responder::Json {
            status,
            body: value,
        },
    }
}

fn expect_content(path: &str, bytes: &[u8], checksum: &str) -> Expect {
    Expect {
        method: "GET",
        path_contains: path.to_owned(),
        respond: Responder::Content {
            bytes: bytes.to_vec(),
            checksum: checksum.to_owned(),
        },
    }
}

fn expect_drop(method: &'static str, path: &str) -> Expect {
    Expect {
        method,
        path_contains: path.to_owned(),
        respond: Responder::Drop,
    }
}

fn read_request(socket: &mut TcpStream) -> SeenRequest {
    let mut buffer = [0_u8; 8192];
    let mut raw = Vec::new();
    while !raw.windows(4).any(|part| part == b"\r\n\r\n") {
        let read = socket.read(&mut buffer).expect("read request headers");
        assert!(read > 0, "connection closed before request headers");
        raw.extend_from_slice(&buffer[..read]);
        assert!(raw.len() < 128 * 1024, "oversized test request header");
    }
    let split = raw
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .expect("header terminator")
        + 4;
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    let content_length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = raw[split..].to_vec();
    while body.len() < content_length {
        let read = socket.read(&mut buffer).expect("read request body");
        assert!(read > 0, "connection closed before request body");
        body.extend_from_slice(&buffer[..read]);
    }
    body.truncate(content_length);
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let lower = head.to_ascii_lowercase();
    assert!(
        lower.contains("authorization: bearer test-token"),
        "missing bearer token in {head}"
    );
    assert!(
        lower.contains("x-orbit-device-id: device-1"),
        "missing device header in {head}"
    );
    SeenRequest {
        method,
        target,
        body,
    }
}

fn accept_with_deadline(listener: &TcpListener) -> (TcpStream, std::net::SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(10);
    listener
        .set_nonblocking(true)
        .expect("set listener non-blocking");
    loop {
        match listener.accept() {
            Ok(pair) => {
                listener
                    .set_nonblocking(false)
                    .expect("restore blocking listener");
                return pair;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for an expected attachment request"
                );
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("accept failed: {error}"),
        }
    }
}

/// Serve exactly the scripted requests, then stop. The returned handle yields
/// every observed request so a test can assert what was actually transmitted.
fn serve_on(port: u16, script: Vec<Expect>) -> thread::JoinHandle<Vec<SeenRequest>> {
    // Bind synchronously so a client can never race the listener start-up.
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind scripted server");
    thread::spawn(move || {
        let mut seen = Vec::new();
        for expect in script {
            let (mut socket, _) = accept_with_deadline(&listener);
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("socket read timeout");
            let request = read_request(&mut socket);
            assert_eq!(request.method, expect.method, "unexpected method");
            assert!(
                request.target.contains(&expect.path_contains),
                "unexpected target {} (wanted {})",
                request.target,
                expect.path_contains
            );
            seen.push(request);
            match expect.respond {
                Responder::Json { status, body } => {
                    let raw = body.to_string();
                    let reason = match status {
                        200 => "200 OK",
                        404 => "404 Not Found",
                        409 => "409 Conflict",
                        413 => "413 Payload Too Large",
                        _ => "500 Internal Server Error",
                    };
                    let response = format!(
                        "HTTP/1.1 {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{raw}",
                        raw.len()
                    );
                    socket.write_all(response.as_bytes()).expect("write reply");
                    socket.flush().expect("flush reply");
                }
                Responder::Content { bytes, checksum } => {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nx-orbit-checksum: {checksum}\r\nx-orbit-size: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len(),
                        bytes.len()
                    );
                    socket
                        .write_all(response.as_bytes())
                        .expect("write headers");
                    socket.write_all(&bytes).expect("write content");
                    socket.flush().expect("flush content");
                }
                Responder::Drop => drop(socket),
            }
        }
        seen
    })
}

/// Observe a quiet window: any request at all is recorded and answered with a
/// server error, so a "nothing was sent" assertion cannot pass by accident.
fn serve_quiet(port: u16, window: Duration) -> thread::JoinHandle<Vec<SeenRequest>> {
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
                    let body = "{\"error\":\"unexpected request\"}";
                    let response = format!(
                        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes());
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

fn attachment_view(id: &str, checksum: &str, filename: &str, size: usize) -> Value {
    json!({
        "id": id,
        "owner_kind": "task",
        "owner_id": "task-1",
        "filename": filename,
        "content_type": "text/plain",
        "size_bytes": size,
        "checksum": checksum,
        "created_at_ms": 1_700_000_000_000_i64,
        "description": "Evidence",
        "is_image": false,
    })
}

fn import_receipt(
    id: &str,
    checksum: &str,
    filename: &str,
    size: usize,
    revision: i64,
    duplicate: bool,
) -> Value {
    json!({
        "revision": revision,
        "duplicate": duplicate,
        "attachment": attachment_view(id, checksum, filename, size),
    })
}

fn remote(replica: &Path, port: u16) -> AuthorityConnection {
    AuthorityConnection::remote(replica, &base_url(port), Some(("device-1", "test-token")))
        .expect("open remote")
}

fn staged_row(remote: &AuthorityConnection, id: &str) -> crate::PendingAttachmentDto {
    remote
        .pending_attachment_uploads()
        .expect("pending uploads")
        .into_iter()
        .find(|row| row.attachment.id == id)
        .expect("staged upload present")
}

fn assert_no_pending(remote: &AuthorityConnection) {
    assert!(
        !remote
            .has_pending_attachment_uploads()
            .expect("pending attachment flag")
    );
    assert!(
        remote
            .pending_attachment_uploads()
            .expect("pending uploads")
            .is_empty()
    );
}

#[test]
fn offline_staging_survives_reopen_and_flushes_exactly_once() {
    let scratch = Scratch::new("offline");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"staged evidence");
    let port = free_port();

    let mut client = remote(&replica, port);
    let staged = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            Some("Evidence".into()),
        )
        .expect("stage offline");
    assert_eq!(staged.filename, "note.txt");
    assert_eq!(staged.checksum, digest(b"staged evidence"));
    assert_eq!(client.pending_command_count().expect("pending count"), 1);
    let row = staged_row(&client, &staged.id);
    assert_eq!(row.state, "pending");
    let error = row.last_error.expect("offline attempt records an error");
    assert!(
        error.contains("server unreachable") || error.contains("retries on the next flush"),
        "unexpected staging error {error}"
    );
    // Accepted reads are unavailable offline, but the staged row is not lost.
    assert!(matches!(
        client.list_attachments("task", "task-1"),
        Err(ClientConnectionError::Unavailable(_))
    ));
    drop(client);

    let mut reopened = remote(&replica, port);
    assert_eq!(
        reopened
            .pending_command_count()
            .expect("pending after reopen"),
        1
    );
    let row = staged_row(&reopened, &staged.id);
    assert_eq!(row.state, "pending");
    assert_eq!(
        reopened
            .attachment_content(&staged.id)
            .expect("staged bytes survive reopen")
            .1,
        b"staged evidence"
    );

    // The server is reachable but has not accepted anything yet: a pending
    // upload must never appear in the accepted listing.
    let list_only = serve_on(
        port,
        vec![expect_json(
            "GET",
            "/v1/authoritative/attachments?",
            json!({"attachments": []}),
        )],
    );
    assert!(
        reopened
            .list_attachments("task", "task-1")
            .expect("accepted list")
            .is_empty()
    );
    let seen = list_only.join().expect("server thread");
    assert_eq!(seen.len(), 1);
    assert_eq!(
        reopened
            .staged_attachments("task", "task-1")
            .expect("staged list")
            .len(),
        1
    );

    let server = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 7})),
            expect_json(
                "POST",
                "/v1/authoritative/attachment-import?",
                import_receipt(&staged.id, &staged.checksum, "note.txt", 15, 8, false),
            ),
        ],
    );
    assert!(matches!(
        reopened.flush(),
        Ok(crate::authority::FlushOutcome::Accepted(1))
    ));
    let seen = server.join().expect("server thread");
    assert_eq!(seen.len(), 2);
    assert!(seen[0].target.contains("snapshot"));
    assert!(
        seen[1]
            .target
            .contains(&format!("attachment_id={}", staged.id))
    );
    assert!(seen[1].target.contains("owner_kind=task"));
    assert!(seen[1].target.contains("expected_revision=7"));
    assert!(seen[1].target.contains("description=Evidence"));
    assert_eq!(seen[1].body, b"staged evidence");
    assert_no_pending(&reopened);
    assert_eq!(reopened.pending_command_count().expect("settled"), 0);

    let listing = serve_on(
        port,
        vec![expect_json(
            "GET",
            "/v1/authoritative/attachments?",
            json!({"attachments": [attachment_view(&staged.id, &staged.checksum, "note.txt", 15)]}),
        )],
    );
    let accepted = reopened
        .list_attachments("task", "task-1")
        .expect("accepted list after flush");
    listing.join().expect("server thread");
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].id, staged.id);
    assert_eq!(accepted[0].checksum, staged.checksum);
    assert!(
        reopened
            .staged_attachments("task", "task-1")
            .expect("staged list")
            .is_empty()
    );

    // Repeating the flush must transmit nothing: the row is accepted.
    let quiet = serve_quiet(port, Duration::from_millis(250));
    let outcome = reopened.flush().expect("duplicate flush");
    assert!(matches!(outcome, crate::authority::FlushOutcome::Empty));
    assert!(
        quiet.join().expect("quiet server").is_empty(),
        "an accepted upload must not be re-sent"
    );
    drop(reopened);
}

#[test]
fn upload_accepted_on_first_attempt_returns_canonical_metadata() {
    let scratch = Scratch::new("accepted");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"accepted bytes");
    let checksum = digest(b"accepted bytes");
    let checksum_for_server = checksum.clone();
    let port = free_port();
    let mut client = remote(&replica, port);
    // A canonical server timestamp differs from the local staging timestamp;
    // the returned DTO must be the server projection.
    let server = thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
        let mut seen = Vec::new();
        for index in 0..2 {
            let (mut socket, _) = accept_with_deadline(&listener);
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let request = read_request(&mut socket);
            seen.push(request.clone());
            let body = if index == 0 {
                json!({"revision": 2}).to_string()
            } else {
                let id = request
                    .target
                    .split("attachment_id=")
                    .nth(1)
                    .and_then(|rest| rest.split('&').next())
                    .expect("attachment id in query")
                    .to_owned();
                let mut receipt =
                    import_receipt(&id, &checksum_for_server, "note.txt", 14, 3, false);
                receipt["attachment"]["created_at_ms"] = json!(1_800_000_000_000_i64);
                receipt.to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).expect("write");
            socket.flush().expect("flush");
        }
        seen
    });
    let accepted = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            Some("Evidence".into()),
        )
        .expect("import accepted");
    let seen = server.join().expect("server thread");
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[1].method, "POST");
    assert_eq!(accepted.checksum, checksum);
    assert_eq!(accepted.size_bytes, 14);
    assert_eq!(
        accepted.created_at_ms, 1_800_000_000_000,
        "canonical server metadata wins over local staging values"
    );
    assert_no_pending(&client);
    assert!(
        client
            .staged_attachments("task", "task-1")
            .expect("staged")
            .is_empty()
    );
}

#[test]
fn lost_acknowledgement_stays_pending_and_retry_is_the_same_duplicate_command() {
    let scratch = Scratch::new("lost-ack");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"exactly once");
    let checksum = digest(b"exactly once");
    let port = free_port();

    let mut client = remote(&replica, port);
    // The server receives and commits the upload, then the connection dies
    // before the client can read a receipt.
    let first = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 5})),
            expect_drop("POST", "/v1/authoritative/attachment-import?"),
        ],
    );
    let staged = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            Some("Evidence".into()),
        )
        .expect("staged after lost acknowledgement");
    let seen = first.join().expect("server thread");
    assert_eq!(seen.len(), 2);
    let first_command = seen[1]
        .target
        .split("command_id=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .expect("command id")
        .to_owned();
    let first_attachment = seen[1]
        .target
        .split("attachment_id=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .expect("attachment id")
        .to_owned();
    assert_eq!(first_attachment, staged.id);
    let row = staged_row(&client, &staged.id);
    assert_eq!(row.state, "pending");
    assert!(
        row.last_error
            .as_deref()
            .is_some_and(|error| error.contains("retries on the next flush")),
        "a lost acknowledgement must remain an actionable pending upload"
    );

    // Retry: the server reports a duplicate at the original revision. The same
    // command id must be reused, so no second attachment can be created.
    let retry = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 6})),
            expect_json(
                "POST",
                "/v1/authoritative/attachment-import?",
                import_receipt(&staged.id, &checksum, "note.txt", 12, 5, true),
            ),
        ],
    );
    assert!(matches!(
        client.flush(),
        Ok(crate::authority::FlushOutcome::Accepted(1))
    ));
    let seen = retry.join().expect("server thread");
    assert_eq!(seen.len(), 2);
    let retry_command = seen[1]
        .target
        .split("command_id=")
        .nth(1)
        .and_then(|rest| rest.split('&').next())
        .expect("command id");
    assert_eq!(
        retry_command, first_command,
        "a retry must reuse the immutable command id"
    );
    assert_eq!(seen[1].body, b"exactly once");
    assert_no_pending(&client);
    assert_eq!(client.pending_command_count().expect("settled"), 0);
}

#[test]
fn unknown_owner_leaves_the_upload_pending_with_an_actionable_error() {
    let scratch = Scratch::new("unknown-owner");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"owner later");
    let port = free_port();
    let mut client = remote(&replica, port);
    let server = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 4})),
            expect_status(
                "POST",
                "/v1/authoritative/attachment-import?",
                404,
                json!({"error": "task task-1 does not exist; create it before attaching files"}),
            ),
        ],
    );
    let staged = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("stage with unknown owner");
    server.join().expect("server thread");
    let row = staged_row(&client, &staged.id);
    assert_eq!(row.state, "pending");
    let error = row.last_error.expect("actionable error");
    assert!(error.contains("unknown attachment owner"), "{error}");
    assert!(error.contains("queued in the outbox"), "{error}");
    assert_eq!(client.pending_command_count().expect("pending"), 1);
}

#[test]
fn download_verifies_checksum_and_rejects_corrupted_bytes() {
    let scratch = Scratch::new("download");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"server bytes");
    let checksum = digest(b"server bytes");
    let port = free_port();
    let mut client = remote(&replica, port);
    // Stage first: the server is unreachable, so the upload stays pending and
    // the generated attachment identity is known before the server comes up.
    let staged = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("stage offline");
    let id = staged.id.clone();
    let server = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 1})),
            expect_json(
                "POST",
                "/v1/authoritative/attachment-import?",
                import_receipt(&id, &checksum, "note.txt", 12, 2, false),
            ),
        ],
    );
    assert!(matches!(
        client.flush(),
        Ok(crate::authority::FlushOutcome::Accepted(1))
    ));
    server.join().expect("server thread");

    let content_path = format!("/v1/authoritative/attachments/{id}/content");
    let good = serve_on(
        port,
        vec![expect_content(&content_path, b"server bytes", &checksum)],
    );
    let (metadata, bytes) = client.attachment_content(&id).expect("verified download");
    assert_eq!(metadata.id, id);
    assert_eq!(bytes, b"server bytes");
    good.join().expect("server thread");

    // Same advertised checksum, corrupted body: never returned.
    let corrupt = serve_on(
        port,
        vec![expect_content(&content_path, b"server BYTES", &checksum)],
    );
    match client.attachment_content(&id) {
        Err(ClientConnectionError::Remote(crate::authority::AuthorityError::Conflict(message))) => {
            assert!(message.contains("checksum"), "{message}");
        }
        other => panic!("expected checksum conflict, got {other:?}"),
    }
    corrupt.join().expect("server thread");

    // A checksum that disagrees with cached accepted metadata is rejected too.
    let mismatched = serve_on(
        port,
        vec![expect_content(
            &content_path,
            b"server bytes",
            &digest(b"other bytes"),
        )],
    );
    assert!(matches!(
        client.attachment_content(&id),
        Err(ClientConnectionError::Remote(
            crate::authority::AuthorityError::Conflict(_)
        ))
    ));
    mismatched.join().expect("server thread");

    let export = scratch.path.join("copy.bin");
    let export_path = export.to_string_lossy().into_owned();
    let exported = serve_on(
        port,
        vec![expect_content(&content_path, b"server bytes", &checksum)],
    );
    let metadata = client
        .export_attachment(&id, export_path.clone())
        .expect("export verified content");
    assert_eq!(metadata.id, id);
    assert_eq!(fs::read(&export).expect("read export"), b"server bytes");
    exported.join().expect("server thread");
    assert!(matches!(
        client.export_attachment(&id, export_path),
        Err(ClientConnectionError::Remote(
            crate::authority::AuthorityError::Conflict(message)
        )) if message.contains("never overwritten")
    ));
}

#[test]
fn accepted_metadata_cache_verifies_content_for_ids_not_imported_here() {
    let scratch = Scratch::new("metadata-cache");
    let replica = scratch.path.join("replica.sqlite");
    let bytes = b"elsewhere";
    let checksum = digest(bytes);
    let port = free_port();
    let client = remote(&replica, port);
    let listing = serve_on(
        port,
        vec![expect_json(
            "GET",
            "/v1/authoritative/attachments?",
            json!({"attachments": [attachment_view("attachment-other", &checksum, "other.txt", bytes.len())]}),
        )],
    );
    let listed = client
        .list_attachments("task", "task-1")
        .expect("accepted list");
    listing.join().expect("server thread");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "attachment-other");

    let content = serve_on(
        port,
        vec![expect_content(
            "/v1/authoritative/attachments/attachment-other/content",
            bytes,
            &checksum,
        )],
    );
    let (metadata, downloaded) = client
        .attachment_content("attachment-other")
        .expect("read accepted content");
    content.join().expect("server thread");
    assert_eq!(metadata.filename, "other.txt");
    assert_eq!(downloaded, bytes);
    // Nothing was cached as an accepted local attachment row; this is purely
    // accepted server metadata.
    assert_no_pending(&client);
}

#[test]
fn a_mismatched_acceptance_receipt_is_rejected_and_stays_pending() {
    let scratch = Scratch::new("bad-receipt");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"receipt bytes");
    let port = free_port();
    let mut client = remote(&replica, port);
    let server = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 9})),
            expect_json(
                "POST",
                "/v1/authoritative/attachment-import?",
                import_receipt(
                    "attachment-other",
                    &digest(b"different bytes"),
                    "note.txt",
                    13,
                    10,
                    false,
                ),
            ),
        ],
    );
    let error = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect_err("a receipt for other bytes is not acceptance");
    server.join().expect("server thread");
    assert!(matches!(
        error,
        ClientConnectionError::Remote(crate::authority::AuthorityError::Invalid(message))
            if message.contains("does not match the staged upload")
    ));
    assert_eq!(client.pending_command_count().expect("pending"), 1);
    // The mismatch is a contract breach, not an acceptance: the row stays
    // pending so the same immutable command can be retried later.
    assert_eq!(
        client
            .pending_attachment_uploads()
            .expect("pending")
            .remove(0)
            .state,
        "pending"
    );
}

#[test]
fn invalid_attachment_input_is_rejected_before_staging() {
    let scratch = Scratch::new("invalid-input");
    let replica = scratch.path.join("replica.sqlite");
    let port = free_port();
    let mut client = remote(&replica, port);

    let directory = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            scratch.path.to_string_lossy().into_owned(),
            None,
        )
        .expect_err("directories are rejected");
    assert!(matches!(
        directory,
        ClientConnectionError::Local(error)
            if error.code == ClientErrorCode::InvalidInput
                && error.message.contains("not a regular file")
    ));
    let unknown_owner = client
        .import_attachment(
            "sprint".into(),
            "sprint-1".into(),
            scratch.path.join("missing").to_string_lossy().into_owned(),
            None,
        )
        .expect_err("unknown owner kind is rejected");
    assert!(matches!(
        unknown_owner,
        ClientConnectionError::Local(error) if error.code == ClientErrorCode::InvalidInput
    ));
    assert!(
        !client
            .has_pending_attachment_uploads()
            .expect("nothing staged")
    );
}

#[test]
fn staged_bytes_are_private_and_tampering_is_detected_before_upload() {
    let scratch = Scratch::new("tamper");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"original staged bytes");
    let port = free_port();
    let mut client = remote(&replica, port);
    let staged = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("stage offline");

    let staging = scratch.path.join("attachment-staging");
    let entries: Vec<PathBuf> = fs::read_dir(&staging)
        .expect("staging directory")
        .map(|entry| entry.expect("staging entry").path())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "exactly one content-addressed staging file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&entries[0])
            .expect("staging metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "staged attachment bytes are owner-only");
    }

    // Corrupt the staged bytes: the checksum no longer matches the recorded
    // identity, so the client must refuse to upload and require a re-import.
    fs::write(&entries[0], b"tampered staged bytes").expect("tamper staging file");
    assert!(matches!(
        client.attachment_content(&staged.id),
        Err(ClientConnectionError::Remote(
            crate::authority::AuthorityError::Conflict(_)
        ))
    ));
    let quiet = serve_quiet(port, Duration::from_millis(250));
    match client.flush().expect("flush reports the conflict honestly") {
        crate::authority::FlushOutcome::Blocked {
            accepted, reason, ..
        } => {
            assert_eq!(accepted, 0);
            assert!(
                reason.contains("conflict"),
                "a queue holding only rejected uploads must report blocked: {reason}"
            );
            assert!(reason.contains("import the file again"), "{reason}");
        }
        other => panic!("expected a blocked outcome, got {other:?}"),
    }
    assert!(
        quiet.join().expect("quiet server").is_empty(),
        "corrupt staged bytes are never uploaded"
    );
    let row = client
        .pending_attachment_uploads()
        .expect("pending")
        .into_iter()
        .find(|row| row.attachment.id == staged.id)
        .expect("staged row retained");
    assert_eq!(row.state, "conflict");
    assert!(
        row.last_error
            .as_deref()
            .is_some_and(|error| error.contains("import the file again"))
    );
    // A conflicted staging row can still be discarded locally.
    assert!(
        !client
            .remove_attachment(staged.id.clone())
            .expect("discard corrupt staging")
    );
    assert!(
        !client
            .has_pending_attachment_uploads()
            .expect("nothing pending")
    );
}

#[test]
fn import_after_remote_removal_discards_staging_without_caching() {
    let scratch = Scratch::new("removed-receipt");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"removed bytes");
    let port = free_port();
    let mut client = remote(&replica, port);
    let staging = scratch.path.join("attachment-staging");
    let server = thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
        let mut seen = Vec::new();
        for index in 0..2 {
            let (mut socket, _) = accept_with_deadline(&listener);
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let request = read_request(&mut socket);
            seen.push(request.clone());
            let body = if index == 0 {
                json!({"revision": 5}).to_string()
            } else {
                json!({
                    "revision": 6,
                    "duplicate": true,
                    "removed": true,
                    "attachment": null,
                })
                .to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).expect("write");
        }
        seen
    });
    let error = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            Some("Evidence".into()),
        )
        .expect_err("a removed attachment is not an accepted upload");
    let seen = server.join().expect("server thread");
    assert_eq!(seen.len(), 2);
    match error {
        ClientConnectionError::Remote(crate::authority::AuthorityError::Conflict(message)) => {
            assert!(message.contains("removed by another client"), "{message}");
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_no_pending(&client);
    assert_eq!(client.pending_command_count().expect("settled"), 0);
    assert_eq!(
        fs::read_dir(&staging)
            .map(|entries| entries.count())
            .unwrap_or(0),
        0,
        "staged bytes for a remotely removed attachment are discarded"
    );
    assert!(
        client
            .staged_attachments("task", "task-1")
            .expect("staged list")
            .is_empty(),
        "no staging row remains for the removed attachment"
    );
}

#[test]
fn attachment_409_distinguishes_stale_revision_from_permanent_receipt_conflict() {
    let scratch = Scratch::new("conflict-409");
    let replica = scratch.path.join("replica.sqlite");
    let source = scratch.file("note.txt", b"retry bytes");
    let port = free_port();
    let mut client = remote(&replica, port);
    let staged = client
        .import_attachment(
            "task".into(),
            "task-1".into(),
            source.to_string_lossy().into_owned(),
            None,
        )
        .expect("stage offline");
    assert_eq!(staged_row(&client, &staged.id).state, "pending");

    let stale = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 2})),
            expect_status(
                "POST",
                "/v1/authoritative/attachment-import?",
                409,
                json!({"error": "stale server revision; refresh and retry"}),
            ),
        ],
    );
    let outcome = client.flush().expect("stale flush is not fatal");
    assert!(matches!(outcome, crate::authority::FlushOutcome::Empty));
    stale.join().expect("server thread");
    let row = staged_row(&client, &staged.id);
    assert_eq!(row.state, "pending", "a stale revision stays retryable");
    assert!(
        row.last_error
            .as_deref()
            .is_some_and(|error| error.contains("stale server revision")),
        "{:?}",
        row.last_error
    );

    let permanent = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 3})),
            expect_status(
                "POST",
                "/v1/authoritative/attachment-import?",
                409,
                json!({
                    "error": "command_id was already accepted with a different attachment payload"
                }),
            ),
        ],
    );
    client
        .flush()
        .expect("receipt conflict is recorded, not fatal");
    permanent.join().expect("server thread");
    let row = staged_row(&client, &staged.id);
    assert_eq!(
        row.state, "conflict",
        "a receipt/metadata conflict can never be retried successfully"
    );

    // Only a rejected upload remains: the flush must say so instead of "empty".
    let quiet = serve_quiet(port, Duration::from_millis(200));
    match client.flush().expect("conflict-only flush") {
        crate::authority::FlushOutcome::Blocked {
            accepted, reason, ..
        } => {
            assert_eq!(accepted, 0);
            assert!(reason.contains("different attachment payload"), "{reason}");
        }
        other => panic!("expected a blocked outcome, got {other:?}"),
    }
    assert!(quiet.join().expect("quiet server").is_empty());
}

#[test]
fn removal_is_idempotent_and_treats_already_removed_as_success() {
    let scratch = Scratch::new("idempotent-removal");
    let replica = scratch.path.join("replica.sqlite");
    let port = free_port();
    let mut client = remote(&replica, port);
    let id = "attachment-removed";

    // Lost acknowledgement: the server may have committed, so the retry must
    // replay exactly the same immutable command identity.
    let lost = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 7})),
            expect_drop("POST", "/v1/authoritative/attachment-remove"),
        ],
    );
    assert!(matches!(
        client.remove_attachment(id.into()),
        Err(ClientConnectionError::Remote(
            crate::authority::AuthorityError::Unavailable(_)
        ))
    ));
    let seen = lost.join().expect("server thread");
    let first: Value = serde_json::from_slice(&seen[1].body).expect("removal body");
    assert_eq!(first["command_id"], format!("remove-{id}"));
    assert_eq!(first["activity_id"], format!("activity-remove-{id}"));

    let retry = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 7})),
            expect_json(
                "POST",
                "/v1/authoritative/attachment-remove",
                json!({"revision": 8, "duplicate": true, "content_removed": false}),
            ),
        ],
    );
    assert!(!client.remove_attachment(id.into()).expect("retry removal"));
    let seen = retry.join().expect("server thread");
    let second: Value = serde_json::from_slice(&seen[1].body).expect("removal body");
    assert_eq!(
        second["command_id"], first["command_id"],
        "a retry must reuse the same deterministic command id"
    );
    assert_eq!(second["activity_id"], first["activity_id"]);

    // Another client already removed it: 200 with `already_removed` is success,
    // with no local content removal to report.
    let already = serve_on(
        port,
        vec![
            expect_json("GET", "/v1/authoritative/snapshot", json!({"revision": 8})),
            expect_json(
                "POST",
                "/v1/authoritative/attachment-remove",
                json!({
                    "revision": 9,
                    "duplicate": false,
                    "already_removed": true,
                    "content_removed": false,
                }),
            ),
        ],
    );
    assert!(
        !client
            .remove_attachment(id.into())
            .expect("already removed is success")
    );
    already.join().expect("server thread");
}

#[test]
fn hostile_server_metadata_is_rejected_before_caching_or_path_derivation() {
    let scratch = Scratch::new("hostile-metadata");
    let replica = scratch.path.join("replica.sqlite");
    let port = free_port();
    let client = remote(&replica, port);
    let checksum = digest(b"x");

    let traversal = serve_on(
        port,
        vec![expect_json(
            "GET",
            "/v1/authoritative/attachments?",
            json!({"attachments": [attachment_view("attachment-evil", &checksum, "../evil", 1)]}),
        )],
    );
    match client.list_attachments("task", "task-1") {
        Err(ClientConnectionError::Remote(crate::authority::AuthorityError::Invalid(message))) => {
            assert!(message.contains("../evil"), "{message}");
            assert!(message.contains("portable display name"), "{message}");
        }
        other => panic!("expected a rejected filename, got {other:?}"),
    }
    traversal.join().expect("server thread");

    let mut hostile_type = attachment_view("attachment-evil", &checksum, "ok.txt", 1);
    hostile_type["content_type"] = json!("text/plain\r\nx-orbit-device-id: attacker");
    let header = serve_on(
        port,
        vec![expect_json(
            "GET",
            "/v1/authoritative/attachments?",
            json!({"attachments": [hostile_type]}),
        )],
    );
    assert!(matches!(
        client.list_attachments("task", "task-1"),
        Err(ClientConnectionError::Remote(
            crate::authority::AuthorityError::Invalid(message)
        )) if message.contains("content type")
    ));
    header.join().expect("server thread");

    let directory = scratch.path.join("exports");
    fs::create_dir_all(&directory).unwrap();
    assert!(
        crate::attachments_remote::export_destination(&directory, "../evil").is_err(),
        "a traversal filename must never resolve inside an export directory"
    );
    let safe = crate::attachments_remote::export_destination(&directory, "report.txt")
        .expect("ordinary filename resolves");
    assert_eq!(safe.parent(), Some(directory.as_path()));
    assert_eq!(
        safe.file_name().and_then(|name| name.to_str()),
        Some("report.txt")
    );
}
