"""Static boundary guards for the M20 implementation contract.

Executable Rust behavior belongs to M15, but source integration must fail if
connected clients silently reintroduce local authority initialization.

Assertions that inspect Rust syntax are whitespace-insensitive so the mandated
`cargo fmt` gate and this source-contract suite cannot disagree.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def source(path):
    return (ROOT / path).read_text()


def compact(text):
    return re.sub(r"\s+", "", text)


def test_shared_deployment_selection_is_used_by_client_adapters():
    # The CLI resolves the same deployment through the shared orbit-client
    # configuration resolver; TUI and MCP still call the shared from_env helper.
    cli = source("crates/orbit-cli/src/lib.rs")
    assert "config::resolve(" in cli
    assert "deployment::resolve_deployment(" in cli
    for adapter in ["orbit-tui/src/main.rs", "orbit-mcp/src/main.rs"]:
        content = source(f"crates/{adapter}")
        assert "deployment::from_env()" in content, adapter


def test_connected_tui_cannot_open_embedded_authority():
    connected = source("crates/orbit-tui/src/connected.rs")
    assert "AuthoritativeHost::open(" not in connected
    assert "SqliteStore::open(" not in connected
    assert "OrbitClient::open(" not in connected


def test_remote_client_cannot_open_authoritative_database():
    for path in ("crates/orbit-client/src/authority.rs", "crates/orbit-client/src/connected.rs"):
        text = source(path)
        assert "AuthoritativeHost::open(" not in text
        assert "SqliteStore::open(" not in text


def test_server_runs_commands_on_canonical_runtime():
    server = source("crates/orbit-server/src/main.rs")
    assert "Arc<Mutex<AuthoritativeHost>>" in server
    assert "AuthoritativeHost::open(" in server
    server_compact = compact(server)
    for operation in ["task_command", "evidence_command", "schedule_command", "routine_command", "milestone_command", "strategy_command"]:
        assert f"{operation}(" in server_compact, operation


def test_authoritative_host_owns_sqlite():
    host = source("crates/orbit-runtime/src/lib.rs")
    assert "authority: store::AuthorityStore" in host
    assert "store::AuthorityStore::open(path)" in host
    assert "SqliteStore::open(path)" not in host


def test_local_client_uses_host_and_not_its_own_store():
    facade = source("crates/orbit-client/src/lib.rs")
    assert "runtime: orbit_runtime::AuthoritativeHost" in facade
    assert "orbit_runtime::AuthoritativeHost::open(path)" in facade
    assert "SqliteStore::open(" not in facade


def test_local_cli_cannot_execute_when_connected_configuration_is_set():
    cli = source("crates/orbit-cli/src/lib.rs")
    assert "connected deployment: use `orbit sync replica`" in cli
    assert cli.index("connected deployment: use `orbit sync replica`") < cli.index("AuthoritativeHost::open(&db_path)")


def test_deployment_resolver_fails_closed_on_partial_configuration():
    code = source("crates/orbit-client/src/deployment.rs")
    for required in ["MissingReplica", "MissingServer", "ConflictingServers", "SameDatabase", "EmptyValue"]:
        assert required in code
    assert "Err(DeploymentError::MissingReplica)" in code
    assert "Err(DeploymentError::MissingServer)" in code


def test_all_authoritative_domain_commands_share_runtime():
    runtime = source("crates/orbit-runtime/src/lib.rs")
    for name in ["domain_commands", "authority_evidence", "authority_schedule", "authority_routines", "authority_milestones", "authority_strategy", "markdown_commands", "folder_commands"]:
        assert f"mod {name};" in runtime or f"mod {name}" in runtime


def test_mcp_backend_modules_are_isolated():
    entry = source("crates/orbit-mcp/src/main.rs")
    connected = source("crates/orbit-mcp/src/connected_backend.rs")
    embedded = source("crates/orbit-mcp/src/embedded_backend.rs")
    assert "mod connected_backend;" in entry
    assert "mod embedded_backend;" in entry
    assert "AuthoritativeHost::open(" not in connected
    assert "AuthorityConnection::remote(" not in embedded
    assert "impl ConnectedMcp" in connected
    assert "impl Server" in embedded

def test_mcp_host_only_constructed_for_local_mode():
    entry = source("crates/orbit-mcp/src/main.rs")
    assert "deployment::Deployment::Connected" in entry
    assert "deployment::Deployment::Local" in entry
    assert entry.count("AuthoritativeHost::open(&database)") == 1
