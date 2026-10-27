# TUI redesign specification

This page is the design input for a future `orbit-tui` redesign. It is derived from proven
use of the web client, not from taste: every pattern below names the evidence that it mattered
to a real person doing real work, and the terminal translation that preserves the reason it
worked. It covers the M31 I5 dogfooding cycle plus everything M32 I1–I4 shipped — unified
search, saved views, project identity editing, task timestamps, knowledge folders and
references, agent launch, and connected Workflow Capability execution.

**This is a specification only. M32 implements no TUI change.** Nothing on this page is a
change to `crates/orbit-tui`, and no pattern here has been built or tested in a terminal.
Scope: `docs/**` only. A redesign that moves a domain rule into the terminal is out of scope,
whatever it buys.

The two clients are strong in different places. The web app has the more mature *connected*
contracts — a domain-backed edge read, a version-aware install, a reviewed upgrade, a
fingerprinted planning receipt, a server-wide search, an executor-backed capability lifecycle.
The TUI has the more mature *local* contracts — direct authority, capability execution, a deep
keyboard model, session restore. The redesign should close the gap through shared commands and
reads, not by teaching the terminal to imitate a browser.

**Contents:** [Method](#method) · [Pattern inventory](#pattern-inventory) ·
[Patterns](#patterns) · [Non-goals](#non-goals) ·
[What the web app still owes the TUI](#what-the-web-app-still-owes-the-tui) ·
[Acceptance](#acceptance-for-a-future-redesign) · [Exclusions](#what-this-spec-does-not-cover) ·
[Evidence index](#evidence-index)

## Method

### Observation source

| Fact | Value |
| --- | --- |
| M32 acceptance record | [I1](../../.agent/evidence/m32-i1-acceptance.md) (search, saved views, project identity, task metadata), [I2](../../.agent/evidence/m32-i2-acceptance.md) (knowledge folders, links, references), [I3](../../.agent/evidence/m32-i3-acceptance.md) (agent launch, claim, review, resume), [I4](../../.agent/evidence/m32-i4-acceptance.md) (connected Workflow Capability execution) |
| Application commits | `1c27354` (I1), `b571d97` (I2), `0cbb79b` (I3), `2212ca3` (I4); this page was written on `2212ca3` |
| Browser acceptance | 136 passed, 4 skipped (opt-in captures), 0 failed at I4 (`m32-i4-acceptance.md`) |
| Documented behaviour | [`docs/web-app.md`](../web-app.md), which describes the build shipped through I4 |
| M31 dogfooding record | [`web/evidence/i5/run.json`](../../web/evidence/i5/run.json) and its generated [`README.md`](../../web/evidence/i5/README.md): commit `987bcf15…`, built SPA `e34b3a77…` (16 files), server binary `4ac3f391…`, Workflow Development v3, 19 tasks, capture window 2026-10-26T04:03:34.206Z → 2026-10-26T04:04:48.902Z, Chromium 1440×900 light |
| Not available | No M32 `run.json`. The M32 cycle records commands, assertions and defects in the four acceptance files; the M31 cycle records one machine-readable run. Where a claim rests on the M31 run it says `run.json → …`; where it rests on M32 it names the evidence file and the spec. |

The M31 evidence set names one commit, one `dist`, one server binary and one workflow version;
its fix pass (`972a09d`, `3f4101f`, `0ff8af9`, `1ae1600`, `987bcf1`, `dfcfb8a`) is part of the
same record, so a finding the first run hit and a later commit closed is described as a lesson
about the surface, not as a current defect. The M32 evidence files were produced on the
reconciled tree after each integration lane landed, and they name the defects found and fixed
inside the milestone.

### How patterns were chosen

A pattern entered this page only if at least one of these is true:

- **U — used in a cycle.** The M31 run changed state through it (a row in `moves`, `triage`,
  `planning`, `conflict`, `boardMove` or `restart`), or the M32 acceptance descriptions record
  the interaction as performed end to end.
- **A — asserted by an automated check.** A Playwright spec runs it against a real server and
  the built client: `search.spec.ts`, `saved-views.spec.ts`, `project-settings.spec.ts`,
  `task-metadata.spec.ts`, `knowledge-folders.spec.ts`, `knowledge-links.spec.ts`,
  `project-knowledge.spec.ts`, `agents-launch.spec.ts`, `agents.spec.ts`,
  `capability-execution.spec.ts`, plus the M31 set (`quality-keyboard.spec.ts`,
  `quality-a11y.spec.ts`, `work-items.spec.ts`, `planning-estimates.spec.ts`,
  `planning-guard.spec.ts`, `upgrades.spec.ts`, `workflow-install.spec.ts`, `conflict.spec.ts`,
  `tasks.spec.ts`, `workspaces-registry.spec.ts`).
- **F — recorded as friction.** The M31 issue log names it with an id (`I5-01` … `I5-17`), or the
  M32 evidence records a defect and its fix.

A pattern with only **A** is evidence that a surface is exercised, not proof that it felt right;
those rows are marked conservatively. A pattern with only **F** is still evidence: it is a place
where a design sent a person looking for a control that did not exist.

### Classification scale

| Class | Meaning | What the redesign does |
| --- | --- | --- |
| **1 — Must match** | The reason the pattern worked is the information and the rules it exposes, not the browser's pixels. Removing it would remove a capability. | Reproduce the same facts, outcomes and refusals in terminal form. |
| **2 — Adapt** | The invariant survives, the interaction does not. A browser mechanism (URL, focus ring, live region) has a different terminal equivalent. | Keep the invariant; replace the mechanism; document the equivalent. |
| **3 — Not appropriate** | The pattern depends on a browser capability a terminal cannot honestly provide, or it would add a second renderer for the same content, or it would put the terminal on the wrong side of an executor boundary. | Reject it, and keep whatever fact it was protecting visible another way. |
| **4 — Requires shared domain/API changes** | The terminal cannot match the pattern without a change below the UI: a missing read, command or migration rule. Implementing it locally would create a second rule, a second store, or a second writer of a shared vocabulary. | File the shared change; do not compensate in the TUI. |

**Counts across the eighteen patterns:** primary class 1 — 6; class 2 — 3; class 3 — 1;
class 4 — 8.

Four rows (7, 9, 14, 17) are **bimodal** and are counted once, under 4: part of the pattern is
already served by a shared command or read and stays class 1, and the row is class 4 because of
one named missing API. Those rows say which facet is which. One pattern (11) is class 4 with no
satisfied facet: a terminal upgrade path genuinely does not exist below the UI yet.

**One classification changed since the M31 page, and one was added.** Pattern 7 (search) was
class 1 when the web had no global search and the TUI's local filter was already the stronger
surface. The web now performs one server query over projects, tasks and knowledge
(`m32-i1-acceptance.md`), and the TUI's `/` still filters only what it has already loaded
(`app/actions.rs:2333-2404`), so the pattern is class 4 until the shared search read exists:
calling the loaded-row filter "search" would advertise something the other client does not
mean by the word. Pattern 9 (agent review) was class 1 and forbade a launch control, because
the browser had none. The browser now queues launches, so the pattern is restated as *agent
jobs* — launch, claim, review, resume — and its launch/resume facet is class 4 because
`orbit-client` has no agent job command. Its honesty invariants stay class 1.

### What the evidence can and cannot prove

- **M32 changed no TUI surface.** The milestone's scope is the web client, the authority and
  the executor; `crates/orbit-tui` is untouched between `1c27354`…`2212ca3`. Every statement
  here about the terminal's current behaviour is read from `crates/orbit-tui/src/**` or
  [`tui-workspace.md`](tui-workspace.md) and is a claim about the documented design, not a new
  TUI test result.
- **Agent review and launch were exercised for the first time.** M31 could not create a job
  without a runtime (`I5-16`); M32 I3 drove a real executor through claim, confirm, handoff,
  review and a real server restart (`m32-i3-acceptance.md`). The M31 claim that approval never
  completes a task is now an observed fact, not an inference from `agents.spec.ts` alone.
- **Capability execution was exercised for the first time.** M32 I4 drove the real
  `orbit capability serve --once` process against a real server over five scenarios
  (`m32-i4-acceptance.md`). The "no executor has announced it" branch is asserted; the
  "capability not installed" branch is **not**, because the product UI has no uninstall route
  and publish-before-install is refused earlier. Say so rather than implying coverage.
- **No upgrade was exercised.** M31 had current = newest = 3 (`run.json` → `upgrade`), and M32
  did not change the upgrade path. The upgrade pattern still rests on `upgrades.spec.ts` and
  the v3 migration notes, not on a dogfooding run.
- **One viewport, one theme, one locale, one host.** The clipped 12-hour time fields (`I5-13`)
  show why a single desktop capture is not a layout proof. The M32 evidence was produced on
  this Linux host with a WSL-hosted Windows `npm`; the installation record is
  `.slim/deepwork/m32-web-completeness/i6-install.md` in the milestone workspace (not tracked in
  git, so it is named rather than linked).
- **No performance data.** This page makes no claim about TUI latency or large-project
  rendering.

## Pattern inventory

| # | Pattern | Class | Evidence type | Primary evidence |
| --- | --- | --- | --- | --- |
| 1 | Navigation and project switching | 1 | U | `03-project-overview`; `nav.ts`, `sidebar.tsx`, `workspace-layout.tsx` |
| 2 | Quick capture | 1 | A | `11-inbox-before-triage`; `quick-capture.tsx`, `tasks.spec.ts` |
| 3 | Task editing and planning fields | 2 | U, F | `11b-status-edges`, `13-planning-day-plan`; `I5-01`, `I5-09` |
| 4 | Transitions, guards and capability chips | 1 | U, A, F | `run.json` → `invalidTransition`; `project-settings.spec.ts`; `I5-11`, `I5-12` |
| 5 | Workflow view navigation and unavailable views | 1 | U, A | `03-project-overview`; `view-registry.tsx`, `workspaces-registry.spec.ts` |
| 6 | Inspector, focus return and deep links | 2 | U, A, F | `15-`, `17-`; `quality-keyboard.spec.ts`; `I5-05`; I2 task-context fix |
| 7 | Search and command palette | 4 (results) / 1 (commands) | A, U | `search.spec.ts` (5); `m32-i1-acceptance.md`; `command-palette.tsx` |
| 8 | Planning review, approval and receipt | 1 | U, F | `13-`, `14-`, `14b-`; `run.json` → `planning`; `I5-10` |
| 9 | Agent jobs: launch, claim, review, resume | 1 + 4 | A, U | `agents-launch.spec.ts`, `agents.spec.ts` (11); `m32-i3-acceptance.md` |
| 10 | Keyboard accessibility and announcements | 2 | F, A | `quality-keyboard.spec.ts`, `quality-a11y.spec.ts`; `I5-12`, `I5-17` |
| 11 | Workflow upgrade, fingerprint and default backfill | 4 | F, A | `24-settings-no-upgrade`; `upgrades.spec.ts`; `I5-03` |
| 12 | Markdown write/preview split | 3 | U, F | `09-task-with-note-link`; `I5-04` |
| 13 | Saved views | 4 | A | `saved-views.spec.ts` (6); `m32-i1-acceptance.md` |
| 14 | Project identity editing and the immutable id | 1 + 4 | U, A | `project-settings.spec.ts` (6); `project-identity-form.tsx` |
| 15 | Task Created and Updated with `Unknown` | 4 | A | `task-metadata.spec.ts` (2); `task-timestamps.tsx` |
| 16 | Knowledge: folders, location, links and references | 4 | A, U | `knowledge-folders.spec.ts` (8), `knowledge-links.spec.ts` (4), `project-knowledge.spec.ts` (7) |
| 17 | Workflow Capability actions | 4 (connected) / 1 (local) | A, U | `capability-execution.spec.ts` (5); `m32-i4-acceptance.md` |
| 18 | Refusal vocabulary: no executor, not authorized, not installed | 1 | A, F | `capability-model.ts:34-110`; `m32-i4-acceptance.md` refusals; `m32-i3-acceptance.md` no-executor |

## Patterns

### 1. Navigation and project switching — class 1

**Evidence.** Five global destinations are first-class in the web shell — Home, Inbox, Projects,
Knowledge, Agents (`web/src/components/navigation/nav.ts:18-24`) — and the M31 cycle used the
Home → Inbox path to triage an unassigned task. `03-project-overview.png` shows the sidebar's
**Recent** project group, the project switcher beside **New task**, four declared view tabs with
count badges where a count is known, and the two project tools. The shell also registers every
project by name as a command, so a project is reachable from the palette as well as the sidebar
(`workspace-layout.tsx:61-125`). Recent projects are browser-local (`orbit.recent-projects`), not
authority state ([`web-app.md`](../web-app.md), Sign in).

One M32 fact sharpens this pattern: a project's **name** is now editable while its **id** never
changes (pattern 14). A rename reaches the sidebar, the project header, the projects list, recent
destinations and search results without a reload, because every one of them is keyed by id
(`project-identity-form.tsx:141-143`; `m32-i1-acceptance.md`).

**Honest limitation in the web build.** The sidebar accepts a `pinned` flag and renders a
**Pinned** group (`sidebar.tsx:91-93`), but the shipped shell passes only `recent`
(`workspace-layout.tsx:51-59`), so that group is always empty. Do not copy "Pinned" into the TUI
until something actually produces the flag.

**Terminal translation.** Keep the five destinations and make the project's identity part of the
frame: name, status, pinned Workflow version, and a stale marker when the last read failed. The
TUI already breadcrumbs the project and pins `[stale]`; the redesign keeps that, and must re-read
the name after an accepted rename instead of caching it in the frame. Add the web's project
switcher as a named palette entry per project rather than a drop-down: `Open project: <name>` is
searchable, needs no pointer, and works at 80 columns. Recent projects must be session memory
shared by the sidebar and the palette, never a new file: the TUI already remembers recent palette
destinations (up to five), and `tui-state.json` already restores the active project, view and
selected item by identifier, which is stronger than a browser's recent list.

Switching projects should preserve the *view id* when the target workflow declares it, and fall
back to the first declared view with a one-line notice when it does not. That is what the web does
by navigating to the project root and resolving the first view; the terminal can be explicit about
the substitution instead of silently landing somewhere else.

**Why class 1.** Every fact here — which destinations exist, which project is open, what it is
called now, which views it declares, which project was opened last — is navigation state a person
needs. None of it depends on a sidebar.

### 2. Quick capture — class 1

**Evidence.** The Inbox capture row is single-line; `Enter` and the **Capture** button submit the
same form; the title is limited to 280 characters; the text clears only after the server accepts
the task; a rejection keeps the text in place with the server's message underneath; an
unconfirmed write keeps the text and offers a refresh-first path. Both ids are minted at submit
and reused while the same text is retried, so a retry cannot create a second task
(`quick-capture.tsx:14-71`). `c` focuses the capture field when it is visible and otherwise opens
the full **New task** dialog (`app-shell.tsx:80-90`). `11-inbox-before-triage.png` shows the row
above the table with its `Enter` hint; the path is asserted by `tasks.spec.ts` (capture → edit →
status change survives a reload) and by the unit test `quick-capture-idempotency.spec.tsx`.

The M31 cycle did not create its 18 seeded tasks through this row — it used the **New task** dialog
(`run.json` → `steps`) — so this pattern is **A**, not **U**. That is a limitation of that run, not
of the pattern: capture is the fastest path for one thought, and the guide documents it as the
Inbox's primary action.

**Terminal translation.** Put a persistent one-line capture field at the top of the Inbox, not
behind a form:

```text
+ Capture a task for the Inbox…                                  Enter capture
```

`c` focuses it; `Enter` submits; `Ctrl+U` clears; focus stays in the line after an accepted capture
so a second thought can follow. The status line owns the feedback: `Sending…`, then
`Captured “<title>” in the Inbox`, then the new row is selected. A rejection keeps the typed text
and prints the server's sentence directly under the line; an uncertain outcome says so and offers
a same-id retry, never a new task id. Do not add project, priority or date fields here: those
belong to the row after capture, which is exactly how the M31 cycle triaged the captured task
(`run.json` → `triage`).

**Why class 1.** One line, one decision, no navigation. A terminal is better at this than a
browser, and the invariants — accepted-only clearing, same-id retry, in-place refusal — are what
make it safe.

### 3. Task editing and planning fields — class 2

**Evidence.** The first real cycle could not size a task at all: no surface called `setPlanning`,
so Review stayed disabled and no plan could be applied (`I5-01`, blocker). After the fix, the
drawer's **Planning** group authors estimate, earliest start and deadline as **one** `set_planning`
command with the usual revision fence; an empty field clears; a refused command resets each draft
to the stored record; and the group registers with the close guard so `Esc` flushes a draft instead
of discarding it (`planning-section.tsx:1-30,121-143,225-231`; `task-drawer.tsx:110-123`). The same
cycle found copy promising a deadline before any surface offered one (`I5-09`).

`11b-status-edges.png` shows the Planning group inside an open drawer, with the stored-value hint
under it; `13-planning-day-plan.png` shows the sized task selected in the day plan at `1 h`.
`run.json` records the capacity that made the gate pass (`capacityMinutes: 240`) and the applied
block. The M32 task-metadata work did not change this surface, and it added one adjacent fact the
terminal must not confuse with planning: **Created/Updated** are recorded history, not authored
fields (pattern 15).

**Terminal translation.** The TUI already has `e` (full edit, including estimate, earliest start and
deadline) and `E` (title and priority only), and its inspector shows planning properties. Keep that
split. What must change is the commit semantics:

- One save writes the three planning fields as **one** command. A terminal form has no natural
  "leave a field" event, so do not invent per-keystroke writes; `Ctrl+S` from anywhere is the
  commit.
- The displayed value is always the *stored* value once a command settles. A refusal restores the
  stored triple and prints the authority's sentence beside the field; it must not leave the refused
  draft visible as if it were saved.
- Empty means clear. A blank estimate is not "0".
- `Esc` on a dirty form asks before discarding; after a successful save the inspector reopens on
  the same entity with the accepted values (both already true in the TUI).
- Copy lists only fields the form shows. The I5-09 failure was a promise in prose with no control
  behind it.

**Why class 2.** The invariant is field-level truth and one fenced command; the browser's
blur-to-save is the mechanism that does not survive. The TUI's existing "Save Ctrl+S · Cancel Esc"
form model is the correct equivalent.

### 4. Transitions, guards and capability chips — class 1

**Evidence.** Task status controls offer only the edges the authority reports as legal: the web
reads `/v1/authoritative/task-transitions` per visible status, caches it for five minutes, and
falls back to every status with the server still deciding if the read fails
(`web/src/api/queries.ts:425-447`, `task-controls.tsx:15-30`). The server's own comment calls that
route a read of `TaskStatus::can_transition_to`, not a second transition table
(`crates/orbit-server/src/main.rs:2311-2328`). The M31 probe is unambiguous: the control did **not**
offer `inbox → done`, the offered set was Inbox, Planned, Ready, Deferred and Cancelled, and the
deliberate probe got `400 invalid task status transition: inbox -> done`
(`run.json` → `invalidTransition`; `I5-11`). The board's Move menu uses the same edge set; an
accepted move refocuses the card in its new column and a refusal announces the server's sentence
without moving anything (`board-view.tsx:144-186`). M32 I1 re-asserted the same contract for
project status: the Settings control offers exactly `GET /v1/authoritative/project-statuses`
(`m32-i1-acceptance.md`; `project-identity-form.tsx:171-211`).

For Work Items the rule is different and equally explicit: the sheet lists exactly the transitions
the kind declares out of the current state, prints each declared guard as a sentence, marks a
property guard satisfied or not from the item's own values, says that a relationship guard is left
to the server, and never disables the move because of a guard. A required capability is a visible
**Requires capability …** chip with the sentence that browser sessions are granted none
(`work-item-sheet.tsx:252-337,344-352,603-654`).

**Terminal translation.** One edge source, two presentations, one authority check:

```text
MOVES · 2 declared from Backlog
▸ Move to In progress
    guard  estimate is present                ✓ 40 min
    guard  all prerequisites in state         authority decides
    Requires capability review.publish        not granted
  Move to Cancelled
    no guard
```

- The TUI builds its `S` menu from `orbit_client::view_data::allowed_task_statuses`
  (`crates/orbit-tui/src/backend.rs:1369-1371`, consumed at `app/navigation.rs:265`); the redesign
  keeps one derivation for the menu, the board and the inspector, and never a literal list.
- Show a refused edge's sentence on the row and leave the row where it is. The cycle's board
  behaviour is the model: accepted state moves the card, nothing moves locally.
- For Work Items, keep the declared-but-guarded rows visible. The web deliberately does not disable
  them, because the authority re-checks and a property guard the client can see is not the whole
  rule.
- Capability chips **adapt** (class 2 facet): the web can only say "not executable here", while the
  local TUI can execute a capability action itself. Keep the chip and add the TUI's availability
  code and reason, and keep the rule that an unavailable offer stays visible, dimmed, and does not
  run on `Enter` (already true: `app/capability.rs:79-108`). The action itself is pattern 17.

**Why class 1.** "Only legal edges are offered, and the authority's refusal is quoted where the
choice was made" is the capability. It is the difference between discovering a lifecycle by trial
and error and reading it.

### 5. Workflow view navigation and unavailable views — class 1

**Evidence.** The web chooses a renderer from the component *type* a view declares, never from the
Workflow id, so five bundled Workflows share one code path (`view-registry.tsx:1-12,71-96`). A
declared type the build cannot render still has a renderer: an explicit panel that names the
declaration and why it is missing (`HabitTrackerView`, `MetricHistoryView`,
`UnknownComponentView`). A `group_by` on a non-board is refused by name rather than dropped
(`:114-132`). Reserved view ids cannot shadow the **Planning** and **Settings** tools; the workflow
is told its view was not rendered and why (`project-workspace-page.tsx:487-500`; the reserved id
belongs to the tool at `:251`). The header action comes from the open view
(`project-workspace-page.tsx:270,392-410`). `03-project-overview.png` shows the declared
tabs with counts plus the two tool tabs.

**Terminal translation.** Keep view selection by **id** (`1`–`9`, `[`/`]`, and the palette), not by
index, because workflows reorder views and a reordered tab must not move the person. Three rules
carry the web's honesty:

1. **Named unavailability.** A component type with no renderer prints the declaration and the
   reason: `habit_tracker — habit tracking is not rendered in this build`, not `unavailable`. The
   TUI already says a component is unavailable and keeps the rest of the view usable; the redesign
   upgrades the sentence to name the type and, where the manifest declares a window, the window.
2. **No blank surface.** A view with no components, a workflow that declares no views, and a
   requested view id that does not exist each get their own sentence. The last one names the id and
   offers the first declared view.
3. **Tool collision is visible.** The TUI's equivalents of the web's reserved ids are its keys and
   actions (`W` Workflow, `p`/`P` planning). A declared view or action that would take one of those
   must be refused with the reason, not silently win or lose.

Counts on tabs follow the web's rule: show a count only when it is honest. A readiness-backed badge
is absent until the readiness read returns; the TUI should do the same rather than print `0` for
"not known yet".

**Why class 1.** "The workflow's declaration decides what exists; the build says what it cannot
draw" is the contract that keeps a Work Item workflow portable. A terminal that invents its own
view list breaks it.

### 6. Inspector, focus return and deep links — class 2

**Evidence.** The web drawer is URL state: `?task=<id>` can be bookmarked, survives a reload and
Back closes it ([`web-app.md`](../web-app.md), The task drawer). The row that opened it is
remembered by a data attribute so focus returns to that row's title control after the drawer
unmounts; closing uses `replace` so Back does not reopen a closed drawer
(`use-resource-drawer.ts:41-79`). When a deep-linked drawer has no trigger element, focus falls
back to the main region (`drawer.tsx:42-65`). A conflict is recovered inside the drawer ("Reload
this task and try again"), which refetches the task and snapshot and clears the rejected draft; the
M31 cycle proved this with one alert and one reload control and `recovered: true`
(`run.json` → `conflict`; `I5-05`; `17-task-a-stale-revision.png`). `quality-keyboard.spec.ts`
asserts the drawer's live-region save announcement and focus return.

M32 I2 added the one case where `?task=` is **not** the drawer: on a knowledge route it is the
note's task *context*, and mounting the drawer there covered the note's own reference control.
The fix keeps the parameter, does not mount the drawer on knowledge routes, and shows a labelled
**Task context** bar with **Open the task** and **Stop linking** (`m32-i2-acceptance.md`, defect 2).
That is an inspector rule, not a knowledge rule: one overlay, and it never covers the surface it
was opened from.

**Terminal translation.** The TUI's inspector is already an overlay, never a third column, and it
already returns focus to the pane that owned the keyboard. Keep that. The redesign must preserve
four functions the URL provides in the browser, without the URL:

- **Stable identity.** The open item is a stable id, not a row index. `tui-state.json` already
  restores the selected item's id; an item that disappeared is reported with its last known title
  and kind, and `r` re-checks.
- **Reopen after restart.** Restoring the project, view and selected id is the terminal's
  equivalent of pasting the same `?task=` URL into a new tab.
- **Focus return.** Closing returns to the same row and the same pane, and the selection and scroll
  do not jump. This is the strongest accessibility property in the web drawer and the TUI already
  implements it; a redesign must not regress it.
- **An id handoff.** A person must be able to open a task by the id another person gave them,
  through the palette (`Open task <id>`) or the console, and land in the same inspector. Do not
  build a second detail screen or a "detail route"; one inspector, one identity.
- **A context that is not the inspector.** When a note names a task, the terminal's note view says
  so in place — task id, title, and a key that opens the inspector on it — and never opens the
  inspector on top of the note unasked. Copy the web's split, not its implementation.

**Why class 2.** The URL and the Back button are browser mechanisms that do not translate; stable
identity, reopenability, focus return and recover-in-place do. The terminal keeps the functions and
drops the mechanism.

### 7. Search and command palette — class 4 for results, class 1 for commands

**Evidence.** Two different surfaces share one entry point in the web app, and M32 changed one of
them completely.

*Commands.* With an empty query the palette lists only commands that are implemented, with two
honest empty states — no commands at all, and no match for the query — plus the populated list.
`Enter` runs the highlighted command, `↑`/`↓` move, `Esc` closes
(`command-palette.tsx:44-50,70-89,121-136`). The command list is exactly new task, new project, the
five destinations, and one entry per project (`workspace-layout.tsx:61-125`). `/` focuses a page's
own search field when one exists and otherwise opens the palette (`app-shell.tsx:92-102`).

*Results.* As soon as the query is non-empty, the palette asks the server **once** after a short
debounce (`GET /v1/authoritative/search`) and replaces the command list with results grouped
**Projects**, **Tasks** and **Knowledge** (`web/src/api/endpoints/search.ts:1-31`;
`command-palette.tsx:12-64`; `m32-i1-acceptance.md`). The server matches project names, task titles
and note paths/titles with a wildcard-escaped substring search, returns at most ten per group,
reports truncation, and answers `400 search query is required` for an empty `q`
(`crates/orbit-server/src/main.rs:2245-2266`; `crates/orbit-runtime/src/search.rs`). `search.spec.ts`
(5) proves the grouping, that each result opens its own destination including `?task=`, the empty
and no-match states, the unauthenticated refusal, and that a result whose note moved away lands on
an honest not-found state. Two recorded limits: `LOWER()` folding is ASCII because SQLite's is, and
an older server without the route says search is unavailable instead of failing silently
(`m32-i1-acceptance.md`).

**The terminal's current position, stated plainly.** `/` in the TUI opens a modal that filters, in
memory, the projects it already loaded, the current view's items and the recent-knowledge list, and
truncates at fifty (`app/actions.rs:2333-2404`). Connected mode has a vault-only search
(`backend.rs:883-908`) and a console command `note.search <q>` (`app/connected.rs:332`). None of
that is the web's search: it cannot find a task in another project, a note that is not recent, or
anything the current view has not loaded. That is why the pattern is class 4 rather than class 1
*and* why the honesty rule below matters more than the feature.

**Terminal translation.** Two layers, two rules.

*Commands and the filter (class 1).* Keep the TUI's action catalogue: one source shared by the
action menu and the palette, deterministic ranking, recent destinations first, disabled rows kept
with their reason, and `?` help. Adopt the web's honesty rules:

- Nothing appears in the palette that cannot run on the current screen or authority, unless it is
  visibly disabled with the reason. A row that looks runnable and then refuses is the palette
  version of `I5-11`.
- `Ctrl+U` clears the query; `Esc` closes and returns focus to where it was.
- The palette's vocabulary matches the surface's: **Workflow**, **Work Item**, **Workflow Action**,
  **state**, **transition**, **guard**, **capability**.
- **Say what is searched.** Until the shared read exists, the palette's placeholder and its empty
  state must name the scope — `Search this project, its views and recent knowledge` — and a query
  that finds nothing must not say "no results" as if the workspace had been searched. The web's own
  rule ("it never downloads the workspace into the browser to filter it there",
  [`web-app.md`](../web-app.md) Search) is the reason: a terminal that silently searched less while
  using the same words would be the more deceptive client.

*Results (class 4).* The result list must come from one shared read, the same one the web uses:
`q`, `limit` (1…20, server default), optional `project_id`, grouped by kind with a truncation flag
and a per-result subtitle the client does not compose. Naming the gap: `orbit-client` has only
`markdown_search` (`unified.rs:1622`), so a TUI search over projects, tasks and notes needs a client
method (proposed: `AuthorityConnection::search(&self, query, limit, project_id)`) over the existing
`orbit_runtime::search` and its `SearchResponse`. Until it exists, do not build a TUI-side index,
and do not merge the vault search with the loaded-row filter and call the result "search".

**Why class 4.** "One query over the whole authority, with the authority's own groups and
truncation" is the capability. Everything a terminal could do locally is a filter over a subset,
and the difference is invisible to the person typing — which is exactly the failure mode the
classification exists to prevent.

### 8. Planning review, approval and receipt — class 1

**Evidence.** The web planning surface separates three steps. **Review this plan** is read-only and
returns the fingerprint and command id of the exact proposed command; it writes nothing. **Apply
approved plan** exists only after a review, stays disabled until the approval box is ticked, and
echoes the reviewed fingerprint and command id unchanged. The receipt shows the accepted revision,
the replayed flag and the schedule-block ids. A new window or a changed revision discards the review
and asks for another (`project-planning.tsx:459-465,630-661,675-833,909-925`). The server refuses a
stale fingerprint, or a plan the review no longer describes.

The M31 cycle's committed facts: 240 authored minutes, 2 day-plan rows, 1 proposed slot read before
review, review enabled, fingerprint `e44ba459…bbab08a0`, command id `a6e476d5-…-d1de0738c02d`, one
block `05c1620f-…`, applied, and still present after reload (`run.json` → `planning`).
`13-planning-day-plan.png`, `14-planning-applied.png` and `14b-planning-after-reload.png` show the
preview, the receipt and the surviving block. The first run could not reach the gate (`I5-01`), and
the disabled state did not say what to fix (`I5-10`).

**Terminal translation.** The TUI's connected approval path already refuses an approval that is not
read-only, is fenced to another revision, names another command, or omits reviewed slots
(`crates/orbit-tui/src/backend.rs:1135-1157`), and its receipt line reports block count, revision
and replayed (`connected_ops.rs:234-247`). Keep that fence and complete the receipt:

```text
RECEIPT · plan applied at revision 125
fingerprint  e44ba45955941e833c4861265fc0c855ac3d3761d4d43b4efd408d57bbab08a0
command      a6e476d5-da12-4e22-9e64-d1de0738c02d
blocks       05c1620f-928a-42a2-b958-55417a435ec3
replayed     no
r refresh accepted state · Esc close
```

Rules the redesign must carry:

- The day plan is authored capacity plus explicit options. `include_unsized` / `default_minutes` are
  a named choice, never a silent guess; dispositions and explanations are the server's, printed
  verbatim per row, including `missing_estimate`.
- Review is read-only and prints the fingerprint, command id, the revision it is fenced to and the
  exact reviewed slots. It must be obvious that nothing was written.
- Apply requires an explicit confirmation that echoes the fingerprint and command id. A changed
  revision or window discards the review and says so.
- The receipt prints the fingerprint, command id, accepted revision, replayed flag and block ids, and
  the blocks are visible in the timeline after a refresh.
- The disabled state names the missing input and offers the first unsized task. The TUI's local
  planning is read-only simulation and says so; local scheduling through ordinary commands is not a
  substitute for a reviewed apply, and must at least report the blocks and revision it changed.

**Why class 1.** "The write is bound to a review the person saw" is the safety property. It is the
difference between approving a plan and pressing a button that might apply a different one.

### 9. Agent jobs: launch, claim, review and resume — class 1 for the invariants, class 4 for the commands

**Evidence.** M32 I3 changed this pattern from "review only" to a full cycle, and the honesty rules
survived the change. The browser now starts a job, watches it, records a review and can resume a
finished one; an **enrolled executor device** — never the browser — claims and runs it
([`web-app.md`](../web-app.md), Agents; `m32-i3-acceptance.md`).

- **Executor roster.** `GET /v1/authoritative/agent-executors` lists every enrolled device with
  whether it is active, when it last claimed work and how many jobs it holds — "the honest answer
  to *is anything able to run this?*". A device that has **never claimed a job** is flagged, and the
  confirmation repeats the consequence: the job waits at *Preparing* until something on that device
  claims it, and *Preparing* does not mean work has started
  (`web/src/features/agents/launch-model.ts:129-135,162-171`). Launching stays **enabled** because a
  roster cannot prove that no runner exists — refusing would be as dishonest as pretending. No
  active executor is a named, disabled state; a server without the route says launching is
  unavailable instead of guessing.
- **Claim state.** The job page distinguishes **Waiting for an executor** (`none`), **Claimed**,
  **Launch confirmed** and **Ambiguous**; the model's vocabulary is
  `unreported | none | claimed | confirmed | ambiguous | unrecognised`
  (`agent-model.ts:130-199`), so an answer the build does not understand is `unrecognised` rather
  than folded onto a neighbour. An ambiguous launch warns **Do not relaunch** and offers no resume:
  a second attempt could start a second agent.
- **One launch is one identity.** A refusal keeps the confirmation open and **Retry the same
  launch** re-sends exactly that `job_id`; the spec asserts exactly one job row exists
  (`m32-i3-acceptance.md`; `agents-launch.spec.ts`).
- **Grouping.** Queued or preparing jobs nobody claimed are grouped as **Awaiting an executor**, not
  as attention and not as "nothing to do" — the server reports `next_action: await_executor` for
  exactly that case (`agent-model.ts:59-64`; `m32-i3-acceptance.md`, defect 3).
- **Review.** One decision against the latest persisted handoff, with a required reason. Approval is
  a write, not a completion: the page says so before the form, and the recorded review carries
  **Task not completed**. `m32-i3-acceptance.md` records the task status unchanged in the server
  record and on screen, and surviving a real server restart.
- **Resume.** Only for a terminal job (`completed | failed | interrupted`, `agent-model.ts:30-34`)
  with a durable handoff. Resume **queues a new job** in the same project with the same frozen brief
  and does not launch anything; an executor must claim it. An unconfirmed resume warns that a new
  job may already exist.
- **Ownership.** A job created for a second enrolled device answers `403 device identity mismatch`,
  is absent from the overview, and reads as *not visible*, never as *not found*
  (`m32-i3-acceptance.md`; `blocks.tsx:33-55`).

**The terminal's current position.** The TUI can list a job's detail and print state, agent kind,
task, runtime, handoff count and any recorded review (`app/actions.rs:1311-1336`), and `review_agent`
is an alias for that same read-only inspect. `orbit-client` exposes only `list_agent_jobs` and
`agent_job_detail` (`view_data.rs:205,258`): there is no start, review, resume or roster method, so a
terminal that offered them today would have to re-implement the HTTP contract.

**Terminal translation.** Keep the web's honesty, not its layout:

```text
AGENT JOBS · this device
▸ Awaiting an executor   DF1 triage the findings   preparing   no claim yet
  Launch confirmed       DF0 read the spec         running     handoff x1
  Needs attention        DF2 review the diff       running     review waiting
```

- List jobs grouped attention-first, then awaiting-an-executor, then by state; each row names the
  task (or a short job id), project, state, claim state, handoff count and next action. A job with
  no claim says `no claim yet`, and the group header says what that means.
- The job view shows state and revision, the claim state in the web's four words, runtime
  references as labels (not a live connection), handoffs verbatim, the recorded review, and the
  task-effect sentence: `Review recorded; the task is still In progress`.
- The review form requires a reason and records one decision. After recording, print
  `Review recorded as approved. The task was not completed.` and re-read the job.
- If a launch is offered at all, it must name the executor, repeat a never-claimed warning verbatim
  in kind, and say that it **queues** — the terminal cannot start a model and must not imply it did.
  A resume says a new job is queued for an executor to claim.
- `403` is rendered as ownership, not absence: `This job is not visible to this device. The
  authority answered 403 for <id>.` Never map it to "not found".
- Capability execution is separate from review. A review decision is not a capability run and never
  advances the task; an executor's claim is not something a terminal may perform (pattern 17).

**Why class 1 with a class-4 facet.** The invariants — a roster states liveness limits, a launch is
a queue and one identity, approval is a recorded decision, resume queues, a foreign job is not
"missing" — are a product boundary that must read the same in both clients or the audit trail stops
meaning the same thing. The **commands** are the gap: a TUI launch/review/resume needs
`orbit-client` methods for `agent-jobs/start`, `agent-jobs/review`, `agent-jobs/resume` and
`agent-executors`, none of which exists today. Do not drive the HTTP routes from the terminal
directly: that would be the second writer the classification forbids.

### 10. Keyboard accessibility and announcements — class 2

**Evidence.** The web's quality gates are unusually concrete. `quality-keyboard.spec.ts` walks the
tab order, asserts a visible focus indicator at every stop, the skip link, the navigation drawer's
trap/Escape/return, the palette's open/Escape/return, board moves with `m` + arrows + `Enter`,
planning review and approval without a pointer, and the drawer's live-region save announcement.
`quality-a11y.spec.ts` runs axe over every primary route in both themes, and explicitly leaves
judgement calls (focus visibility, focus order, the meaning of an announcement) to the keyboard
test. Command outcomes are announced in a live region; status is text plus a badge, never colour
alone ([`web-app.md`](../web-app.md), Accessibility and theme).

The friction is equally instructive. In the first cycle, completion from `Ready` took two moves and
nine ArrowDown presses with no mnemonic (`I5-12`); the regenerated run records four presses with the
same two legal moves. The two-move shape is the domain's, not a defect; the press count is a
menu-ergonomics question, and it should be measured, not assumed to have improved because the number
changed. A refusal could sit beside the previous success sentence, and the footer still carried the
neutral save hint next to the error (`I5-17`); the fix hides that hint when a command is refused or
unconfirmed.

**Terminal translation.** A terminal has no screen reader and no live region; it has a status line,
and that line is the announcement channel. The redesign must make it complete:

- Every accepted, pending, rejected, stale and unavailable outcome appears in the status line, in
  the same words as the action, and stays until the next command (not a timed toast). `I5-17` is the
  failure mode: two contradictory sentences at once.
- No meaning by colour. Focus is a thick border plus a bright title; selection is `▸`; state is a
  glyph plus a label. The TUI already does this, and `NO_COLOR`/`TERM=dumb` must stay legible.
- Every action bar entry names a real key; a key is advertised only when it identifies the row
  uniquely. A disabled row keeps its reason and `Enter` reports it without running.
- Shortcuts are ignored while typing; `Ctrl+Q`/`Ctrl+C` always quit; `?` lists only implemented keys.
- Completion from `Ready` stays two moves because the domain forbids `Ready → Done`. Do not add a `d`
  mnemonic that pretends otherwise; make the first move cheap (the status menu on the focused card,
  legal edges only) and say why the second exists.
- At 80×24 and at 120×40, no value needed for a decision may be clipped. The web's clipped 12-hour
  fields (`I5-13`) are the cautionary example: the stored value was right, the field was unreadable.

**Why class 2.** The goal — every action operable without a pointer, every outcome perceivable,
focus never lost — survives. The mechanisms (skip link, focus ring, live region, axe) do not.

### 11. Workflow upgrade, fingerprint and default backfill — class 4

**Evidence.** The web's Settings drives a reviewed upgrade: choose a target, get a read-only preview
with fingerprint, view and kind diffs, blockers and notices; apply behind a confirmation that
repeats the preview; a changed fingerprint refuses; the receipt names previous → new version and the
previewed fingerprint ([`web-app.md`](../web-app.md), Workflow upgrades). The runtime fills new
required-field defaults into live Work Items **inside the upgrade savepoint**
(`crates/orbit-runtime/src/blueprint_upgrade.rs:310-405`), so the project is valid under the new
schema immediately. This is what made Development v3 safe for existing v2 projects after the Feature
kind gained a required `title` (`I5-03`; fix `3f4101f`).

The TUI has no upgrade entry point at all: its `W` picker and the palette's version-binding entry pin
a version, which is not an upgrade, and no `upgrade` path exists in `orbit-tui`. The M31 run offered
no upgrade because current and newest were both 3 (`24-settings-no-upgrade.png`, `run.json` →
`upgrade`), and M32 did not add one; `upgrades.spec.ts` is the only proof of the flow.

One open edge matters for a redesign: v3's default `title` is also a *creation* default, so two
untouched Features can share the name "Untitled feature" and differ only by id
([`web-app.md`](../web-app.md), known rough edges). A migration default and a creation default are
not the same thing.

**Terminal translation.** Not possible honestly today, and that is the classification. A TUI upgrade
must come from the same shared command as the web's: preview → fingerprint → explicit confirm →
apply, with the default backfill inside the same transaction, and a separate creation default so a
new Work Item is not born with the migration name. Implementing the diff, the fingerprint or the
backfill in the TUI would create a second schema engine.

**Until the shared path exists:** `W` may pin an already-published version and must say that this is
a re-pin, not an upgrade; it must not silently move a project to a newer bundle when the binary
changes; and it must not fill defaults itself. The version table in the TUI guide is already stale —
it lists Development v2 while the binary bundles v3
(`crates/orbit-blueprint/src/bundled.rs:16-17`; `docs/guides/tui-workspace.md:252-259`) — which is
exactly the drift a shared, version-aware path removes.

**Why class 4.** The missing piece is below the UI: a shared upgrade command and a migration rule.
This is the one pattern where the redesign is blocked with nothing already satisfied, and the block
belongs in the upstream backlog, not in a terminal workaround.

### 12. Markdown write/preview split — class 3

**Evidence.** The task description editor is a tabbed **Write** / **Preview** field; Markdown is
rendered in Preview; only absolute `http(s)` links are live and they open a new tab
(`description-editor.tsx`; [`web-app.md`](../web-app.md), The task drawer). The M31 cycle needed
Preview to discover that a relative `/knowledge/...` link stayed plain text while an absolute
loopback URL became a link (`I5-04`; `09-task-with-note-link.png`). M32 I2 shipped the answer that
matters more than the preview: notes are linked by **id** through a picker
(`[[orbit_id]]`, pattern 16), so the relative-path question is no longer the only way to move
between a task and a note.

**Terminal translation.** Reject the preview pane. A terminal renders text; a second rendered view
would need a Markdown layout engine and would still not make a link clickable. The TUI should show
raw Markdown in a multi-line field (it already does) and, where a read-only note view exists, print
the source with its structure intact rather than imitating a browser.

What must be kept is the *fact* the preview was protecting: a relative in-app link is not live, and
an absolute URL is printed as a URL, not silently linked. Say that once in the TUI's help or in the
description field's hint, and keep ids and paths copyable. Do not add a preview tab to prove it.

**Why class 3.** It is a browser affordance for rendering, not a capability of the task. The
capability — "you can see what the stored text will look like to a reader" — is served in a terminal
by predictable plain text, which is what the TUI already shows.

### 13. Saved views — class 4

**Evidence.** **Saved views** stores the current URL query (search, status, priority, sort, columns,
page) plus the display choice (`list` or `board`) under a name, and reopens it later. They are
personal presentation preferences stored **by the server**: they survive a browser restart and a
different browser profile, because the proof is a second browser context with fresh cookies
(`saved-views.spec.ts` (6); `m32-i1-acceptance.md`). Save, rename and delete are three separate
server commands with the usual revision fence; a stale version is refused with the server's words
and the list refreshes. Saving a view changes neither the workflow binding nor the authority
revision, which the spec asserts (`m32-i1-acceptance.md`).

Two product rules are load-bearing, and both are easy for a terminal to break:

1. **A saved view is not a workflow view.** The model says it in one sentence: "A saved view is a
   *personal presentation preference*: the URL query string plus the list/board display choice. It
   is never a workflow definition and never a workflow view"
   (`web/src/features/saved-views/model.ts:1-9`). A terminal that put saved views in the same
   picker as declared views (`1`–`9`) would make a personal filter look like part of the workflow
   every collaborator sees.
2. **Personal means server-stored, per actor.** The read is scoped to the authenticated actor and
   deleted rows are not returned (`api/endpoints/saved-views.ts:1-16`). A view saved in the browser
   is the same view in the terminal for that person — that is the point of storing it on the server
   rather than in `localStorage`.

**The terminal's current position.** The TUI has no saved views. What it has is session memory:
`tui-state.json` restores the active project, view and selection, and the palette remembers recent
destinations. That is navigation memory, not a saved view, and the two must not share a name.

**Terminal translation.** When the shared API exists:

- Saved views live in their own picker, visibly separate from the workflow's declared views, and
  each row says its scope (project or workspace) and its display (`list`/`board`).
- Restoring a view restores the filter, the sort and the display, and prints what it restored in one
  line: `View “High priority” · priority=high · sort=title · list`.
- Saving captures what the current surface actually holds, and says so when the terminal's filter
  model is narrower than the web's (a board without URL-backed filters saves less — the web has the
  same limitation and states it, [`web-app.md`](../web-app.md) Board). A view whose parameters this
  build cannot express is listed and refused by name, never silently narrowed.
- Save, rename and delete are the server's three commands with the server's revision fence. A
  refusal is quoted; a stale list refreshes.
- Until then: no local saved-view file, and no reuse of the word "view" for it. The terminal's own
  recall of "last screen" is described as session restore.

**Naming the gap.** `orbit-runtime` already owns the store and the read
(`crates/orbit-runtime/src/saved_views.rs:128`) and `orbit-server` already serves
`/v1/authoritative/saved-views` and `/saved-views/commands`
(`crates/orbit-server/src/main.rs:4034-4040`). `orbit-client` has **no** saved-view read or command,
and no connected transport for either. Propose `fetch_saved_views(actor)` and
`send_saved_view_command(command)` on both the local facade and `AuthorityConnection`.

**Why class 4.** The whole value of the pattern is that the preference is durable and shared across
that person's clients. A terminal-local list would be a second store with the same name and the
wrong lifetime.

### 14. Project identity editing and the immutable id — class 1 for status and id, class 4 for rename

**Evidence.** M32 I1 turned project identity into an editable surface. **Project name** and
**Status** are one fenced command (`action: update`) and the domain decides whether each is
acceptable; **Save changes** stays disabled until something changed; **Cancel** discards the draft; a
refusal is inline in the server's words; a stale revision says the project changed elsewhere and
refreshes without claiming success; an unconfirmed answer says the outcome is unknown
(`project-identity-form.tsx:40,100-143`; `m32-i1-acceptance.md`; `project-settings.spec.ts` (6)).

The status control offers exactly `GET /v1/authoritative/project-statuses` and nothing else
(`project-identity-form.tsx:171-211`), and the domain's own table is the only transition table:
`Proposed → Active | Cancelled | Archived`, `Active → On Hold | Completed | Cancelled | Archived`,
`On Hold → Active | Cancelled | Archived`, `Completed | Cancelled → Archived`, `Archived →` nothing
(`crates/orbit-domain/src/project.rs:38-53`). A same-status command is a no-op, not an error.

**The id never changes.** It is shown and never an input (`project-identity-form.tsx:40`; the
**Project ID** row in `project-settings.tsx:264-270`), and after
an accepted rename the new name reaches the sidebar, the project header, the projects list, recent
destinations and search results without a reload, while tasks, Work Items and the workflow binding
still point at the same project (`:141-143`; `m32-i1-acceptance.md`; `project-settings.spec.ts`).

**The terminal's current position.** The TUI's project form offers status choices filtered by the
domain's own derivation (`app/forms.rs:251`, committed through `set_project_status` at `:1981`), and
`orbit-client` has `set_project_status` (`lib.rs:472`). There is **no rename**: no command in
`orbit-client`, none in the TUI. The terminal therefore already satisfies the status and id facets,
and cannot rename a project at all.

**Terminal translation.**

- *Status (class 1, satisfied).* Keep the derived choice list and the domain's sentence on refusal.
  Never print a status the derivation did not offer, and never show the new badge before the
  authority accepts.
- *Identity (class 1).* The id is displayed, copyable, and never editable. Every address the TUI uses
  for a project — `tui-state.json`, the palette's project entries, the frame's breadcrumb — is an id.
  A rename updates the *label* everywhere it appears, including the frame and the palette's cached
  entries, and must not require a restart.
- *Rename (class 4).* Not buildable today. Name the gap: a shared project identity command covering
  name and status in one fenced `ProjectCommand` with `action: update`
  (`crates/orbit-runtime/src/domain_commands.rs:142-155`; `crates/orbit-server/src/main.rs:5823-5872`).
  The authority already accepts it; `orbit-client` has only the status transition. Propose
  `update_project_identity(request)` on both transports, returning the same `ProjectDto`.
- Until then the project form shows the id and the status and **states that the name is edited in the
  other client**, rather than offering a dead control — the same rule the web used to follow for
  status before I1.

**Why class 1 with a class-4 facet.** "Which project is this, and what am I allowed to change about
it right now" is a capability, and the id-is-identity rule is the reason a rename is safe. The name
command is missing below the UI, so the facet that needs it cannot be built honestly.

### 15. Task Created and Updated with `Unknown` — class 4

**Evidence.** The drawer shows **Created** and **Updated** as the authority's recorded times, taken
from the task's own activity history, with the full timestamp available on hover and to a screen
reader. A task with no recorded history shows **Unknown** rather than a guessed date, and a change
advances **Updated** while leaving **Created** alone (`task-timestamps.tsx:1-58`;
`m32-i1-acceptance.md`; `task-metadata.spec.ts` (2)). The `null` → `Unknown` branch is unit-covered
because a task created through the API always has history, so the browser cannot reach that state
against a real server — the evidence says so instead of implying browser coverage.

The server's shape matters for the terminal: the values are `Option<i64>`, "derived from the
recorded activity history; null when the task has no history row, **never a read-time timestamp**"
(`crates/orbit-server/src/main.rs:1914-1969`, served by `GET /v1/authoritative/tasks/{id}` at
`:4123-4126`, populated from `store.task_timestamps(&id)` at `:2019-2022`).

**The terminal's current position.** The TUI's inspector shows no timestamps, and the shared client
cannot supply them: `TaskDto` carries id, title, status, scope, planning, description, priority and
labels, and no timestamps (`crates/orbit-client/src/contract.rs:171-186`).

**Terminal translation.**

- Two labelled values in the inspector header block, in the local timezone, with the absolute
  timestamp shown in full somewhere the person can read and copy (`Created 2026-10-26 05:12:44
  +09:00`), because a terminal has neither a tooltip nor `sr-only` text.
- A missing row prints `Unknown` and nothing else. Never derive it from the activity list, the
  deadline, the file's mtime or the process start time. Never print `now`.
- `Updated` is not "last edited by me": it is the authority's record, and a status move, a planning
  edit and a label edit all advance it. Do not annotate it with the acting device.
- A refused write leaves both values as the last accepted read.
- `Unknown` must be reachable in a test even though a real server rarely produces it: a terminal can
  show it for any record whose history read failed, which the web cannot.

**Naming the gap.** `orbit_runtime::TaskTimestamps` and `store.task_timestamps` already exist and
the HTTP detail route already carries them. `orbit-client`'s `TaskDto` does not. Propose
`created_at_ms: Option<i64>` and `updated_at_ms: Option<i64>` on `TaskDto`, filled from the same
runtime call — not a second computation of "first and last activity".

**Why class 4.** The values are a read the shared client does not expose. A terminal that derived
them from activity rows it happened to load would be answering a different question (pattern 3's
warning, in a different field).

### 16. Knowledge: folders, location, note links and references — class 4, with class-1 honesty and a class-3 rendering boundary

**Evidence.** M32 I2 made the vault navigable and linked, and every step was asserted twice: through
the interface and through the server read it claims to be a view of (`knowledge-tree`,
`markdown-inventory`, `markdown-document`, `markdown-relations`, `markdown-references`,
`project-knowledge`) — `m32-i2-acceptance.md`; `knowledge-folders.spec.ts` (8),
`knowledge-links.spec.ts` (4), `project-knowledge.spec.ts` (7).

- **Folders and location.** A folder is a real destination: the breadcrumb walks parents and
  children, the folder view lists exactly what is inside it, and the open folder survives a reload.
  **New folder** creates at the root or inside the selected folder; **Rename** and **Move** change
  the name or the parent, and a rename carries every note underneath it. The server refuses a missing
  parent, a destination that already exists and a folder moved into itself, and the dialog quotes the
  refusal without changing the tree. Creating an existing folder is an accepted no-op and the toast
  says **Folder ready**, not "created". A note page shows its **location** with the folder path, its
  stable id, and **Reload / Edit / Rename / Move / Link a note** ([`web-app.md`](../web-app.md),
  Knowledge).
- **Links and back-references.** A note's stable id lives in its frontmatter (`orbit_id`), so the
  path is not the identity and a move does not break `[[id]]`. **Link a note** picks a note by title
  and appends the reference through the same fenced `Update` command, so the picker cannot produce an
  id that does not exist; **Unlink** removes the outgoing chip, the backlink and the token while
  keeping the prose. The Relations panel separates **Links to**, **Linked from**, **Unresolved
  references** and **References**. A hand-written unknown id is shown as unresolved rather than
  fabricated, and if the inventory cannot be read the ids are still shown with a sentence saying they
  could not be resolved.
- **Typed project and task references.** A reference is one Markdown link whose visible label is
  `project:<id>` or `task:<id>` and whose target is the `orbit://` form of the same pair (for example
  a label of `project:orbit` pointing at `orbit://project:orbit`), added by **Reference this
  project / task** or the **Link a note** dialog's reference tab, and removed from the chip. The
  reference travels with the file, so it needs no database. `habit:` and work-item references are
  refused by the server. A reference to something that no longer exists is a **broken** chip with the
  reason, stays removable, and never links to a dead end.
- **Project knowledge and the task Notes section.** A project's `knowledge_refs` panel reads the notes
  that reference the project or any of its tasks, says *which* reference caused each row, and opens
  the note; without a project it refuses by name instead of falling back to the whole vault. The task
  drawer's **Notes** section does the same for one task and can link or unlink a note
  (`task-drawer/notes-section.tsx`).
- **Context, not containment.** `/knowledge/notes/x.md?task=<id>` is a context bar, not the drawer
  (pattern 6), and **Stop linking** removes only `task`, keeping `?project=` and `?folder=`.

Two limits are recorded, not hidden: there is no note or folder deletion, no note kind or status, and
no work-item references, because the contracts do not cover them (`m32-i2-acceptance.md`).

**The terminal's current position.** The TUI shows a recent-knowledge list, can open a note, and has
a connected console (`note.search`, `note.cached`, `note.show`, `app/connected.rs:271-332`). The
shared client has `knowledge_tree` (`unified.rs:1403`), `folder_create` / `folder_move`
(`unified.rs:981-1010`) and `markdown_relations` (`unified.rs:1645`), but **no**
`markdown_references` and **no** `project_knowledge`, although the runtime implements both
(`crates/orbit-runtime/src/lib.rs:949,969`).

**Terminal translation.**

- *Location (class 1).* A note's header line carries the folder path as a breadcrumb the keys can
  walk (`knowledge › research › 2026-10 › spec`), the stable id in short form, and the note's title.
  A move reports the new path; a stale path says the note moved and offers the note it found, which
  is what the browser does with an old deep link. Never show a path as identity.
- *Folders (class 4 for the picker, class 1 for the rules).* The tree, create, rename and move are
  already reachable through the shared client; what is missing is nothing on the write side and
  everything on the *read* side only if the terminal wants the browser's exact tree shape — the
  `knowledge_tree` read is enough. Keep the server's three refusals by name, keep the "already
  exists is a no-op, say **Folder ready**" rule, and never re-flow a path locally.
- *Links and back-references (class 1).* The Relations block prints **Links to**, **Linked from**,
  **Unresolved** and **References** as four labelled groups with counts. A broken target is a row
  with the target id and the reason, still listed, still removable. This is the pattern's core: a
  terminal that silently dropped an unresolved id would be the most damaging possible simplification,
  because the id is the only thing the person can act on.
- *Typed references (class 4).* Adding a project or task reference needs the shared
  `markdown_references` read to know what is already there and `project_knowledge` to answer "which
  notes belong to this project". Until both exist, the terminal may show the references it can parse
  from the body **only if** it labels them as parsed from the file rather than resolved by the
  authority — and it must not offer to add or remove one.
- *Project context and the task Notes section (class 4).* `project-knowledge` is the read that makes
  "which notes reference this project or its tasks" answerable. The terminal's project screen and
  task inspector need the same read, for the same reason: a local scan of the vault would answer a
  different question (what this machine can see) in the same words.
- *Rendering (class 3).* No Markdown layout, no link activation, no hover: print the source
  (pattern 12).

**Naming the gap.** Propose `markdown_references(id)` and `project_knowledge(project_id)` on
`orbit-client`, delegating to `orbit_runtime::{markdown_references, project_knowledge}` exactly as
`markdown_relations` already does. Nothing below the runtime needs to change.

**Why class 4.** The patterns that matter here — "what links here", "what does this project
reference", "is this target real" — are answers only the authority can give, and two of the three
are not on the shared client.

### 17. Workflow Capability actions — class 4 in connected mode, class 1 locally, class 3 for the executor boundary

**Evidence.** M32 I4 drove the real `orbit capability serve --once` process against a real server
(`capability-execution.spec.ts` (5); `m32-i4-acceptance.md`). The full cycle, as the browser sees it:

- **Offers with availability codes.** Each declared action either is requestable or carries the
  server's own code and reason. The vocabulary is closed and ordered:
  `capability_missing`, `capability_disabled`, `capability_not_installed`, `executor_missing`,
  `requirement_missing`, `not_authorized` (`web/src/features/workflows/capability-model.ts:34-42`).
  Each code renders as **its own sentence**; `executor_missing` says no executor has announced the
  capability and "is not an authorization problem" (`:65-83`). A code this build does not know is
  quoted, not folded onto a neighbour, and a refusal with no code says exactly that (`:85-110`).
  Capability facts are shown beside the offers: installed, granted to this project, announced by an
  executor, what it requires, and its permission (`read | mutate | external_effect`).
- **Typed inputs and confirmation.** An available action opens a form generated from the capability's
  declared input schema; an empty required input is refused **in the form with zero POSTs**
  (`m32-i4-acceptance.md`). An effectful or mutating action asks for confirmation in a separate step
  that states the request waits for an **executor** and that the browser does not run it. A forged
  body carrying `to`/`provider` is `422`.
- **Five states.** `pending` → "Waiting for an executor to claim it"; `claimed` → "An executor has
  taken it; no result yet"; `succeeded`; `failed`; `uncertain`
  (`capability-model.ts:409-450`). A `pending` request has not run; a settled state is already
  recorded.
- **Receipt.** The executor's immutable receipt — reporter, time, outcome, and the **declared effect**
  validated against the capability's own contract and written in the same savepoint as the state and
  the receipt. The spec asserts exactly one effect, a byte-identical receipt on replay, and that the
  Work Item's state and revision are unchanged (`m32-i4-acceptance.md`).
- **Reconcile.** `uncertain` and `claimed` are the two reconcilable states and they are different
  questions: `uncertain` is an executor saying it could not tell whether the effect happened;
  `claimed` is silence after a claim, which may mean the answer was lost (`capability-model.ts:476-510`).
  The panel offers a decision (**Confirm it succeeded** / confirm failed with a reason) and **no
  retry anywhere**; `reconciled_by` is the operator device and `recovered_from` records the state it
  settled. `pending` offers no reconcile.
- **Executor-only claim and result.** `orbit-server` contains no `TrustedComposition`, no executor
  registry and no provider execution; a cookie session cannot claim or report
  (`m32-i4-acceptance.md`, Trust boundary). Claim and result are bearer-credential executor routes,
  and `orbit capability serve --once` is the only shipped claimant.

**The terminal's current position, and it is unusual: the local TUI is ahead of the web and the
connected TUI is behind it.** Locally the TUI already builds its action rows from cached offers
(`app/capability.rs:71-108`), renders an unavailable offer as a disabled row whose reason is
`<code>: <reason>`, refuses `Enter` on it, mints one command id per form so a retry cannot duplicate,
records the request (pending) and then asks in an explicit modal — *Run the trusted executor now?*
with the sentence that `Esc` leaves it pending — before claiming it through the local authority
(`app/forms.rs:2260-2286`, `app/actions.rs:1650-1690`). Its receipt carries outcome, provider,
action, capability, item, command and detail, and only `uncertain` offers reconcile
(`app/capability.rs:160-205`). In connected mode the whole surface refuses with one sentence:
`Capability actions require the local authoritative workspace; connected parity is tracked as M25
EC-004` (`backend.rs:141-142`, `:1980-2026`).

**Terminal translation.**

- *Local (class 1, already satisfied).* Keep the vocabulary exactly: the six availability codes as
  the server's sentences, the five request states, `code: reason` on a disabled row, the one-command-id
  retry, the explicit "run it now?" step, and reconcile only for `uncertain` **and** `claimed`.
  Complete it in two places: `claimed` is currently not offered a settle control locally
  (`app/capability.rs:195-200` gates on `uncertain` alone, while the web serves both), and the
  history should be readable as a list rather than only the last receipt.
- *Connected (class 4).* The terminal must show the offers the authority computed, not a locally
  re-derived availability. Name the gap: `orbit-client` implements offers, status, request, execute,
  state, history and reconcile for the **local** authority only
  (`crates/orbit-client/src/capability_actions.rs:162-270`), while `orbit-server` already serves
  `/v1/authoritative/capability-actions{,/request,/show,/list,/reconcile}` and
  `/capability-executors` (`crates/orbit-server/src/main.rs:4302-4354`). Propose the same seven
  methods on `AuthorityConnection`, minus `execute` — a connected client records a request, it never
  runs one.
- *Confirmation copy (class 1).* The local modal's question is right for a local authority and wrong
  for a connected one. In connected mode the sentence must be the web's: the request waits for an
  **executor** to claim it, and this terminal does not run it. Same control, different promise, and
  the promise is the part that matters.
- *Executor boundary (class 3).* A terminal must never claim or report a capability action as an
  executor. Making the TUI a claimant would make every analyst's terminal a trusted executor, put
  provider code inside the client and dissolve the boundary the receipt exists to prove. If a person
  wants to run capabilities, they run `orbit capability serve` on an enrolled executor device. The
  TUI may *show* which executor announced, claimed and reported; it may not be one.
- *Item movement (class 1 honesty).* Nothing in the panel may claim the item moved. The receipt and
  the effect are the durable result; only a re-read of the item can report a state change.

**Why the split.** The rules ("availability is the server's, the effect is declared, the claim is
single, reconcile is a decision and not a retry") are a capability and must match. The connected
transport is a shared-API gap. Executor identity is a boundary the terminal must not cross, and
crossing it would be worse in a terminal than in a browser because a TUI is where a power user
already has local authority.

### 18. Refusal vocabulary: no executor, not authorized, not installed — class 1

**Evidence.** M32 I3 and I4 each turned a class of refusal into a first-class, distinct state, and
the spec proves the distinctions rather than the codes alone:

| Refusal | What it means | Proven by |
| --- | --- | --- |
| `executor_missing` (capability) | No enrolled executor has announced this capability. **Not** an authorization problem. | `capability-model.ts:69-72`; `m32-i4-acceptance.md` scenario 2 |
| `not_authorized` (capability) | This project has not been granted the capability; a grant is an operator step. | `capability-model.ts:77-80`; `m32-i4-acceptance.md` scenario 2 |
| `capability_not_installed` | The capability is not installed on the server; installation is an operator step and cannot be done from either client. | `capability-model.ts:65-68`; **not** reachable over HTTP in this build (`m32-i4-acceptance.md`, limits) |
| `requirement_missing` | A capability this one requires is not installed; the status block names the ids. | `capability-model.ts:73-76` |
| No executor enrolled / all revoked | A queued job would wait for a claim that never comes, so launching is disabled — and the sentence says which of the two it is. | `launch-model.ts:129-135`; `m32-i3-acceptance.md` |
| Never-claimed executor | Launch stays enabled; the warning states that a roster cannot show whether a runner is active and that *Preparing* does not mean work started. | `launch-model.ts:162-171`; `m32-i3-acceptance.md` |
| Foreign job | `403 device identity mismatch` reads as *not visible to this device*, never as *not found*. | `blocks.tsx:33-55`; `m32-i3-acceptance.md` |
| Cookie session on an operator/executor route | `403 operator credential required` / `403 executor credential required`, quoted as the reason the browser cannot do it. | `m32-i4-acceptance.md` scenario 2 |

**Terminal translation.** One refusal vocabulary, shared by every surface, printed as three parts:

```text
ACTION REFUSED · review.publish
reason   executor_missing — no executor has announced this capability
meaning  A request would wait in the queue. This is not an authorization problem.
next     start `orbit capability serve` on an enrolled device, then reload
```

- The code, the server's sentence and the "what would change it" line are three separate facts. Do
  not collapse them into one string, and do not translate a code into a different code's meaning.
- Distinguish *not permitted* from *not able*: `not_authorized` and `capability_not_installed` are
  durable, operator-fixable states; `executor_missing` is a liveness state that a running executor
  clears without anyone granting anything.
- A refusal is not an error colour and not a crash. It is a state of the surface, listed until the
  condition changes, with the re-read key beside it.
- Two refusals that look alike must not share copy: *no executor enrolled* (launching disabled) and
  *this executor has never claimed a job* (launching enabled, warned) are opposite decisions.
- An availability code this build does not know is quoted with its raw name plus "this build does not
  know that code", which is what the web does (`capability-model.ts:104-110`).
- `403` never becomes "not found", anywhere: task, job, capability request or note.

**Why class 1.** These sentences are what stop a person from "fixing" the wrong thing — enrolling a
device when they needed a grant, or granting a capability when they needed an executor. The
distinctions are the capability; the terminal has no excuse for fewer of them than the browser, and
has better places to put them.

## Non-goals

These are explicit. A redesign that adds any of them is out of scope.

- **Modal-heavy flows.** No stacks of confirmation dialogs, no per-field popovers, no dialog for
  something a single overlay or a status line can do. The web's upgrade flow has three steps because
  a workflow change is destructive and fingerprinted, not because three dialogs are good. The
  terminal equivalent is one overlay with one explicit confirm that repeats the fingerprint.
- **Pointer or hover-only affordances.** No hover menu, no tooltip-only explanation, no drag-and-drop.
  The web refuses drag-and-drop on the board for the same reason: keyboard parity was the requirement
  for offering a pointer gesture. `prefers-reduced-motion` has no terminal equivalent and is not a
  pattern.
- **A second copy of any domain rule.** No transition table, no readiness or blocking recomputation,
  no plan fingerprint, no upgrade diff, no default duration, no backfill, no "is this project status
  transition allowed" check, no capability availability derived locally, no "first and last activity"
  computed from a partial activity list.
- **A second store for a shared preference.** No saved-view file, no local copy of another device's
  roster, no cached rename that survives a rejected command.
- **A second writer of a shared vocabulary.** The TUI does not claim capability actions, does not
  report results, does not review on behalf of another device, and does not record a launch
  confirmation an executor did not send.
- **A terminal re-implementation of a browser mechanism.** No skip link, focus ring, live region,
  theme toggle, responsive drawer or axe audit. Their *intent* is covered by status-line
  announcements, border-and-title focus, glyphs plus labels, and a width-tested layout.
- **Claims the authority cannot back.** No "search" that only filters loaded rows, no "plan approved"
  without a review, no "saved" before the authority accepts, no "not found" for a 403, no "the task
  was completed" anywhere near a review, no timestamp that is not recorded history.
- **A second detail surface.** One inspector, one identity, one focus-return path.

## What the web app still owes the TUI

Shared domain/API follow-ups that building the web app exposed. They are listed so a redesign does
not compensate for them locally. Each names the layer that owns it; none is a TUI rendering task.

One item from the M31 page is **closed** and is no longer asked for: the project status transition is
mounted on the HTTP authority (`action: update`, `/v1/authoritative/project-statuses`,
`m32-i1-acceptance.md`). The M31 item about humanized activity summaries is retained as item 9: it is
still true, and the typed-reference work in item 5 makes it easier to fix by making the subject a
record instead of a sentence.

### 1. Unified search on the shared client

**Evidence.** The server search is done (`crates/orbit-server/src/main.rs:2245-2266`,
`crates/orbit-runtime/src/search.rs`, `search.spec.ts`). `orbit-client` has `markdown_search` only
(`unified.rs:1622`), so the TUI's `/` filters loaded rows (`app/actions.rs:2333-2404`).

**Owed.** `search(query, limit, project_id) -> SearchResponse` on the local facade and on
`AuthorityConnection`, with the server's clamp (1…20), its default limit and its `400` for an empty
query.

**Done when.** A terminal query finds a task in another project and a note that is not recent, with
the server's groups, subtitles and truncation flag, and the palette's scope sentence disappears
because it is no longer true.

### 2. Saved views

**Evidence.** Runtime and server are done (`crates/orbit-runtime/src/saved_views.rs:128`,
`crates/orbit-server/src/main.rs:4034-4040`); the shared client has nothing.

**Owed.** A list read scoped to the actor and one command method, on both transports.

**Done when.** A view saved in the browser restores in the terminal for the same person, and no
terminal-local file uses the word "view".

### 3. Project identity (rename)

**Evidence.** The authority accepts name and status in one fenced `action: update`
(`domain_commands.rs:142-155`, `crates/orbit-server/src/main.rs:5823-5872`); `orbit-client` has only
`set_project_status` (`lib.rs:472`).

**Owed.** `update_project_identity` covering name and status, returning the accepted `ProjectDto`.

**Done when.** A project can be renamed from the terminal, the new name appears in the frame, the
palette and search results without a restart, and the id is unchanged everywhere.

### 4. Task timestamps on the shared DTO

**Evidence.** `orbit_runtime::TaskTimestamps` and `GET /v1/authoritative/tasks/{id}` carry the
values (`crates/orbit-server/src/main.rs:1914-1969,2019-2022,4123-4126`); `TaskDto` does not
(`contract.rs:171-186`).

**Owed.** `created_at_ms` / `updated_at_ms` as `Option<i64>` on `TaskDto`, filled from the same
runtime call.

**Done when.** The inspector prints the authority's two moments, `Unknown` when absent, and never a
derived date.

### 5. Knowledge references and project knowledge

**Evidence.** `orbit_runtime::{markdown_references, project_knowledge}` exist
(`crates/orbit-runtime/src/lib.rs:949,969`) and the routes are mounted
(`crates/orbit-server/src/main.rs:4082-4090`); `orbit-client` exposes neither, only
`markdown_relations`.

**Owed.** Two delegating methods on both transports, with the same "unresolved ids are still shown"
contract.

**Done when.** The terminal's Relations block and the task Notes section answer from the authority,
and no surface scans the vault locally to answer them.

### 6. Agent job commands

**Evidence.** The routes exist (`agent-jobs/start`, `review`, `resume-context`, `agent-executors`,
`crates/orbit-server/src/main.rs:4404-4437`) and the browser drives them; `orbit-client` has only
`list_agent_jobs` and `agent_job_detail` (`view_data.rs:205,258`).

**Owed.** Start (with a pinned `job_id` and the never-claimed warning fields), review, resume, and
the executor roster read.

**Done when.** A terminal can queue one job with one identity, record one review with a required
reason, resume a terminal job into the queue, and never claims the task was completed.

### 7. Connected capability actions

**Evidence.** `orbit-client`'s seven capability methods run against the local authority only
(`capability_actions.rs:162-270`); the connected backend refuses with M25 EC-004
(`backend.rs:141-142`); the server serves every route (`crates/orbit-server/src/main.rs:4302-4354`).

**Owed.** Offers, status, request, state, history and reconcile on `AuthorityConnection` — and
**not** execute, which stays local or executor-side.

**Done when.** A connected terminal shows the authority's offers with the server's availability
codes, records a request that waits for an executor, follows the five states, and reconciles
`uncertain` and `claimed` without ever running anything.

### 8. Transition-edge read for connected clients

**Evidence.** The web reads the authority's edges per status
(`/v1/authoritative/task-transitions`) and falls back to every status when the read fails
(`queries.ts:425-447`, `task-controls.tsx:15-30`). The shared client exposes only a locally compiled
derivation (`orbit_client::allowed_task_statuses`, `view_data.rs:55-74`), and the TUI builds its
status menu from that (`backend.rs:1369-1371`). In connected mode the TUI therefore offers the
**binary's** edges, not the **authority's** — the one M31 item that M32 did not close.

**Owed.** One shared edge read that asks the authority in connected mode and delegates to the domain
locally, with one documented fallback.

**Done when.** A TUI binary older than its server offers exactly the server's edges in connected
mode; a failed read offers all statuses with a visible warning; no view contains a literal status
list.

### 9. Humanized activity summaries

**Evidence.** The server composes `Task <id> <fields> updated`
(`crates/orbit-application/src/task_detail_commands.rs:70`) and `Task assigned to milestone <id>`
(`crates/orbit-runtime/src/store.rs:1642`), and the attachment summary embeds the task id. The web
renders the sentence verbatim (`features/tasks/task-drawer/activity-section.tsx:58`); the TUI does
the same (`crates/orbit-tui/src/render/modals.rs:387-394`). `19-task-a-complete-activity.png` shows
the raw 36-character ids next to the drawer header's short id, and the M31 run's `taskActivity`
records `Attachment task-a-evidence.md added to task cbaeca85-…` (`I5-07`).

**Owed.** Compose the sentence with the entity's title or name, or emit a structured subject (kind,
id, changed fields) and let each client format it. Keep ids in machine-readable fields, never in the
human sentence.

**Until then.** Neither client should regex an id out of the server's sentence. The surrounding
surface names the subject; the activity row need not.

**Done when.** Activity in both clients names the task and the milestone, no 36-character id appears
in a human sentence, and a test asserts the absence. This matters for the terminal more than for the
browser: an 80-column activity row cannot afford a 36-character id in the middle of its prose.

## Acceptance for a future redesign

Measure the redesign against these. "Pass" requires the evidence named in the last two columns; taste
is not a criterion. Run the checks at 80×24 and 120×40, with colour and with `NO_COLOR=1`, and record
the commit, binary hash, Workflow version and terminal sizes with the results.

The **shared service** column says what already exists below the UI today: `—` means the check waits
on an item in the previous section.

| # | Criterion | How to measure | Web evidence it mirrors | Shared service today |
| --- | --- | --- | --- | --- |
| A1 | The five destinations are reachable with a `g` sequence, every project by name through the palette; the frame names project, status and pinned Workflow version, and re-reads a renamed project's name without a restart. | Scripted key sequence from each screen; palette query per project; rename then re-read. | `nav.ts`, `workspace-layout.tsx:61-125`, `03-project-overview` | Partial: status yes, name no (gap 3) |
| A2 | Capture is one line: `Enter` creates; accepted text clears and selects the new row; rejected text stays with the reason; an unconfirmed retry creates exactly one task. | Create one; force a refusal (expired session); replay an uncertain submit; count tasks. | `quick-capture.tsx:14-71`, `11-inbox-before-triage` | Yes (local commands) |
| A3 | Estimate, earliest start and deadline are editable from the inspector without leaving it; the values survive a restart; empty clears; a refused save restores the stored values. | Edit, restart, re-read; send a stale save. | `planning-section.tsx:1-30,225-231`, `I5-01` | Yes (`set_planning`) |
| A4 | For every status, the offered edges equal the authority's read; a failed read offers all statuses with a warning; an undeclared edge cannot be chosen; a refusal leaves the row unchanged and quotes the authority. | Compare the menu with the read for all ten statuses; probe a refused edge. | `run.json` → `invalidTransition`; `queries.ts:425-447` | Local yes; connected no (gap 8) |
| A5 | Every declared Work Item transition is visible with its guard sentences; a false property guard is marked; a relationship guard says the authority decides; a capability chip stays visible and dimmed with its availability code and reason; a refusal does not move the item. | Walk every transition of one kind; probe one guarded move; probe one unavailable offer. | `work-item-sheet.tsx:252-337,603-654` | Local yes; connected no (gap 7) |
| A6 | Every declared view is selectable by id and position; an unrendered component is named with its declaration and reason; a missing view names the id and offers the first declared view; a tool-key collision is visible. | Visit every view; request an unknown id; publish a colliding id. | `view-registry.tsx:71-132`, `project-workspace-page.tsx:487-500` | Yes |
| A7 | Opening and closing the inspector returns focus to the same row and pane; selection and scroll are unchanged; an id handoff opens the same inspector; a note's task context does not open the drawer on the note. | Keyboard-only open/close; restart; paste an id; open a note with a task context. | `use-resource-drawer.ts:41-79`, `drawer.tsx:42-65`, `m32-i2` defect 2 | Yes (TUI overlay) |
| A8 | The palette lists every implemented destination and action, nothing that cannot run, disabled rows with reasons, recent destinations first; `Enter` runs and `Esc` restores focus; its scope sentence is true. | Compare the catalogue with the action bar and help; walk every row; query for something outside the loaded view. | `command-palette.tsx:44-89,121-136` | Yes for actions; results need gap 1 |
| A9 | Review writes nothing; apply requires an explicit confirmation echoing the fingerprint; the receipt shows fingerprint, command id, accepted revision, replayed flag and block ids; a changed revision discards the review; the block appears after refresh. | Read the revision before/after review; change the window; apply; refresh. | `run.json` → `planning`; `project-planning.tsx:675-833,909-925` | Yes (connected fence exists) |
| A10 | A launch, if offered, names the executor, repeats the never-claimed warning, pins one `job_id` and says it **queues**; a foreign job reads as ownership (`403`), not absence; resume queues a new job and never launches one. | Queue one with a real executor; retry after a refusal; request a foreign id; resume a terminal job. | `agents-launch.spec.ts`, `launch-model.ts:129-171` | No (gap 6) |
| A11 | An approved review leaves the task status unchanged, says `Task not completed`, and no second conflicting decision is offered; the job row and the task status are re-read after recording. | Review a job, read the task; retry the decision. | `agents.spec.ts`, `m32-i3-acceptance.md` | No (gap 6) |
| A12 | Every action is reachable without a pointer; focus is visible without colour; every command outcome appears in the status line and persists until the next command; shortcuts are ignored while typing; `?` lists only real keys. | Keyboard-only pass over every screen; run with `NO_COLOR=1`; trigger one of each outcome. | `quality-keyboard.spec.ts`, `quality-a11y.spec.ts`, `I5-17` | Yes |
| A13 | An unavailable action prints the server's availability code, its meaning and what would change it as three separate facts; `executor_missing`, `not_authorized` and `capability_not_installed` never share copy; an unknown code is quoted. | Trigger each availability code; compare with the server's read. | `capability-model.ts:34-110`, `m32-i4-acceptance.md` | Local yes; connected no (gap 7) |
| A14 | A capability request shows all five states, prints the receipt verbatim including the declared effect, offers reconcile for `uncertain` **and** `claimed` and for nothing else, offers no retry anywhere, and never claims the item moved. | Run one request through an executor; replay the result; settle one `uncertain` and one `claimed`. | `capability-model.ts:409-510`, `m32-i4-acceptance.md` scenarios 1-4 | Local partial (`claimed` reconcile missing); connected no |
| A15 | Saved views are listed apart from declared workflow views, restore the filter, sort and display, survive a restart and a second client, and a stale version is refused with the server's words. | Save in the browser, restore in the terminal; stale-save; rename; delete. | `saved-views.spec.ts`, `saved-views/model.ts:1-20` | No (gap 2) |
| A16 | Project name and status are one fenced command; the control offers exactly the authority's moves; a refusal is inline and keeps the stored values; the id is displayed, copyable and never editable, and survives a rename everywhere. | Rename; move status; probe an illegal move; stale-save; re-read the id in every surface. | `project-identity-form.tsx:40-211`, `project-settings.spec.ts` | Status yes, rename no (gap 3) |
| A17 | Created and Updated are the authority's recorded values, `Updated` advances on a change while `Created` does not, and a missing value prints `Unknown` — never a derived date, never `now`. | Read both, change the task, re-read; force the missing branch. | `task-metadata.spec.ts`, `task-timestamps.tsx:1-58` | No (gap 4) |
| A18 | A note shows folder location as a walkable path plus its stable id; the four relation groups are separate; an unresolved id is listed and removable, not dropped; a moved note is followed and an old path says the note moved. | Walk the breadcrumb; link, unlink, hand-write an unknown id, move a note. | `m32-i2-acceptance.md`, `knowledge-links.spec.ts` | No (gap 5) |
| A19 | Project knowledge and the task Notes section answer from the authority, say which reference caused each row, and refuse by name without a project; a note's task context is not the drawer. | Open a project, open a task, unlink one reference; open a note with `?task=`. | `project-knowledge.spec.ts`, `m32-i2` defect 2 | No (gap 5) |
| A20 | Every "not available" names the declaration and the reason; no simulated data is shown as accepted; no capability execution is claimed in connected mode; no executor claim is performed by a terminal; no `saved` precedes the authority's answer. | Audit every empty and disabled state; compare with the authority's read; inspect the claim path. | `view-registry.tsx:99-132`, `m32-i4-acceptance.md` Trust boundary | Yes (local boundary holds) |
| A21 | Activity sentences name their subject: no 36-character id appears in a human sentence, and an id that must be shown is a separate, copyable field. | Read one task's activity after a status change, an attachment and a milestone move; measure the widest row at 80 columns. | `activity-section.tsx:58`, `19-task-a-complete-activity.png`, `I5-07` | No (gap 9) |
| A22 | Layout is usable at 80×24 and 120×40 with no decision-critical value clipped; the same is true with colour off. | Two terminal sizes, two colour modes, one pass over planning, transitions and the knowledge relations block. | `I5-13`, `web-app.md` Accessibility | Yes (existing layout tests) |
| A23 | The run that proves A1–A22 records the commit, binary hash, Workflow version, terminal sizes and the failing cases; failures stay in the record instead of being waived. | Inspect the evidence directory. | M31 `README.md` capture identity; M32 acceptance files | — |

**Two gates on top of the checklist.**

1. Do not call the redesign complete while any shared item in the previous section is faked in the
   terminal. A TUI that filters edges from its own table, prints an upgrade receipt without a
   fingerprint, derives capability availability locally, stores saved views in its own file, or
   computes task timestamps from a partial activity list would pass a superficial demo and fail
   A4, A9, A13, A14, A15, A17 or A20 on the first version skew.
2. Do not call it complete while any surface uses a shared word for a narrower thing. "Search" over
   loaded rows, "view" for session restore, "created" for a guessed date, and "completed" near a
   review are the four known traps; each has a test above.

## What this spec does not cover

- **Attachments, milestone CRUD, sign-in and device enrolment.** Both cycles used them, but they are
  not in this page's pattern set; the web guide documents their behaviour, and a terminal spec for
  them would be a separate page.
- **The web's visual language** — type, colour, motion, spacing, theme. The only visual facts that
  cross into the terminal are meaning-bearing ones: status glyphs, the focus border, the selection
  rail.
- **Project archive and deletion, and note or folder deletion.** No client can do them; the terminal
  must not invent a control, and this page does not specify one.
- **Workflow authoring and publishing.** Both clients only install and pin; authoring stays
  console/HTTP.
- **Executor dispatch itself** — machine enrollment, `orbit capability serve`, agent claims and
  result reporting. Those are executor-side contracts; this page only requires that the terminal
  never performs one and reports the outcome honestly.
- **Performance and large-data rendering.** No measured TUI data exists, so no budget is claimed.
- **Connected-mode transport.** The outbox, flush and conflict review are documented in
  [`tui-workspace.md`](tui-workspace.md); this page only requires that the same authority answers
  appear in the status line and that no queued intention is shown as accepted.

## Evidence index

M31 captures, all under `web/evidence/i5/`:

| Capture | What it proves here |
| --- | --- |
| [`03-project-overview.png`](../../web/evidence/i5/03-project-overview.png) | Declared view tabs with counts, project switcher, Recent group, `Proposed` status, install toast |
| [`09-task-with-note-link.png`](../../web/evidence/i5/09-task-with-note-link.png) | An absolute link rendered from Markdown; the reason Preview mattered (`I5-04`) |
| [`11-inbox-before-triage.png`](../../web/evidence/i5/11-inbox-before-triage.png) | The one-line capture row, inline status/project/priority controls, keyboard hints |
| [`11b-status-edges.png`](../../web/evidence/i5/11b-status-edges.png) | The edge hint, the Planning group in the drawer, Write/Preview tabs |
| [`13-planning-day-plan.png`](../../web/evidence/i5/13-planning-day-plan.png) | Authored capacity, dispositions, free windows, the sized task selected |
| [`14-planning-applied.png`](../../web/evidence/i5/14-planning-applied.png) | The receipt: fingerprint, command id, block count, revision |
| [`14b-planning-after-reload.png`](../../web/evidence/i5/14b-planning-after-reload.png) | The applied block surviving a reload |
| [`15-task-a-opened.png`](../../web/evidence/i5/15-task-a-opened.png) | Drawer identity, badges, project context |
| [`17-task-a-stale-revision.png`](../../web/evidence/i5/17-task-a-stale-revision.png) | Conflict recovery in place, one inline alert, one reload action |
| [`19-task-a-complete-activity.png`](../../web/evidence/i5/19-task-a-complete-activity.png) | Activity attribution and raw ids in server-composed sentences (`I5-07`) |
| [`22-board-task-b-done.png`](../../web/evidence/i5/22-board-task-b-done.png) | Keyboard completion on the board (`I5-12`) |
| [`24-settings-no-upgrade.png`](../../web/evidence/i5/24-settings-no-upgrade.png) | No upgrade offered when current equals newest |
| [`25-agents-no-jobs.png`](../../web/evidence/i5/25-agents-no-jobs.png) | The M31 empty state's honesty — superseded for launch by M32 I3, kept for the read-only history |

`run.json` keys used above: `identity`, `project`, `moves`, `triage`, `milestones`, `planning`,
`conflict`, `invalidTransition`, `boardMove`, `restart`, `activityOrigins`, `upgrade`, `agentJobs`,
`taskActivity`, `counts`, `steps`.

M32 acceptance records, all under `.agent/evidence/`:

| File | What it proves here |
| --- | --- |
| [`m32-i1-acceptance.md`](../../.agent/evidence/m32-i1-acceptance.md) | Unified search, saved views, project identity editing, task Created/Updated; 19 focused specs, 106 total |
| [`m32-i2-acceptance.md`](../../.agent/evidence/m32-i2-acceptance.md) | Knowledge folders, locations, note links and back-references, typed references, project knowledge, task Notes; the "valid write after a refusal" regression |
| [`m32-i3-acceptance.md`](../../.agent/evidence/m32-i3-acceptance.md) | Agent launch, executor roster, never-claimed warning, claim states, review, resume, 403 ownership, restart durability |
| [`m32-i4-acceptance.md`](../../.agent/evidence/m32-i4-acceptance.md) | Capability offers and availability codes, typed inputs, confirmation, five states, receipt, reconcile, executor-only claim/result, declared effects |

Web sources cited: `web/src/components/navigation/nav.ts`, `components/layout/sidebar.tsx`,
`components/ui/command-palette.tsx`, `components/ui/drawer.tsx`, `app/app-shell.tsx`,
`app/workspace-layout.tsx`, `features/inbox/quick-capture.tsx`,
`features/tasks/task-drawer/{task-drawer,planning-section,description-editor,activity-section,task-timestamps,notes-section}.tsx`,
`features/tasks/{task-controls,use-task-drawer}.ts`, `hooks/use-resource-drawer.ts`,
`features/workflows/{view-registry,work-item-sheet,capability-model,capability-panel,capability-request-history,bundled-workflow-install}.tsx`,
`features/workflows/views/board-view.tsx`, `features/planning/project-planning.tsx`,
`features/projects/{project-workspace-page,project-identity-form}.tsx`,
`features/agents/{agent-model,launch-model}.ts`,
`features/agents/launch/{executors-panel,launch-agent-job-dialog,task-agent-job-section}.tsx`,
`features/agents/agent-job/{review-section,blocks}.tsx`, `features/saved-views/model.ts`,
`api/{queries,client}.ts`, `api/endpoints/{tasks,search,saved-views}.ts`.

Server, runtime and client sources: `crates/orbit-server/src/main.rs` (search `:2245-2266`, project
commands `:5823-5872`, task detail timestamps `:1914-1969,2019-2022`, capability routes
`:4302-4354`, agent routes `:4404-4437`, saved views `:4034-4040`, knowledge reads `:4078-4090`),
`crates/orbit-domain/src/{task,project}.rs`,
`crates/orbit-runtime/src/{search,saved_views,references,blueprint_upgrade,domain_commands,store,lib}.rs`,
`crates/orbit-application/src/{project_commands,task_detail_commands}.rs`,
`crates/orbit-client/src/{lib,contract,view_data,capability_actions,unified,connected}.rs`, and the
TUI sources named per pattern.

Acceptance specs cited: `web/tests/e2e/{search,saved-views,project-settings,task-metadata,knowledge-folders,knowledge-links,project-knowledge,agents-launch,agents,capability-execution}.spec.ts`,
plus the M31 set `quality-keyboard`, `quality-a11y`, `work-items`, `planning-estimates`,
`planning-guard`, `upgrades`, `workflow-install`, `conflict`, `tasks`, `workspaces-registry`.
