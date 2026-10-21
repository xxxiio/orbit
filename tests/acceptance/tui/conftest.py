"""Skip real-binary PTY tests when the required binaries are not built."""
from __future__ import annotations

import os
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
TUI = Path(os.environ.get("ORBIT_TUI_BIN", ROOT / "target" / "debug" / "orbit-tui"))
CLI = Path(os.environ.get("ORBIT_CLI_BIN", ROOT / "target" / "debug" / "orbit"))


def pytest_collection_modifyitems(config, items):
    missing = [str(path) for path in (TUI, CLI) if not path.is_file()]
    if not missing:
        return
    marker = pytest.mark.skip(
        reason="real TUI binaries are not built: " + ", ".join(missing)
        + "; run cargo build -p orbit-tui -p orbit-cli"
    )
    for item in items:
        item.add_marker(marker)
