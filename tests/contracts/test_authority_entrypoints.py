"""Production command entrypoint architecture checks (source-level, M15 runtime acceptance later).

These guards are path-aware: the shared TUI shell was split out of `app.rs`
into `app/`, `input/` and `render/`, so a boundary that only names the old
single-file layout cannot prove anything. Every assertion below names the
real owner module and fails if that module disappears.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TUI = ROOT / "crates/orbit-tui/src"


def source(rel):
    return (ROOT / rel).read_text()


def tui(rel):
    return (TUI / rel).read_text(encoding="utf-8")


def tui_files():
    """Every Rust source of the TUI crate, including nested modules."""
    return sorted(TUI.rglob("*.rs"))


def production_tui_files():
    """TUI sources excluding inline and integration tests."""
    return [
        path
        for path in tui_files()
        if "tests" not in path.relative_to(TUI).parts
    ]


# Constructors that would give a connected shell an embedded authority again.
# The local facade opener is checked separately because its one legitimate
# caller is the deployment-selection site.
FORBIDDEN_CONNECTED_CONSTRUCTORS = (
    "AuthorityConnection::embedded(",
    "EmbeddedAuthorityClient",
    "AuthoritativeHost::open",
    "SqliteStore::open",
    "OrbitClient::open(",
)


def guarded_connected_modules():
    """Production modules that must never reach an embedded authority.

    This is the exact boundary the M29 review probed: adding a constructor to
    any of these files must fail this suite, not only to the CLI/runtime tests.
    """
    paths = [
        TUI / "backend.rs",
        TUI / "connected_ops.rs",
        TUI / "connected_actions.rs",
    ]
    for directory in ("app", "input", "render"):
        paths.extend(sorted((TUI / directory).rglob("*.rs")))
    return paths


def body_of(text, signature):
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


def test_cli_has_single_local_authority():
    cli = source("crates/orbit-cli/src/lib.rs")
    assert "AuthoritativeHost::open(&db_path)" in cli
    assert "run_habit_host(&mut host" in cli
    assert "SqliteStore" not in cli
    assert "fn run_habit(store:" not in cli
    assert "fn set_habit_active_cli(" not in cli


def test_connected_construction_goes_through_the_remote_connection():
    backend = tui("backend.rs")
    # The only connected constructor is the remote authority connection.
    assert backend.count("AuthorityConnection::remote(") == 1
    assert "AuthorityConnection::remote(" in body_of(
        backend, "pub fn open_connected_with_identity("
    )
    # Both public connected entrypoints delegate to that one constructor.
    for signature in ("pub fn open_connected(", "pub fn open_connected_with_identity("):
        body = body_of(backend, signature)
        assert "AuthorityConnection::remote(" in body or "open_connected_with_identity(" in body


def test_clients_do_not_own_connected_authority():
    assert not (TUI / "connected.rs").exists()
    assert "mod connected;" not in tui("main.rs")

    # No TUI module may construct an embedded authority or open a raw store.
    for path in tui_files():
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(ROOT).as_posix()
        assert "AuthoritativeHost::open" not in text, rel
        assert "SqliteStore::open" not in text, rel

    # The local facade is the only local constructor. It lives in the
    # local-only module, so no connected file (and no app/input/render module)
    # can reach it.
    openers = {
        path.relative_to(ROOT).as_posix()
        for path in tui_files()
        if "OrbitClient::open(" in path.read_text(encoding="utf-8")
    }
    assert openers == {"crates/orbit-tui/src/local_backend.rs"}, openers
    local = tui("local_backend.rs")
    assert local.count("OrbitClient::open(") == 1
    assert "OrbitClient::open(" in body_of(local, "pub fn open_local(")

    backend = tui("backend.rs")
    assert "OrbitClient::open(" not in backend
    for signature in (
        "pub fn open_connected(",
        "pub fn open_connected_with_identity(",
    ):
        body = body_of(backend, signature)
        assert "AuthorityConnection::remote(" in body or "open_connected_with_identity(" in body
        assert "OrbitClient::open(" not in body, signature
        assert "AuthoritativeHost::open" not in body, signature
        assert "SqliteStore::open" not in body, signature


def test_connected_modules_forbid_every_embedded_constructor():
    """No connected owner may construct embedded authority or a raw store.

    The older guard only named `backend.rs` and the console modules, so adding
    `AuthorityConnection::embedded(path, ...)` to an `app/`, `input/` or
    `render/` module left the suite green. Every module that participates in
    the connected shell is now checked, including the constructors the M29
    review used to probe the boundary.
    """
    files = guarded_connected_modules()
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
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(ROOT).as_posix()
        for forbidden in FORBIDDEN_CONNECTED_CONSTRUCTORS:
            assert forbidden not in text, (
                f"{rel} must not construct an embedded authority: {forbidden}"
            )


def test_backend_open_local_is_confined_to_deployment_selection():
    """The local facade opener lives only at selection + definition.

    `App` is allowed to choose a local backend (that is the deployment
    decision); no connected module may call the opener directly.
    """
    production = production_tui_files()
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

    # No other production module may even name the local opener.
    named = {
        path.relative_to(ROOT).as_posix()
        for path in production
        if re.search(r"\bopen_local\s*\(", path.read_text(encoding="utf-8"))
    }
    assert named == {
        "crates/orbit-tui/src/app/mod.rs",
        "crates/orbit-tui/src/local_backend.rs",
    }, named

    local = tui("local_backend.rs")
    assert local.count("fn open_local(") == 1
    selection = tui("app/mod.rs")
    assert "Backend::open_local(" in body_of(selection, "pub fn open_deployment(")


def test_legacy_connected_switch_is_gone():
    for path in tui_files():
        text = path.read_text(encoding="utf-8")
        assert "ORBIT_TUI_LEGACY_CONNECTED" not in text, path.relative_to(ROOT).as_posix()


def test_key_dispatch_has_one_owner_per_surface():
    """`handle_key` and `handle_modal` must each be defined exactly once.

    The I5 split moved dispatch into `input/dispatch.rs` and modal key handling
    into `input/modal.rs`; a resurrected copy in the shell or the app modules
    would make shortcut precedence order-dependent again.
    """
    pattern = re.compile(r"\bfn\s+(handle_key|handle_modal)\s*[<(]")
    owners = {}
    for path in tui_files():
        rel = path.relative_to(ROOT).as_posix()
        for name in pattern.findall(path.read_text(encoding="utf-8")):
            owners.setdefault(name, []).append(rel)
    assert owners == {
        "handle_key": ["crates/orbit-tui/src/input/dispatch.rs"],
        "handle_modal": ["crates/orbit-tui/src/input/modal.rs"],
    }, owners
    assert "pub(crate) use dispatch::handle_key;" in tui("input/mod.rs")
    assert "pub(crate) use modal::handle_modal;" in tui("input/mod.rs")


def test_runtime_owns_storage():
    assert "SqliteStore" in source("crates/orbit-runtime/src/store.rs")
