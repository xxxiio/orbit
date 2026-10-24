# M29 terminal captures

Real `target/debug/orbit-tui` sessions driven through
`tests/acceptance/tui/pty_harness.py`. Each `.txt` file is the *reconstructed screen*
at the moment named below — not a stream log, not a hand-written mock. Trailing
padding and trailing blank lines are removed; everything else is exactly what the
terminal showed.

## Reproduce

```sh
cargo build -p orbit-tui -p orbit-cli -p orbit-server
python3 scripts/tui-captures.py
```

The script seeds its own throw-away workspace with the real CLI (three projects,
eleven tasks spanning the Basic workflow statuses, estimates and deadlines), writes
PTY evidence under a temporary directory, and replaces this directory's `.txt` files.
`--only <surface>` limits a run; `--list` prints the inventory.

Connected captures each start a disposable `orbit-server` on a free loopback port,
enrol a throw-away device and queue one real intention, so every connected screen shows
exactly the work that session queued. They are skipped with a printed reason
when `orbit-server` cannot be built or started.

## Inventory

| File | Size | Surface | Shows | Reproduce |
|---|---|---|---|---|
| `60x18-home.txt` | 60x18 | home | Compact shell: only the workspace is visible, and the pane bar above it names both panes plus `Tab switch · D details`. The Home grid still fits at this height, so every component is painted and long lines truncate inside their panel. | `launch at 60x18` |
| `100x30-home.txt` | 100x30 | home | Sidebar + workspace (details dock only from 120 columns), the Today/Next planning component and the compact action bar. | `launch at 100x30` |
| `160x45-home.txt` | 160x45 | home | Full three-region shell: the sidebar sizes to its labels, the workspace keeps every Home component and the detail pane is docked on the right. | `launch at 160x45` |
| `60x18-projects.txt` | 60x18 | projects | Projects list at the smallest supported size, with the seeded project and its status glyph. | `g p` |
| `100x30-projects.txt` | 100x30 | projects | Projects list with keyboard selection, the project status glyph (`○` proposed, `●` active) and the contextual action bar. | `g p` |
| `160x45-projects.txt` | 160x45 | projects | Projects list with the docked detail pane for the selected project. | `g p` |
| `60x18-tasks.txt` | 60x18 | tasks | The Tasks table in compact mode: one pane, the title column plus the status and priority columns that fit. The deadline column is dropped rather than squashed; open the row's detail for every field. | `g p, Enter, ], j j` |
| `100x30-tasks.txt` | 100x30 | tasks | The Tasks table with a selected row (`▸` rail and filled row), the status glyph and priority; details are not docked at this width, so Enter/D opens them. | `g p, Enter, ], j j` |
| `160x45-tasks.txt` | 160x45 | tasks | The Tasks table at full width: every column gets room and the detail pane stays docked. | `g p, Enter, ], j j` |
| `60x18-board.txt` | 60x18 | board | The Kanban in compact mode: two readable columns of ten (`columns 2-3 of 10`), the column window centred on the selected card, and the `j/k card · h/l panel · S status` hints. | `g p, Enter, ] ]` |
| `100x30-board.txt` | 100x30 | board | The Kanban at a medium width: three readable columns of ten (`columns 2-4 of 10`), the window following the selected card and the quick-edit/status hints. | `g p, Enter, ] ]` |
| `160x45-board.txt` | 160x45 | board | The wide Kanban with the details pane docked: five readable columns of ten (`columns 1-5 of 10`), column headers with counts, the selected-card marker and per-column status glyphs. | `g p, Enter, ] ]` |
| `100x30-task-detail.txt` | 100x30 | task-detail | The task detail overlay (`… · detail`): status, readiness, estimate, due date, deadline, attachments and every action the selection authorizes. | `g p, Enter, ], j j, Enter` |
| `160x45-task-detail.txt` | 160x45 | task-detail | The same task detail overlay on a large terminal, where the property list and the full action list fit without scrolling. | `g p, Enter, ], j j, Enter` |
| `100x30-action-menu.txt` | 100x30 | action-menu | The grouped action menu (`Actions · …`): Suggested operations first, then More actions; only keys that identify a row uniquely are advertised, the rest stay keyless for Enter. | `g p, Enter, ], j j, a` |
| `160x45-action-menu.txt` | 160x45 | action-menu | The action menu on a large terminal: the same groups and actions with every label on one line (a disabled row would print its reason here instead of being hidden). | `g p, Enter, ], j j, a` |
| `100x30-palette-query.txt` | 100x30 | palette-query | The command palette with the query `tas`: ranked matches (`prefix · word · fuzzy · substring`) with the `Ctrl+U clear` hint; disabled entries stay listed with a reason. | `launch, :, type tas` |
| `100x30-quick-edit.txt` | 100x30 | quick-edit | The `E` quick-edit form: only Title and Priority, with `Save Ctrl+S · Cancel Esc` in the title bar and the field-level footer; status changes stay in the `S` menu. | `g p, Enter, ], j j, E` |
| `60x18-pending-inspector.txt` | 60x18 | pending-inspector | Connected pending inspector at 60x18 (`Pending commands · 1 queued (not accepted)`): the durable row is labelled `[pending]` and its detail is clipped to the modal width, never promoted to accepted data. | `connected launch, g p, c, type Pending Scope, Enter, !` |
| `100x30-pending-inspector.txt` | 100x30 | pending-inspector | Connected pending inspector with the header's connected summary visible; the detail panel shows the canonical payload and recorded state, and the footer lists `f flush · c review conflict · Enter detail · r refresh · Esc close`. | `connected launch, g p, c, type Pending Scope, Enter, !` |
| `100x30-connected-console.txt` | 100x30 | connected-console | The connected command console (`queued intents are PENDING`) after a read-only `sync.status`: read-only output appears inline, nothing is queued. | `connected launch, :, Connected commands, sync.status` |

## Reading a capture

- The header row is `ORBIT › <screen>`. A project screen adds its pinned workflow
  (`Basic v1`). A short summary (counts, or `CONNECTED … N pending`) follows when the row
  can fit it and is dropped only when it does not; the 60x18 Home capture still shows
  `3 projects · 1 inbox` because that text fits.
- The focused pane has a thick border and an accent title. The selected row has a `▸` rail
  and a filled style; the status glyph (`✓ ! ◐ ▶ ○ ~`) repeats the state without colour.
- In the sidebar `»` marks the current destination and `●` is the project status `active`;
  the selected row still uses the `▸` rail.
- Below 76 columns only one pane is visible; the row above it reads
  `▸ Workspace │ Sidebar │ Tab switch · D details`.
- From 76 columns the sidebar is shown. Details dock from 120 columns only while the active
  component still gets its readable minimum width; if docking would squeeze it the pane is
  refused (unless the detail pane owns focus), and `D`/`Enter` opens the detail overlay.
- The action bar is the second-to-last row; the status line is the last row.
- Connected captures show `CONNECTED … N pending · ! inspect` when the header can fit it;
  the 60x18 connected capture is the narrow case where it is dropped. Every queued row is
  labelled `[pending]` or `[conflict]`; none of it is accepted data.
- Dates and command ids are generated at run time; project names, task titles, statuses and
  the capture sequence are fixed by the script.
