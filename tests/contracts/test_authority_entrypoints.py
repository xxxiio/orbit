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
    source = (ROOT / "crates/orbit-tui/src/connected.rs").read_text()
    assert "AuthorityReplica" in source
    assert "AuthoritativeHost::open" not in source
    assert "SqliteStore::open" not in source

def test_runtime_owns_storage():
    source = (ROOT / "crates/orbit-runtime/src/store.rs").read_text()
    assert "SqliteStore" in source
