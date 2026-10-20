//! Transactional reconciliation invariants for independently edited clients.
//! These tests exercise the real SQLite outbox, not mock in-memory vectors.
use super::*;

fn accepted(revision: i64, tag: char) -> Value {
    json!({"revision":revision,"history_fingerprint":tag.to_string().repeat(64),
        "projects":[],"tasks":[],"habits":[],"habit_occurrences":[],
        "metrics":[],"metric_observations":[],"schedule":[],"routines":[],
        "milestones":[],"strategies":[]})
}
fn new_replica() -> AuthorityReplica {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica.accept_snapshot(&accepted(7, 'a')).unwrap();
    for (id, task) in [("first", "t1"), ("second", "t2"), ("third", "t3")] {
        replica
            .queue(
                id,
                CommandKind::Task,
                &json!({"type":"create","task_id":task,"title":"original"}),
            )
            .unwrap();
    }
    replica
        .db
        .execute(
            "UPDATE authority_outbox SET state='conflict' WHERE command_id IN ('first','second')",
            [],
        )
        .unwrap();
    replica
}
fn decision(replica: &AuthorityReplica, original: &str, next: &str) -> ReviewedReplacement {
    ReviewedReplacement {
        review: replica.conflict_review(original).unwrap(),
        replacement_id: next.into(),
        kind: CommandKind::Task,
        action: json!({"type":"change_status","task_id":original,"status":"ready"}),
    }
}
fn rows(replica: &AuthorityReplica) -> Vec<(String, String, i64, Option<i64>)> {
    let mut stmt=replica.db.prepare(
        "SELECT command_id,state,ordinal,expected_revision FROM authority_outbox ORDER BY ordinal").unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn two_reviewed_conflicts_are_staged_atomically_in_original_order() {
    let mut replica = new_replica();
    let decisions = vec![
        decision(&replica, "second", "second-new"),
        decision(&replica, "first", "first-new"),
    ];
    assert_eq!(replica.replace_reviewed_batch(&decisions).unwrap(), 2);
    let actual = rows(&replica);
    assert_eq!(actual[0].0, "first-new");
    assert_eq!(actual[1].0, "second-new");
    assert_eq!(actual[2].0, "third");
    assert_eq!(actual[0].3, Some(7));
    assert_eq!(actual[1].3, Some(7));
    assert_eq!(actual[3].0, "second");
    assert_eq!(actual[4].0, "first");
    assert_eq!(actual[3].1, "superseded");
    assert_eq!(actual[4].1, "superseded");
    assert!(replica.accepted_receipt("first-new").unwrap().is_none());
    assert!(replica.accepted_receipt("second-new").unwrap().is_none());
}

#[test]
fn a_stale_second_review_rejects_the_entire_batch() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "new-1");
    let mut second = decision(&replica, "second", "new-2");
    second.review.history_fingerprint = "b".repeat(64);
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn prior_review_cannot_survive_an_accepted_history_change() {
    let mut replica = new_replica();
    let decision = decision(&replica, "first", "new-1");
    replica.accept_snapshot(&accepted(8, 'b')).unwrap();
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[decision]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn duplicate_rejected_identity_rejects_all_decisions() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "new-a");
    let second = decision(&replica, "first", "new-b");
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn duplicate_replacement_identity_rejects_all_decisions() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "new-a");
    let second = decision(&replica, "second", "new-a");
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn replacing_another_rejected_command_identity_is_forbidden() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "second");
    let second = decision(&replica, "second", "new-b");
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn existing_pending_identity_cannot_be_reused() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "new-a");
    let second = decision(&replica, "second", "third");
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn changed_conflicted_action_invalidates_the_review() {
    let mut replica = new_replica();
    let decision = decision(&replica, "first", "new-a");
    replica
        .db
        .execute(
            "UPDATE authority_outbox SET action=?1 WHERE command_id='first'",
            [json!({"type":"delete","task_id":"t1"}).to_string()],
        )
        .unwrap();
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[decision]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn altered_original_queue_position_invalidates_review() {
    let mut replica = new_replica();
    let decision = decision(&replica, "first", "new-a");
    replica
        .db
        .execute(
            "UPDATE authority_outbox SET ordinal=20 WHERE command_id='first'",
            [],
        )
        .unwrap();
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[decision]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn no_conflicts_and_oversized_batch_are_rejected_without_writes() {
    let mut replica = new_replica();
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[]).is_err());
    let repeated = vec![decision(&replica, "first", "new-a"); 101];
    assert!(replica.replace_reviewed_batch(&repeated).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn no_implicit_cross_family_replacement() {
    let mut replica = new_replica();
    let mut item = decision(&replica, "first", "new-a");
    item.kind = CommandKind::Project;
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[item]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn file_conflicts_must_be_resolved_using_file_revision_rules() {
    let mut replica = new_replica();
    let mut item = decision(&replica, "first", "new-a");
    item.kind = CommandKind::Markdown;
    item.review.kind = "markdown".into();
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[item]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn malformed_action_cannot_mutate_original_audit_history() {
    let mut replica = new_replica();
    let mut item = decision(&replica, "first", "new-a");
    item.action = Value::Null;
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[item]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn rejected_original_stays_queryable_after_successful_staging() {
    let mut replica = new_replica();
    let item = decision(&replica, "first", "new-a");
    replica.replace_reviewed_batch(&[item]).unwrap();
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "superseded"
    );
    assert_eq!(
        replica.command_status("new-a").unwrap().unwrap()["state"],
        "pending"
    );
    let unresolved = replica.unresolved().unwrap();
    assert_eq!(unresolved.len(), 3);
    assert_eq!(unresolved[0]["command_id"], "new-a");
    assert_eq!(unresolved[1]["command_id"], "second");
    assert_eq!(unresolved[2]["command_id"], "third");
}

#[test]
fn second_attempt_with_same_replacements_is_not_a_silent_duplicate() {
    let mut replica = new_replica();
    let item = decision(&replica, "first", "new-a");
    replica
        .replace_reviewed_batch(std::slice::from_ref(&item))
        .unwrap();
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[item]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn conflict_review_rejects_pending_commands_and_absent_history() {
    let mut replica = AuthorityReplica::open(":memory:").unwrap();
    replica
        .queue(
            "pending",
            CommandKind::Task,
            &json!({"type":"create","task_id":"t","title":"T"}),
        )
        .unwrap();
    assert!(replica.conflict_review("pending").is_err());
    replica
        .db
        .execute(
            "UPDATE authority_outbox SET state='conflict' WHERE command_id='pending'",
            [],
        )
        .unwrap();
    assert!(replica.conflict_review("pending").is_err());
}

#[test]
fn durable_batch_survives_restart_with_review_revision_frozen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reviewed.sqlite");
    {
        let mut replica = AuthorityReplica::open(&path).unwrap();
        replica.accept_snapshot(&accepted(19, 'c')).unwrap();
        replica
            .queue(
                "first",
                CommandKind::Task,
                &json!({"type":"create","task_id":"t1","title":"First"}),
            )
            .unwrap();
        replica
            .queue(
                "second",
                CommandKind::Task,
                &json!({"type":"create","task_id":"t2","title":"Second"}),
            )
            .unwrap();
        replica
            .db
            .execute("UPDATE authority_outbox SET state='conflict'", [])
            .unwrap();
        let decisions = vec![
            decision(&replica, "first", "reviewed-first"),
            decision(&replica, "second", "reviewed-second"),
        ];
        replica.replace_reviewed_batch(&decisions).unwrap();
    }
    let mut reopened = AuthorityReplica::open(&path).unwrap();
    assert_eq!(reopened.convergence_status().unwrap().pending, 2);
    assert_eq!(reopened.convergence_status().unwrap().conflicts, 0);
    assert_eq!(
        reopened.command_status("first").unwrap().unwrap()["state"],
        "superseded"
    );
    assert_eq!(
        reopened.command_status("second").unwrap().unwrap()["state"],
        "superseded"
    );
    let first = reopened.next_envelope(999).unwrap().unwrap();
    assert_eq!(first.0, "reviewed-first");
    assert_eq!(first.2["expected_revision"], 19);
    reopened
        .db
        .execute(
            "UPDATE authority_outbox SET state='accepted' WHERE command_id='reviewed-first'",
            [],
        )
        .unwrap();
    let second = reopened.next_envelope(999).unwrap().unwrap();
    assert_eq!(second.0, "reviewed-second");
    assert_eq!(second.2["expected_revision"], 19);
}

#[test]
fn failed_batch_preserves_accepted_revision_and_pending_queue() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "new-first");
    let mut second = decision(&replica, "second", "new-second");
    second.review.ordinal = 999;
    let old_revision = replica.revision().unwrap();
    let old_status = replica.convergence_status().unwrap();
    let old_unresolved = replica.unresolved().unwrap();
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(replica.revision().unwrap(), old_revision);
    assert_eq!(replica.unresolved().unwrap(), old_unresolved);
    assert_eq!(replica.convergence_status().unwrap(), old_status);
}

#[test]
fn outbox_is_healthy_after_multiple_snapshot_bound_replacements() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "new-first");
    let second = decision(&replica, "second", "new-second");
    replica.replace_reviewed_batch(&[first, second]).unwrap();
    let audit = replica.audit().unwrap();
    assert!(audit.healthy, "{:?}", audit.issues);
    let status = replica.convergence_status().unwrap();
    assert_eq!(status.pending, 3);
    assert_eq!(status.conflicts, 0);
    assert_eq!(status.superseded, 2);
    assert_eq!(status.accepted_revision, Some(7));
    assert!(!status.server_convergence_verified);
}

#[test]
fn invalid_id_and_embedded_revision_fields_are_forbidden() {
    let mut replica = new_replica();
    let mut invalid = decision(&replica, "first", "not allowed whitespace");
    assert!(
        replica
            .replace_reviewed_batch(std::slice::from_ref(&invalid))
            .is_err()
    );
    invalid.replacement_id = "safe-id".into();
    invalid.action = json!({"type":"change_status","expected_revision":7});
    assert!(
        replica
            .replace_reviewed_batch(std::slice::from_ref(&invalid))
            .is_err()
    );
    invalid.action = json!({"type":"change_status","command_id":"foreign"});
    assert!(replica.replace_reviewed_batch(&[invalid]).is_err());
    assert_eq!(
        replica.command_status("first").unwrap().unwrap()["state"],
        "conflict"
    );
}

#[test]
fn original_rejected_id_cannot_be_used_as_its_own_replacement() {
    let mut replica = new_replica();
    let decision = decision(&replica, "first", "first");
    let old = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[decision]).is_err());
    assert_eq!(rows(&replica), old);
}

#[test]
fn tampered_review_kind_is_not_sufficient_to_change_family() {
    let mut replica = new_replica();
    let mut item = decision(&replica, "first", "new-first");
    item.review.kind = "project".into();
    item.kind = CommandKind::Project;
    let old = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[item]).is_err());
    assert_eq!(rows(&replica), old);
}

#[test]
fn two_sequential_batches_do_not_reuse_old_reviews() {
    let mut replica = new_replica();
    let first = decision(&replica, "first", "new-first");
    let second = decision(&replica, "second", "new-second");
    replica
        .replace_reviewed_batch(std::slice::from_ref(&first))
        .unwrap();
    assert!(replica.replace_reviewed_batch(&[first]).is_err());
    replica.replace_reviewed_batch(&[second]).unwrap();
    assert_eq!(
        replica.command_status("new-first").unwrap().unwrap()["state"],
        "pending"
    );
    assert_eq!(
        replica.command_status("new-second").unwrap().unwrap()["state"],
        "pending"
    );
}

#[test]
fn reviewed_batch_never_marks_an_intention_accepted_without_server_receipt() {
    let mut replica = new_replica();
    let before = replica.snapshot().unwrap();
    let decisions = vec![
        decision(&replica, "first", "new-first"),
        decision(&replica, "second", "new-second"),
    ];
    replica.replace_reviewed_batch(&decisions).unwrap();
    assert_eq!(replica.snapshot().unwrap(), before);
    assert!(replica.accepted_receipt("new-first").unwrap().is_none());
    assert!(replica.accepted_receipt("new-second").unwrap().is_none());
    assert!(
        replica
            .unresolved()
            .unwrap()
            .iter()
            .all(|row| row["state"] == "pending")
    );
}

#[test]
fn null_action_and_empty_action_are_rejected_before_any_write() {
    let mut replica = new_replica();
    for invalid in [Value::Null, json!({}), json!([]), json!("plain-text")] {
        let mut item = decision(&replica, "first", "new-first");
        item.action = invalid;
        let old = rows(&replica);
        assert!(replica.replace_reviewed_batch(&[item]).is_err());
        assert_eq!(rows(&replica), old);
    }
}

#[test]
fn empty_replacement_id_and_invalid_characters_do_not_move_rejected_rows() {
    let mut replica = new_replica();
    let old = rows(&replica);
    for bad_id in [
        "",
        "not allowed",
        "invalid/slash",
        "unicode-東京",
        "dot.name",
    ] {
        let item = decision(&replica, "first", bad_id);
        assert!(replica.replace_reviewed_batch(&[item]).is_err(), "{bad_id}");
        assert_eq!(rows(&replica), old);
    }
}

#[test]
fn duplicate_originals_are_rejected_even_when_replacements_differ() {
    let mut replica = new_replica();
    let old = rows(&replica);
    let first = decision(&replica, "first", "fresh-1");
    let second = decision(&replica, "first", "fresh-2");
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(rows(&replica), old);
    assert_eq!(replica.convergence_status().unwrap().conflicts, 2);
}

#[test]
fn a_late_cross_family_attempt_cannot_stage_a_valid_prefix() {
    let mut replica = new_replica();
    let valid = decision(&replica, "first", "fresh-1");
    let mut wrong = decision(&replica, "second", "fresh-2");
    wrong.kind = CommandKind::Schedule;
    let old = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[valid, wrong]).is_err());
    assert_eq!(rows(&replica), old);
    assert!(replica.command_status("fresh-1").unwrap().is_none());
}

#[test]
fn a_late_invalid_intention_cannot_stage_a_valid_prefix() {
    let mut replica = new_replica();
    let valid = decision(&replica, "first", "fresh-1");
    let mut invalid = decision(&replica, "second", "fresh-2");
    invalid.action = json!({});
    let old = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[valid, invalid]).is_err());
    assert_eq!(rows(&replica), old);
    assert!(replica.command_status("fresh-1").unwrap().is_none());
}

#[test]
fn a_review_bound_to_another_server_fingerprint_cannot_stage_anything() {
    let mut replica = new_replica();
    let mut first = decision(&replica, "first", "fresh-1");
    let second = decision(&replica, "second", "fresh-2");
    first.review.history_fingerprint = "f".repeat(64);
    let old = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[first, second]).is_err());
    assert_eq!(rows(&replica), old);
    assert_eq!(replica.revision().unwrap(), Some(7));
}

#[test]
fn explicit_reviews_do_not_change_cached_markdown_or_accepted_domain_data() {
    let mut replica = new_replica();
    let old_snapshot = replica.snapshot().unwrap();
    let old_cache = replica.cached_markdown_inventory(100).unwrap();
    let decisions = vec![
        decision(&replica, "first", "fresh-1"),
        decision(&replica, "second", "fresh-2"),
    ];
    replica.replace_reviewed_batch(&decisions).unwrap();
    assert_eq!(replica.snapshot().unwrap(), old_snapshot);
    assert_eq!(replica.cached_markdown_inventory(100).unwrap(), old_cache);
    assert_eq!(replica.revision().unwrap(), Some(7));
}

#[test]
fn a_review_for_a_command_that_is_no_longer_conflicted_is_not_valid() {
    let mut replica = new_replica();
    let item = decision(&replica, "first", "fresh-1");
    replica
        .db
        .execute(
            "UPDATE authority_outbox SET state='pending' WHERE command_id='first'",
            [],
        )
        .unwrap();
    let before = rows(&replica);
    assert!(replica.replace_reviewed_batch(&[item]).is_err());
    assert_eq!(rows(&replica), before);
}

#[test]
fn an_independent_replica_never_inherits_another_clients_resolution() {
    let mut first = new_replica();
    let second = new_replica();
    let decision = decision(&first, "first", "reviewed-first");
    first.replace_reviewed_batch(&[decision]).unwrap();
    assert!(first.command_status("reviewed-first").unwrap().is_some());
    assert!(second.command_status("reviewed-first").unwrap().is_none());
    assert_eq!(
        second.command_status("first").unwrap().unwrap()["state"],
        "conflict"
    );
    assert_eq!(second.revision().unwrap(), Some(7));
    assert_eq!(first.revision().unwrap(), Some(7));
}
