# Development 243 — Agent Git evidence and review

## Added

- `orbit agent capture JOB_ID --max-commits 20` snapshots bounded, read-only Git state from the job's *persisted* repository. It never attributes commits to a particular agent without further proof. It saves the snapshot as an append-only handoff.
- `orbit agent review JOB_ID --handoff-id ID --reviewer NAME --decision approved|changes_requested --reason TEXT` records an immutable review decision, backed by an existing handoff.
- `orbit agent review-status JOB_ID` reads the review, independent of Herdr session availability.
- Schema v18 adds `agent_job_reviews`, with migration from v13–v17. A review does not complete the Orbit task.

## Limitations and acceptance

This is a local-authority implementation. Git evidence requires a local repository checkout. It captures repository state, not causality or agent authorship, and excludes untracked paths to avoid inadvertent collection. It does not run a user's tests or launch an agent. Connected-mode review authorization remains future work. Rust compilation and executable tests were unavailable in this environment; SQLite schema and review constraints were checked independently. Progress is therefore provisional source-scope only.

## Usage

```sh
orbit agent capture JOB_ID --max-commits 20
orbit agent handoffs JOB_ID
orbit agent review JOB_ID --handoff-id HANDOFF_ID --reviewer me --decision approved
orbit agent review-status JOB_ID
```
