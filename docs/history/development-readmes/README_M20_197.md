# M20 Development 197 — Connected TUI physical-vault commands

The connected TUI command palette can now stage Markdown note creation, guarded update,
 guarded move, folder creation, and folder move using the canonical authority command
 payloads. Each operation is queued in the existing globally ordered durable outbox;
 no connected-mode handler writes local authoritative Markdown or mutates the accepted
 server snapshot. A multi-domain batch preflights all commands and uses the existing
 atomic local queue staging.

Commands: `note.create`, `note.update`, `note.move`, `folder.create`, `folder.move`.
 The `note.update` and `note.move` commands intentionally require the full expected
 previous body, preserving the server's optimistic-content-concurrency contract.
 Quotes are supported by the command palette, not a shell.

Scope: this adds connected TUI command submission, not a Markdown browser or an
 accepted content sync mechanism. Local-only workflows are unchanged. More
 importantly, M20 still needs an explicit residual implementation audit; do not
 mark it complete based on source-level additions alone. Broad compilation and
 release acceptance are deferred to M15 as requested.

The connected TUI now also has an accepted Markdown Notes tab. `note.search <query>`
queries `/v1/authoritative/markdown-search` with the authenticated server client;
results are checked for expected fields before replacing the list. Offline
search and malformed remote results never discard the last accepted result
set. The Notes view never projects local pending note edits as accepted notes.
