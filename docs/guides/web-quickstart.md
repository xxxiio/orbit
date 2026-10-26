# Web app quickstart for internal testers

This page takes a new internal tester from a checkout to a signed-in browser, a first project,
and a green acceptance run. It assumes Linux x86_64, a checkout of this repository, Node 18.18
or newer, and a Rust toolchain. Nothing here needs the public release channel or a deployed
server: the server runs on loopback with a throwaway database.

Every command below was run against this commit while the page was written. Where the
machine's `npm` is the Windows interop build, the page uses the Linux `node` binary directly,
because `npm run …` fails there before the script starts (`UNC paths are not supported`).

## 1. Build the client

Dependencies live in `web/node_modules`; the install step is documented in
[`web/README.md`](../../web/README.md). Build with the Linux Node:

```sh
cd web
node node_modules/typescript/bin/tsc --noEmit
node node_modules/vite/bin/vite.js build
```

The type check exits silently on success. The build ends with `✓ built in …` and writes
`web/dist/index.html` plus `web/dist/assets/`. The server serves exactly that directory. If
either command fails, fix the error before starting the server; a stale `dist` otherwise hides
the problem behind a working page.

## 2. Start the server

From the repository root:

```sh
mkdir -p /tmp/orbit-web-quickstart
export ORBIT_SERVER_ADMIN_KEY="$(openssl rand -hex 32)"
export ORBIT_SERVER_DB=/tmp/orbit-web-quickstart/orbit-server.db
export ORBIT_SERVER_BIND=127.0.0.1:8787
cargo build -p orbit-server
target/debug/orbit-server
```

`ORBIT_SERVER_ADMIN_KEY` must be at least 32 characters; keep it in this shell, because device
enrolment below uses it. The server serves the API, `/console` and the built app on
`http://127.0.0.1:8787/`. In a second terminal, check it is listening:

```sh
curl -sS http://127.0.0.1:8787/health
{"status":"ok"}
```

The database and the vault are created under `/tmp/orbit-web-quickstart/`; delete that
directory when you are finished. Leave the server running for the next steps.

## 3. Enrol a browser device

Sign-in uses a device credential, not a username and password. Enrol one device for this
browser (use a different device ID for a second browser, or rotate the token):

```sh
TOKEN="$(openssl rand -hex 32)"
curl -sS -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:8787/v1/devices \
  -H "Authorization: Bearer $ORBIT_SERVER_ADMIN_KEY" \
  -H 'Content-Type: application/json' \
  -d "{\"device_id\":\"browser-laptop\",\"token\":\"$TOKEN\"}"
201
printf 'device token: %s\n' "$TOKEN"
```

Copy the printed token somewhere safe; the server stores only its hash and cannot show it
again. The `201` proves the device was enrolled. An already-enrolled ID is refused; rotate that
device's token or pick a new ID. To check the whole credential exchange without a browser:

```sh
curl -sS -i -X POST http://127.0.0.1:8787/v1/web/session \
  -H 'Content-Type: application/json' \
  -d "{\"device_id\":\"browser-laptop\",\"token\":\"$TOKEN\"}"
```

The response is `201 Created` with a `set-cookie: orbit_session=…; HttpOnly; SameSite=Strict`
header. The browser performs the same exchange from its sign-in form.

## 4. Sign in and create the first project

1. Open `http://127.0.0.1:8787/` and enter **Device ID** `browser-laptop` and **Device token**
   (the printed value). The eye button reveals the token while pasting.
2. **Projects → New project.** Name it (for example `Orbit quickstart`). **Basic** is the
   default workflow. If you want the Development workflow, use **Bundled workflows → Install**
   in the same dialog first; it becomes selectable without a page reload.
3. Create the project. The app navigates to its Overview. Add a task with **New task** or `c`.
   Open the task and use the drawer's **Planning** group to set an estimate and a deadline if
   you want to try the planning screen.
4. Reload once. The project, its pinned workflow badge and the task must still be there. If they
   are, the browser and the server agree on accepted state.

The same create-project path, including duplicate names and validation, is covered by
`web/tests/e2e/projects.spec.ts`; the commands are in step 6.

## 5. The dogfood script and the evidence

The real end-to-end cycle lives at `web/tests/e2e/i5/dogfood.mjs`. It drives the product through
the browser against an isolated server on `127.0.0.1:8790`, with state in `/tmp/orbit-i5`, and
writes its results to `web/evidence/i5/`:

```sh
cd web
node tests/e2e/i5/dogfood.mjs
```

Before running it:

- Commit or stash everything. The script refuses to start on a dirty git tree, because a
  capture must name the commit it came from.
- Build `web/dist` (step 1) and `target/debug/orbit-server` first. The script uses both hashes
  in its record.
- Stop anything already on port `8790`.

The run deletes and recreates `/tmp/orbit-i5`, enrols two devices, drives projects, tasks,
milestones, Work Items, a note, an attachment, a planning review/approval/apply and a
two-device conflict through the UI, then rewrites `web/evidence/i5/run.json`,
`web/evidence/i5/README.md` and the SQLite backup. It is deliberately not part of the
Playwright suite, which must stay repeatable and parallel.

The committed capture sets are `web/evidence/i1/` through `web/evidence/i5/`. Each directory
has a README naming the commit, the built `dist` hash and the screens. The I5 README is
generated from `run.json` and is the place to look for what the last real cycle actually did.

## 6. Run the acceptance suite

The Playwright suite drives the built app in Chromium against a real `target/debug/orbit-server`.
Each spec starts its own server on a free port with a throwaway database, so no fixture mocks
responses and no two specs share state. From `web/`:

```sh
node node_modules/@playwright/test/cli.js test --project=chromium
```

To iterate on one area, name its spec:

```sh
node node_modules/@playwright/test/cli.js test tests/e2e/projects.spec.ts --project=chromium
```

The suite reads `web/dist`, so rebuild after any `web/src` change. `ORBIT_E2E_EVIDENCE=1` keeps
traces and screenshots under `web/evidence/e2e/<run-id>/`; `ORBIT_KEEP_E2E_STATE=1` keeps each
test's temporary server directory. Neither changes committed evidence.

Run the documentation and contract checks from the repository root:

```sh
python3 -m pytest -q tests/documentation tests/contracts
node web/scripts/check-terminology.mjs
```

The first command checks the guides and contract documents; the second checks that no
user-visible legacy workflow name appears in the web client's source. Both must exit `0`.

## 7. Stop and clean up

Stop the server with `Ctrl+C`. Delete `/tmp/orbit-web-quickstart` (and `/tmp/orbit-i5` if you
ran the dogfood cycle). The repository should show no new files: the database and vault live
outside the checkout.

## If something goes wrong

| Symptom | Check |
| --- | --- |
| The page says `503 Orbit web app is not built` | `web/dist/index.html` exists and the server was started from the repository root (or `ORBIT_WEB_DIST` is absolute). |
| Sign-in returns `Those credentials did not match` | The device ID matches the enrolment and the token is the full printed value. |
| `429 Too many attempts` | Five failures in five minutes; wait, then retry. |
| `npm run build` fails with `UNC paths are not supported` | Use the two `node` commands in step 1; the project scripts cannot run through the Windows `npm` shim. |

For the product itself — every screen, shortcut, refusal and known gap — see
[the web app guide](../web-app.md).
