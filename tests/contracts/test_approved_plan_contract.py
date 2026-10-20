"""Source-level smoke assertions for the approval-to-schedule integration.

These intentionally do not replace Cargo's executable Rust regression tests.
Rust-syntax assertions are whitespace-insensitive so the `cargo fmt` gate and
this suite cannot disagree.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = (ROOT / 'crates/orbit-runtime/src/planning_approval.rs').read_text()
SERVER = (ROOT / 'crates/orbit-server/src/main.rs').read_text()
EMBEDDED = (ROOT / 'crates/orbit-client/src/lib.rs').read_text()
CONNECTED = (ROOT / 'crates/orbit-client/src/connected.rs').read_text()
CLI = (ROOT / 'crates/orbit-cli/src/task_commands.rs').read_text()
CLI_ARGS = (ROOT / 'crates/orbit-cli/src/commands.rs').read_text()


def compact(text):
    return re.sub(r"\s+", "", text)


def test_approval_requires_exact_review_and_confirmation():
    runtime = compact(RUNTIME)
    for term in ['validate_approval(&approval',
                 'apply_task_plan_accepted(actor,command_id,expected_revision,command)',
                 'plan execution requires explicit approval',
                 'accepted revision changed before plan review',
                 'total_reserved_minutes']:
        assert compact(term) in runtime, term


def test_remote_and_local_adapters_use_authoritative_path():
    for route in ('/v1/authoritative/plan-review', '/v1/authoritative/plan-apply-approved'):
        assert route in SERVER and route in CONNECTED
    assert compact('let actor=device(&state,&headers)?;') in compact(SERVER)
    assert 'apply_approved_task_plan_accepted' in EMBEDDED
    assert 'review_task_plan' in EMBEDDED and 'approve_task_plan' in EMBEDDED


def test_cli_requires_second_confirmation_and_receipt():
    assert 'TaskSubcommand::ApplyReviewedSchedule' in CLI
    assert compact('if !args.confirm') in compact(CLI)
    assert 'TaskSubcommand::ReviewSchedule' in CLI
    assert compact('confirm:bool') in compact(CLI_ARGS)
    assert 'review_schedule_request' in CLI
    assert 'execute_schedule_request' in CLI


def test_canonical_receipt_retry_and_no_partial_mutations():
    assert 'permit_existing_receipt_blocks' in RUNTIME
    assert 'wire_approval_commits_atomically_and_retries_idempotently' in RUNTIME
    assert 'multi_task_approval_creates_one_atomic_revision' in RUNTIME
    assert 'reviewed_plan_rejects_changed_command_id' in RUNTIME
