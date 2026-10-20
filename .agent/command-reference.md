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

- Linux x86_64 (current verified platform).
- Rust 1.99 with `cargo`, `rustfmt` and `clippy`.
- Python 3.12 with `pytest` for maintained Python tests.
- Bash, `tar`, `sha256sum` and `openssl` for packaging/installation checks.
- `git` for disposable live Herdr acceptance repositories; not required for ordinary local use.
- Docker for TLS/self-hosted ingress checks; currently unavailable.

## Install

```bash
./scripts/install.sh --prefix "$HOME/.local"
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
./scripts/m15-full-automated.sh --profile full --live-herdr
bash tests/packaging/test_real_release_archive.sh
```

## Canonical full health gate

```bash
./scripts/m15-full-automated.sh --profile full
```

## Package and release

```bash
./scripts/package-release.sh --build --components cli,mcp,tui,server --output ./dist
./scripts/verify-release.sh dist/orbit-0.1.0-<target>.tar.gz
./scripts/install.sh --from-archive dist/orbit-0.1.0-<target>.tar.gz --prefix /tmp/orbit-install
./scripts/install.sh --prefix /tmp/orbit-install --check
./scripts/uninstall.sh --prefix /tmp/orbit-install
```

## Security and performance checks

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Unavailable checks

- TLS/Caddy ingress: Docker is not installed.
- Remote-machine Herdr: requires a second enrolled machine.
- macOS/Windows acceptance: requires those platforms or cross-compilation.
- Distribution signing: no signing key or release channel is configured.
- Installed-version database upgrade: no older release fixture is available.
