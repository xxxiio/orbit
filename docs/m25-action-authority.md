# M25 authoritative workflow action intents

A blueprint action declaration is an offer, not permission. A trusted composition
registers capabilities and supplies grants, then an authenticated application
use case may request an action only at the expected work-item revision and on a
published transition into its current state. The intent is durably recorded under
a stable command ID. Replayed identical requests return the existing record;
conflicting command IDs and already-claimed actions fail closed.

Only a separately authorized executor may claim an intent. Claiming is one-time;
uncertain outcomes must be reconciled and not executed again automatically.
The host's claim/result methods are **internal trusted hooks**, not directly
exposed as end-user HTTP commands. An action result never completes a project
work item. The next M25 slice must register concrete capability executors,
audit and permission policies, and client-facing command routing before an
action can be invoked from CLI/MCP/server.

M15 owns broad build and integration validation, including any fixes discovered.
