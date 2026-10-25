# M30 I1 — terminal capture evidence

Generated from the real `target/debug/orbit-tui` binary via the existing capture
driver:

```sh
cargo build -p orbit-tui -p orbit-cli --offline
python3 scripts/tui-captures.py
```

Result: `captures: 24 written, 0 skipped` (the run additionally rewrites the
capture `README.md` index, 25 files total under `.agent/evidence/m29-captures/`).

## Captures added for M30 I1

| Capture | Size | Shows |
|---|---|---|
| `60x18-inspector.txt` | 60x18 | Whole-frame inspector fallback (inspector covers the shell) |
| `100x30-inspector.txt` | 100x30 | Inset inspector overlay with `Esc close · j/k scroll · Enter close` |
| `160x45-inspector.txt` | 160x45 | Inset inspector at the wide breakpoint; workspace keeps its width |
| `100x30-capability-action-menu.txt` | 100x30 | Capability action row with its availability reason (`not_authorized: …`) |
| `120x35-capability-inspector.txt` | 120x35 | `CAPABILITY ACTIONS`, `RECENT CAPABILITY ACTIONS` and a durable `succeeded` receipt |

## Captures removed (surface no longer exists)

- `100x30-task-detail.txt`
- `160x45-task-detail.txt`

## Evidence that the layout is two-region

`160x45-home.txt` — the workspace panel runs to the last column; nothing is
painted to its right:

```
 ORBIT › Home / Today   3 projects · 1 inbox
╭ Orbit ────────────╮┏ Today / Next • 12h • [/] horizon ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓╭ Today / Next tasks ───────────────────────────────────────────────────╮
│▸ » Home / Today   │┃Today / Next                                                    ┃│  ○ Wire the task table  planned                                       │
```

`60x18-inspector.txt` — the inspector takes the whole frame below 80x20:

```
╔ Inspector · Map the command palette ═════════════════════╗
```

`120x35-capability-inspector.txt` — capability surfaces inside the inspector:

```
│           ║CAPABILITY ACTIONS 1 · press a to run                                                         ║           ┃
│           ║RECENT CAPABILITY ACTIONS 1 · durable state                                                   ║           ┃
│           ║succeeded append_note · orbit.reference.work-item-note · command                              ║           ┃
```

## Reproduce

The capture driver seeds an isolated workspace through the real CLI (projects,
tasks, reference capability install → manifest publish → bind → item creation →
authorize → execute) before driving the TUI in a sized PTY. No user data is
touched.
