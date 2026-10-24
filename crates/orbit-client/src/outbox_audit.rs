//! Read-only, offline audit for a connected replica's authoritative outbox.
//!
//! This is deliberately not an automatic repair routine. If a replica has
//! malformed ordering, broken receipts or an impossible state, the operator
//! must not flush it blindly. Reports never include command action bodies.
use crate::authority::AuthorityError;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

const VALID_KINDS: &[&str] = &[
    "project",
    "task",
    "evidence",
    "schedule",
    "routine",
    "milestone",
    "strategy",
    "markdown",
    "folder_create",
    "folder_move",
    "agent",
    "blueprint",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuditIssue {
    pub code: &'static str,
    pub command_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutboxAudit {
    pub healthy: bool,
    pub total_commands: usize,
    pub states: BTreeMap<String, usize>,
    pub families: BTreeMap<String, usize>,
    pub first_unresolved: Option<String>,
    pub first_conflict: Option<String>,
    pub next_ordinal: Option<i64>,
    pub accepted_receipts: usize,
    pub issues: Vec<AuditIssue>,
}

#[derive(Debug)]
struct Row {
    id: String,
    kind: String,
    action: String,
    ordinal: i64,
    revision: Option<i64>,
    state: String,
    receipt: Option<String>,
}

fn issue(
    issues: &mut Vec<AuditIssue>,
    code: &'static str,
    id: Option<&str>,
    message: impl Into<String>,
) {
    issues.push(AuditIssue {
        code,
        command_id: id.map(ToOwned::to_owned),
        message: message.into(),
    });
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Audit all outbox rows within one SQLite read transaction.
/// This is intended for pre-submission inspection of the local replica, not
/// server acceptance validation. No command is mutated or acknowledged.
pub(crate) fn audit(db: &Connection) -> Result<OutboxAudit, AuthorityError> {
    let mut stmt = db.prepare(
        "SELECT command_id, kind, action, ordinal, expected_revision, state, receipt
        FROM authority_outbox ORDER BY ordinal ASC, command_id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Row {
            id: row.get(0)?,
            kind: row.get(1)?,
            action: row.get(2)?,
            ordinal: row.get(3)?,
            revision: row.get(4)?,
            state: row.get(5)?,
            receipt: row.get(6)?,
        })
    })?;
    let mut rows: Vec<Row> = rows.collect::<Result<_, _>>()?;
    let total_commands = rows.len();
    let mut states = BTreeMap::new();
    let mut families = BTreeMap::new();
    let mut issues = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_ordinals = HashSet::new();
    let mut first_unresolved = None;
    let mut first_conflict = None;
    let mut accepted_receipts = 0;
    let mut previous_ordinal: Option<i64> = None;
    // A permanent conflict does not block later rows: the flush skips it and
    // continues. Only an earlier *pending* intent before an accepted row proves
    // that the global order was bypassed.
    let mut seen_pending = false;
    let mut max_ordinal = 0;

    for row in &mut rows {
        *states.entry(row.state.clone()).or_insert(0) += 1;
        *families.entry(row.kind.clone()).or_insert(0) += 1;
        if !valid_identity(&row.id) {
            issue(
                &mut issues,
                "identity",
                Some(&row.id),
                "invalid command identity",
            );
        }
        if !seen_ids.insert(row.id.as_str()) {
            issue(
                &mut issues,
                "duplicate_identity",
                Some(&row.id),
                "repeated command identity",
            );
        }
        if !VALID_KINDS.contains(&row.kind.as_str()) {
            issue(
                &mut issues,
                "unknown_family",
                Some(&row.id),
                "unknown command family",
            );
        }
        if row.ordinal <= 0 {
            issue(
                &mut issues,
                "ordinal",
                Some(&row.id),
                "ordinal must be positive",
            );
        }
        if !seen_ordinals.insert(row.ordinal) {
            issue(
                &mut issues,
                "duplicate_ordinal",
                Some(&row.id),
                "repeated global ordinal",
            );
        }
        if previous_ordinal.is_some_and(|old| row.ordinal <= old) {
            issue(
                &mut issues,
                "ordering",
                Some(&row.id),
                "non-increasing global ordinal",
            );
        }
        previous_ordinal = Some(row.ordinal);
        max_ordinal = max_ordinal.max(row.ordinal);
        if row.revision.is_some_and(|revision| revision < 0) {
            issue(
                &mut issues,
                "revision",
                Some(&row.id),
                "negative frozen revision",
            );
        }
        let action = match serde_json::from_str::<Value>(&row.action) {
            Ok(value) if value.is_object() => Some(value),
            _ => {
                issue(
                    &mut issues,
                    "action",
                    Some(&row.id),
                    "malformed action JSON",
                );
                None
            }
        };
        if action.as_ref().is_some_and(|action| {
            action.get("command_id").is_some() || action.get("expected_revision").is_some()
        }) {
            issue(
                &mut issues,
                "embedded_envelope",
                Some(&row.id),
                "outbox action contains server-owned envelope fields",
            );
        }
        if !matches!(
            row.state.as_str(),
            "pending" | "accepted" | "conflict" | "superseded"
        ) {
            issue(&mut issues, "state", Some(&row.id), "unknown command state");
        }
        match row.state.as_str() {
            "pending" | "conflict" => {
                if row.state == "pending" {
                    seen_pending = true;
                }
                if first_unresolved.is_none() {
                    first_unresolved = Some(row.id.clone());
                }
                if row.state == "conflict" && first_conflict.is_none() {
                    first_conflict = Some(row.id.clone());
                }
                // A conflict row may carry its rejection reason as
                // `{"error": "..."}`. That is an explanation written by the
                // flush, never an acceptance receipt, so it is valid only when
                // it makes no acceptance claim.
                let explanatory = row.state == "conflict"
                    && row.receipt.as_deref().is_some_and(|raw| {
                        serde_json::from_str::<Value>(raw).is_ok_and(|receipt| {
                            receipt.get("error").and_then(Value::as_str).is_some()
                                && receipt.get("revision").is_none()
                                && receipt.get("command_id").is_none()
                                && receipt.get("response").is_none()
                        })
                    });
                if row.receipt.is_some() && !explanatory {
                    issue(
                        &mut issues,
                        "unresolved_receipt",
                        Some(&row.id),
                        "unresolved intention already has an acceptance receipt",
                    );
                }
            }
            "accepted" => {
                accepted_receipts += 1;
                if seen_pending {
                    issue(
                        &mut issues,
                        "accepted_after_pending",
                        Some(&row.id),
                        "accepted command follows an unresolved pending intention",
                    );
                }
                let parsed = row
                    .receipt
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
                match parsed {
                    None => issue(
                        &mut issues,
                        "missing_receipt",
                        Some(&row.id),
                        "accepted command has no parseable receipt",
                    ),
                    Some(receipt) => {
                        // File-operation receipts carry command_id; ordinary
                        // project/domain receipts instead carry revision,
                        // duplicate and an accepted entity. Do not reject a
                        // valid domain receipt for omitting an optional ID.
                        if let Some(receipt_id) = receipt.get("command_id").and_then(Value::as_str)
                        {
                            if receipt_id != row.id {
                                issue(
                                    &mut issues,
                                    "receipt_identity",
                                    Some(&row.id),
                                    "acceptance receipt references another command",
                                );
                            }
                        } else if receipt.get("revision").and_then(Value::as_i64).is_none() {
                            issue(
                                &mut issues,
                                "receipt_shape",
                                Some(&row.id),
                                "receipt lacks both command identity and accepted revision",
                            );
                        }
                    }
                }
            }
            "superseded" if row.receipt.is_some() => {
                // Reviewing/replacing a conflict preserves its original
                // explanatory `{"error": ...}` receipt. That is history, not
                // an acceptance claim, so only reject acceptance-shaped data.
                let explanatory = row.receipt.as_deref().is_some_and(|raw| {
                    serde_json::from_str::<Value>(raw).is_ok_and(|receipt| {
                        receipt.get("error").and_then(Value::as_str).is_some()
                            && receipt.get("revision").is_none()
                            && receipt.get("command_id").is_none()
                            && receipt.get("response").is_none()
                    })
                });
                if !explanatory {
                    issue(
                        &mut issues,
                        "superseded_receipt",
                        Some(&row.id),
                        "superseded command must not carry an acceptance receipt",
                    );
                }
            }
            _ => {}
        }
    }
    let next_ordinal = max_ordinal.checked_add(1);
    if next_ordinal.is_none() {
        issue(
            &mut issues,
            "ordinal_exhaustion",
            None,
            "global ordinal exhausted; cannot queue further commands",
        );
    }
    // Detect stale/corrupted snapshots independently of outbox sequence.
    let snapshot_metadata: Option<(i64, String)> = db
        .query_row(
            "SELECT revision, fingerprint FROM authority_snapshot WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((revision, fingerprint)) = snapshot_metadata
        && (revision < 0
            || fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    {
        issue(
            &mut issues,
            "snapshot_metadata",
            None,
            "invalid accepted snapshot metadata",
        );
    }
    Ok(OutboxAudit {
        healthy: issues.is_empty(),
        total_commands,
        states,
        families,
        first_unresolved,
        first_conflict,
        next_ordinal,
        accepted_receipts,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE authority_outbox(
            command_id TEXT PRIMARY KEY, kind TEXT, action TEXT, ordinal INTEGER,
            expected_revision INTEGER, state TEXT, receipt TEXT
        ); CREATE TABLE authority_snapshot(id INTEGER PRIMARY KEY, revision INTEGER, fingerprint TEXT);").unwrap();
        db
    }
    fn insert(db: &Connection, id: &str, ordinal: i64, state: &str, receipt: Option<&str>) {
        db.execute(
            "INSERT INTO authority_outbox VALUES(?1,'task','{\"type\":\"create\"}',?2,NULL,?3,?4)",
            rusqlite::params![id, ordinal, state, receipt],
        )
        .unwrap();
    }
    #[test]
    fn empty_replica_is_healthy() {
        let report = audit(&db()).unwrap();
        assert!(report.healthy);
        assert_eq!(report.total_commands, 0);
        assert!(report.first_unresolved.is_none());
    }
    #[test]
    fn pending_commands_are_not_accepted() {
        let db = db();
        insert(&db, "one", 1, "pending", None);
        insert(&db, "two", 2, "pending", None);
        let report = audit(&db).unwrap();
        assert!(report.healthy);
        assert_eq!(report.first_unresolved.as_deref(), Some("one"));
        assert_eq!(report.accepted_receipts, 0);
    }
    #[test]
    fn conflicting_queue_blocks_later_intentions() {
        let db = db();
        insert(&db, "conflict", 1, "conflict", None);
        insert(&db, "later", 2, "pending", None);
        let report = audit(&db).unwrap();
        assert!(report.healthy);
        assert_eq!(report.first_conflict.as_deref(), Some("conflict"));
        assert_eq!(report.first_unresolved.as_deref(), Some("conflict"));
    }
    #[test]
    fn accepted_command_requires_matching_receipt() {
        let db = db();
        insert(
            &db,
            "accepted",
            1,
            "accepted",
            Some(r#"{"command_id":"different"}"#),
        );
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|i| i.code == "receipt_identity")
        );
    }
    #[test]
    fn accepted_without_receipt_is_corruption() {
        let db = db();
        insert(&db, "accepted", 1, "accepted", None);
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|i| i.code == "missing_receipt")
        );
    }
    #[test]
    fn accepted_after_unresolved_is_detected() {
        let db = db();
        insert(&db, "pending", 1, "pending", None);
        insert(
            &db,
            "accepted",
            2,
            "accepted",
            Some(r#"{"command_id":"accepted"}"#),
        );
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|i| i.code == "accepted_after_pending")
        );
    }
    #[test]
    fn invalid_action_is_rejected_by_audit() {
        let db = db();
        insert(&db, "broken", 1, "pending", None);
        db.execute("UPDATE authority_outbox SET action='not json'", [])
            .unwrap();
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|i| i.code == "action")
        );
    }
    #[test]
    fn envelope_fields_cannot_be_embedded_in_staged_action() {
        let db = db();
        insert(&db, "one", 1, "pending", None);
        db.execute(
            "UPDATE authority_outbox SET action='{\"expected_revision\":3}'",
            [],
        )
        .unwrap();
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|i| i.code == "embedded_envelope")
        );
    }
    #[test]
    fn invalid_family_detected() {
        let db = db();
        insert(&db, "one", 1, "pending", None);
        db.execute("UPDATE authority_outbox SET kind='obsolete'", [])
            .unwrap();
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|i| i.code == "unknown_family")
        );
    }
    #[test]
    fn report_never_discloses_action_bodies() {
        let db = db();
        insert(&db, "one", 1, "pending", None);
        db.execute(
            "UPDATE authority_outbox SET action='{\"body\":\"PRIVATE NOTE\"}'",
            [],
        )
        .unwrap();
        let report = serde_json::to_string(&audit(&db).unwrap()).unwrap();
        assert!(!report.contains("PRIVATE NOTE"));
    }
}

#[cfg(test)]
mod audit_invariants {
    use super::*;
    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE authority_outbox(
          command_id TEXT, kind TEXT, action TEXT, ordinal INTEGER,
          expected_revision INTEGER, state TEXT, receipt TEXT);
          CREATE TABLE authority_snapshot(id INTEGER PRIMARY KEY, revision INTEGER, fingerprint TEXT);").unwrap();
        db
    }
    fn row(
        db: &Connection,
        id: &str,
        kind: &str,
        ordinal: i64,
        rev: Option<i64>,
        state: &str,
        receipt: Option<&str>,
    ) {
        db.execute(
            "INSERT INTO authority_outbox VALUES(?1,?2,'{\"type\":\"create\"}',?3,?4,?5,?6)",
            rusqlite::params![id, kind, ordinal, rev, state, receipt],
        )
        .unwrap();
    }
    #[test]
    fn overlapping_ordinals_are_reported_without_panicking() {
        let db = db();
        row(&db, "one", "task", 1, None, "pending", None);
        row(&db, "two", "task", 1, None, "pending", None);
        let report = audit(&db).unwrap();
        assert!(!report.healthy);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "duplicate_ordinal")
        );
    }
    #[test]
    fn invalid_identifier_is_reported_not_normalized() {
        let db = db();
        row(&db, "unsafe id", "task", 1, None, "pending", None);
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.code == "identity")
        );
    }
    #[test]
    fn frozen_revision_must_not_be_negative() {
        let db = db();
        row(&db, "bad-revision", "task", 1, Some(-1), "pending", None);
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.code == "revision")
        );
    }
    #[test]
    fn positive_frozen_revision_can_remain_pending() {
        let db = db();
        row(&db, "frozen", "task", 1, Some(15), "pending", None);
        assert!(audit(&db).unwrap().healthy);
    }
    #[test]
    fn audit_reports_all_canonical_families() {
        let db = db();
        for (index, kind) in VALID_KINDS.iter().enumerate() {
            row(
                &db,
                &format!("command-{index}"),
                kind,
                (index + 1) as i64,
                None,
                "pending",
                None,
            );
        }
        let report = audit(&db).unwrap();
        assert!(report.healthy, "{:?}", report.issues);
        // The canonical set is every family admitted by `CommandKind`,
        // including the later `agent` and `blueprint` transport families.
        let families: Vec<&str> = report.families.keys().map(String::as_str).collect();
        assert_eq!(
            families,
            vec![
                "agent",
                "blueprint",
                "evidence",
                "folder_create",
                "folder_move",
                "markdown",
                "milestone",
                "project",
                "routine",
                "schedule",
                "strategy",
                "task",
            ]
        );
        assert_eq!(report.families.values().sum::<usize>(), VALID_KINDS.len());
        assert_eq!(report.total_commands, VALID_KINDS.len());
    }
    #[test]
    fn superseded_record_stays_auditable() {
        let db = db();
        row(&db, "replacement", "task", 1, None, "pending", None);
        row(&db, "old", "task", 2, None, "superseded", None);
        let report = audit(&db).unwrap();
        assert!(report.healthy);
        assert_eq!(report.states["superseded"], 1);
        assert_eq!(report.first_unresolved.as_deref(), Some("replacement"));
    }
    #[test]
    fn orphaned_receipts_on_pending_are_detected() {
        let db = db();
        row(
            &db,
            "pending",
            "task",
            1,
            None,
            "pending",
            Some(r#"{"command_id":"pending"}"#),
        );
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.code == "unresolved_receipt")
        );
    }
    #[test]
    fn receipts_on_superseded_are_detected() {
        let db = db();
        row(
            &db,
            "old",
            "task",
            1,
            None,
            "superseded",
            Some(r#"{"command_id":"old"}"#),
        );
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.code == "superseded_receipt")
        );
    }
    #[test]
    fn accepted_receipts_count_is_explicit() {
        let db = db();
        row(
            &db,
            "first",
            "project",
            1,
            Some(0),
            "accepted",
            Some(r#"{"command_id":"first","revision":1}"#),
        );
        row(
            &db,
            "second",
            "task",
            2,
            Some(1),
            "accepted",
            Some(r#"{"command_id":"second","revision":2}"#),
        );
        row(&db, "third", "task", 3, None, "pending", None);
        let report = audit(&db).unwrap();
        assert!(report.healthy, "{:?}", report.issues);
        assert_eq!(report.accepted_receipts, 2);
        assert_eq!(report.next_ordinal, Some(4));
    }
    #[test]
    fn invalid_accepted_snapshot_metadata_detected() {
        let db = db();
        db.execute("INSERT INTO authority_snapshot VALUES(1,-1,'invalid')", [])
            .unwrap();
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.code == "snapshot_metadata")
        );
    }
    #[test]
    fn valid_accepted_snapshot_metadata_does_not_fail_audit() {
        let db = db();
        db.execute(
            "INSERT INTO authority_snapshot VALUES(1,0,?1)",
            ["a".repeat(64)],
        )
        .unwrap();
        assert!(audit(&db).unwrap().healthy);
    }
    #[test]
    fn empty_action_is_unparseable() {
        let db = db();
        row(&db, "empty", "task", 1, None, "pending", None);
        db.execute("UPDATE authority_outbox SET action='[]'", [])
            .unwrap();
        assert!(
            audit(&db)
                .unwrap()
                .issues
                .iter()
                .any(|issue| issue.code == "action")
        );
    }
    #[test]
    fn audit_is_read_only_even_on_corruption() {
        let db = db();
        row(&db, "one", "bad-kind", 1, Some(-1), "pending", None);
        let before: i64 = db
            .query_row("SELECT COUNT(*) FROM authority_outbox", [], |row| {
                row.get(0)
            })
            .unwrap();
        let report = audit(&db).unwrap();
        let after: i64 = db
            .query_row("SELECT COUNT(*) FROM authority_outbox", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!report.healthy);
        assert_eq!(before, after);
    }
}
