//! Browser sessions for the self-hosted web app.
//!
//! Sessions are delegated credentials: a browser proves possession of a device
//! token once, and receives an opaque, expiring cookie session in return. Only
//! SHA-256 hashes of session tokens are stored. Business rules stay in
//! [`orbit_runtime::AuthoritativeHost`]; this module owns storage and the login
//! attempt throttle.
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

/// 12 hours; overridable with `ORBIT_WEB_SESSION_TTL_SECONDS`.
pub const DEFAULT_TTL_MS: i64 = 12 * 60 * 60 * 1000;
/// 7 days; a sliding session never outlives this.
pub const DEFAULT_ABSOLUTE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const THROTTLE_WINDOW_MS: i64 = 5 * 60 * 1000;
const THROTTLE_MAX_FAILURES: u32 = 5;
/// Idle throttle entries are dropped so failed-login spam cannot grow forever.
const THROTTLE_ENTRY_TTL_MS: i64 = 15 * 60 * 1000;
const THROTTLE_MAX_ENTRIES: usize = 4096;
const SESSION_TOKEN_BYTES: usize = 32;

#[derive(Debug)]
pub enum SessionError {
    Sql(rusqlite::Error),
    Invalid(&'static str),
    Random,
}
impl From<rusqlite::Error> for SessionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

#[derive(Debug)]
pub struct NewSession {
    /// Raw token. Never stored and never logged; only its hash is durable.
    pub token: String,
    pub expires_at_ms: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated {
    pub device_id: String,
    pub expires_at_ms: i64,
}

/// A failed login/verification attempt keyed by the claimed device identity.
struct Attempts {
    failures: u32,
    last_failure_ms: i64,
}

#[derive(Default)]
pub struct LoginThrottle {
    entries: HashMap<String, Attempts>,
}
impl LoginThrottle {
    pub fn new() -> Self {
        Self::default()
    }
    /// Consecutive failures inside the window block further attempts.
    pub fn blocked(&self, device_id: &str, now_ms: i64) -> bool {
        self.entries.get(device_id).is_some_and(|attempts| {
            attempts.failures >= THROTTLE_MAX_FAILURES
                && now_ms.saturating_sub(attempts.last_failure_ms) <= THROTTLE_WINDOW_MS
        })
    }
    pub fn record_failure(&mut self, device_id: &str, now_ms: i64) {
        self.prune(now_ms);
        let entry = self
            .entries
            .entry(device_id.to_owned())
            .or_insert(Attempts {
                failures: 0,
                last_failure_ms: now_ms,
            });
        if now_ms.saturating_sub(entry.last_failure_ms) > THROTTLE_WINDOW_MS {
            entry.failures = 0;
        }
        entry.failures = entry.failures.saturating_add(1);
        entry.last_failure_ms = now_ms;
    }
    /// A successful login clears the failure series.
    pub fn reset(&mut self, device_id: &str) {
        self.entries.remove(device_id);
    }
    fn prune(&mut self, now_ms: i64) {
        self.entries.retain(|_, attempts| {
            now_ms.saturating_sub(attempts.last_failure_ms) <= THROTTLE_ENTRY_TTL_MS
        });
        while self.entries.len() >= THROTTLE_MAX_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, attempts)| attempts.last_failure_ms)
                .map(|(device_id, _)| device_id.clone());
            match oldest {
                Some(device_id) => {
                    self.entries.remove(&device_id);
                }
                None => break,
            }
        }
    }
}

pub struct SessionStore {
    conn: Connection,
    /// Sliding lifetime applied on every successful authentication.
    ttl_ms: i64,
    absolute_ttl_ms: i64,
}
impl SessionStore {
    pub fn open(path: &Path) -> Result<Self, SessionError> {
        Self::open_with_ttl(path, ttl_from_env(), absolute_ttl_from_env())
    }
    pub fn open_with_ttl(
        path: &Path,
        ttl_ms: i64,
        absolute_ttl_ms: i64,
    ) -> Result<Self, SessionError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS web_sessions(\
               token_hash TEXT PRIMARY KEY, device_id TEXT NOT NULL,\
               created_at_ms INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL,\
               last_seen_ms INTEGER NOT NULL, absolute_expires_at_ms INTEGER NOT NULL\
             );\
             CREATE INDEX IF NOT EXISTS idx_web_sessions_expiry ON web_sessions(expires_at_ms);",
        )?;
        Ok(Self {
            conn,
            ttl_ms: ttl_ms.max(0),
            absolute_ttl_ms: absolute_ttl_ms.max(0),
        })
    }
    pub fn ttl_ms(&self) -> i64 {
        self.ttl_ms
    }
    pub fn absolute_ttl_ms(&self) -> i64 {
        self.absolute_ttl_ms
    }
    pub fn create(
        &mut self,
        device_id: &str,
        now_ms: i64,
        ttl_ms: i64,
        absolute_ttl_ms: i64,
    ) -> Result<NewSession, SessionError> {
        if device_id.trim().is_empty() {
            return Err(SessionError::Invalid("device id required"));
        }
        let token = random_token()?;
        let hash = token_hash(&token);
        let absolute_expires_at_ms = now_ms.saturating_add(absolute_ttl_ms.max(0));
        let expires_at_ms = now_ms
            .saturating_add(ttl_ms.max(0))
            .min(absolute_expires_at_ms);
        self.purge_expired(now_ms)?;
        self.conn.execute(
            "INSERT INTO web_sessions(token_hash,device_id,created_at_ms,expires_at_ms,last_seen_ms,absolute_expires_at_ms)\
             VALUES(?1,?2,?3,?4,?3,?5)",
            params![hash, device_id, now_ms, expires_at_ms, absolute_expires_at_ms],
        )?;
        Ok(NewSession {
            token,
            expires_at_ms,
        })
    }
    /// Returns `None` for unknown, revoked or expired sessions. A valid session
    /// slides forward, capped by its absolute deadline.
    pub fn authenticate(
        &mut self,
        token: &str,
        now_ms: i64,
    ) -> Result<Option<Authenticated>, SessionError> {
        if token.is_empty() {
            return Ok(None);
        }
        self.purge_expired(now_ms)?;
        let hash = token_hash(token);
        let row: Option<(String, i64, i64)> = self
            .conn
            .query_row(
                "SELECT device_id,expires_at_ms,absolute_expires_at_ms FROM web_sessions WHERE token_hash=?1",
                [&hash],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((device_id, expires_at_ms, absolute_expires_at_ms)) = row else {
            return Ok(None);
        };
        if now_ms >= expires_at_ms || now_ms >= absolute_expires_at_ms {
            self.conn
                .execute("DELETE FROM web_sessions WHERE token_hash=?1", [&hash])?;
            return Ok(None);
        }
        let slid = now_ms
            .saturating_add(self.ttl_ms)
            .min(absolute_expires_at_ms);
        self.conn.execute(
            "UPDATE web_sessions SET expires_at_ms=?2,last_seen_ms=?3 WHERE token_hash=?1",
            params![hash, slid, now_ms],
        )?;
        Ok(Some(Authenticated {
            device_id,
            expires_at_ms: slid,
        }))
    }
    /// Logout. Returns whether a live session row was removed.
    pub fn revoke(&mut self, token: &str, _now_ms: i64) -> Result<bool, SessionError> {
        Ok(self.conn.execute(
            "DELETE FROM web_sessions WHERE token_hash=?1",
            [token_hash(token)],
        )? == 1)
    }
    /// Drop every browser session for a device after revoke or token rotation.
    pub fn revoke_device_sessions(&mut self, device_id: &str) -> Result<usize, SessionError> {
        Ok(self
            .conn
            .execute("DELETE FROM web_sessions WHERE device_id=?1", [device_id])?)
    }
    fn purge_expired(&self, now_ms: i64) -> Result<(), SessionError> {
        self.conn.execute(
            "DELETE FROM web_sessions WHERE expires_at_ms<=?1 OR absolute_expires_at_ms<=?1",
            [now_ms],
        )?;
        Ok(())
    }
}

fn ttl_from_env() -> i64 {
    seconds_from_env("ORBIT_WEB_SESSION_TTL_SECONDS", DEFAULT_TTL_MS)
}
fn absolute_ttl_from_env() -> i64 {
    seconds_from_env(
        "ORBIT_WEB_SESSION_ABSOLUTE_SECONDS",
        DEFAULT_ABSOLUTE_TTL_MS,
    )
}
fn seconds_from_env(name: &str, default_ms: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(|seconds| seconds.saturating_mul(1000))
        .unwrap_or(default_ms)
}

fn random_token() -> Result<String, SessionError> {
    let mut bytes = [0u8; SESSION_TOKEN_BYTES];
    getrandom::getrandom(&mut bytes).map_err(|_| SessionError::Random)?;
    Ok(to_hex(&bytes))
}
fn token_hash(token: &str) -> String {
    to_hex(&Sha256::digest(token.as_bytes()))
}
fn to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SessionStore {
        SessionStore::open_with_ttl(Path::new(":memory:"), 1_000, 10_000).unwrap()
    }

    #[test]
    fn created_session_authenticates_with_a_64_hex_token() {
        let mut store = store();
        let new = store.create("device-1", 0, 1_000, 10_000).unwrap();
        assert_eq!(new.token.len(), 64);
        assert!(new.token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(new.expires_at_ms, 1_000);
        let authenticated = store.authenticate(&new.token, 1).unwrap().unwrap();
        assert_eq!(authenticated.device_id, "device-1");
        assert_eq!(authenticated.expires_at_ms, 1_001);
    }

    #[test]
    fn unknown_and_malformed_tokens_are_rejected() {
        let mut store = store();
        store.create("device-1", 0, 1_000, 10_000).unwrap();
        assert!(store.authenticate("not-a-token", 1).unwrap().is_none());
        assert!(store.authenticate(&"0".repeat(64), 1).unwrap().is_none());
        assert!(store.authenticate("", 1).unwrap().is_none());
    }

    #[test]
    fn expired_sessions_are_rejected_and_purged() {
        let mut store = store();
        let new = store.create("device-1", 0, 1_000, 10_000).unwrap();
        assert!(store.authenticate(&new.token, 1_000).unwrap().is_none());
        let rows: i64 = store
            .conn
            .query_row("SELECT count(*) FROM web_sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn revoked_sessions_are_rejected() {
        let mut store = store();
        let new = store.create("device-1", 0, 1_000, 10_000).unwrap();
        assert!(store.revoke(&new.token, 1).unwrap());
        assert!(!store.revoke(&new.token, 2).unwrap());
        assert!(store.authenticate(&new.token, 2).unwrap().is_none());
    }

    #[test]
    fn device_revoke_clears_all_remaining_sessions() {
        let mut store = store();
        let first = store.create("device-1", 0, 1_000, 10_000).unwrap();
        let second = store.create("device-1", 0, 1_000, 10_000).unwrap();
        let other = store.create("device-2", 0, 1_000, 10_000).unwrap();
        assert_eq!(store.revoke_device_sessions("device-1").unwrap(), 2);
        assert!(store.authenticate(&first.token, 1).unwrap().is_none());
        assert!(store.authenticate(&second.token, 1).unwrap().is_none());
        assert!(store.authenticate(&other.token, 1).unwrap().is_some());
    }

    #[test]
    fn sliding_expiry_never_exceeds_the_absolute_cap() {
        let mut store = store();
        let new = store.create("device-1", 0, 1_000, 4_000).unwrap();
        assert_eq!(new.expires_at_ms, 1_000);
        let slid = |store: &mut SessionStore, now_ms| {
            store
                .authenticate(&new.token, now_ms)
                .unwrap()
                .expect("session should be live")
                .expires_at_ms
        };
        // Each use slides the window by the TTL...
        assert_eq!(slid(&mut store, 999), 1_999);
        assert_eq!(slid(&mut store, 1_998), 2_998);
        // ...until the absolute deadline caps it.
        assert_eq!(slid(&mut store, 2_997), 3_997);
        assert_eq!(slid(&mut store, 3_996), 4_000);
        assert_eq!(slid(&mut store, 3_999), 4_000);
        assert!(store.authenticate(&new.token, 4_000).unwrap().is_none());
    }

    #[test]
    fn throttle_blocks_after_five_consecutive_failures_and_resets() {
        let mut throttle = LoginThrottle::new();
        for attempt in 1..=5 {
            assert!(!throttle.blocked("device-1", attempt * 1_000));
            throttle.record_failure("device-1", attempt * 1_000);
        }
        assert!(throttle.blocked("device-1", 6_000));
        // Other devices are unaffected.
        assert!(!throttle.blocked("device-2", 6_000));
        // The window is consecutive: idling past it starts a fresh series.
        assert!(!throttle.blocked("device-1", 6_000 + THROTTLE_WINDOW_MS + 1));
        throttle.record_failure("device-1", 6_000 + THROTTLE_WINDOW_MS + 1);
        assert!(!throttle.blocked("device-1", 6_000 + THROTTLE_WINDOW_MS + 2));
        for _ in 0..4 {
            throttle.record_failure("device-1", 6_000 + THROTTLE_WINDOW_MS + 2);
        }
        assert!(throttle.blocked("device-1", 6_000 + THROTTLE_WINDOW_MS + 3));
        throttle.reset("device-1");
        assert!(!throttle.blocked("device-1", 6_000 + THROTTLE_WINDOW_MS + 3));
    }

    #[test]
    fn throttle_map_is_bounded() {
        let mut throttle = LoginThrottle::new();
        for index in 0..(THROTTLE_MAX_ENTRIES + 100) {
            throttle.record_failure(&format!("device-{index}"), index as i64);
        }
        assert!(throttle.entries.len() <= THROTTLE_MAX_ENTRIES);
        assert!(!throttle.blocked("device-0", THROTTLE_MAX_ENTRIES as i64));
    }
}
