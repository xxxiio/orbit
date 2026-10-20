---
template: risk-register
template_version: 2.0.0
template_digest: "d163693352f16a3d914539632ca56bcdcc987744a280afe1640d8b21cccbf94f"
project_owned: true
record_contract: risk-register
record_schema_version: 1
status: current
---

# Risk Register

| ID | Risk event | Trigger | Likelihood | Impact | Mitigation | Owner | Review condition | Status |
|---|---|---|---|---|---|---|---|---|
| RISK-0001 | Self-hosted TLS ingress fails or leaks secrets when deployed | First real remote deployment | medium | high | Keep TLS ingress as a separate gate; use disposable certificates and loopback tests; never commit `.env` | maintainers | Before any remote connected deployment | open |
| RISK-0002 | Remote machine-worker path diverges from its contract double | First real remote agent run | medium | high | Run the real server/worker cycle on a second machine; keep remote execution out of local readiness claims | maintainers | Before claiming remote-agent readiness | open |
| RISK-0003 | Public release lacks provenance/signing and cross-platform evidence | Publishing binaries | high | high | Add signing/authenticated publishing, target builds and upgrade fixtures; checksums alone are documented as consistency only | maintainers | Before public release | open |
| RISK-0004 | Blueprint capability actions remain unreachable and capability-requiring blueprints fail closed | Publishing a blueprint with `agent.launch` | high | medium | Implement NEXT-0001: trusted composition plus explicit action request/execute with fake-runtime acceptance | maintainers | M25-I3 completion | open |
| RISK-0005 | Connected MCP blueprint writes bypass the outbox/receipts | Connected MCP mutation with a lost acknowledgement | medium | high | Route MCP writes through the canonical outbox and add a lost-ack replay test | maintainers | M25-I5 completion | open |
| RISK-0006 | Connected server is single-user trust; any enrolled device can mutate | Multi-user or exposed deployment | medium | high | Document the trust boundary; add role separation before multi-tenant use | maintainers | Before shared/networked deployment | monitoring |
| RISK-0007 | Public installer or update overwrites an unrelated executable | Running installer/update in a prefix containing unmanaged binaries | medium | high | Reuse the managed manifest, refuse unmanaged destinations, stage and roll back, and test preservation on failure | maintainers | Every M27 installer/update change | open |
| RISK-0008 | Configuration precedence diverges between CLI, TUI and MCP | Adding a config source or adapter-specific override | medium | high | One `orbit-client` resolver, pure precedence tests, and doctor output showing effective values | maintainers | Every configuration change | open |
| RISK-0009 | Update irreversibly migrates a database before the new binary is accepted | Replacing a binary over an older schema | medium | high | Read-only SQLite backup before replacement; fail closed if backup fails; no automatic downgrade | maintainers | Every update release | open |
| RISK-0010 | Release workflow is unverified because no real GitHub Release is published | Claiming public-install readiness from fixture tests | high | high | Keep draft/manual publication; run the workflow and verify the real installer before any public readiness claim | release owners | First authorized release | open |
| RISK-0011 | Connected TUI diverges from the shared project-centric presentation | Shipping local M28 while connected remains command-centric | medium | medium | Keep shared contracts renderer-neutral; add a connected adapter and parity tests before claiming connected parity | maintainers | Connected adoption | open |
| RISK-0012 | Blueprint view metadata is mistaken for action authority | Adding a component/action without checking the derived action catalogue | low | high | Validate action IDs, derive offers from selection, revalidate every mutation, and keep declaration-vs-authority tests | maintainers | Every view/action change | open |
