//! Regression tests for the per-command outcome reporting owned by `super`.

use super::*;

#[test]
fn per_command_outcomes_keep_pending_and_conflict_out_of_success() {
    let (message, kind) = describe_mutation_outcome(
        Some(MutationOutcome::Pending {
            command_id: Some("cmd-1".into()),
            state: "pending".into(),
        }),
        true,
        "saved".into(),
    );
    assert_eq!(kind, StatusKind::Pending);
    assert!(message.contains("pending"), "{message}");
    assert!(
        message.contains("cmd-1") || message.contains("queued state"),
        "{message}"
    );

    let (message, kind) = describe_mutation_outcome(
        Some(MutationOutcome::Conflict {
            command_id: "cmd-1".into(),
            state: "conflict".into(),
            reason: "revision 4 is stale".into(),
        }),
        true,
        "saved".into(),
    );
    assert_eq!(kind, StatusKind::Warning);
    assert!(message.contains("rejected"), "{message}");
    assert!(message.contains("revision 4 is stale"), "{message}");
    assert!(message.contains("inspect the outbox"), "{message}");

    // Missing identity/state is fail-closed, never a success message.
    let (message, kind) = describe_mutation_outcome(
        Some(MutationOutcome::Pending {
            command_id: None,
            state: "unavailable".into(),
        }),
        true,
        "saved".into(),
    );
    assert_eq!(kind, StatusKind::Pending);
    assert!(message.contains("unavailable"), "{message}");

    let (message, kind) =
        describe_mutation_outcome(Some(MutationOutcome::Accepted), true, "saved".into());
    assert_eq!(kind, StatusKind::Success);
    assert_eq!(message, "saved");
}

#[test]
fn a_missing_outcome_is_pending_connected_and_success_local() {
    // A connected mutation with no recorded disposition is unobserved, so it
    // must not be reported as accepted. The local authority commits
    // synchronously and records no per-command receipt at all.
    let (message, kind) = describe_mutation_outcome(None, true, "saved".into());
    assert_eq!(kind, StatusKind::Pending, "{message}");
    assert!(message.contains("not accepted"), "{message}");
    assert!(message.contains("saved"), "{message}");

    let (message, kind) = describe_mutation_outcome(None, false, "saved".into());
    assert_eq!(kind, StatusKind::Success);
    assert_eq!(message, "saved");
}
