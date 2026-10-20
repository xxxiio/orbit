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
