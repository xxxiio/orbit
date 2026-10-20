# Research project workflow

Use Orbit for research questions, evidence gathering and iterative review
without installing Herdr. The research blueprint is optional; core tasks and
notes work on their own.

## Start with accountable tasks

```sh
orbit project add "Investigate indexing design"
orbit project status PROJECT_ID active
orbit project configure PROJECT_ID \
  --mode custom \
  --objective "Choose an indexing strategy with evidence" \
  --signal knowledge-evidence \
  --signal tasks \
  --criterion memo=Decision memo reviewed

orbit task add "Collect benchmark datasets" --project PROJECT_ID
orbit task add "Compare index strategies" --project PROJECT_ID
orbit task add "Write decision memo" --project PROJECT_ID
```

Copy the real IDs from the output. Make the comparison depend on collection and
the memo depend on comparison:

```sh
orbit task depends-on COMPARISON_TASK_ID DATASET_TASK_ID
orbit task depends-on MEMO_TASK_ID COMPARISON_TASK_ID
orbit task graph --now-ms 1792480200000 --all
```

## Keep the evidence in notes

Tasks record workflow state, not the research itself. Put sources and findings
in the vault:

```sh
orbit note create indexing-evidence.md \
  --id note-indexing-evidence \
  --body "# Evidence
## Datasets
- Dataset A: ...
## Measurements
- Strategy 1: ..."
orbit note kind note-indexing-evidence finding
orbit note reference note-indexing-evidence project:PROJECT_ID
orbit note search "latency"
```

Use one note per decision or finding when it helps review. Reference the
project and, if useful, a specific task:

```sh
orbit note reference note-indexing-evidence task:COMPARISON_TASK_ID
orbit note reference-backlinks task:COMPARISON_TASK_ID
```

## Optional research blueprint

```sh
orbit blueprint bundled
orbit blueprint install research
orbit blueprint list
orbit blueprint bind PROJECT_ID research 1
orbit blueprint binding PROJECT_ID
```

Inspect `orbit blueprint show research 1` before creating typed research items.
Their required properties and legal transitions are defined by the manifest,
not by the core task interface. Read the returned revision before transitioning.
A transition can fail when a guard is unmet.

Blueprint items are not tasks. If an item represents deliverable work that an
agent will execute, associate it with an existing Orbit task explicitly.

## Review, not automatic approval

- Inspect the actual datasets, measurements and memo.
- Complete core tasks only after the deliverable is reviewed.
- Mark the project criterion satisfied explicitly, or leave it pending.
- In connected mode, acceptance occurs only after server acknowledgment.

```sh
orbit task start COMPARISON_TASK_ID
orbit task complete COMPARISON_TASK_ID
orbit project criterion PROJECT_ID memo satisfied
orbit project health PROJECT_ID
```

## Limitations

- There is no standalone milestone command; use tasks, criteria or notes.
- There is no core priority field; rank by deadline and dependencies.
- Capability actions offered by a blueprint are not executable in this build.
- Recurrence/streak automation is not part of research tracking.

See [Blueprints versus tasks](blueprints-vs-tasks.md),
[Notes and folders](notes-and-folders.md), and
[Projects and planning](projects-and-planning.md).
