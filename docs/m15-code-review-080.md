# Orbit 080 — source review and Git smoke remediation

## Findings

- The uploaded source builds and passes the user's earlier `cargo check`, `cargo test`, Clippy and formatting according to the supplied evidence; those commands **were not executed** here, as the Rust toolchain is not installed in this environment.
- Confirmed Git branch portability defect: `sync init` previously used `git init` without a branch while push/pull defaulted to `main`. Machines with `init.defaultBranch=master` could fail the first push with `src refspec main does not match any`.
- Fixed initial branch selection in `GitSyncTransport::git_init_with_branch` and exposed `--branch` on `orbit sync init` (default `main`). Existing Git workspaces keep their HEAD unchanged. Git validates branch names.
- Added a two-client harness assertion immediately after sync initialization, so future failures identify a wrong initial branch directly.
- Added regression test source for initial branch selection, existing-workspace stability and invalid branch names.
- The smoke suite's later Git/HTTP reconciliation, remote restore and token-rotation phases were **not reached** in the previous failure, and must not be called passed.

## Source review notes

- The source uses separate crates for domain, application, storage, client, sync, transports and server. Retain these boundaries.
- The uploaded agent modifications must be treated as the new baseline; this archive was constructed from them, not from the previous 079 snapshot.
- Some `#[allow(dead_code)]` attributes and a targeted `#[allow(clippy::too_many_arguments)]` remain in the source. These are technical-debt candidates, not evidence of broad lint suppression.
- A full behavioral/security audit and comparison against the user's uncommitted local work is not possible from passing test logs alone. Do not replace a newer local checkout without inspecting differences.

## Verification

- Python compilation of `deploy/server/smoke-two-clients.py`: passed.
- Standalone Git simulation with `init.defaultBranch=master` and explicit initial branch `main`, followed by commit/push: passed.
- Cargo check/test/Clippy/rustfmt and full HTTP/Git/restore smoke: **not run** in this environment. Rerun the M15 full-profile evidence collector.
