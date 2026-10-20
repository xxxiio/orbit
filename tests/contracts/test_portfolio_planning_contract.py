"""Source-shape checks for the new portfolio recommendation vertical slice.

These are NOT substitutes for executable Rust compilation or behavioral tests.
Rust-syntax assertions are whitespace-insensitive so the `cargo fmt` gate and
this suite cannot disagree.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

def source(path):
    return (ROOT / path).read_text()

def compact(text):
    return re.sub(r"\s+", "", text)


def test_authoritative_and_offline_clients_share_algorithm():
    host = source('crates/orbit-runtime/src/lib.rs')
    client = source('crates/orbit-client/src/unified/planning_queries.rs')
    assert compact('pub fn portfolio_actions(&self') in compact(host)
    assert compact('pub fn portfolio_actions(&self') in compact(client)
    assert 'portfolio_actions::recommend_portfolio_actions' in host
    assert 'orbit_runtime::portfolio_actions::recommend_portfolio_actions' in client
    assert compact('revision!=readiness.revision') in compact(host)
    assert compact('revision!=readiness.revision') in compact(client)


def test_mcp_and_registry_parity():
    reg = source('crates/orbit-runtime/src/capabilities.rs')
    connected = source('crates/orbit-mcp/src/connected_backend.rs')
    embedded = source('crates/orbit-mcp/src/embedded_backend.rs')
    main = source('crates/orbit-mcp/src/main.rs')
    assert compact('id:"planning.portfolio_actions"') in compact(reg)
    for adapter in [connected,embedded]:
        assert compact('"planning.portfolio_actions"=>') in compact(adapter)
        assert 'project-scoped agents must request a specific authorized project' in adapter
        assert 'requested project is not authorized' in adapter
    assert main.count('mcp_tool("planning.portfolio_actions")') == 2


def test_cli_and_server_have_read_only_entrypoints():
    commands=source('crates/orbit-cli/src/commands.rs')
    cli=source('crates/orbit-cli/src/task_commands.rs')
    server=source('crates/orbit-server/src/main.rs')
    assert 'PortfolioActions(TaskPortfolioActionsArgs)' in commands
    assert 'TaskSubcommand::PortfolioActions(args)' in cli
    assert '/v1/authoritative/portfolio-actions' in server
    assert 'authoritative_portfolio_actions' in server


def test_no_unaccepted_pending_commands_in_recommendation_model():
    model=compact(source('crates/orbit-runtime/src/portfolio_actions.rs'))
    for term in ['evaluate_readiness(revision,tasks,edges',
                 'analyze_task_graph(revision,tasks,edges',
                 'visible_downstream_task_ids',
                 '.filter(|child|visible(child))',
                 'fn scoped_response_never_leaks_foreign_task_names_or_ids',
                 'fn source_order_is_immaterial',
                 'fn cannot_claim_immediate_unlock_if_another_prerequisite_is_open']:
        assert compact(term) in model, term


def test_portfolio_capacity_is_integrated_without_mutations():
    model=compact(source('crates/orbit-runtime/src/portfolio_capacity.rs'))
    for term in ['recommend_portfolio_actions(revision,tasks,edges,&query)',
                 'fn unsized_work_requires_explicit_opt_in_and_positive_fallback',
                 'fn scoped_plan_has_no_foreign_identifiers_in_exclusions',
                 'fn selection_is_stable_under_input_permutation']:
        assert compact(term) in model, term
    assert 'PortfolioCapacity(TaskPortfolioCapacityArgs)' in source('crates/orbit-cli/src/commands.rs')
    assert 'TaskSubcommand::PortfolioCapacity(args)' in source('crates/orbit-cli/src/task_commands.rs')
    assert '/v1/authoritative/portfolio-capacity' in source('crates/orbit-server/src/main.rs')
    assert compact('pub fn portfolio_capacity_plan(&self') in compact(source('crates/orbit-client/src/unified/planning_queries.rs'))
    for path in ['crates/orbit-mcp/src/embedded_backend.rs','crates/orbit-mcp/src/connected_backend.rs']:
        assert compact('"planning.portfolio_capacity"=>') in compact(source(path))
    assert source('crates/orbit-mcp/src/main.rs').count('mcp_tool("planning.portfolio_capacity")')==2
