# TUI redesign specification

This page is the design input for a future `orbit-tui` redesign. It is derived from the M31 I5
web dogfooding cycle, not from taste: every pattern below names the evidence that it mattered to
a real person doing real work, and the terminal translation that preserves the reason it worked.
It changes no product code, and it does not move domain rules into the terminal.

Status: design input, not an implemented design. Scope: `docs/**` only.

The two clients are strong in different places. The web app has the more mature *connected*
contracts — a domain-backed edge read, a version-aware install, a reviewed upgrade, a
fingerprinted planning receipt. The TUI has the more mature *local* contracts — direct authority,
capability execution, a deep keyboard model, session restore. The redesign should close the gap
through shared commands and reads, not by teaching the terminal to imitate a browser.

**Contents:** [Method](#method) · [Pattern inventory](#pattern-inventory) ·
[Patterns](#patterns) · [Non-goals](#non-goals) ·
[What the web app still owes the TUI](#what-the-web-app-still-owes-the-tui) ·
[Acceptance](#acceptance-for-a-future-redesign) · [Exclusions](#what-this-spec-does-not-cover) ·
[Evidence index](#evidence-index)

## Method

### Observation source

| Fact | Value |
| --- | --- |
| Run record | [`web/evidence/i5/run.json`](../../web/evidence/i5/run.json), summarised by its generated [`README.md`](../../web/evidence/i5/README.md) |
| Application commit | `987bcf15f0597252338c42bc2c42ab0ce8fdac13`, clean tree at capture |
| Built SPA | `e34b3a77acb03530e098e1b91b51def0aaafe3b07a6d816ea2f356c80e7bd26d` (16 files) |
| Server binary | `4ac3f39141a436fbeb379f69a871311a10c96a04ed1f2443149ac770ec96de1d` |
| Workflow | Development v3 (`development` v3) |
| Project | Orbit Development, status `proposed` |
| Content | 19 tasks, 2 milestones, 1 Work Item, 1 note, 1 attachment, 1 schedule block |
| Capture window | 2026-10-26T04:03:34.206Z → 2026-10-26T04:04:48.902Z |
| Viewport | Chromium 1440×900, light theme, headless |
| Reproduce | `cd web && node tests/e2e/i5/dogfood.mjs` |

The README is generated from `run.json`; neither is hand-edited. An earlier evidence set was a
hybrid of two runs (Gate 5 B1) and was regenerated from one clean run; the committed set names
one commit, one `dist`, one server binary and one workflow version. The issue log's fix pass is
part of the same record: `972a09d`, `3f4101f`, `0ff8af9`, `1ae1600`, `987bcf1`, `dfcfb8a`. This
spec reads the **fixed** build, so a finding that the first run hit and a later commit closed is
described as a lesson about the surface, not as a current defect.

### How patterns were chosen

A pattern entered this page only if at least one of these is true:

- **U — used in the cycle.** The run changed state through it: a row in `moves`, `triage`,
  `planning`, `conflict`, `boardMove` or `restart`, or a step in `steps`.
- **A — asserted by an automated check.** The dogfood script asserts it, or a Playwright spec
  runs it against a real server and the built client (`quality-keyboard.spec.ts`,
  `quality-a11y.spec.ts`, `agents.spec.ts`, `work-items.spec.ts`, `planning-estimates.spec.ts`,
  `planning-guard.spec.ts`, `upgrades.spec.ts`, `workflow-install.spec.ts`, `conflict.spec.ts`,
  `tasks.spec.ts`, `workspaces-registry.spec.ts`, and the unit test
  `quick-capture-idempotency.spec.tsx`).
- **F — recorded as friction.** The issue log names it with an id (`I5-01` … `I5-17`), and the
  fix pass says what changed.

A pattern with only **F** is still evidence: it is a place where the first design sent a person
looking for a control that did not exist. A pattern with only **A** is evidence that the surface
is exercised, not proof that it felt right; those rows are marked conservatively.

### Classification scale

| Class | Meaning | What the redesign does |
| --- | --- | --- |
| **1 — Must match** | The reason the pattern worked is the information and the rules it exposes, not the browser's pixels. Removing it would remove a capability. | Reproduce the same facts, outcomes and refusals in terminal form. |
| **2 — Adapt** | The invariant survives, the interaction does not. A browser mechanism (URL, focus ring, live region) has a different terminal equivalent. | Keep the invariant; replace the mechanism; document the equivalent. |
| **3 — Not appropriate** | The pattern depends on a browser capability a terminal cannot honestly provide, or it would add a second renderer for the same content. | Reject it, and keep whatever fact it was protecting visible another way. |
| **4 — Requires shared domain/API changes** | The terminal cannot match the pattern without a change below the UI: a missing read, command or migration rule. Implementing it locally would create a second rule. | File the shared change; do not compensate in the TUI. |

**Counts across the twelve patterns in this page:** class 1 — 7; class 2 — 3; class 3 — 1;
class 4 — 1. One pattern is class 4 on purpose: it is the only place where a faithful translation
is blocked by a missing shared capability, and pretending otherwise would produce exactly the
duplication the scale forbids.

### What the evidence can and cannot prove

- **I5 exercised no TUI surface.** The issue log says so explicitly: “No TUI surface was
  exercised, so there is no TUI-specific finding.” Every TUI claim here is therefore read from
  [`tui-workspace.md`](tui-workspace.md) or the TUI source; it is a statement about the documented
  design, not a new test result.
- **No agent review was exercised.** The run could not create an agent job without a runtime
  (`I5-16`); `run.json` records `agentJobs.created: false`, `visibleJobs: 0`. The agent-review
  pattern rests on the empty-state copy, the code paths, and `agents.spec.ts`, which does exercise
  review and asserts the task status is unchanged.
- **No upgrade was available.** Current and newest published version were both 3 (`run.json` →
  `upgrade`), so “no upgrade offered” is the correct outcome and not a missing surface. The
  upgrade pattern is proven by the acceptance spec and by the v3 migration notes, not by this run.
- **One viewport, one theme, one locale.** The clipped 12-hour time fields (`I5-13`) show why a
  single desktop capture is not a layout proof.
- **No performance data.** This page makes no claim about TUI latency or large-project rendering.

## Pattern inventory

| # | Pattern | Class | Inclusion | Primary evidence |
| --- | --- | --- | --- | --- |
| 1 | Navigation and project switching | 1 | U | `03-project-overview`, `11-inbox-before-triage`; `nav.ts`, `sidebar.tsx`, `workspace-layout.tsx` |
| 2 | Quick capture | 1 | A | `11-inbox-before-triage`; `quick-capture.tsx`, `tasks.spec.ts`, `quick-capture-idempotency.spec.tsx` |
| 3 | Task editing and planning fields | 2 | U, F | `11b-status-edges`, `13-planning-day-plan`; `I5-01`, `I5-09`; `run.json` → `planning` |
| 4 | Transitions, guards and capability chips | 1 | U, A, F | `run.json` → `invalidTransition`, `boardMove`; `22-board-task-b-done`; `I5-11`, `I5-12` |
| 5 | Workflow view navigation and unavailable views | 1 | U, A | `03-project-overview`; `view-registry.tsx`, `workspaces-registry.spec.ts` |
| 6 | Inspector, focus return and deep links | 2 | U, A, F | `15-task-a-opened`, `17-task-a-stale-revision`; `quality-keyboard.spec.ts`; `use-resource-drawer.ts`; `I5-05` |
| 7 | Search and command palette | 1 | A | `command-palette.tsx`, `workspace-layout.tsx`; `quality-keyboard.spec.ts` |
| 8 | Planning review, approval and receipt | 1 | U, F | `13-planning-day-plan`, `14-planning-applied`, `14b-planning-after-reload`; `run.json` → `planning`; `I5-01`, `I5-10` |
| 9 | Agent review | 1 | F, A | `25-agents-no-jobs`; `run.json` → `agentJobs`; `review-section.tsx`, `blocks.tsx`; `agents.spec.ts`; `I5-16` |
| 10 | Keyboard accessibility and announcements | 2 | F, A | `17-task-a-stale-revision`, `22-board-task-b-done`; `quality-keyboard.spec.ts`, `quality-a11y.spec.ts`; `I5-12`, `I5-17` |
| 11 | Workflow upgrade, fingerprint and default backfill | 4 | F, A | `24-settings-no-upgrade`; `run.json` → `upgrade`; `I5-03`; `upgrades.spec.ts` |
| 12 | Markdown write/preview split | 3 | U, F | `09-task-with-note-link`, `17-task-a-stale-revision`; `I5-04` |

## Patterns

### 1. Navigation and project switching — class 1

**Evidence.** Five global destinations are first-class in the web shell — Home, Inbox, Projects,
Knowledge, Agents (`web/src/components/navigation/nav.ts:18-24`) — and the cycle used the Home →
Inbox path to triage an unassigned task. `03-project-overview.png` shows the sidebar's **Recent**
project group, the project switcher beside **New task**, four declared view tabs with count badges
where a count is known, and the two project tools. The shell also registers every project by name as a command,
so a project is reachable from the palette as well as the sidebar (`workspace-layout.tsx:61-125`).
Recent projects are browser-local (`orbit.recent-projects`), not authority state
(`docs/web-app.md`, Sign in).

**Honest limitation in the web build.** The sidebar accepts a `pinned` flag and renders a
**Pinned** group (`sidebar.tsx:91-93`), but the shipped shell passes only `recent`
(`workspace-layout.tsx:51-59`), so that group is always empty. Do not copy “Pinned” into the TUI
until something actually produces the flag.

**Terminal translation.** Keep the five destinations and make the project's identity part of the
frame: name, status, pinned Workflow version, and a stale marker when the last read failed. The
TUI already breadcrumbs the project and pins `[stale]`; the redesign keeps that. Add the web's
project switcher as a named palette entry per project rather than a drop-down: `Open project:
<name>` is searchable, needs no pointer, and works at 80 columns. Recent projects must be session
memory shared by the sidebar and the palette, never a new file: the TUI already remembers recent
palette destinations (up to five), and `tui-state.json` already restores the active project and
view by identifier, which is stronger than a browser's recent list.

Switching projects should preserve the *view id* when the target workflow declares it, and fall
back to the first declared view with a one-line notice when it does not. That is what the web
does by navigating to the project root and resolving the first view; the terminal can be explicit
about the substitution instead of silently landing somewhere else.

**Why class 1.** Every fact here — which destinations exist, which project is open, which views
it declares, which project was opened last — is navigation state a person needs. None of it
depends on a sidebar.

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

The cycle did not create its 18 seeded tasks through this row — it used the **New task** dialog
(`run.json` → `steps`) — so this pattern is **A**, not **U**. That is a limitation of the run,
not of the pattern: capture is the fastest path for one thought, and the web guide documents it as
the Inbox's primary action.

**Terminal translation.** Put a persistent one-line capture field at the top of the Inbox, not
behind a form:

```text
+ Capture a task for the Inbox…                                  Enter capture
```

`c` focuses it; `Enter` submits; `Ctrl+U` clears; focus stays in the line after an accepted
capture so a second thought can follow. The status line owns the feedback: `Sending…`, then
`Captured “<title>” in the Inbox`, then the new row is selected. A rejection keeps the typed text
and prints the server's sentence directly under the line; an uncertain outcome says so and
offers a same-id retry, never a new task id. Do not add project, priority or date fields here:
those belong to the row after capture, which is exactly how the cycle triaged the captured task
(`run.json` → `triage`, Inbox inline project and priority controls).

**Why class 1.** One line, one decision, no navigation. A terminal is better at this than a
browser, and the invariants — accepted-only clearing, same-id retry, in-place refusal — are what
make it safe.

### 3. Task editing and planning fields — class 2

**Evidence.** The first real cycle could not size a task at all: no surface called `setPlanning`,
so Review stayed disabled and no plan could be applied (`I5-01`, blocker). After the fix, the
drawer's **Planning** group authors estimate, earliest start and deadline as **one**
`set_planning` command with the usual revision fence; an empty field clears; a refused command
resets each draft to the stored record; and the group registers with the close guard so `Esc`
flushes a draft instead of discarding it (`planning-section.tsx:1-30,121-143,225-231`;
`task-drawer.tsx:110-123`). The same cycle found copy promising a deadline before any surface
offered one (`I5-09`), which is the same defect in words.

`11b-status-edges.png` shows the Planning group inside an open drawer, with the stored-value hint
under it; `13-planning-day-plan.png` shows the sized task selected in the day plan at `1 h`.
`run.json` records the capacity that made the gate pass (`capacityMinutes: 240`) and the applied
block.

**Terminal translation.** The TUI already has `e` (full edit, including estimate, earliest start
and deadline) and `E` (title and priority only), and its inspector shows planning properties.
Keep that split. What must change is the commit semantics:

- One save writes the three planning fields as **one** command. A terminal form has no natural
  “leave a field” event, so do not invent per-keystroke writes; `Ctrl+S` from anywhere is the
  commit.
- The displayed value is always the *stored* value once a command settles. A refusal restores
  the stored triple and prints the authority's sentence beside the field; it must not leave the
  refused draft visible as if it were saved.
- Empty means clear. A blank estimate is not “0”.
- `Esc` on a dirty form asks before discarding; after a successful save the inspector reopens on
  the same entity with the accepted values (both already true in the TUI).
- Copy lists only fields the form shows. The I5-09 failure was a promise in prose with no control
  behind it.

**Why class 2.** The invariant is field-level truth and one fenced command; the browser's
blur-to-save is the mechanism that does not survive. The TUI's existing “Save Ctrl+S · Cancel Esc”
form model is the correct equivalent.

### 4. Transitions, guards and capability chips — class 1

**Evidence.** Task status controls offer only the edges the authority reports as legal: the web
reads `/v1/authoritative/task-transitions` per visible status, caches it for five minutes, and
falls back to every status with the server still deciding if the read fails
(`web/src/api/queries.ts:279-306`, `task-controls.tsx:15-29`). The server's own comment calls that
route a read of `TaskStatus::can_transition_to`, not a second transition table
(`crates/orbit-server/src/main.rs:2172-2196`). The cycle's probe is unambiguous: the control did
**not** offer `inbox → done`, the offered set was Inbox, Planned, Ready, Deferred and Cancelled,
and the deliberate probe got `400 invalid task status transition: inbox -> done`
(`run.json` → `invalidTransition`; `I5-11`). The board's Move menu uses the same edge set; an
accepted move refocuses the card in its new column and a refusal announces the server's sentence
without moving anything (`board-view.tsx:144-186`).

For Work Items the rule is different and equally explicit: the sheet lists exactly the
transitions the kind declares out of the current state, prints each declared guard as a sentence,
marks a property guard satisfied or not from the item's own values, says that a relationship
guard is left to the server, and never disables the move because of a guard. A required
capability is a visible **Requires capability …** chip with the sentence that browser sessions are
granted none (`work-item-sheet.tsx:252-337,344-352,603-654`).

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
- For Work Items, keep the declared-but-guarded rows visible. The web deliberately does not
  disable them, because the authority re-checks and a property guard the client can see is not
  the whole rule.
- Capability chips **adapt** (class 2 facet): the web can only say “not executable here”, while
  the TUI can execute a capability action through the local authority. Keep the chip and add the
  TUI's offer/deny code and reason, and keep the rule that an unavailable offer stays visible,
  dimmed, and does not run on `Enter`.

**Why class 1.** “Only legal edges are offered, and the authority's refusal is quoted where the
choice was made” is the capability. It is the difference between discovering a lifecycle by trial
and error and reading it.

### 5. Workflow view navigation and unavailable views — class 1

**Evidence.** The web chooses a renderer from the component *type* a view declares, never from the
Workflow id, so five bundled Workflows share one code path (`view-registry.tsx:1-12,71-96`). A
declared type the build cannot render still has a renderer: an explicit panel that names the
declaration and why it is missing (`HabitTrackerView`, `MetricHistoryView`,
`UnknownComponentView`). A `group_by` on a non-board is refused by name rather than dropped
(`:114-132`). Reserved view ids cannot shadow the **Planning** and **Settings** tools; the
workflow is told its view was not rendered and why (`project-workspace-page.tsx:214-220,454-468`).
The header action comes from the open view (`:238-246`). `03-project-overview.png` shows the
declared tabs with counts plus the two tool tabs.

**Terminal translation.** Keep view selection by **id** (`1`–`9`, `[`/`]`, and the palette), not
by index, because workflows reorder views and a reordered tab must not move the person. Three
rules carry the web's honesty:

1. **Named unavailability.** A component type with no renderer prints the declaration and the
   reason: `habit_tracker — habit tracking is not rendered in this build`, not `unavailable`.
   The TUI already says a component is unavailable and keeps the rest of the view usable; the
   redesign upgrades the sentence to name the type and, where the manifest declares a window,
   the window.
2. **No blank surface.** A view with no components, a workflow that declares no views, and a
   requested view id that does not exist each get their own sentence. The last one names the id
   and offers the first declared view.
3. **Tool collision is visible.** The TUI's equivalents of the web's reserved ids are its keys
   and actions (`W` Workflow, `p`/`P` planning). A declared view or action that would take one
   of those must be refused with the reason, not silently win or lose.

Counts on tabs follow the web's rule: show a count only when it is honest. A readiness-backed
badge is absent until the readiness read returns; the TUI should do the same rather than print
`0` for “not known yet”.

**Why class 1.** “The workflow's declaration decides what exists; the build says what it cannot
draw” is the contract that keeps a Work Item workflow portable. A terminal that invents its own
view list breaks it.

### 6. Inspector, focus return and deep links — class 2

**Evidence.** The web drawer is URL state: `?task=<id>` can be bookmarked, survives a reload and
Back closes it (`docs/web-app.md`, The task drawer). The row that opened it is remembered by a
data attribute so focus returns to that row's title control after the drawer unmounts; closing
uses `replace` so Back does not reopen a closed drawer (`use-resource-drawer.ts:41-79`). When a
deep-linked drawer has no trigger element, focus falls back to the main region
(`drawer.tsx:42-65`). A conflict is recovered inside the drawer (“Reload this task and try
again”), which refetches the task and snapshot and clears the rejected draft; the cycle proved
this with one alert and one reload control and `recovered: true` (`run.json` → `conflict`;
`I5-05`; `17-task-a-stale-revision.png`). `quality-keyboard.spec.ts` asserts the drawer's
live-region save announcement and focus return.

**Terminal translation.** The TUI's inspector is already an overlay, never a third column, and it
already returns focus to the pane that owned the keyboard. Keep that. The redesign must preserve
four functions the URL provides in the browser, without the URL:

- **Stable identity.** The open item is a stable id, not a row index. `tui-state.json` already
  restores the selected item's id; an item that disappeared is reported with its last known
  title and kind, and `r` re-checks.
- **Reopen after restart.** Restoring the project, view and selected id is the terminal's
  equivalent of pasting the same `?task=` URL into a new tab.
- **Focus return.** Closing returns to the same row and the same pane, and the selection and
  scroll do not jump. This is the strongest accessibility property in the web drawer and the TUI
  already implements it; a redesign must not regress it.
- **An id handoff.** A person must be able to open a task by the id another person gave them,
  through the palette (`Open task <id>`) or the console, and land in the same inspector. Do not
  build a second detail screen or a “detail route”; one inspector, one identity.

**Why class 2.** The URL and the Back button are browser mechanisms that do not translate; stable
identity, reopenability, focus return and recover-in-place do. The terminal keeps the functions
and drops the mechanism.

### 7. Search and command palette — class 1

**Evidence.** The web palette lists only commands that are implemented, with two honest empty
states — no commands at all, and no match for the query — plus the populated list. `Enter` runs
the highlighted command, `↑`/`↓` move, `Esc` closes
(`command-palette.tsx:44-50,70-89,121-136`).
The command list is exactly new task, new project, the five destinations, and one entry per
project (`workspace-layout.tsx:61-125`). `/` focuses a page's own search field when one exists and
otherwise opens the palette (`app-shell.tsx:92-102`). There is deliberately **no global search**
over tasks, projects and notes; knowledge search is vault-only (`docs/web-app.md`, Not available
yet). The cycle did not use the palette, but `quality-keyboard.spec.ts` asserts open, focus,
Escape and focus return.

**Terminal translation.** The TUI already has the stronger version of this pattern: one action
catalogue shared by the action menu and the palette, deterministic ranking, recent destinations
first, disabled rows kept with their reason, and `?` help. The redesign should keep it and adopt
the web's honesty rules:

- Nothing appears in the palette that cannot run on the current screen or authority, unless it is
  visibly disabled with the reason. A row that looks runnable and then refuses is the palette
  version of `I5-11`.
- `Ctrl+U` clears the query; `Esc` closes and returns focus to where it was.
- The palette's vocabulary matches the surface's: **Workflow**, **Work Item**, **Workflow
  Action**, **state**, **transition**, **guard**, **capability**.
- Do not advertise “search everything”. If a query only searches the current view, the vault or
  recent destinations, say which.

**Why class 1.** “Every implemented action is findable by name, and nothing false is advertised”
is a capability, not a widget. A terminal palette is the natural home for the project switcher
and the id handoff in pattern 6.

### 8. Planning review, approval and receipt — class 1

**Evidence.** The web planning surface separates three steps. **Review this plan** is read-only
and returns the fingerprint and command id of the exact proposed command; it writes nothing.
**Apply approved plan** exists only after a review, stays disabled until the approval box is
ticked, and echoes the reviewed fingerprint and command id unchanged. The receipt shows the
accepted revision, the replayed flag and the schedule-block ids. A new window or a changed
revision discards the review and asks for another (`project-planning.tsx:459-465,630-661,
675-833,909-925`). The server refuses a stale fingerprint, or a plan the review no longer
describes.

The cycle's committed facts: 240 authored minutes, 2 day-plan rows, 1 proposed slot read before
review, review enabled, fingerprint `e44ba459…bbab08a0`, command id `a6e476d5-…-d1de0738c02d`,
one block `05c1620f-…`, applied, and still present after reload (`run.json` → `planning`).
`13-planning-day-plan.png`, `14-planning-applied.png` and `14b-planning-after-reload.png` show
the preview, the receipt and the surviving block. The first run could not reach the gate
(`I5-01`), and the disabled state did not say what to fix (`I5-10`); both are lessons about the
empty and disabled states, not about the gate.

**Terminal translation.** The TUI's connected approval path already refuses an approval that is
not read-only, is fenced to another revision, names another command, or omits reviewed slots
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

- The day plan is authored capacity plus explicit options. `include_unsized` /
  `default_minutes` are a named choice, never a silent guess; dispositions and explanations are
  the server's, printed verbatim per row, including `missing_estimate`.
- Review is read-only and prints the fingerprint, command id, the revision it is fenced to and the
  exact reviewed slots. It must be obvious that nothing was written.
- Apply requires an explicit confirmation that echoes the fingerprint and command id. A changed
  revision or window discards the review and says so.
- The receipt prints the fingerprint, command id, accepted revision, replayed flag and block ids,
  and the blocks are visible in the timeline after a refresh.
- The disabled state names the missing input and offers the first unsized task. The TUI's local
  planning is read-only simulation and says so; local scheduling through ordinary commands is
  not a substitute for a reviewed apply, and must at least report the blocks and revision it
  changed.

**Why class 1.** “The write is bound to a review the person saw” is the safety property. It is
the difference between approving a plan and pressing a button that might apply a different one.

### 9. Agent review — class 1

**Evidence.** The web offers no launch control anywhere; the overview says an executor or the
command line starts a job and the browser never does (`run.json` → `agentJobs.overviewCopy`;
`25-agents-no-jobs.png`). A review records **one** decision against the latest persisted handoff,
with a required reason. Approval is a write, not a completion: the page says so before the form,
and the recorded review carries a **Task not completed** badge and the task's current status
(`review-section.tsx:102-133,135-184`). A conflicting second decision is refused by the server and
this page does not offer one. An unconfirmed review offers retry-same-attempt or refresh; a
refresh that finds no review discards the pinned attempt so a different decision can be recorded.
A job the device does not own answers `403`, and the page says the job is not visible to this
device, with the id, without confirming whether it exists elsewhere (`blocks.tsx:33-55`;
`agent-job-page.tsx:95`). `agents.spec.ts` asserts the task status is unchanged after approval.

**Evidence limit.** I5 could not create a job without a runtime (`I5-16`), so no review was
exercised in the cycle; this pattern rests on the empty state, the code and the acceptance spec.

**Terminal translation.** Keep the web's honesty, not its layout:

- List jobs grouped by attention first, then by state; each row names the task (or a short job
  id), project, state, handoff count and next action.
- The job inspector shows state and revision, runtime references as labels (not a live
  connection), handoffs verbatim, the recorded review, and the task effect sentence: `Review
  recorded; the task is still In progress`.
- The review form requires a reason and records one decision. After recording, print
  `Review recorded as approved. The task was not completed.` and re-read the job.
- No launch control. If a resume action exists, it queues a new job and says that an executor
  must claim it; it must not imply a run started.
- `403` is rendered as ownership, not absence: `This job is not visible to this device. The
  authority answered 403 for <id>.` Never map it to “not found”.
- Capability execution is separate from review. A review decision is not a capability run and
  never advances the task.

**Why class 1.** “Approval is a recorded decision, not task acceptance, and a browser/TUI never
starts an agent” is a product boundary. It must read the same in both clients or the audit trail
stops meaning the same thing.

### 10. Keyboard accessibility and announcements — class 2

**Evidence.** The web's quality gates are unusually concrete. `quality-keyboard.spec.ts` walks the
tab order, asserts a visible focus indicator at every stop, the skip link, the navigation drawer's
trap/Escape/return, the palette's open/Escape/return, board moves with `m` + arrows + `Enter`,
planning review and approval without a pointer, and the drawer's live-region save announcement.
`quality-a11y.spec.ts` runs axe over every primary route in both themes, and explicitly leaves
judgement calls (focus visibility, focus order, the meaning of an announcement) to the keyboard
test. Command outcomes are announced in a live region; status is text plus a badge, never colour
alone (`docs/web-app.md`, Accessibility and theme).

The friction is equally instructive. In the first cycle, completion from `Ready` took two moves
and nine ArrowDown presses with no mnemonic (`I5-12`); the regenerated run records four presses
with the same two legal moves. The two-move shape is the domain's, not a defect; the press count
is a menu-ergonomics question, and it should be measured, not assumed to have improved because
the number changed. A refusal could sit beside the previous success sentence, and the footer still
carried the neutral save hint next to the error (`I5-17`); the fix hides that hint when a command
is refused or unconfirmed.

**Terminal translation.** A terminal has no screen reader and no live region; it has a status
line, and that line is the announcement channel. The redesign must make it complete:

- Every accepted, pending, rejected, stale and unavailable outcome appears in the status line,
  in the same words as the action, and stays until the next command (not a timed toast). `I5-17`
  is the failure mode: two contradictory sentences at once.
- No meaning by colour. Focus is a thick border plus a bright title; selection is `▸`; state is a
  glyph plus a label. The TUI already does this, and `NO_COLOR`/`TERM=dumb` must stay legible.
- Every action bar entry names a real key; a key is advertised only when it identifies the row
  uniquely. A disabled row keeps its reason and `Enter` reports it without running.
- Shortcuts are ignored while typing; `Ctrl+Q`/`Ctrl+C` always quit; `?` lists only implemented
  keys.
- Completion from `Ready` stays two moves because the domain forbids `Ready → Done`. Do not add a
  `d` mnemonic that pretends otherwise; make the first move cheap (the status menu on the focused
  card, legal edges only) and say why the second exists.
- At 80×24 and at 120×40, no value needed for a decision may be clipped. The web's clipped
  12-hour fields (`I5-13`) are the cautionary example: the stored value was right, the field was
  unreadable.

**Why class 2.** The goal — every action operable without a pointer, every outcome perceivable,
focus never lost — survives. The mechanisms (skip link, focus ring, live region, axe) do not.

### 11. Workflow upgrade, fingerprint and default backfill — class 4

**Evidence.** The web's Settings drives a reviewed upgrade: choose a target, get a read-only
preview with fingerprint, view and kind diffs, blockers and notices; apply behind a confirmation
that repeats the preview; a changed fingerprint refuses; the receipt names previous → new
version and the previewed fingerprint (`docs/web-app.md`, Workflow upgrades). The runtime fills
new required-field defaults into live Work Items **inside the upgrade savepoint**
(`crates/orbit-runtime/src/blueprint_upgrade.rs:310-405`), so the project is valid under the new
schema immediately. This is what made Development v3 safe for existing v2 projects after the
Feature kind gained a required `title` (`I5-03`; fix `3f4101f`).

The TUI has no upgrade entry point at all: its `W` picker and the palette's version-binding entry
pin a version, which is not an upgrade, and no `upgrade` path exists in `orbit-tui`. The run
offered no upgrade because current and newest were both 3 (`24-settings-no-upgrade.png`,
`run.json` → `upgrade`), so I5 could not exercise the flow; `upgrades.spec.ts` does.

One open edge matters for a redesign: v3's default `title` is also a *creation* default, so two
untouched Features can share the name “Untitled feature” and differ only by id (remaining I6
item; `docs/web-app.md`, known rough edges). A migration default and a creation default are not
the same thing.

**Terminal translation.** Not possible honestly today, and that is the classification. A TUI
upgrade must come from the same shared command as the web's: preview → fingerprint → explicit
confirm → apply, with the default backfill inside the same transaction, and a separate creation
default so a new Work Item is not born with the migration name. Implementing the diff, the
fingerprint or the backfill in the TUI would create a second schema engine.

**Until the shared path exists:** `W` may pin an already-published version and must say that this
is a re-pin, not an upgrade; it must not silently move a project to a newer bundle when the binary
changes; and it must not fill defaults itself. The version table in the TUI guide is already
stale — it lists Development v2 while the binary bundles v3 (`crates/orbit-blueprint/src/bundled.rs:16-17`)
— which is exactly the drift a shared, version-aware path removes.

**Why class 4.** The missing piece is below the UI: a shared upgrade command and a migration rule.
This is the one pattern where the redesign is blocked, and the block belongs in the upstream
backlog, not in a terminal workaround.

### 12. Markdown write/preview split — class 3

**Evidence.** The task description editor is a tabbed **Write** / **Preview** field; Markdown is
rendered in Preview; only absolute `http(s)` links are live and they open a new tab
(`description-editor.tsx`; `docs/web-app.md`, The task drawer). The cycle needed Preview to
discover that a relative `/knowledge/...` link stayed plain text while an absolute loopback URL
became a link (`I5-04`; `09-task-with-note-link.png`; `17-task-a-stale-revision.png` shows the
Write tab).

**Terminal translation.** Reject the preview pane. A terminal renders text; a second rendered
view would need a Markdown layout engine and would still not make a link clickable. The TUI
should show raw Markdown in a multi-line field (it already does) and, where a read-only note view
exists, print the source with its structure intact rather than imitating a browser.

What must be kept is the *fact* the preview was protecting: a relative in-app link is not live,
and an absolute URL is printed as a URL, not silently linked. Say that once in the TUI's help or
in the description field's hint, and keep ids and paths copyable. Do not add a preview tab to
prove it.

**Why class 3.** It is a browser affordance for rendering, not a capability of the task. The
capability — “you can see what the stored text will look like to a reader” — is served in a
terminal by predictable plain text, which is what the TUI already shows.

## Non-goals

These are explicit. A redesign that adds any of them is out of scope.

- **Modal-heavy flows.** No stacks of confirmation dialogs, no per-field popovers, no dialog for
  something a single overlay or a status line can do. The web's upgrade flow has three steps
  because a workflow change is destructive and fingerprinted, not because three dialogs are good.
  The terminal equivalent is one overlay with one explicit confirm that repeats the fingerprint.
- **Pointer or hover-only affordances.** No hover menu, no tooltip-only explanation, no
  drag-and-drop. The web already refuses drag-and-drop on the board for the same reason: keyboard
  parity was the requirement for offering a pointer gesture. The palette's `onMouseEnter`
  highlight is an accelerator, never the only way to select a row.
- **A second copy of any domain rule.** No transition table, no readiness or blocking
  recomputation, no plan fingerprint, no upgrade diff, no default duration, no backfill, no
  “is this project status transition allowed” check in the TUI. Read the domain or ask the
  authority; quote the refusal.
- **A terminal re-implementation of a browser mechanism.** No skip link, focus ring, live region,
  theme toggle, responsive drawer or axe audit. Their *intent* is covered by status-line
  announcements, border-and-title focus, glyphs plus labels, and a width-tested layout.
- **Claims the authority cannot back.** No launch control for an agent job, no capability
  execution claim in connected mode, no “plan approved” without a review, no “saved” before the
  authority accepts, no “not found” for a 403.
- **A second detail surface.** One inspector, one identity, one focus-return path.

## What the web app still owes the TUI

These are shared domain/API follow-ups that building the web app exposed. They are listed here
because a TUI redesign would otherwise be tempted to compensate for them locally. Each item names
the layer that owns it; none of them is a TUI rendering task.

### 1. Transition-edge read for connected clients

**Evidence.** The web reads the authority's edges per status
(`/v1/authoritative/task-transitions`) and falls back to every status when the read fails
(`queries.ts:279-306`, `task-controls.tsx:15-29`). The route is implemented in `orbit-server` and
tested there; the shared client exposes only a locally compiled derivation
(`orbit_client::allowed_task_statuses`, `view_data.rs:55-74`), and the TUI builds its status menu
from that derivation (`backend.rs:1369-1371`). In connected mode the TUI therefore offers the
**binary's** edges, not the **authority's**.

**Owed.** One shared edge read that asks the authority in connected mode and delegates to the
domain locally, with one documented fallback (offer all statuses, keep the server's refusal). The
fallback must be a contract, not a habit in each client.

**Until then.** The redesign keeps `S`, the board and the inspector on the one shared derivation
and must not add a third list. In connected mode, a refusal that names an edge the TUI offered is
a version-skew defect to report, not to hide.

**Done when.** A TUI binary older than its server offers exactly the server's edges in connected
mode; a failed read offers all statuses with a visible warning; no view contains a literal status
list.

### 2. Version-aware Workflow install in connected mode

**Evidence.** The web lists the workflows *the server ships* (id, version, name), offers Install
or Update by comparing the stored version, publishes an immutable new revision, and invalidates
the list so the choice appears without a reload
(`bundled-workflow-install.tsx:37-53,67-81`; `03-project-overview.png` shows the
“Workflow installed — Development v3 is now available” toast). A clean server can be given
Development from the New project dialog (`run.json` → `steps`; `I5-02`). The connected TUI instead
returns its **compiled-in** descriptors and refuses install by pointing at the connected console
(`crates/orbit-tui/src/backend.rs:2509-2524,1826-1837`), and its guide still lists Development v2
while the binary bundles v3.

**Owed.** Expose the server-scoped bundled list and install through the shared client, and return
the published version. Generate the version table from the descriptor rather than maintaining it
by hand; show `installed v2 → v3` when the store is older.

**Until then.** The redesign prints the version it actually installed and never repeats a
hand-written version table. A connected session says that install is an authority action, not a
local compile.

**Done when.** A clean connected server can be given a bundled Workflow from the TUI without the
console, the picker shows the version the server now holds, and no document lists a bundled
version as a constant.

### 3. Upgrade preview and default backfill on the shared path

**Evidence.** The runtime's upgrade applies the binding and fills target defaults in one
savepoint (`blueprint_upgrade.rs:310-405`); the web drives preview, fingerprint, confirmation and
receipt (`docs/web-app.md`, Workflow upgrades). The TUI has no upgrade path and no fingerprint,
and the v3 creation default lets untouched Work Items share a name (remaining I6 item; `I5-03`).

**Owed.** One shared upgrade command with preview, fingerprint and apply, consumed by web and
TUI; a creation default that is distinct from the migration default; and a receipt that names the
backfilled fields. Until the defaults are separated, an upgrade can still leave two items
indistinguishable.

**Until then.** `W` pins a published version and says `re-pin`, not `upgrade`. No local diff, no
local fingerprint, no local backfill.

**Done when.** A v2 project upgrades to v3 from the TUI through a fingerprinted preview, the live
Feature gains its default title, the receipt names what was backfilled, and a newly created
Feature has a distinct name.

### 4. Humanized activity summaries

**Evidence.** The server composes `Task <id> <fields> updated`
(`crates/orbit-application/src/task_detail_commands.rs:70`) and `Task assigned to milestone <id>`
(`crates/orbit-runtime/src/store.rs`, `TaskAction::AssignMilestone`), and the attachment summary
embeds the task id. The web renders the sentence verbatim (`activity-section.tsx:58`); the TUI
does the same (`crates/orbit-tui/src/render/modals.rs:394`). `19-task-a-complete-activity.png`
shows the raw 36-character ids next to the drawer header's short id; the run's `taskActivity`
records `Attachment task-a-evidence.md added to task cbaeca85-…` (`I5-07`).

**Owed.** Compose the sentence with the entity's title or name, or emit a structured subject
(kind, id, changed fields) and let each client format it. Keep ids in machine-readable fields,
never in the human sentence.

**Until then.** Neither client should regex an id out of the server's sentence. The surrounding
surface names the subject; the activity row need not.

**Done when.** Activity in both clients names the task and milestone, no 36-character id appears
in a human sentence, and a test asserts the absence.

### 5. Project status transitions on the HTTP authority

**Evidence.** A worked project with 19 tasks and two completions still read **Proposed**
(`run.json` → `project.status`; `03-project-overview.png`; `I5-08`), because the HTTP project
command is create-only (`ProjectCommand`, `crates/orbit-runtime/src/domain_commands.rs:128-139`;
`main.rs:2149-2162`) even though the application layer has `transition_project`
(`project_commands.rs:105`). The TUI's **local** backend already exposes `set_project_status` and
filters its choices with `ProjectStatus::can_transition_to` (`app/forms.rs:271`); the connected
backend refuses and points at the console (`backend.rs:1710-1717`). So the local TUI can do what
the connected TUI and the web cannot.

**Owed.** Mount the fenced project-transition route, add the shared client method, and offer the
same lifecycle in web Settings and the TUI project form. Do not invent a second status model or
a client-side “active” badge.

**Until then.** The badge is stored state and the UI says so, rather than offering a dead control
(the web's current behaviour).

**Done when.** A project created and worked on can be activated from either client; the badge
survives a reload; a stale revision is refused; both clients show the same status and the same
allowed next states.

## Acceptance for a future redesign

Measure the redesign against these. “Pass” requires the evidence named in the last column; taste
is not a criterion. Run the checks at 80×24 and 120×40, with colour and with `NO_COLOR=1`, and
record the commit, binary hash, Workflow version and terminal sizes with the results.

| # | Criterion | How to measure | Web evidence it mirrors |
| --- | --- | --- | --- |
| A1 | The five destinations are reachable with a `g` sequence, and every project is reachable by name through the palette; the header names project, status and pinned Workflow version. | Scripted key sequence from each screen; palette query for each project name. | `nav.ts`, `workspace-layout.tsx:61-125`, `03-project-overview` |
| A2 | Capture is one line: `Enter` creates; accepted text clears and selects the new row; rejected text stays with the reason; an unconfirmed retry creates exactly one task. | Create one; force a refusal (expired session); replay an uncertain submit; count tasks. | `quick-capture.tsx:14-71`, `11-inbox-before-triage` |
| A3 | Estimate, earliest start and deadline are editable from the inspector without leaving it; the values survive a restart; empty clears; a refused save restores the stored values. | Edit, restart, re-read; send a stale save. | `planning-section.tsx:1-30,225-231`, `I5-01`, `run.json` → `planning` |
| A4 | For every status, the offered edges equal the authority's read; a failed read offers all statuses with a warning; an undeclared edge cannot be chosen; a refusal leaves the row unchanged and quotes the authority. | Compare the menu with the read for all ten statuses; probe a refused edge. | `run.json` → `invalidTransition`; `queries.ts:279-306` |
| A5 | Every declared Work Item transition is visible with its guard sentences; a false property guard is marked; a relationship guard says the authority decides; a capability chip stays visible and dimmed with its reason; a refusal does not move the item. | Walk every transition of one kind; probe one guarded move. | `work-item-sheet.tsx:252-337,603-654` |
| A6 | Every declared view is selectable by id and position; an unrendered component is named with its declaration and reason; a missing view names the id and offers the first declared view; a tool-key collision is visible. | Visit every view; request an unknown id; publish a colliding id. | `view-registry.tsx:71-132`, `project-workspace-page.tsx:454-468` |
| A7 | Opening and closing the inspector returns focus to the same row and pane; selection and scroll are unchanged; an id handoff opens the same inspector; a dirty close never discards silently. | Keyboard-only open/close; restart; paste an id; close a dirty form. | `use-resource-drawer.ts:41-79`, `drawer.tsx:42-65` |
| A8 | The palette lists every implemented destination and action, nothing that cannot run, disabled rows with reasons, recent destinations first; `Enter` runs and `Esc` restores focus. | Compare the catalogue with the action bar and help; walk every row. | `command-palette.tsx:44-89,121-136` |
| A9 | Review writes nothing; apply requires an explicit confirmation echoing the fingerprint; the receipt shows fingerprint, command id, accepted revision, replayed flag and block ids; a changed revision discards the review; the block appears after refresh. | Read the revision before/after review; change the window; apply; refresh. | `run.json` → `planning`; `project-planning.tsx:675-833,909-925` |
| A10 | No agent launch control exists; an approved review leaves the task status unchanged and says `Task not completed`; a foreign job shows the 403 ownership sentence, not “not found”; no second conflicting review is offered. | Review a job, read the task; request a foreign id. | `agents.spec.ts`, `review-section.tsx:135-184`, `blocks.tsx:33-55` |
| A11 | Every action is reachable without a pointer; focus is visible without colour; every command outcome appears in the status line and persists until the next command; shortcuts are ignored while typing; `?` lists only real keys. | Keyboard-only pass over every screen; run with `NO_COLOR=1`; trigger one of each outcome. | `quality-keyboard.spec.ts`, `quality-a11y.spec.ts`, `I5-17` |
| A12 | Every “not available” names the declaration and the reason; no simulated data is shown as accepted; no capability execution is claimed in connected mode; no `saved` precedes the authority's answer. | Audit every empty and disabled state; compare with the authority's read. | `view-registry.tsx:99-132`, `work-item-sheet.tsx:656-732` |
| A13 | Layout is usable at 80×24 and 120×40 with no decision-critical value clipped; the same is true with colour off. | Two terminal sizes, two colour modes, one pass over planning and transitions. | `I5-13`, `docs/web-app.md` Accessibility |
| A14 | The run that proves A1–A13 records the commit, binary hash, Workflow version, terminal sizes and the failing cases; failures stay in the record instead of being waived. | Inspect the evidence directory. | I5 `README.md` capture identity; `run.json` |

**One gate on top of the checklist.** Do not call the redesign complete while any shared item in
the previous section is faked in the terminal. A TUI that filters edges from its own table, prints
an upgrade receipt without a fingerprint, or fills defaults outside the savepoint would pass a
superficial demo and fail A4, A9 or A12 on the first version skew.

## What this spec does not cover

- **Knowledge authoring, attachments, milestone CRUD, sign-in and device enrolment.** The I5
  cycle used a note and an attachment, but they are not in this page's pattern set; the web guide
  already documents their behaviour, and a terminal spec for them would be a separate page.
- **The web's visual language** — type, colour, motion, spacing, theme. The only visual facts that
  cross into the terminal are meaning-bearing ones: status glyphs, the focus border, the
  selection rail.
- **Project settings beyond Workflow upgrade**, including rename, archive and delete. The web
  build exposes only what the API supports; the TUI should do the same, and that is a per-command
  decision, not a redesign pattern.
- **Performance and large-data rendering.** No measured TUI data exists, so no budget is claimed.
- **Connected-mode transport.** The outbox, flush and conflict review are already documented in
  [`tui-workspace.md`](tui-workspace.md); this page only requires that the same authority answers
  appear in the status line and that no queued intention is shown as accepted.

## Evidence index

Captures, all under `web/evidence/i5/`:

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
| [`25-agents-no-jobs.png`](../../web/evidence/i5/25-agents-no-jobs.png) | No launch; device-owned jobs; the empty state's honesty (`I5-16`) |

`run.json` keys used above: `identity`, `project`, `moves`, `triage`, `milestones`, `planning`,
`conflict`, `invalidTransition`, `boardMove`, `restart`, `activityOrigins`, `upgrade`, `agentJobs`,
`taskActivity`, `counts`, `steps`.

Web sources cited: `web/src/components/navigation/nav.ts`, `components/layout/sidebar.tsx`,
`components/ui/command-palette.tsx`, `components/ui/drawer.tsx`, `app/app-shell.tsx`,
`app/workspace-layout.tsx`, `features/inbox/quick-capture.tsx`,
`features/tasks/task-drawer/{task-drawer,planning-section,description-editor,activity-section}.tsx`,
`features/tasks/{task-controls,use-task-drawer}.ts`, `hooks/use-resource-drawer.ts`,
`features/workflows/{view-registry,work-item-sheet,bundled-workflow-install}.tsx`,
`features/workflows/views/board-view.tsx`, `features/planning/project-planning.tsx`,
`features/projects/project-workspace-page.tsx`, `features/agents/agent-job/review-section.tsx`,
`features/agents/agent-job/blocks.tsx`, `api/queries.ts`, `api/endpoints/tasks.ts`.

Server and runtime sources: `crates/orbit-server/src/main.rs` (transition read, project command,
bundled routes), `crates/orbit-domain/src/task.rs` (`can_transition_to`),
`crates/orbit-runtime/src/{domain_commands.rs,store.rs,blueprint_upgrade.rs}`,
`crates/orbit-application/src/{project_commands.rs,task_detail_commands.rs}`,
`crates/orbit-client/src/{view_data.rs,task_details.rs}`, and the TUI sources named per pattern.

Acceptance specs cited: `web/tests/e2e/quality-keyboard.spec.ts`,
`web/tests/e2e/quality-a11y.spec.ts`, `web/tests/e2e/agents.spec.ts`,
`web/tests/e2e/work-items.spec.ts`, `web/tests/e2e/planning-estimates.spec.ts`,
`web/tests/e2e/planning-guard.spec.ts`, `web/tests/e2e/upgrades.spec.ts`,
`web/tests/e2e/workflow-install.spec.ts`, `web/tests/e2e/conflict.spec.ts`,
`web/tests/e2e/tasks.spec.ts`, `web/tests/e2e/workspaces-registry.spec.ts`, and
`web/tests/unit/features/quick-capture-idempotency.spec.tsx`.
