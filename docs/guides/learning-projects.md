# Learning projects

A learning project tracks understanding and evidence rather than only finished
deliverables. Use the `learning` mode to make learning evidence, habit
consistency and time invested visible in project health.

## Create a learning project

```sh
orbit project add "Distributed consensus"
orbit project status PROJECT_ID active
orbit project configure PROJECT_ID \
  --mode learning \
  --objective "Explain and implement Raft safely" \
  --signal learning-evidence \
  --signal habit-consistency \
  --criterion explain=Can explain leader election without notes \
  --criterion implement=Implementation passes the course tests
```

Learning mode's default signals are milestones, learning evidence, habit
consistency and time invested. `--signal` replaces those defaults when supplied.
This build has no milestone CRUD; use tasks and criteria for concrete outcomes.

## Structure the work

```sh
orbit task add "Read the Raft paper" --project PROJECT_ID
orbit task add "Summarize leader election" --project PROJECT_ID
orbit task add "Implement log replication" --project PROJECT_ID
orbit task depends-on SUMMARY_TASK_ID READING_TASK_ID
orbit task planning SUMMARY_TASK_ID --estimate-minutes 60
```

Use notes for the actual learning artifact:

```sh
orbit note create raft-leader-election.md \
  --id note-raft-leader-election \
  --body "# Leader election
- Terms and votes
- Election timeout
- Split-vote handling"
orbit note kind note-raft-leader-election concept
orbit note reference note-raft-leader-election project:PROJECT_ID
```

A note is file-authoritative inside the vault. Keep sources, questions and
your own explanations there; task status is not evidence of understanding.

## Build consistency without streaks

Habits represent repeated practice; they do not prove mastery.

```sh
orbit habit add "Read distributed systems" --weekly-target 5
orbit habit check HABIT_ID --at-ms 1792480200000 --duration-minutes 45
orbit habit history HABIT_ID
```

Automatic recurrence and streak extraction are deferred in this build. Use a
weekly target, explicit check-ins and history. Do not invent a streak command.

Optionally bind the habit as project evidence:

```sh
orbit project evidence bind PROJECT_ID habit HABIT_ID
orbit project evidence report PROJECT_ID \
  --start-ms 1792393800000 \
  --end-ms 1792655400000 \
  --as-of-ms 1792655400000
```

## Review and close a learning goal

```sh
orbit project health PROJECT_ID
orbit task start SUMMARY_TASK_ID
orbit task complete SUMMARY_TASK_ID
orbit project criterion PROJECT_ID explain satisfied
orbit project status PROJECT_ID completed
```

Mark a criterion satisfied only after you can demonstrate it. Health reporting
is deterministic and does not infer understanding from completed tasks.

## Suggested note kinds

| Kind | Use |
| --- | --- |
| `concept` | Durable explanation of an idea |
| `finding` | Result of an experiment or reading |
| `assumption` | Belief that still needs checking |
| `risk` | Known gap or misunderstanding |
| `resource` | External reference worth revisiting |
| `decision` | Chosen approach and rationale |

```sh
orbit note catalog --kind concept
orbit note search "leader election"
```

## Common failures

- **Learning health looks empty** — bind evidence and use an explicit reporting
  window; mode progress is not inferred from reading time alone.
- **No streak** — expected; use weekly targets plus history.
- **Note reference not found** — verify the note ID and `project:PROJECT_ID`
  grammar.
- **Criterion marked too early** — reopen the project; evidence should support
  the claim.
- **Wrong workspace** — run `orbit status`.

Next: [notes and folders](notes-and-folders.md),
[research workflow](research-workflow.md) or
[projects and planning](projects-and-planning.md).
