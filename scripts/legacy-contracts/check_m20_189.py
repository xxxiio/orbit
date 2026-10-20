"""M20 strategy authority wiring and storage invariants.

This source audit supplements (never replaces) compiled Rust and actual server
integration tests. It fails when any transport layer is omitted after editing
strategy command support in one part of the codebase.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

def source(relative):
    path = ROOT / relative
    assert path.is_file(), f"missing source: {relative}"
    return path.read_text()

strategy = source("crates/orbit-runtime/src/authority_strategy.rs")
runtime = source("crates/orbit-runtime/src/lib.rs")
server = source("crates/orbit-server/src/main.rs")
embedded = source("crates/orbit-client/src/embedded_authority.rs")
unified = source("crates/orbit-client/src/unified.rs")
connected = source("crates/orbit-client/src/connected.rs")
replica = source("crates/orbit-client/src/authority.rs")
docs = source("README_M20_189.md")

checks = {
    "Strategy command has explicit allowed actions": all(token in strategy for token in [
        "pub enum StrategyAction", "Configure {", "SetObjective {",
        "SetCriterionStatus {", "SetProgressSignals {",
    ]),
    "Strategy input is strictly deserialized": strategy.count("deny_unknown_fields") >= 5,
    "All six supported project modes checked": all(token in strategy for token in [
        '"standard"', '"learning"', '"habit_driven"', '"metric_target"',
        '"maintenance"', '"custom"',
    ]),
    "Objective and criteria are bounded": all(token in strategy for token in [
        "text.len() <= 8192", "criteria.len() > 100", "len() > 4096",
        "validate_criteria(&criteria)?", "HashSet::new()",
    ]),
    "Signal domain is explicitly enumerated": all(token in strategy for token in [
        '"learning_evidence"', '"knowledge_evidence"', '"metric_value"',
        '"habit_consistency"', '"time_invested"',
    ]),
    "History receipt uses the same immediate SQL transaction": all(token in strategy for token in [
        "TransactionBehavior::Immediate", "pending.check(&tx)?",
        "pending.commit(&tx,revision,semantic,&receipt)?", "tx.commit()?",
    ]),
    "Accepted snapshot includes project strategy": all(token in strategy for token in [
        "pub fn authority_strategies", "StrategySnapshot", "ORDER BY id",
    ]),
    "Strategy command checks project existence": all(token in strategy for token in [
        "require_project(&tx, &project_id)?", "strategy project does not exist",
    ]),
    "Success criteria preserve ordered positions": all(token in strategy for token in [
        "ORDER BY position", "position as i64", "criterion_key",
    ]),
    "Criterion status does not reset configuration": all(token in strategy for token in [
        "SetCriterionStatus", "UPDATE project_success_criteria SET status",
    ]),
    "Core host exposes reads and writes": all(token in runtime for token in [
        "pub mod authority_strategy", "pub fn strategies(&self)",
        "pub fn strategy_command(&mut self",
    ]),
    "Server offers strategy endpoints": all(token in server for token in [
        "/v1/authoritative/strategies", "/v1/authoritative/strategy-commands",
        "authoritative_strategy_command", "authoritative_strategies",
    ]),
    "Server snapshot includes strategies and revision check": all(token in server for token in [
        "strategies:strategies.strategies", "strategies.revision != revision",
    ]),
    "Embedded client uses shared authority host": all(token in embedded for token in [
        "self.host.strategies()", "self.host.strategy_command(&self.actor,command)",
    ]),
    "Remote client queues typed strategy action": all(token in connected for token in [
        "queue_strategy_action", "CommandKind::Strategy", "strategies:collection",
    ]),
    "Replica knows strategy outbox kind and endpoint": all(token in replica for token in [
        "Self::Strategy", '"strategy-commands"', "'strategy'",
    ]),
    "Replica validates receipt identity": "strategy receipt project identity mismatch" in replica,
    "Replica requires accepted strategy collection": '"milestones","strategies"' in replica,
    "Unified embedded and remote commands": all(token in unified for token in [
        "pub fn strategy_command", "host.strategy_command(command)?",
        "replica.queue_strategy_action", "remote_disposition(replica,&id)",
    ]),
    "Unified accepted snapshot includes strategies": all(token in unified for token in [
        '"strategies": strategies.strategies', '"strategies": workspace.strategies',
    ]),
    "Regression suite covers domain and remote semantics": (
        strategy.count("#[test]") >= 30
        and "remote_strategy_is_pending_until_server_acceptance" in unified
        and "remote_conflicting_reuse_of_identity_fails" in unified
        and "stale_revision_is_rejected_before_domain_updates" in strategy
    ),
    "Documentation explicitly names verification caveat": all(token in docs for token in [
        "Rust toolchain is unavailable", "not** final acceptance", "Internal test readiness",
    ]),
}

for title, passed in checks.items():
    print(f"{'PASS' if passed else 'FAIL'}: {title}")
assert all(checks.values()), "M20 strategy source checks failed"
print(f"M20 strategy source contracts: {len(checks)} passed (NOT Rust execution)")
