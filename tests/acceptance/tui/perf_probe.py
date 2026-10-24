#!/usr/bin/env python3
"""Measure interaction latency of the real ``orbit-tui`` on a realistic fixture.

This is a helper, not a pytest test.  It launches the compiled binary in the
shared PTY harness, drives a fixed interaction sequence, and reports median /
p90 wall-clock durations per operation.

    python3 tests/acceptance/tui/perf_probe.py \
        --workspace /tmp/opencode/m29-<stamp> --repeat 5

If ``--workspace`` is omitted, the script builds the full 20-project / 200-task
fixture with ``scripts/tui-fixture.py`` first.  Set ``ORBIT_TUI_FULL_FIXTURE=1``
to force the full fixture when integrating with another runner.

Evidence is written to ``.agent/evidence/m29-perf-baseline.json`` and
``.agent/evidence/m29-perf-baseline.md`` by default.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import math
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
TUI_DIR = REPO_ROOT / "tests" / "acceptance" / "tui"
SCRIPTS_DIR = REPO_ROOT / "scripts"
EVIDENCE_DIR = REPO_ROOT / ".agent" / "evidence"
DEFAULT_JSON = EVIDENCE_DIR / "m29-perf-baseline.json"
DEFAULT_MD = EVIDENCE_DIR / "m29-perf-baseline.md"

sys.path.insert(0, str(TUI_DIR))

from pty_harness import (  # noqa: E402
    DEFAULT_TUI_BINARY,
    ENTER,
    ESC,
    TuiSession,
)

SEARCH_QUERY = "Retrieval"
SEARCH_RESULT = "Research: Retrieval Quality"
DENSE_PROJECT = "Performance Lab"
DEVELOPMENT_PROJECT = "Sync Reliability"
SEARCH_MODAL_RE = re.compile(r"[╭╔]\s*Search\b")
HOME_HEADER_RE = r"(?m)^\s*ORBIT\s*›\s*Home / Today"
DETAIL_ACTIONS = "Actions (press a to run)"
PREFIX_HINT = "h Home"


def _load_fixture_module():
    path = SCRIPTS_DIR / "tui-fixture.py"
    spec = importlib.util.spec_from_file_location("tui_fixture", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


FIXTURE = _load_fixture_module()


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------
def _now_ms() -> float:
    return time.perf_counter() * 1000.0


def _percentile(values: list[float], percentile: float) -> float:
    """Nearest-rank percentile (p90 of five samples is the maximum)."""
    ordered = sorted(values)
    if not ordered:
        return float("nan")
    rank = max(1, math.ceil(percentile / 100.0 * len(ordered)))
    return ordered[min(rank, len(ordered)) - 1]


def _reset_session_state(workspace) -> None:
    """Remove the durable TUI navigation state so every sample starts at Home."""
    state = workspace.home / ".orbit" / "state" / "tui-state.json"
    state.unlink(missing_ok=True)


def _git_revision() -> tuple[str, bool]:
    try:
        revision = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=str(REPO_ROOT),
            text=True,
            capture_output=True,
            check=False,
        ).stdout.strip()
        dirty = bool(
            subprocess.run(
                ["git", "status", "--porcelain"],
                cwd=str(REPO_ROOT),
                text=True,
                capture_output=True,
                check=False,
            ).stdout.strip()
        )
        return revision, dirty
    except OSError:
        return "", False


def _binary_info() -> dict[str, object]:
    binary = Path(
        os.environ.get("ORBIT_TUI_BIN", str(DEFAULT_TUI_BINARY))
    ).expanduser()
    info: dict[str, object] = {"path": str(binary), "exists": binary.is_file()}
    if binary.is_file():
        stat = binary.stat()
        info.update(
            {
                "size_bytes": stat.st_size,
                "mtime_ns": stat.st_mtime_ns,
                "mtime_iso": time.strftime(
                    "%Y-%m-%dT%H:%M:%SZ", time.gmtime(stat.st_mtime)
                ),
            }
        )
    return info


def _wait_search_closed(session: TuiSession, timeout: float = 5.0) -> None:
    session.wait_until_screen(
        lambda view: SEARCH_MODAL_RE.search(view) is None,
        timeout=timeout,
        description="search modal to close",
    )


def wait_home(session: TuiSession) -> None:
    session.wait_for_screen_regex(
        HOME_HEADER_RE, timeout=15.0, description="Home / Today header"
    )


def open_project_via_search(session: TuiSession, name: str) -> None:
    mark = session.mark()
    session.send(b"/", note="open search")
    session.wait_for_screen_regex(
        SEARCH_MODAL_RE, timeout=10.0, description="search modal"
    )
    session.send_text(name, delay=0.006)
    session.wait_for_screen_text(name, timeout=10.0, description=f"search hit {name!r}")
    session.send(ENTER, note=f"open project {name}")
    session.wait_for_screen_text(
        "switch view", timeout=15.0, description=f"project workspace {name!r}"
    )
    if name not in session.screen_text:
        session.wait_for_screen_text(name, timeout=5.0, description=f"header {name!r}")
    session.note("setup", {"opened_project": name, "mark": mark})


def cancel_prefix(session: TuiSession) -> None:
    session.send(ESC, note="cancel g prefix")
    session.wait_for_screen_text(
        "Prefix cancelled", timeout=5.0, description="prefix cancellation"
    )


# ---------------------------------------------------------------------------
# One measured session
# ---------------------------------------------------------------------------
class ProbeRun:
    def __init__(
        self,
        workspace,
        *,
        cols: int,
        rows: int,
        repeat: int,
    ) -> None:
        self.workspace = workspace
        self.cols = cols
        self.rows = rows
        self.repeat = repeat
        self.samples: dict[str, list[float]] = {}
        self.mutation_counter = 0

    def record(self, name: str, started_ms: float) -> None:
        self.samples.setdefault(name, []).append(_now_ms() - started_ms)

    def _session(self, sample: int) -> TuiSession:
        return TuiSession(
            self.workspace,
            cols=self.cols,
            rows=self.rows,
            wait_for_launch=False,
            label=(
                f"m29-perf-{self.cols}x{self.rows}-"
                f"r{sample + 1:02d}"
            ),
            launch_timeout=30.0,
        )

    def execute(self) -> None:
        for sample in range(self.repeat):
            _reset_session_state(self.workspace)
            started = _now_ms()
            session = self._session(sample)
            try:
                session.wait_for_text("ORBIT", timeout=30.0, description="first Orbit frame")
                self.record("cold_start_first_frame", started)
                wait_home(session)

                self._measure_search(session)
                self._measure_project_switch(session)
                self._measure_task_detail(session)
                self._measure_scroll(session)
                self._measure_quick_edit(session)
                self._measure_refresh(session)
                self._measure_view_switch(session)

                session.quit_via(b"q")
                session.assert_clean_exit()
            finally:
                session.close()

    # -- measured operations -------------------------------------------------
    def _measure_search(self, session: TuiSession) -> None:
        started = _now_ms()
        session.send(b"/", note="search: open")
        session.wait_for_screen_regex(
            SEARCH_MODAL_RE, timeout=10.0, description="search modal"
        )
        session.send_text(SEARCH_QUERY, delay=0.006)
        session.wait_for_screen_text(
            SEARCH_RESULT, timeout=10.0, description="first search result"
        )
        self.record("search_open_query_first_result", started)
        session.send(ESC, note="search: close")
        _wait_search_closed(session)

    def _measure_project_switch(self, session: TuiSession) -> None:
        # Sidebar switch: Home is the first sidebar row, so four ``j`` presses
        # land on the first project.  Open it, leave with Esc (history restores
        # the sidebar cursor), then move one row and open the next project.
        started = _now_ms()
        # BackTab from the workspace always lands on the sidebar; plain Tab
        # would reach the docked Detail pane first at the wide size.
        session.send(b"\x1b[Z", note="project switch: focus sidebar")
        session.send_keys(*([b"j"] * 4), delay=0.01)
        session.send(ENTER, note="project switch: open first sidebar project")
        session.wait_for_screen_text(
            "switch view", timeout=15.0, description="first project workspace"
        )
        session.send(ESC, note="project switch: leave")
        session.wait_for_screen_regex(
            HOME_HEADER_RE, timeout=10.0, description="Home after leave"
        )
        session.send(b"j", note="project switch: next sidebar row")
        session.send(ENTER, note="project switch: open next")
        session.wait_for_screen_text(
            "switch view", timeout=15.0, description="next project workspace"
        )
        self.record("project_switch_open_leave_open_next", started)

    def _open_dense_tasks(self, session: TuiSession) -> None:
        open_project_via_search(session, DENSE_PROJECT)
        session.send(b"2", note="Tasks view")
        session.wait_for_screen_text(
            "DEADLINE", timeout=15.0, description="dense Tasks table"
        )

    def _measure_task_detail(self, session: TuiSession) -> None:
        self._open_dense_tasks(session)
        started = _now_ms()
        session.send(ENTER, note="open task detail")
        session.wait_for_screen_text(
            DETAIL_ACTIONS, timeout=10.0, description="task detail modal"
        )
        self.record("open_task_detail", started)
        session.send(ESC, note="close task detail")
        session.wait_until_screen(
            lambda view: DETAIL_ACTIONS not in view,
            timeout=5.0,
            description="task detail modal to close",
        )

    def _measure_scroll(self, session: TuiSession) -> None:
        mark = session.mark()
        started = _now_ms()
        session.send_keys(*([b"j"] * 20), delay=0.005)
        # The g prefix hint is emitted only after every preceding j has been
        # consumed, so it is an input-order barrier without a sleep.
        session.send(b"g", note="scroll barrier")
        session.wait_for_text(
            PREFIX_HINT, timeout=10.0, since=mark, description="scroll barrier hint"
        )
        self.record("scroll_task_table_20_rows", started)
        cancel_prefix(session)

    def _measure_quick_edit(self, session: TuiSession) -> None:
        started = _now_ms()
        session.send(b"E", note="quick edit: open")
        session.wait_for_screen_text(
            "Quick edit", timeout=10.0, description="quick edit form"
        )
        session.send(b"\x7f")  # delete the final title character
        # Delete one character and append a longer unique suffix so every
        # sample really changes the title (and therefore exercises a save).
        self.mutation_counter += 1
        session.send_text(f"v{self.mutation_counter}", delay=0.01)
        session.send(ENTER, note="quick edit: save")
        session.wait_until_screen(
            lambda view: "Task renamed" in view or "Quick edit saved" in view,
            timeout=15.0,
            description="quick edit save",
        )
        self.record("save_quick_edit_title", started)

    def _measure_refresh(self, session: TuiSession) -> None:
        mark = session.mark()
        started = _now_ms()
        session.send(b"r", note="refresh")
        session.send(b"g", note="refresh barrier")
        session.wait_for_text(
            PREFIX_HINT, timeout=10.0, since=mark, description="refresh barrier hint"
        )
        self.record("refresh_project", started)
        cancel_prefix(session)

    def _measure_view_switch(self, session: TuiSession) -> None:
        open_project_via_search(session, DEVELOPMENT_PROJECT)
        started = _now_ms()
        session.send(b"]", note="switch blueprint view")
        session.wait_for_screen_text(
            "Blueprint work", timeout=15.0, description="development Board view"
        )
        self.record("switch_blueprint_view", started)


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------
def summarize(samples_by_size: dict[str, dict[str, list[float]]]) -> dict[str, object]:
    operations: dict[str, object] = {}
    all_names: list[str] = []
    for samples in samples_by_size.values():
        for name in samples:
            if name not in all_names:
                all_names.append(name)
    for name in all_names:
        per_size: dict[str, object] = {}
        for size, samples in samples_by_size.items():
            values = samples.get(name, [])
            if not values:
                continue
            per_size[size] = {
                "count": len(values),
                "samples_ms": [round(value, 3) for value in values],
                "min_ms": round(min(values), 3),
                "median_ms": round(statistics.median(values), 3),
                "p90_ms": round(_percentile(values, 90), 3),
                "max_ms": round(max(values), 3),
            }
        operations[name] = per_size
    return operations


def _markdown(
    report: dict[str, object],
    samples_by_size: dict[str, dict[str, list[float]]],
) -> str:
    operations = report["operations"]
    sizes = list(samples_by_size)
    counts = report["fixture_counts"]
    pinned = sorted(
        {
            (row["id"], row["version"])
            for row in counts.get("pinned_blueprints", [])
        }
    )
    lines = [
        "# M29 TUI interaction baseline",
        "",
        f"- Git revision: `{report['git_revision']}`"
        + (" (dirty tree)" if report["git_dirty"] else ""),
        f"- Binary: `{report['binary']['path']}`  ",
        f"  mtime `{report['binary'].get('mtime_iso', 'unknown')}`, "
        f"{report['binary'].get('size_bytes', 0)} bytes",
        f"- Workspace: `{report['workspace']}`",
        f"- Repeats per size: {report['repeat']}",
        f"- Sizes: {', '.join(sizes)}",
        "- Counts: "
        + ", ".join(
            f"{key}={counts[key]}"
            for key in (
                "projects",
                "tasks",
                "activities",
                "attachments",
                "agent_jobs",
            )
        )
        + f", distinct pinned blueprint versions={counts['distinct_pinned_versions']}",
        "- Pinned versions: "
        + ", ".join(f"`{blueprint_id}` v{version}" for blueprint_id, version in pinned),
        "",
        "## Measurements",
        "",
        "| Operation | " + " | ".join(f"{size} median / p90 (ms)" for size in sizes) + " |",
        "|---|" + "|".join("---:" for _ in sizes) + "|",
    ]
    for name, per_size in operations.items():
        cells = []
        for size in sizes:
            value = per_size.get(size)
            if value is None:
                cells.append("—")
            else:
                cells.append(f"{value['median_ms']:.1f} / {value['p90_ms']:.1f}")
        lines.append(f"| `{name}` | " + " | ".join(cells) + " |")

    lines += [
        "",
        "## How it was measured",
        "",
        "Each repeat launches a fresh `orbit-tui` process in the shared PTY harness",
        "against the same fixture workspace. `tui-state.json` is removed before every",
        "launch so the session starts at Home. A single session performs the measured",
        "operations in a fixed order; the wall-clock sample for an operation covers",
        "keystroke writes plus the wait until the target state is reconstructed from",
        "the PTY stream.",
        "",
        "- **cold_start_first_frame** — process launch through the first `ORBIT` output.",
        "- **search_open_query_first_result** — `/`, type `Retrieval`, wait for the first",
        "  project hit (`Research: Retrieval Quality`).",
        "- **project_switch_open_leave_open_next** — BackTab to the sidebar, `j` ×4 to",
        "  the first project, Enter to open, Esc back to Home, `j` + Enter to open the",
        "  next project.",
        f"- **open_task_detail** — Enter on the first task in `{DENSE_PROJECT}` Tasks; wait",
        "  for the detail modal action line.",
        "- **scroll_task_table_20_rows** — 20 `j` presses, then the `g` prefix hint as an",
        "  input-order barrier (no sleeps are used as assertions).",
        "- **save_quick_edit_title** — `E`, backspace one character, append a unique",
        "  suffix, Enter; wait for the rename outcome notification.",
        "- **refresh_project** — `r`, then the `g` prefix hint as an input-order barrier.",
        f"- **switch_blueprint_view** — `]` from Overview to Board in `{DEVELOPMENT_PROJECT}`",
        "  (development workflow); wait for the `Blueprint work` panel.",
        "",
        "## Reproduce",
        "",
        "```sh",
        "WS=/tmp/opencode/m29-$(date -u +%Y%m%dT%H%M%SZ)",
        "python3 scripts/tui-fixture.py --out \"$WS\" --json",
        "python3 tests/acceptance/tui/perf_probe.py --workspace \"$WS\" --repeat 5",
        "python3 -m pytest -q tests/acceptance/tui/test_m29_fixture.py",
        "```",
        "",
        "## Caveats",
        "",
        "These are interaction latencies measured through a PTY, not a benchmark claim.",
        "Wall-clock time includes process scheduling, terminal emulation, harness",
        "polling, and Rust debug-build overhead. The median and nearest-rank p90 are",
        "descriptive for this machine and fixture; do not compare them across hosts",
        "without rebuilding and re-running the probe. Later operations run with warm",
        "OS caches inside each repeat, and the serial order is part of the method.",
        "",
    ]
    return "\n".join(lines)


def _fixture_counts(workspace) -> dict[str, object]:
    return FIXTURE._read_counts(workspace)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--workspace",
        help="existing fixture workspace (built with scripts/tui-fixture.py --full)",
    )
    parser.add_argument("--repeat", type=int, default=5)
    parser.add_argument(
        "--sizes",
        default="100x30,160x45",
        help="comma-separated COLSxROWS list (default: 100x30,160x45)",
    )
    parser.add_argument("--json-out", default=str(DEFAULT_JSON))
    parser.add_argument("--md-out", default=str(DEFAULT_MD))
    args = parser.parse_args(argv)

    if args.repeat < 1:
        parser.error("--repeat must be at least 1")
    sizes = []
    for token in args.sizes.split(","):
        token = token.strip()
        if not token:
            continue
        match = re.fullmatch(r"(\d+)x(\d+)", token)
        if not match:
            parser.error(f"invalid size {token!r}; expected COLSxROWS")
        sizes.append((int(match.group(1)), int(match.group(2))))
    if not sizes:
        parser.error("at least one size is required")

    workspace_path = args.workspace
    if not workspace_path:
        import tempfile

        base = Path("/tmp/opencode")
        base.mkdir(parents=True, exist_ok=True)
        workspace_path = tempfile.mkdtemp(prefix="m29-perf-fixture-", dir=str(base))
        print(f"building full fixture at {workspace_path} ...", flush=True)
        FIXTURE.build_fixture(workspace_path, verbose=True)

    workspace = FIXTURE.workspace_from_path(workspace_path)
    counts = _fixture_counts(workspace)
    binary = _binary_info()
    revision, dirty = _git_revision()

    started_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    samples_by_size: dict[str, dict[str, list[float]]] = {}
    for cols, rows in sizes:
        size = f"{cols}x{rows}"
        print(f"probing {size} x {args.repeat} repeats ...", flush=True)
        run = ProbeRun(
            workspace,
            cols=cols,
            rows=rows,
            repeat=args.repeat,
        )
        run.execute()
        samples_by_size[size] = run.samples
        for name, values in run.samples.items():
            print(
                f"  {name}: median {statistics.median(values):.1f} ms "
                f"(n={len(values)})",
                flush=True,
            )

    operations = summarize(samples_by_size)
    report = {
        "schema": "m29-perf-baseline/v1",
        "generated_at": started_at,
        "git_revision": revision,
        "git_dirty": dirty,
        "binary": binary,
        "workspace": str(workspace.root),
        "repeat": args.repeat,
        "sizes": [f"{cols}x{rows}" for cols, rows in sizes],
        "fixture_counts": counts,
        "operations": operations,
        "method": {
            "harness": "tests/acceptance/tui/pty_harness.py TuiSession (real binary)",
            "workspace_reset": "tui-state.json removed before each launch",
            "session_order": (
                "cold start -> search -> project switch -> task detail -> "
                "scroll -> quick edit -> refresh -> view switch"
            ),
            "barriers": (
                "input-order barriers use the g prefix hint; no sleeps are used "
                "as assertions"
            ),
            "percentile": "nearest-rank p90",
            "units": "milliseconds of PTY wall-clock time",
        },
        "caveats": [
            "PTY wall-clock includes process scheduling, terminal emulation and harness polling",
            "interaction latency only; not a benchmark claim",
            "Rust debug binary unless ORBIT_TUI_BIN points at a release build",
            "later operations run with warm OS caches inside each repeat",
            "p90 over a small number of samples is the maximum for n=5",
        ],
    }

    json_path = Path(args.json_out).expanduser()
    md_path = Path(args.md_out).expanduser()
    json_path.parent.mkdir(parents=True, exist_ok=True)
    md_path.parent.mkdir(parents=True, exist_ok=True)
    json_path.write_text(
        json.dumps(report, indent=2, sort_keys=True), encoding="utf-8"
    )
    md_path.write_text(_markdown(report, samples_by_size), encoding="utf-8")
    print(f"wrote {json_path}")
    print(f"wrote {md_path}")

    print("\noperation | " + " | ".join(f"{cols}x{rows}" for cols, rows in sizes))
    for name, per_size in operations.items():
        cells = []
        for cols, rows in sizes:
            value = per_size.get(f"{cols}x{rows}")
            cells.append(
                "—" if value is None else f"{value['median_ms']:.1f} / {value['p90_ms']:.1f}"
            )
        print(f"{name} | " + " | ".join(cells))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
