# M15 fully automated acceptance runner

Run from the Orbit repository root:

```bash
./scripts/m15-full-automated.sh
```

Or with Python (including Windows):

```bash
python scripts/collect-release-evidence.py --profile full --no-setup-prompt
```

The runner never prompts or installs dependencies. Preinstall Python 3, Rust/Cargo with rustfmt and clippy, and Python pytest. Cargo may download Rust dependencies; use `--offline` for an air-gapped run with dependencies already cached. Each check has a configurable timeout (default 1200 seconds); customize using `--timeout 300`. `--profile quick` omits the full build and server smokes. The full profile executes Rust metadata, formatting, check, unit tests, Clippy,
binaries build, documentation contracts, installation/release-archive contracts,
a real CLI archive build/install smoke, Python worker and maintained Python tests,
and disposable acceptance smokes: compiled local CLI persistence,
local-authoritative workflows, canonical task/domain two-device flows, offline
replica replay, server backup/restore durability, blueprint workflows, and the
deterministic agent-execution contract. See `docs/m15-runner-coverage.md` for the current check list. Tests continue after an individual failure; integration smokes are gated on a successful *current* binary build, never stale binaries.

**Exit codes:** 0 = every selected check passed; 1 = one or more failures/timeouts; 2 = one or more required checks did not run (for example missing Cargo). A missing dependency never counts as a pass.

**Live Herdr:** `--require-herdr` checks only CLI presence. `--live-herdr` explicitly opts in to `smoke-herdr-live.py`, which starts a real agent in a disposable temporary repository and closes the Herdr workspace it created. It is never enabled implicitly. Production TLS ingress, browser deployment, distribution signing, cross-platform acceptance and real installed-version upgrades remain separate release gates.

An `orbit-m15-evidence-<UTC>.zip` artifact includes `manifest.json`, `SUMMARY.md`, and per-check logs. Choose a custom path with `--output /path/to/evidence.zip`. Inspect logs before sharing: automatic redaction is best effort. No real user DBs or externally hosted servers are selected by the runner. To check Herdr binary availability only, pass `--require-herdr`; live Codex/OpenCode execution is **not** automatically attempted without a separately approved, configured test sandbox. Packaging, production deployment, and live Herdr acceptance remain separate release gates.

This runner replaces the optional interactive Rust setup prompt in the previous M15 evidence command. To provision Rust explicitly, run `python scripts/setup-toolchain.py --yes` yourself in an environment you trust.
