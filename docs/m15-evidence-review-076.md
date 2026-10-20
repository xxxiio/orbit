# M15 evidence review — development 076

Evidence: `orbit-m15-evidence-20261011T235536Z.zip` supplied by the user.

- Passed: Rust toolchain availability, Cargo version, Python version, Cargo metadata, Python agent worker suite, Python automation worker suite (6 total).
- Failed: rustfmt, cargo check, cargo test, cargo clippy, cargo build (5 total).
- Not run: two-client HTTP/Git/restore smoke (1 total).
- Root cause for the four Cargo compile/build/test/clippy failures: `orbit-cli` incorrectly requested `serde` feature `env`; Serde has no such feature. Fixed in `crates/orbit-cli/Cargo.toml` by retaining `derive` only. Clap's legitimate `env` feature is unchanged.
- rustfmt failure: large workspace-wide formatting differences (~1.4 MB of diff). This is an independent formatting gate, not proof of incorrect runtime behavior. Run `cargo fmt --all` followed by `cargo fmt --all -- --check` on a developer machine with rustfmt, inspect the formatting diff, and keep the resulting changes in the next snapshot. Avoid treating formatting differences as compile errors.

Verification in authoring environment: all 14 Cargo.toml files parsed with Python `tomllib`; 13 agent worker and 3 automation worker tests passed. Cargo and rustfmt were not available, so Rust compile and format are **not verified** in development 076.

Next handoff: Run `./scripts/m15-review.sh --profile full` from development 076; send new evidence ZIP. Additional compiler failures may emerge after dependency resolution succeeds. Do not declare M15 accepted until runnable binary/integration checks pass.
