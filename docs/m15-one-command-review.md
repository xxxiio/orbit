# M15 one-command local acceptance evidence

Run from the Orbit repository root:

```sh
./scripts/m15-review.sh --profile full
```

Windows PowerShell (Python 3 required):

```powershell
python scripts/collect-release-evidence.py --profile full
```

The command writes `orbit-m15-evidence-<UTC>.zip` to the repository root. It includes a summary, machine-readable per-check outcomes, and per-command logs. The runner continues after failures and still creates a ZIP if Rust tools are absent. An exit status of 0 means all selected checks ran and passed; 1 means at least one check failed, timed out, or errored; 2 means at least one required check did not run (for example missing Cargo), which is incomplete evidence, not a pass.

The quick profile (`--profile quick`) runs toolchain checks, fmt, cargo check and cargo tests; the full profile additionally runs Clippy, compiles client/server executables, runs Python worker tests and (only if that build passed) the isolated two-client Git/HTTP/restore smoke. Use `--offline` only if Cargo dependencies are already cached. Use `--output path/to/evidence.zip` to choose a destination.

Requires Python 3. Rust toolchain (`cargo`, `rustc`, `rustfmt`, `clippy`) and Git are needed for complete evidence. The smoke script runs only against an ephemeral local server and temporary databases. Rust dependencies may need fetching on a first run unless `--offline` is specified. Never run the smoke against production data.

## Evidence boundaries

The bundle intentionally excludes local Orbit DB files, tokens, the process environment and repository contents. It includes command output, which may contain unexpected paths or identifiers; **inspect the ZIP before sharing**. The runner redacts common credential patterns, but cannot guarantee all secrets are removed from diagnostics.

If the ZIP reports failed checks, it is still useful: upload it as-is rather than repeatedly rerunning blindly. The final M15 review also needs separate deployment, packaging, migration and security acceptance; this runner is a reproducible starting gate, not a release certification.

### Developer toolchain setup (current)

The M15 runner is fully noninteractive: it never prompts and never installs
software (`--no-setup-prompt` is accepted but deprecated). Missing Rust/Cargo,
rustfmt, or clippy makes the dependent checks `not_run` with exit status 2,
never a pass. Provision the toolchain separately with
`python3 scripts/setup-toolchain.py` (or `--no-install` to check only), or use
https://rustup.rs/ on any platform. After setup, rerun the full M15 review.

**End users do not need Rust or Cargo** when running prebuilt Orbit packages.
This guided setup is for source builds, contributors, and final-review testing.
Automatic system package installation, Windows installers, prebuilt end-user
packaging, and distribution signing remain separate release-readiness work.
