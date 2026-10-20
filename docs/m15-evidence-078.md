# Orbit 078 — M15 evidence remediation

Based on user evidence `orbit-m15-evidence-20261012T000549Z.zip`:

- Explicitly select `ActivityRepository` for activity reads and append calls in the task transition unit-of-work closure (E0034).
- Preserve the project identifier for evidence replay after `Project::new` consumes its ID (E0382).
- Remove unused binding in dependent-habit preflight (compiler warning).
- Collapse `TaskPlanning` window condition for Clippy.
- Retain the public nine-argument `KnowledgeItem::restore` compatibility surface with a targeted `too_many_arguments` lint allowance, pending a future migration to an input record.

**Not yet validated:** Rust compiler, tests, Clippy, integration workflows. Re-run `./scripts/m15-review.sh --profile full` on a system with Rust. Do not infer that downstream crates pass until the next evidence bundle is reviewed.
