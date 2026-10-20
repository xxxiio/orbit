---
template: blocker-register
template_version: 2.0.0
template_digest: "dade8d1382774ff710499f9f5c756577fc697e9b57ea3a068bd5aae04a6ca62a"
project_owned: true
record_contract: blocker-register
record_schema_version: 1
status: current
---

# Blocker Register

| ID | Blocked work | Cause | Owner | Unblock condition | Next check | Status |
|---|---|---|---|---|---|---|
| BLOCKER-0001 | Remote/TLS release acceptance | No Docker daemon and no second enrolled machine in this environment | maintainers | Provision Docker and a disposable second machine | Re-run the deferred M15 gates | waiting |
| BLOCKER-0002 | Real GitHub Release installation and update verification (M27 EC-001/EC-002) | No release exists; the authenticated local account has read-only permission and publication needs explicit authorization | release owners | A write-capable maintainer authorizes a draft/test release | NEXT-0004: run the workflow and curl installer | blocked |
