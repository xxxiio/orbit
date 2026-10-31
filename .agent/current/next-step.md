---
template: next-step
template_version: 3.0.0
template_digest: "b88835ce70ef09490ad55db0e910dc05ff721384d6ccad8d45eafedd7654aaf5"
project_owned: true
record_contract: next-step
record_schema_version: 1
id: NEXT-0020
status: ready
milestone_id: MILESTONE-0033
advances_exit_criteria: [EC-010, EC-011]
---

# Next Deterministic Step

## Objective

Hand the corrected M33 build to a human for visual and usability review of the
running app and the representative captures, including both browser authentication
modes. M33 remains active until a person records that review; automated
screenshots and DOM assertions are not approval.

## Why this is next

The corrective implementation is complete: Milestone is the only grouping entity,
Epic is removed, rich task creation is one accepted command, List/Board/Backlog share
one Tasks tab, the Board is compact by default, and the Roadmap is a Milestone
hierarchy. Authentication now also offers an explicit opt-in local-trusted mode for
a personal loopback install, while the default device authentication is unchanged.
Rust and web gates are green, the legacy conversion and restart were exercised on a
real database, and `web/evidence/m33-corrective/` plus `web/evidence/m33-auth/` hold
the captures. The open exit criteria are the human review (EC-010) and the
authentication experience review (EC-011).

## Preconditions

- `target/debug/orbit-server` and `web/dist` are built from the current tree.
- A device credential for a project with several milestones, 100+ tasks, deadlines,
  dependencies, unscheduled work and Work Items.
- The browser-libraries bundle at `/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu` for the
  Playwright Chromium build.
- For the local-trusted half: a throwaway database copy and a loopback port, so the
  review does not touch the dogfooding database.

## Inputs

- `.slim/deepwork/m33-web-ux/progress.md`, `corrective-plan.md`, `p2-ui-briefs.md`.
- `.slim/deepwork/ses_e7076bbabffejijzQEfVyIuLrj.md` (authentication iteration).
- `web/evidence/m33-corrective/README.md` and its PNGs.
- `web/evidence/m33-auth/README.md` and its PNGs.
- `.agent/current/acceptance.md`, `.agent/current/evidence.md`.
- `docs/web-app.md`, `docs/guides/local-web-quickstart.md`,
  `docs/guides/tui-redesign-spec.md`.

## Ordered actions

1. Start the server against a copy of the dogfooding database and open the built SPA.
2. Review Tasks List, Board (compact and comfortable), Backlog and the view switcher;
   confirm filters survive switching and the URL carries the view.
3. Create a fully configured task (title, description, priority, deadline, estimate,
   milestone, two acceptance criteria, one dependency) and confirm every value is
   present without a second edit.
4. Review a milestone card, its details, child tasks, Work Items, progress and
   activity; change a status and reload.
5. Review the Roadmap at week/month/quarter: duration bars, checkpoint markers,
   unscheduled work, child windows, progress and blocked cues.
6. Review the same routes at 390px in light and dark for crowding, overflow and
   navigation friction.
7. Start a second server copy with `ORBIT_AUTH_MODE=local-trusted` on a loopback
   port against a throwaway database and open a clean browser profile: confirm the
   workspace opens with no Device ID, no token and no login screen, that the sidebar
   shows only "Local trusted mode" and no sign-out, and that refresh, browser restart
   and server restart never prompt.
8. Review the authenticated mode on the normal server: confirm the sign-in form,
   that a wrong token is refused, that a valid session survives refresh and browser
   restart, and that sign-out returns to the form. Confirm `GET /v1/web/session`
   reports `mode` in both modes.
9. Record the findings and any defects in `.slim/deepwork/m33-web-ux/acceptance.md`.
10. Fix only review findings; re-run the affected gates. Mark M33 complete only when
    the human review is recorded and the remaining exit criteria still hold.

## Expected outputs

- A human review note naming what was inspected, at which sizes/themes, and any
  issues found.
- Defect fixes (if any) with focused re-validation.
- An honest M33 status: complete only after EC-010 is satisfied.

## Acceptance criteria

- [ ] NS-001: A human has reviewed the running app at 1440×900 and 390×844 in light
  and dark, plus the representative captures.
- [ ] NS-002: The review note records what was inspected and every material issue.
- [ ] NS-003: Any review-driven fix preserves the Milestone model, one Tasks tab,
  compact Board and authoritative commands, and the affected gates are re-run.
- [ ] NS-004: M33 is marked complete only after the review and remains active
  otherwise.
- [ ] NS-005: The reviewer has opened a local-trusted server in a clean browser
  profile and confirmed the workspace enters without a Device ID, token or login
  screen, with a plain mode note and no sign-out control, surviving refresh,
  browser restart and server restart.
- [ ] NS-006: The reviewer has confirmed the default authenticated mode still signs
  in, survives refresh and browser restart, and signs out to the form, and that no
  privileged operation became reachable from the local browser principal.

## Validation

```sh
cargo build -p orbit-server --offline
cd web && node node_modules/vite/bin/vite.js build
cargo test --workspace --offline
cd web && node node_modules/typescript/bin/tsc --noEmit
cd web && node node_modules/vitest/vitest.mjs run
cd web && LD_LIBRARY_PATH=/tmp/pwlibs/root/usr/lib/x86_64-linux-gnu \
  node node_modules/@playwright/test/cli.js test --project=chromium
```

## Stop conditions

- No human is available to review: keep EC-010 open and do not claim approval.
- A review finding requires a product-model change: record it and return to planning
  rather than patching around it.
- A failure points at an unrelated pre-existing area: record it, do not waive it
  silently.

## Out of scope

- TUI implementation; the specification is updated, but no TUI feature change is part
  of this phase.
- New milestones, public release work, or reopening M31/M32 areas M33 did not touch.
- A permanent compatibility layer for the removed grouping tables.
