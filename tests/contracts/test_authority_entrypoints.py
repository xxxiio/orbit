"""Production command entrypoint architecture checks (source-level, M15 runtime acceptance later)."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

def test_cli_has_single_local_authority():
    source = (ROOT / "crates/orbit-cli/src/lib.rs").read_text()
    assert "AuthoritativeHost::open(&db_path)" in source
    assert "run_habit_host(&mut host" in source
    assert "SqliteStore" not in source
    assert "fn run_habit(store:" not in source
    assert "fn set_habit_active_cli(" not in source

def test_clients_do_not_own_connected_authority():
    backend = (ROOT / "crates/orbit-tui/src/backend.rs").read_text()
    assert "AuthorityConnection::remote(" in backend
    assert not (ROOT / "crates/orbit-tui/src/connected.rs").exists()
    for path in (
        "crates/orbit-tui/src/backend.rs",
        "crates/orbit-tui/src/app.rs",
        "crates/orbit-tui/src/connected_ops.rs",
    ):
        source = (ROOT / path).read_text()
        assert "AuthoritativeHost::open" not in source, path
        assert "SqliteStore::open" not in source, path

def test_runtime_owns_storage():
    source = (ROOT / "crates/orbit-runtime/src/store.rs").read_text()
    assert "SqliteStore" in source
