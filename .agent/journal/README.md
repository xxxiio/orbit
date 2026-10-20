---
template: journal-readme
template_version: 1.1.0
template_digest: "70e09f5f7bb76089e9d9c1c23ab16a9e3f2d56d84beebe5197f383641fea2ad0"
project_owned: true
---

# Journal

Create append-only entries using `YYYY/YYYY-MM-DD.md`.

Journal entries are durable historical memory in both task and project workflows. They
record completed work, decisions, changed components, validation, problems, remaining
work, TODOs, and risks.

Journal entries never own continuation state. Task workflow has no next-step obligation.
Project workflow stores continuation separately, especially in
`.agent/current/next-step.md`.

Do not load the complete journal by default. Retrieve prior entries only when the current
request, a durable decision, or a known historical dependency makes them relevant.
