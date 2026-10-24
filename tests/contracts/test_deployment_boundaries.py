"""Static boundary guards for the M20 implementation contract.

Executable Rust behavior belongs to M15, but source integration must fail if
connected clients silently reintroduce local authority initialization.

Assertions that inspect Rust syntax are whitespace-insensitive so the mandated
`cargo fmt` gate and this source-contract suite cannot disagree.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TUI = ROOT / "crates/orbit-tui/src"


def source(path):
    return (ROOT / path).read_text()


def tui(path):
    return (TUI / path).read_text(encoding="utf-8")


def compact(text):
    return re.sub(r"\s+", "", text)


def function_body(text, signature):
    """Extract one function body by brace matching from its signature."""
    start = text.index(signature)
    opening = text.index("{", start)
    depth = 0
    for index in range(opening, len(text)):
        character = text[index]
        if character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return text[start : index + 1]
    raise AssertionError(f"unterminated function body for {signature}")


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
    # The legacy connected event loop is retired; the connected TUI is the
    # shared shell over the connected backend plus the console operations module.
    backend = tui("backend.rs")
    console = tui("connected_ops.rs")
    connected_actions = tui("connected_actions.rs")
    for name, text in (
        ("backend.rs", backend),
        ("connected_ops.rs", console),
        ("connected_actions.rs", connected_actions),
    ):
        assert "AuthoritativeHost::open(" not in text, name
        assert "SqliteStore::open(" not in text, name
    # The connected console modules never construct any client at all.
    for name, text in (
        ("connected_ops.rs", console),
        ("connected_actions.rs", connected_actions),
        ("app/connected.rs", tui("app/connected.rs")),
        ("app/actions.rs", tui("app/actions.rs")),
    ):
        assert "OrbitClient::open(" not in text, name
        assert "AuthoritativeHost::open(" not in text, name

    # The connected constructor is remote-only: it never falls back to a local
    # store or a local facade, in either entrypoint.
    assert backend.count("AuthorityConnection::remote(") == 1
    # The local facade opener lives in the local-only module, never here.
    assert "OrbitClient::open(" not in backend
    assert "OrbitClient::open(" in tui("local_backend.rs")
    assert "AuthorityConnection::remote(" not in tui("local_backend.rs")
    for signature in (
        "pub fn open_connected(",
        "pub fn open_connected_with_identity(",
    ):
        body = function_body(backend, signature)
        assert "OrbitClient::open(" not in body, signature
        assert "AuthoritativeHost::open(" not in body, signature
        assert "SqliteStore::open(" not in body, signature
    assert "AuthorityConnection::remote(" in function_body(
        backend, "pub fn open_connected_with_identity("
    )
    assert "mod connected;" not in tui("main.rs")
    assert not (TUI / "connected.rs").exists()


FORBIDDEN_CONNECTED_CONSTRUCTORS = (
    "AuthorityConnection::embedded(",
    "EmbeddedAuthorityClient",
    "AuthoritativeHost::open",
    "SqliteStore::open",
    "OrbitClient::open(",
)


def connected_guard_modules():
    """Every production module that participates in the connected shell."""
    paths = [
        TUI / "backend.rs",
        TUI / "connected_ops.rs",
        TUI / "connected_actions.rs",
    ]
    for directory in ("app", "input", "render"):
        paths.extend(sorted((TUI / directory).rglob("*.rs")))
    return paths


def test_every_connected_module_forbids_embedded_constructors():
    """Adding an embedded constructor anywhere in the shell must fail here.

    The M29 gate probe added `AuthorityConnection::embedded(path, ...)` (and
    `EmbeddedAuthorityClient::open` / `Backend::open_local`) to connected code
    and the suite stayed green. This guard names every owner module, not only
    `backend.rs`.
    """
    files = connected_guard_modules()
    rels = {path.relative_to(ROOT).as_posix() for path in files}
    for expected in (
        "crates/orbit-tui/src/backend.rs",
        "crates/orbit-tui/src/app/connected.rs",
        "crates/orbit-tui/src/app/mod.rs",
        "crates/orbit-tui/src/input/modal.rs",
        "crates/orbit-tui/src/render/shell.rs",
        "crates/orbit-tui/src/connected_ops.rs",
        "crates/orbit-tui/src/connected_actions.rs",
    ):
        assert expected in rels, f"guard baseline module disappeared: {expected}"

    for path in files:
        text = compact(path.read_text(encoding="utf-8"))
        rel = path.relative_to(ROOT).as_posix()
        for forbidden in FORBIDDEN_CONNECTED_CONSTRUCTORS:
            assert compact(forbidden) not in text, (
                f"{rel} must not construct an embedded authority: {forbidden}"
            )


def test_local_backend_constructor_is_confined_to_selection_and_definition():
    """`Backend::open_local` only appears at deployment selection + definition."""
    production = [
        path
        for path in TUI.rglob("*.rs")
        if "tests" not in path.relative_to(TUI).parts
    ]
    callers = {
        path.relative_to(ROOT).as_posix()
        for path in production
        if "Backend::open_local(" in path.read_text(encoding="utf-8")
    }
    assert callers == {"crates/orbit-tui/src/app/mod.rs"}, callers
    definitions = {
        path.relative_to(ROOT).as_posix()
        for path in production
        if re.search(r"\bfn\s+open_local\s*\(", path.read_text(encoding="utf-8"))
    }
    assert definitions == {"crates/orbit-tui/src/local_backend.rs"}, definitions
    named = {
        path.relative_to(ROOT).as_posix()
        for path in production
        if re.search(r"\bopen_local\s*\(", path.read_text(encoding="utf-8"))
    }
    assert named == {
        "crates/orbit-tui/src/app/mod.rs",
        "crates/orbit-tui/src/local_backend.rs",
    }, named
    assert "Backend::open_local(" in function_body(
        tui("app/mod.rs"), "pub fn open_deployment("
    )


def test_console_modules_cannot_open_an_embedded_authority():
    console = tui("connected_ops.rs")
    for forbidden in (
        "OrbitClient::open(",
        "AuthoritativeHost::open(",
        "SqliteStore::open(",
        "AuthorityConnection::remote(",
    ):
        assert forbidden not in console, forbidden
    # The console only produces typed requests; the shell performs the reads.
    assert "pub fn parse(" in console
    assert "pub fn approve_request(" in console


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
