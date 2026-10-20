---
template: command-reference
template_version: 2.0.0
template_digest: "e4d8cbb2ae8f31215b2bee559925c4cf61753151e2dab8712969a460b7a78ef1"
project_owned: true
record_contract: command-reference
record_schema_version: 1
status: current
---

# Project Commands

## Environment prerequisites

- Linux x86_64 (current verified runtime platform).
- Installing a prebuilt CLI: `sh`, `curl`, `tar`, `mktemp` and `sha256sum` or `shasum`; no Rust or Python.
- Source builds/tests: Rust 1.99 with `cargo`, `rustfmt` and `clippy`.
- Python 3.12 with `pytest` for maintained Python tests.
- `git` for disposable live Herdr acceptance repositories; not required for ordinary local use.
- Docker for TLS/self-hosted ingress checks; currently unavailable.

## Install

```bash
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh
# explicit version/prefix:
curl -fsSL https://github.com/xxxiio/orbit/releases/latest/download/install.sh | sh -s -- --version v0.1.0 --prefix "$HOME/.local"
```

No public release currently exists; the command is implemented and fixture-tested, but real GitHub installation requires an authorized release.

### First run

```bash
orbit init
orbit doctor
orbit update --check
```

### Source-tree install

```bash
./scripts/install.sh --prefix "$HOME/.local"
./scripts/install.sh --from-archive dist/orbit-0.1.0-<target>.tar.gz --prefix /tmp/orbit-install
./scripts/install.sh --prefix /tmp/orbit-install --check
./scripts/uninstall.sh --prefix /tmp/orbit-install
```

## Development build

```bash
cargo build --workspace --locked
```

## Format

```bash
cargo fmt --all -- --check
```

## Lint

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Static analysis and type checking

```bash
cargo check --workspace --all-targets --locked
```

## Unit tests

```bash
cargo test --workspace --locked
python3 -m pytest -q tests
python3 -m unittest discover -s deploy/server -p 'test_*.py' -v
```

## Integration and end-to-end tests

```bash
./scripts/m15-full-automated.sh --profile full
python3 tests/acceptance/m27_init_doctor.py --binary target/debug/orbit
python3 tests/acceptance/m27_update.py --binary target/debug/orbit
python3 tests/packaging/test_github_installer.py -v
python3 tests/packaging/test_release_workflow.py -v
python3 -m unittest discover -s tests/documentation -v
bash tests/packaging/test_real_release_archive.sh
```

## Canonical full health gate

```bash
./scripts/m15-full-automated.sh --profile full
```

Current result: 42 passed, 0 failed, 0 not_run (`.agent/evidence/m27-m15-final.zip`).

## Package and release

```bash
./scripts/package-release.sh --build --components cli,mcp,tui,server --output ./dist
./scripts/verify-release.sh dist/orbit-0.1.0-<target>.tar.gz
scripts/release/validate-release-version.sh v0.1.0
```

## Security and performance checks

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Unavailable checks

- Real GitHub Actions run and real-release installer/update verification: requires maintainer authorization and write access.
- macOS/aarch64 runtime acceptance: requires those platforms or native runners.
- TLS/Caddy ingress: Docker is not installed.
- Remote-machine Herdr: requires a second enrolled machine.
- Distribution signing: no signing key or release channel is configured.
- Installed-version database downgrade: intentionally unsupported.
