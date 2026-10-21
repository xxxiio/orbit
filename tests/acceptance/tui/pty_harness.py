"""Reusable PTY harness for driving the real Orbit terminal UI.

The harness deliberately uses only the Python standard library (``pty``,
``select``, ``fcntl``, ``termios``, ``struct``, ``os``/``subprocess``).  It
starts ``target/debug/orbit-tui`` in an isolated local deployment
(``ORBIT_DB`` plus an isolated ``HOME``/``ORBIT_CONFIG``), forces an explicit
window size, and captures everything the program writes.

Design notes
------------
* The child gets its own session and a controlling terminal so crossterm's
  raw-mode and resize code sees a normal TTY.
* Reads are non-blocking through ``select`` with deadlines.  A test that waits
  for a condition always has a deadline; on expiry the process group is
  terminated and the captured evidence is reported.
* ``session.raw`` holds the exact ANSI byte stream.  ``session.text`` is a
  stream-oriented, ANSI-stripped view: the concatenation of printable output.
  Use ``text_since(mark)`` to avoid matching text that was only present before
  an action.  ``session.screen_text`` reconstructs the *current* screen with a
  small VT model (cursor moves, erases, alternate screen), which is the right
  view for state assertions such as "the modal is closed" or "the footer
  currently shows this".
* Every session writes ``raw.ansi``, ``text.txt`` and ``meta.json`` under
  ``target/tui-evidence/<run-id>/<test>-<n>/`` (override with
  ``ORBIT_TUI_EVIDENCE_DIR``).

The expected product behaviour is documented in ``README.md`` next to this
module. The suite runs against the real compiled binary; it skips during
collection when `target/debug/orbit-tui` or `target/debug/orbit` is absent.
"""
from __future__ import annotations

import codecs
import errno
import fcntl
import json
import os
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Mapping

__all__ = [
    "CTRL_C",
    "CTRL_Q",
    "DEFAULT_CLI_BINARY",
    "DEFAULT_EVIDENCE_ROOT",
    "DEFAULT_TUI_BINARY",
    "ENTER",
    "ESC",
    "QUIT_SHORTCUT_RE",
    "REPO_ROOT",
    "TuiSession",
    "Workspace",
    "strip_ansi",
]

REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_TUI_BINARY = REPO_ROOT / "target" / "debug" / "orbit-tui"
DEFAULT_CLI_BINARY = REPO_ROOT / "target" / "debug" / "orbit"
DEFAULT_EVIDENCE_ROOT = REPO_ROOT / "target" / "tui-evidence"

ESC = b"\x1b"
ENTER = b"\r"
CTRL_C = b"\x03"
CTRL_Q = b"\x11"

# A footer quit shortcut is either `q ... quit` or `Ctrl+Q ... quit`.
# The gap tolerates separators such as "/", "·", ":" and "or".
QUIT_SHORTCUT_RE = re.compile(
    r"(?:\bq\b|Ctrl\s*\+\s*Q)[^\n]{0,32}?\bquit\b", re.IGNORECASE
)

_OSC_RE = re.compile(r"\x1b\](?:[^\x07\x1b]|\x1b(?!\\))*(?:\x07|\x1b\\)")
_CSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
_ESC_RE = re.compile(r"\x1b(?:[()][0-9A-Za-z]|[@-_])")
_STANDALONE_YES_RE = re.compile(r"(?<![A-Za-z])y(?![A-Za-z])")
_STANDALONE_NO_RE = re.compile(r"(?<![A-Za-z])n(?![A-Za-z])")


def strip_ansi(data: bytes | str) -> str:
    """Return a printable, stream-oriented view of terminal output."""
    if isinstance(data, bytes):
        text = data.decode("utf-8", "replace")
    else:
        text = data
    text = _OSC_RE.sub("", text)
    text = _CSI_RE.sub("", text)
    text = _ESC_RE.sub("", text)
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    # Drop remaining C0 controls except tab/newline.  Keep printable text.
    return "".join(
        character
        for character in text
        if character in "\n\t" or ord(character) >= 0x20
    )


class _TerminalScreen:
    """Small VT screen model for the escape sequences ratatui emits.

    This is intentionally not a complete terminal emulator.  It handles
    cursor positioning, erasing, line feeds and printable cells, which is
    enough to answer "what is on screen now?" inside assertions and evidence.
    """

    def __init__(self, cols: int, rows: int) -> None:
        self.cols = cols
        self.rows = rows
        self.grid = [[" "] * cols for _ in range(rows)]
        self.row = 0
        self.col = 0
        self.saved = (0, 0)
        # Bytes/characters held between feed() calls: an escape sequence or a
        # UTF-8 code point can straddle two PTY reads.
        self._pending = ""
        self._decoder = codecs.getincrementaldecoder("utf-8")("replace")

    def clear(self) -> None:
        self.grid = [[" "] * self.cols for _ in range(self.rows)]
        self.row = 0
        self.col = 0

    def _scroll(self) -> None:
        self.grid.pop(0)
        self.grid.append([" "] * self.cols)

    def _put(self, character: str) -> None:
        if character == "\x7f":
            return
        width = unicodedata.east_asian_width(character)
        if unicodedata.combining(character):
            return
        cell_width = 2 if width in ("W", "F") else 1
        if self.col >= self.cols:
            self.col = 0
            self.row += 1
            if self.row >= self.rows:
                self._scroll()
                self.row = self.rows - 1
        self.grid[self.row][self.col] = character
        if cell_width == 2 and self.col + 1 < self.cols:
            self.grid[self.row][self.col + 1] = ""
        self.col += cell_width

    def _erase_line(self, mode: int) -> None:
        if mode == 0:
            start, end = self.col, self.cols
        elif mode == 1:
            start, end = 0, min(self.cols, self.col + 1)
        else:
            start, end = 0, self.cols
        for index in range(start, end):
            self.grid[self.row][index] = " "

    def _erase_display(self, mode: int) -> None:
        if mode in (2, 3):
            self.clear()
        elif mode == 0:
            self._erase_line(0)
            for row in range(self.row + 1, self.rows):
                self.grid[row] = [" "] * self.cols
        elif mode == 1:
            self._erase_line(1)
            for row in range(0, self.row):
                self.grid[row] = [" "] * self.cols

    def _csi(self, parameter_text: str, final: str) -> None:
        private = parameter_text.startswith("?")
        raw = parameter_text[1:] if private else parameter_text
        try:
            params = [int(part) if part else 0 for part in raw.split(";")]
        except ValueError:
            params = []
        first = params[0] if params else 0
        if final in ("H", "f"):
            row = (params[0] if len(params) > 0 and params[0] else 1) - 1
            col = (params[1] if len(params) > 1 and params[1] else 1) - 1
            self.row = max(0, min(self.rows - 1, row))
            self.col = max(0, min(self.cols - 1, col))
        elif final == "A":
            self.row = max(0, self.row - max(1, first))
        elif final == "B":
            self.row = min(self.rows - 1, self.row + max(1, first))
        elif final == "C":
            self.col = min(self.cols - 1, self.col + max(1, first))
        elif final == "D":
            self.col = max(0, self.col - max(1, first))
        elif final == "E":
            self.row = min(self.rows - 1, self.row + max(1, first))
            self.col = 0
        elif final == "F":
            self.row = max(0, self.row - max(1, first))
            self.col = 0
        elif final in ("G", "`"):
            self.col = max(0, min(self.cols - 1, max(1, first) - 1))
        elif final == "d":
            self.row = max(0, min(self.rows - 1, max(1, first) - 1))
        elif final == "J":
            self._erase_display(first)
        elif final == "K":
            self._erase_line(first)
        elif final == "X":
            count = max(1, first)
            for index in range(self.col, min(self.cols, self.col + count)):
                self.grid[self.row][index] = " "
        elif final == "s":
            self.saved = (self.row, self.col)
        elif final == "u":
            self.row, self.col = self.saved
        elif final in ("h", "l") and private:
            if "1049" in raw or "1047" in raw or raw == "47":
                if final == "h":
                    self.clear()

    def feed(self, data: bytes) -> None:
        text = self._pending + self._decoder.decode(data)
        index = 0
        length = len(text)
        while index < length:
            character = text[index]
            if character == "\x1b":
                if index + 1 >= length:
                    break  # wait for the rest of the sequence
                following = text[index + 1]
                if following == "[":
                    end = index + 2
                    while end < length and not ("@" <= text[end] <= "~"):
                        end += 1
                    if end >= length:
                        break  # incomplete CSI
                    self._csi(text[index + 2 : end], text[end])
                    index = end + 1
                    continue
                if following == "]":
                    end = index + 2
                    complete = False
                    while end < length:
                        if text[end] == "\x07":
                            end += 1
                            complete = True
                            break
                        if (
                            text[end] == "\x1b"
                            and end + 1 < length
                            and text[end + 1] == "\\"
                        ):
                            end += 2
                            complete = True
                            break
                        end += 1
                    if not complete:
                        break  # incomplete OSC
                    index = end
                    continue
                if following in "()":
                    if index + 2 >= length:
                        break
                    index += 3
                    continue
                index += 2
                continue
            if character == "\r":
                self.col = 0
            elif character == "\n":
                self.row += 1
                if self.row >= self.rows:
                    self._scroll()
                    self.row = self.rows - 1
            elif character == "\b":
                self.col = max(0, self.col - 1)
            elif character == "\t":
                self.col = min(self.cols - 1, (self.col // 8 + 1) * 8)
            elif character >= " " and character != "\x7f":
                self._put(character)
            index += 1

        # Keep an incomplete escape sequence for the next feed() call.  If a
        # malformed fragment somehow never completes, cap the buffer.
        self._pending = text[index:]
        if len(self._pending) > 4096:
            self._pending = self._pending[-1:]

    def lines(self) -> list[str]:
        return ["".join(row).rstrip() for row in self.grid]


def _slug(value: str, limit: int = 60) -> str:
    value = re.sub(r"[^A-Za-z0-9_.-]+", "-", value).strip("-")
    return value[:limit] or "session"


def _resolve_binary(explicit: str | os.PathLike[str] | None) -> Path:
    candidate = Path(
        explicit
        or os.environ.get("ORBIT_TUI_BIN")
        or DEFAULT_TUI_BINARY
    ).expanduser()
    if not candidate.is_file():
        raise FileNotFoundError(
            f"orbit-tui binary not found at {candidate}. "
            "Build it with `cargo build -p orbit-tui` or set ORBIT_TUI_BIN."
        )
    if not os.access(candidate, os.X_OK):
        raise PermissionError(f"orbit-tui binary is not executable: {candidate}")
    return candidate


def _isolated_environment(db: Path, config: Path, home: Path) -> dict[str, str]:
    """Environment for a local deployment with no accidental connected mode."""
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("ORBIT_")
    }
    env.update(
        {
            "HOME": str(home),
            "ORBIT_CONFIG": str(config),
            "ORBIT_DB": str(db),
            "TERM": "xterm-256color",
            "LANG": env.get("LANG") or "C.UTF-8",
        }
    )
    # Keep output deterministic/readable and avoid locale surprises.
    env.pop("NO_COLOR", None)
    return env


@dataclass(frozen=True)
class Workspace:
    """One isolated Orbit local deployment used by a TUI session."""

    root: Path
    db: Path
    config: Path
    home: Path
    env: Mapping[str, str]

    @classmethod
    def create(cls, base: Path) -> "Workspace":
        root = Path(tempfile.mkdtemp(prefix="orbit-tui-", dir=str(base)))
        db = root / "orbit.db"
        config = root / "orbit.config.toml"
        home = root / "home"
        home.mkdir(parents=True, exist_ok=True)
        return cls(
            root=root,
            db=db,
            config=config,
            home=home,
            env=_isolated_environment(db, config, home),
        )

    def create_project(
        self,
        name: str,
        *,
        cli_binary: str | os.PathLike[str] | None = None,
    ) -> None:
        """Seed a project through the real CLI so the TUI sees real data."""
        binary = Path(
            cli_binary
            or os.environ.get("ORBIT_CLI_BIN")
            or DEFAULT_CLI_BINARY
        ).expanduser()
        if not binary.is_file():
            raise FileNotFoundError(
                f"orbit CLI not found at {binary}; needed to seed a project. "
                "Build it with `cargo build -p orbit-cli` or set ORBIT_CLI_BIN."
            )
        completed = subprocess.run(
            [str(binary), "--db", str(self.db), "--json", "project", "add", name],
            env=dict(self.env),
            cwd=str(REPO_ROOT),
            text=True,
            capture_output=True,
            timeout=60,
        )
        if completed.returncode != 0:
            raise RuntimeError(
                "failed to seed Orbit project via CLI\n"
                f"command: {binary} --db {self.db} --json project add {name!r}\n"
                f"stdout: {completed.stdout}\nstderr: {completed.stderr}"
            )


def _run_id() -> str:
    explicit = os.environ.get("ORBIT_TUI_RUN_ID")
    if explicit:
        return _slug(explicit)
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    return f"{stamp}-{os.getpid()}"


_RUN_ID = _run_id()


class TuiSession:
    """A live ``orbit-tui`` process attached to a sized PTY master."""

    def __init__(
        self,
        workspace: Workspace,
        *,
        cols: int = 100,
        rows: int = 30,
        binary: str | os.PathLike[str] | None = None,
        evidence_dir: str | os.PathLike[str] | None = None,
        label: str = "session",
        launch_timeout: float = 20.0,
        wait_for_launch: bool = True,
    ) -> None:
        if cols <= 0 or rows <= 0:
            raise ValueError("cols and rows must be positive")
        self.workspace = workspace
        self.cols = cols
        self.rows = rows
        self.binary = _resolve_binary(binary)
        self.label = _slug(label)
        self.raw = bytearray()
        self._actions: list[dict[str, object]] = []
        self._notes: list[dict[str, object]] = []
        self._master: int | None = None
        self._pid: int | None = None
        self._returncode: int | None = None
        self._exit_signal: int | None = None
        self._closed = False
        self._screen = _TerminalScreen(cols, rows)
        self._screen_offset = 0
        self._started_at = time.monotonic()
        self._last_output_at = self._started_at
        base = Path(
            evidence_dir
            or os.environ.get("ORBIT_TUI_EVIDENCE_DIR")
            or DEFAULT_EVIDENCE_ROOT
        )
        self.evidence_dir = base / _RUN_ID / f"{self.label}"
        self._spawn()
        if wait_for_launch:
            self.wait_for_text(
                "ORBIT", timeout=launch_timeout, description="first Orbit frame"
            )

    # ------------------------------------------------------------------
    # Process lifecycle
    # ------------------------------------------------------------------
    def _spawn(self) -> None:
        master, slave = os.openpty()
        try:
            fcntl.ioctl(
                slave,
                termios.TIOCSWINSZ,
                struct.pack("HHHH", self.rows, self.cols, 0, 0),
            )
        except OSError:
            os.close(master)
            os.close(slave)
            raise

        pid = os.fork()
        if pid == 0:  # pragma: no cover - child branch
            try:
                os.setsid()
                try:
                    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
                except OSError:
                    pass
                os.dup2(slave, 0)
                os.dup2(slave, 1)
                os.dup2(slave, 2)
                if slave > 2:
                    os.close(slave)
                os.close(master)
                os.execvpe(str(self.binary), [str(self.binary)], dict(self.workspace.env))
            except BaseException as error:  # keep the parent's evidence useful
                try:
                    os.write(2, f"orbit-tui harness exec failure: {error!r}\n".encode())
                except OSError:
                    pass
            finally:
                os._exit(127)

        os.close(slave)
        self._master = master
        self._pid = pid
        self.record("spawn", {"binary": str(self.binary), "cols": self.cols, "rows": self.rows})

    @property
    def pid(self) -> int:
        if self._pid is None:
            raise RuntimeError("session was not started")
        return self._pid

    @property
    def returncode(self) -> int | None:
        return self._returncode

    @property
    def exit_signal(self) -> int | None:
        return self._exit_signal

    @property
    def alive(self) -> bool:
        if self._returncode is not None:
            return False
        return not self._reap_nonblocking()

    def _reap_nonblocking(self) -> bool:
        if self._returncode is not None:
            return True
        try:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
        except ChildProcessError:
            self._returncode = self._returncode if self._returncode is not None else -1
            return True
        if pid == 0:
            return False
        code = os.waitstatus_to_exitcode(status)
        self._returncode = code
        if code < 0:
            self._exit_signal = -code
        self.record("exit", {"code": code, "signal": self._exit_signal})
        return True

    def _read_once(self, timeout: float) -> bool:
        if self._master is None:
            return False
        try:
            ready, _, _ = select.select([self._master], [], [], max(0.0, timeout))
        except (OSError, ValueError):
            return False
        if not ready:
            return False
        try:
            chunk = os.read(self._master, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                self._reap_nonblocking()
                return False
            raise
        if not chunk:
            self._reap_nonblocking()
            return False
        self.raw.extend(chunk)
        self._last_output_at = time.monotonic()
        return True

    def _drain_quiet(self, quiet_for: float = 0.15, max_time: float = 1.0) -> None:
        deadline = time.monotonic() + max_time
        while time.monotonic() < deadline:
            quiet = min(quiet_for, max(0.0, deadline - time.monotonic()))
            if not self._read_once(quiet):
                break

    def _terminate(self, timeout: float = 2.0) -> None:
        if self._returncode is not None:
            return
        try:
            os.killpg(self.pid, signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            try:
                os.kill(self.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self._reap_nonblocking():
                self._drain_quiet(0.05, 0.2)
                return
            self._read_once(0.05)
        try:
            os.killpg(self.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            try:
                os.kill(self.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            os.waitpid(self.pid, 0)
        except ChildProcessError:
            pass
        self._reap_nonblocking()
        self._drain_quiet(0.05, 0.2)

    def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        if self._returncode is None:
            self._terminate()
        self._drain_quiet(0.05, 0.2)
        self._write_evidence()
        if self._master is not None:
            try:
                os.close(self._master)
            except OSError:
                pass
            self._master = None

    def __enter__(self) -> "TuiSession":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    # ------------------------------------------------------------------
    # Output views and evidence
    # ------------------------------------------------------------------
    @property
    def text(self) -> str:
        return strip_ansi(bytes(self.raw))

    def text_since(self, mark: int) -> str:
        return strip_ansi(bytes(self.raw[mark:]))

    @property
    def screen_lines(self) -> list[str]:
        """Current visible rows reconstructed incrementally from the ANSI stream."""
        if self._screen_offset < len(self.raw):
            self._screen.feed(bytes(self.raw[self._screen_offset :]))
            self._screen_offset = len(self.raw)
        return self._screen.lines()

    @property
    def screen_text(self) -> str:
        return "\n".join(self.screen_lines)

    def mark(self) -> int:
        return len(self.raw)

    def record(self, action: str, detail: object | None = None) -> None:
        self._actions.append(
            {
                "at": round(time.monotonic() - self._started_at, 3),
                "action": action,
                "detail": detail,
            }
        )

    def note(self, note: str, detail: object | None = None) -> None:
        self._notes.append(
            {
                "at": round(time.monotonic() - self._started_at, 3),
                "note": note,
                "detail": detail,
            }
        )

    def _write_evidence(self) -> None:
        try:
            self.evidence_dir.mkdir(parents=True, exist_ok=True)
            (self.evidence_dir / "raw.ansi").write_bytes(bytes(self.raw))
            (self.evidence_dir / "text.txt").write_text(self.text, encoding="utf-8")
            meta = {
                "label": self.label,
                "binary": str(self.binary),
                "argv": [str(self.binary)],
                "cols": self.cols,
                "rows": self.rows,
                "database": str(self.workspace.db),
                "config": str(self.workspace.config),
                "home": str(self.workspace.home),
                "started_at": time.strftime(
                    "%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - (time.monotonic() - self._started_at))
                ),
                "elapsed_seconds": round(time.monotonic() - self._started_at, 3),
                "last_output_seconds_ago": round(
                    time.monotonic() - self._last_output_at, 3
                ),
                "returncode": self._returncode,
                "exit_signal": self._exit_signal,
                "raw_bytes": len(self.raw),
                "text_chars": len(self.text),
                "alternate_screen_entered": self._alternate_screen_entered(),
                "alternate_screen_left": self._alternate_screen_left(),
                "actions": self._actions,
                "notes": self._notes,
            }
            (self.evidence_dir / "meta.json").write_text(
                json.dumps(meta, indent=2, ensure_ascii=False), encoding="utf-8"
            )
        except OSError as error:  # evidence must never mask the real failure
            print(f"orbit-tui harness: could not write evidence: {error}", flush=True)

    @property
    def alternate_screen_entered(self) -> bool:
        raw = bytes(self.raw)
        return any(
            sequence in raw
            for sequence in (b"\x1b[?1049h", b"\x1b[?1047h", b"\x1b[?47h")
        )

    @property
    def alternate_screen_left(self) -> bool:
        raw = bytes(self.raw)
        return any(
            sequence in raw
            for sequence in (b"\x1b[?1049l", b"\x1b[?1047l", b"\x1b[?47l")
        )

    # Backwards-compatible private aliases used internally.
    def _alternate_screen_entered(self) -> bool:
        return self.alternate_screen_entered

    def _alternate_screen_left(self) -> bool:
        return self.alternate_screen_left

    def _failure_tail(self, limit: int = 3000) -> str:
        tail = self.text[-limit:]
        return tail if tail else "<no output captured>"

    # ------------------------------------------------------------------
    # Input
    # ------------------------------------------------------------------
    def send(self, data: bytes | str, *, note: str | None = None) -> None:
        if isinstance(data, str):
            data = data.encode()
        if self._master is None:
            raise RuntimeError("session is closed")
        try:
            os.write(self._master, data)
        except OSError as error:
            raise AssertionError(
                f"could not send {data!r} to orbit-tui ({error}); "
                f"process returncode={self._returncode}; evidence={self.evidence_dir}"
            ) from error
        self.record(note or "send", {"bytes": repr(data)})

    def send_keys(self, *keys: bytes | str, delay: float = 0.12) -> None:
        for key in keys:
            self.send(key)
            time.sleep(delay)

    def send_text(self, text: str, *, delay: float = 0.03) -> None:
        for character in text:
            self.send(character)
            time.sleep(delay)

    # ------------------------------------------------------------------
    # Waiting and assertions
    # ------------------------------------------------------------------
    def _wait_until(
        self,
        predicate: Callable[[str], bool],
        *,
        timeout: float,
        description: str,
        since: int = 0,
    ) -> str:
        deadline = time.monotonic() + timeout
        while True:
            view = self.text_since(since)
            if predicate(view):
                return view
            if self._reap_nonblocking():
                self._drain_quiet(0.2, 1.0)
                view = self.text_since(since)
                if predicate(view):
                    return view
                raise AssertionError(
                    f"orbit-tui exited (returncode={self._returncode}) before "
                    f"{description}.\nLast output:\n{self._failure_tail()}\n"
                    f"Evidence: {self.evidence_dir}"
                )
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            self._read_once(min(0.1, remaining))
        raise AssertionError(
            f"timed out after {timeout:.1f}s waiting for {description} "
            f"({self.cols}x{self.rows}).\nLast output:\n{self._failure_tail()}\n"
            f"Evidence: {self.evidence_dir}"
        )

    def wait_for_text(
        self,
        needle: str,
        *,
        timeout: float = 5.0,
        since: int = 0,
        description: str | None = None,
    ) -> str:
        return self._wait_until(
            lambda view: needle in view,
            timeout=timeout,
            description=description or f"text {needle!r}",
            since=since,
        )

    def wait_for_regex(
        self,
        pattern: str | re.Pattern[str],
        *,
        timeout: float = 5.0,
        since: int = 0,
        description: str | None = None,
    ) -> str:
        regex = re.compile(pattern) if isinstance(pattern, str) else pattern
        return self._wait_until(
            lambda view: regex.search(view) is not None,
            timeout=timeout,
            description=description or f"pattern {regex.pattern!r}",
            since=since,
        )

    def wait_until(
        self,
        predicate: Callable[[str], bool],
        *,
        timeout: float = 5.0,
        description: str = "condition",
        since: int = 0,
    ) -> str:
        return self._wait_until(
            predicate, timeout=timeout, description=description, since=since
        )

    def _wait_for_screen(
        self,
        predicate: Callable[[str], bool],
        *,
        timeout: float,
        description: str,
    ) -> str:
        deadline = time.monotonic() + timeout
        while True:
            view = self.screen_text
            if predicate(view):
                return view
            if self._reap_nonblocking():
                self._drain_quiet(0.2, 1.0)
                view = self.screen_text
                if predicate(view):
                    return view
                raise AssertionError(
                    f"orbit-tui exited (returncode={self._returncode}) before "
                    f"{description}.\nCurrent screen:\n{view}\n"
                    f"Evidence: {self.evidence_dir}"
                )
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            self._read_once(min(0.1, remaining))
        raise AssertionError(
            f"timed out after {timeout:.1f}s waiting for {description} "
            f"({self.cols}x{self.rows}).\nCurrent screen:\n{self.screen_text}\n"
            f"Evidence: {self.evidence_dir}"
        )

    def wait_for_screen_text(
        self,
        needle: str,
        *,
        timeout: float = 5.0,
        description: str | None = None,
    ) -> str:
        return self._wait_for_screen(
            lambda view: needle in view,
            timeout=timeout,
            description=description or f"screen text {needle!r}",
        )

    def wait_for_screen_regex(
        self,
        pattern: str | re.Pattern[str],
        *,
        timeout: float = 5.0,
        description: str | None = None,
    ) -> str:
        regex = re.compile(pattern) if isinstance(pattern, str) else pattern
        return self._wait_for_screen(
            lambda view: regex.search(view) is not None,
            timeout=timeout,
            description=description or f"screen pattern {regex.pattern!r}",
        )

    def wait_until_screen(
        self,
        predicate: Callable[[str], bool],
        *,
        timeout: float = 5.0,
        description: str = "screen condition",
    ) -> str:
        return self._wait_for_screen(
            predicate, timeout=timeout, description=description
        )

    def expect_exit(self, timeout: float = 8.0) -> int:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self._reap_nonblocking():
                self._drain_quiet(0.25, 2.0)
                assert self._returncode is not None
                return self._returncode
            self._read_once(min(0.05, max(0.0, deadline - time.monotonic())))
        self._terminate()
        raise AssertionError(
            f"orbit-tui did not exit within {timeout:.1f}s "
            f"({self.cols}x{self.rows}); it was terminated.\n"
            f"Last output:\n{self._failure_tail()}\nEvidence: {self.evidence_dir}"
        )

    def _looks_like_quit_confirmation(self, view: str) -> bool:
        """Require Quit + unsaved wording plus y/n affordances.

        This deliberately rejects a footer ``q quit`` hint and a typed draft
        containing the word "unsaved" on their own.  The target contract only
        guarantees the stable substrings "Quit" and "unsaved", not exact copy.
        """
        lower = view.lower()
        if "quit" not in lower or "unsaved" not in lower:
            return False
        return bool(
            _STANDALONE_YES_RE.search(view) and _STANDALONE_NO_RE.search(view)
        )

    def quit_via(self, key: bytes | str, *, timeout: float = 8.0) -> int:
        """Send a quit key; confirm an unsaved-input prompt with ``y`` if shown."""
        mark = self.mark()
        self.send(key, note="quit request")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self._reap_nonblocking():
                self._drain_quiet(0.2, 1.5)
                assert self._returncode is not None
                return self._returncode
            view = self.text_since(mark)
            if self._looks_like_quit_confirmation(view) or self._looks_like_quit_confirmation(
                self.screen_text
            ):
                self.note("quit confirmation shown", view[-500:] or self.screen_text[-500:])
                self.send(b"y", note="confirm quit")
                return self.expect_exit(timeout)
            self._read_once(min(0.1, max(0.0, deadline - time.monotonic())))
        self._terminate()
        raise AssertionError(
            f"quit key {key!r} did not exit orbit-tui within {timeout:.1f}s "
            f"({self.cols}x{self.rows}) and no clear confirmation prompt appeared.\n"
            f"Last output:\n{self._failure_tail()}\nEvidence: {self.evidence_dir}"
        )

    def wait_for_quit_response(self, *, timeout: float = 3.0, since: int = 0) -> str:
        """Return ``"exited"`` or ``"confirmation"`` after a quit request.

        Raises when the process stays alive without showing a clear prompt.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            view = self.text_since(since)
            if self._looks_like_quit_confirmation(view) or self._looks_like_quit_confirmation(
                self.screen_text
            ):
                return "confirmation"
            if self._reap_nonblocking():
                self._drain_quiet(0.2, 1.0)
                return "exited"
            self._read_once(min(0.1, max(0.0, deadline - time.monotonic())))
        raise AssertionError(
            f"orbit-tui stayed alive for {timeout:.1f}s after a quit request without "
            "showing a clear unsaved-input confirmation.\n"
            f"Last output:\n{self._failure_tail()}\nEvidence: {self.evidence_dir}"
        )

    def assert_clean_exit(self, timeout: float = 8.0) -> int:
        code = self.expect_exit(timeout)
        text = self.text
        assert code == 0, (
            f"orbit-tui exited with status {code} "
            f"(signal={self._exit_signal}).\nLast output:\n{self._failure_tail()}\n"
            f"Evidence: {self.evidence_dir}"
        )
        assert "panicked at" not in text, (
            f"orbit-tui panicked:\n{self._failure_tail()}\nEvidence: {self.evidence_dir}"
        )
        assert self._alternate_screen_entered(), (
            "orbit-tui never entered the alternate screen; "
            f"evidence={self.evidence_dir}"
        )
        assert self._alternate_screen_left(), (
            "orbit-tui exited without leaving the alternate screen "
            "(\x1b[?1049l / \x1b[?47l not found); "
            f"evidence={self.evidence_dir}"
        )
        raw = bytes(self.raw)
        leave_at = raw.rfind(b"\x1b[?1049l")
        cursor_at = raw.rfind(b"\x1b[?25h")
        assert cursor_at > leave_at >= 0, (
            "orbit-tui did not leave the alternate screen before restoring the cursor; "
            f"evidence={self.evidence_dir}"
        )
        return code
