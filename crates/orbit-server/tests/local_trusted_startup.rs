//! Process-level contract for `ORBIT_AUTH_MODE=local-trusted`.
//!
//! The in-process tests cover request authorization; this one proves startup
//! refuses a non-loopback bind before storage is opened, and that a loopback
//! server serves the product surface without credentials.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ADMIN_KEY: &str = "local-trusted-integration-admin-key-0123456789abcdef";
const STARTUP_DEADLINE: Duration = Duration::from_secs(10);

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_orbit-server")
}

fn scratch_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "orbit-local-trusted-{}-{label}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Pick a currently free loopback port and release it immediately.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn try_request(address: &str, request: &str) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(address).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let status = raw
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    Some((status, raw))
}

fn request(address: &str, request: &str) -> (u16, String) {
    try_request(address, request).expect("connect to the spawned server")
}

fn get(address: &str, host: &str, path: &str) -> (u16, String) {
    request(
        address,
        &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"),
    )
}

fn try_get(address: &str, host: &str, path: &str) -> Option<(u16, String)> {
    try_request(
        address,
        &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"),
    )
}

fn post(address: &str, host: &str, path: &str, body: &str) -> (u16, String) {
    post_with_headers(address, host, path, "x-orbit-csrf: 1\r\n", body)
}

fn post_with_headers(
    address: &str,
    host: &str,
    path: &str,
    headers: &str,
    body: &str,
) -> (u16, String) {
    request(
        address,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
             Content-Type: application/json\r\n{headers}\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    )
}

fn assert_rejected_bind(label: &str, bind: &str) {
    let dir = scratch_dir(label);
    let database = dir.join("orbit-server.db");
    let mut child = Command::new(binary())
        .env("ORBIT_AUTH_MODE", "local-trusted")
        .env("ORBIT_SERVER_BIND", bind)
        .env("ORBIT_SERVER_ADMIN_KEY", ADMIN_KEY)
        .env("ORBIT_SERVER_DB", &database)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn orbit-server");

    let deadline = Instant::now() + STARTUP_DEADLINE;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{label}: server did not reject bind {bind} within the deadline");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        !status.success(),
        "{label}: expected a non-zero exit for {bind}, stderr: {stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("loopback"),
        "{label}: rejection should mention loopback, stderr: {stderr}"
    );
    assert!(
        !Path::new(&database).exists(),
        "{label}: bind validation must happen before the database is created"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn start_server(bind: &str, database: &Path) -> ChildGuard {
    let child = Command::new(binary())
        .env("ORBIT_AUTH_MODE", "local-trusted")
        .env("ORBIT_SERVER_BIND", bind)
        .env("ORBIT_SERVER_ADMIN_KEY", ADMIN_KEY)
        .env("ORBIT_SERVER_DB", database)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn orbit-server");
    ChildGuard::new(child)
}

fn wait_for_health(guard: &mut ChildGuard, address: &str, host: &str) {
    let deadline = Instant::now() + STARTUP_DEADLINE;
    loop {
        if let Some(status) = guard.child.as_mut().unwrap().try_wait().unwrap() {
            panic!("server exited before becoming healthy: {status}");
        }
        if try_get(address, host, "/health").is_some_and(|(status, _)| status == 200) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("server did not answer /health within the deadline");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn local_trusted_rejects_non_loopback_binds_before_storage() {
    let port = free_port();
    assert_rejected_bind("ipv4-any", &format!("0.0.0.0:{port}"));
    assert_rejected_bind("ipv6-any", &format!("[::]:{port}"));
    assert_rejected_bind("private-lan", &format!("192.168.1.5:{port}"));
}

#[test]
fn local_trusted_loopback_serves_without_credentials() {
    let dir = scratch_dir("loopback");
    let database = dir.join("orbit-server.db");
    let port = free_port();
    let bind = format!("127.0.0.1:{port}");
    let host = bind.clone();
    let mut guard = start_server(&bind, &database);
    wait_for_health(&mut guard, &bind, &host);

    let (status, raw) = get(&bind, &host, "/v1/web/session");
    assert_eq!(status, 200, "{raw}");
    assert!(
        raw.contains("\"local-trusted\""),
        "session body should name the mode: {raw}"
    );

    let (status, raw) = get(&bind, &host, "/v1/authoritative/projects");
    assert_eq!(status, 200, "{raw}");

    let (status, raw) = get(&bind, "public.example.com", "/v1/authoritative/projects");
    assert_eq!(status, 403, "{raw}");
    assert!(
        raw.contains("host not allowed in local trusted mode"),
        "refusal should name the host rule: {raw}"
    );

    let (status, raw) = post(
        &bind,
        &host,
        "/v1/devices",
        r#"{"device_id":"new-device","token":"new-device-token-0123456789abcdef"}"#,
    );
    assert_eq!(status, 403, "{raw}");
    assert!(
        raw.contains("administrator credential required"),
        "refusal should name the missing credential: {raw}"
    );

    // Worker transports are not part of the local product surface.
    let (status, raw) = post(&bind, &host, "/v1/automation-runs/claim", "");
    assert_eq!(status, 403, "{raw}");
    assert!(
        raw.contains("enrolled device credential required"),
        "worker route should require an enrolled device: {raw}"
    );

    // The synthetic principal id cannot be enrolled as a real device even with
    // the administrator key.
    let admin = format!("Authorization: Bearer {ADMIN_KEY}\r\n");
    let (status, raw) = post_with_headers(
        &bind,
        &host,
        "/v1/devices",
        &admin,
        r#"{"device_id":" local-trusted ","token":"reserved-device-token-0123456789abcdef"}"#,
    );
    assert_eq!(status, 400, "{raw}");
    assert!(
        raw.contains("reserved for the local-trusted principal"),
        "reserved id should be rejected with a clear reason: {raw}"
    );

    drop(guard);
    let _ = std::fs::remove_dir_all(&dir);
}
