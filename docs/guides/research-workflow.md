# Research project workflow

Use Orbit for research questions, evidence gathering, and iterative review without installing Herdr. The research blueprint is optional: local core tasks remain usable independently of the blueprint.

## Start with accountable tasks

```sh
orbit project add "Investigate indexing design"
orbit project list
orbit task add "Collect benchmark datasets" --project PROJECT_ID
orbit task add "Compare index strategies" --project PROJECT_ID
orbit task add "Write decision memo" --project PROJECT_ID
orbit task list
```

Use the actual project/task IDs from the CLI output. Make the comparison task depend on data collection, then make the memo depend on the comparison:

```sh
orbit task depends-on COMPARISON_TASK_ID DATASET_TASK_ID
orbit task depends-on MEMO_TASK_ID COMPARISON_TASK_ID
orbit task graph --now-ms 1792454400000 --all
```

## Optional research blueprint

```sh
orbit blueprint bundled
orbit blueprint install research
orbit blueprint list
orbit blueprint bind PROJECT_ID research 1
orbit blueprint binding PROJECT_ID
```

Inspect `orbit blueprint show research 1` before creating typed research work items. Their **required properties and legal transitions are defined by the manifest**, not by the core task interface. For a work item, use `orbit blueprint create-item --help` and `orbit blueprint transition --help`; read its returned revision before transitioning. A transition can fail if the blueprint guard is unmet.

## Review, not automatic approval

Keep sources and conclusions in your actual research artifacts. Orbit tasks and blueprint items record workflow state but are not a substitute for inspecting evidence. Mark tasks complete only after reviewing the deliverable. In connected mode, acceptance occurs only after server acknowledgment.

See [Blueprints versus tasks](blueprints-vs-tasks.md) and [Projects and planning](projects-and-planning.md).
