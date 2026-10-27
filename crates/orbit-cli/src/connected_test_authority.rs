//! Minimal loopback authority for the connected transport tests.
//!
//! A handler decides each response and every call is recorded, so a test asserts
//! exactly what the CLI sent and in which order. Nothing here talks to a real
//! Orbit server: `cargo test` never needs one.
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// One received request.
#[derive(Clone, Debug)]
pub(crate) struct Call {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: Value,
}

impl Call {
    /// The last path segment, which is how these tests name a route.
    pub(crate) fn route(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or_default()
    }
}

/// A loopback authority: `handler` answers, every call is recorded, and an
/// undeclared path is answered by the handler like any other.
pub(crate) struct Stub {
    base: String,
    calls: Arc<Mutex<Vec<Call>>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Stub {
    pub(crate) fn start(handler: impl Fn(&Call) -> (u16, Value) + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let base = format!("http://{}", listener.local_addr().expect("local addr"));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let stop = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            while !signal.load(Ordering::SeqCst) {
                let (mut socket, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => break,
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("read timeout");
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let read = socket.read(&mut buffer).unwrap_or(0);
                    if read == 0 || request.len() > 65536 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                }
                let head_end = request
                    .windows(4)
                    .position(|part| part == b"\r\n\r\n")
                    .map(|index| index + 4)
                    .unwrap_or(request.len());
                let head = String::from_utf8_lossy(&request[..head_end]).to_string();
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    })
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                let mut body = request[head_end..].to_vec();
                while body.len() < length {
                    let read = socket.read(&mut buffer).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    body.extend_from_slice(&buffer[..read]);
                }
                let target = head
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_owned();
                let (path, query) = match target.split_once('?') {
                    Some((path, query)) => (path.to_owned(), query.to_owned()),
                    None => (target, String::new()),
                };
                let call = Call {
                    method: head
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    path,
                    query,
                    body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                };
                if let Ok(mut calls) = recorded.lock() {
                    calls.push(call.clone());
                }
                let (status, payload) = handler(&call);
                let raw = format!(
                    "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    if (200..300).contains(&status) {
                        "OK"
                    } else {
                        "ERROR"
                    },
                    payload.to_string().len(),
                    payload
                );
                let _ = socket.write_all(raw.as_bytes());
                let _ = socket.flush();
            }
        });
        Self {
            base,
            calls,
            stop,
            handle: Some(handle),
        }
    }

    pub(crate) fn base(&self) -> String {
        self.base.trim_end_matches('/').to_owned()
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("calls").clone()
    }

    pub(crate) fn calls_to(&self, path: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.path.ends_with(path))
            .collect()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
