"""Settled-screen waits shared by the PTY acceptance tests.

The harness reconstructs the screen from a live terminal emulator, so a single
sample can land in the middle of a frame write: part of the previous frame has
been erased and part of the next frame is not painted yet.  A predicate that
looks for a modal title's *absence* -- or that matches content which was
already on screen underneath the modal -- can pass on such a torn frame while
the application still owns the next keystroke.

``wait_until_settled`` requires the predicate to hold on two consecutive
samples separated by a harness read cycle.  A frame that is still being
written changes on that next sample, so it cannot satisfy the wait.
"""
from __future__ import annotations

from typing import Callable

from pty_harness import TuiSession


def wait_until_settled(
    session: TuiSession,
    predicate: Callable[[str], bool],
    *,
    timeout: float = 5.0,
    description: str = "screen condition",
) -> str:
    """Wait for ``predicate`` to hold on a screen that has stopped changing."""
    seen: str | None = None

    def settled(view: str) -> bool:
        nonlocal seen
        if not predicate(view):
            seen = None
            return False
        stable = seen is not None and seen == view
        seen = view
        return stable

    return session.wait_until_screen(settled, timeout=timeout, description=description)
