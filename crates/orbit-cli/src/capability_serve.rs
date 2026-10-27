//! `orbit capability serve --once`: the connected deployment's capability runner.
//!
//! # Where provider code runs
//!
//! The connected authority (`orbit-server`) persists an authorized, fenced
//! intent and never dispatches it. This module is the other half: a separately
//! enrolled device announces the capabilities it can execute, claims one
//! pending intent, runs the claimed `execution_plan` through the **local**
//! [`TrustedComposition`], and reports exactly one outcome.
//!
//! # What the local run means, honestly
//!
//! A provider is a `BlueprintCapabilityExecutor`, whose contract takes a host
//! (`&mut AuthoritativeHost`). The executor is not the authority, so it builds a
//! short-lived, in-memory *scratch* authority that mirrors the claimed request
//! (a project, the claimed transition, the claimed item, a capability grant and
//! the intent itself) and dispatches the provider against it. That scratch
//! authority is discarded when the process exits.
//!
//! So the provider's write lands in the executor's scratch authority. What makes
//! it durable is the **declared effect**: when the capability's contract declares
//! one (the reference capability declares `work_item_note`) and the provider
//! recorded it, this runner sends the payload in the result body. The authority
//! validates it against that same compile-time declaration and persists it in the
//! same savepoint as the receipt, so the item really does change. When the
//! provider produced nothing readable — or the capability declares no effect at
//! all — no payload is sent, and the receipt detail says why instead of inventing
//! one.
//!
//! The receipt detail still describes the *scratch* run (`effect.scope`,
//! `effect.recorded`, `effect.submitted_to_authority`); the authority's own
//! `effect.applied` statement in the same receipt is the durable claim.
//!
//! The scratch grant is a *mirror*, not a second authorization decision: the
//! server only let this request reach `claimed` after it checked the durable
//! installation, the project grant and this device's announcement. What the
//! mirror buys is a provider that can run its own contract checks at all.
//!
//! # What this process refuses to do
//!
//! * run a request it did not claim with a token minted by this invocation;
//! * mint a second claim token for a request, or re-claim after a lost response;
//! * retry an `uncertain` outcome, or a result report whose acknowledgement was
//!   lost — the durable state is reported instead, and reconciliation is the
//!   operator's explicit decision;
//! * print a bearer token or a claim token.
use super::*;
use orbit_runtime::capability_composition::{CapabilityActionRequest, TrustedComposition};
use orbit_runtime::capability_server::CapabilityOutcome;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::time::Duration;

/// The scratch authority's project. It is local to the executor process and is
/// never written to the connected authority.
const SCRATCH_PROJECT: &str = "capability-executor-scratch";
/// The scratch blueprint that mirrors exactly the claimed transition.
const SCRATCH_BLUEPRINT_ID: &str = "capability-executor-mirror";
const SCRATCH_KIND: &str = "capability_mirror";
/// Recorded as the actor of the scratch install/grant, never as the real one.
const SCRATCH_ACTOR: &str = "orbit-capability-serve";
/// The authority refuses a longer `detail`; stay well below it.
const DETAIL_BUDGET: usize = 1_800;
/// How much of a capability's own output is quoted in the receipt detail.
const EFFECT_TEXT_BUDGET: usize = 200;

// ---------------------------------------------------------------------------
// Wire shapes this runner reads. They are the server's, not the runner's.
// ---------------------------------------------------------------------------

/// One claimed intent, as this runner needs it.
struct Claim {
    command_id: String,
    item_id: String,
    action_id: String,
    capability_id: String,
    from: String,
    to: String,
    /// The frozen invocation the server recorded. The local provider runs it.
    plan: orbit_blueprint::ActionInvocation,
}

impl Claim {
    /// Accept a claim only when the server says this invocation holds it: the
    /// state, the executor device and the claim token all have to be ours.
    fn parse(body: &Value, token: &str, device: &str) -> Option<Self> {
        let request = body.get("request")?;
        let text = |key: &str| request.get(key).and_then(Value::as_str);
        if text("state")? != "claimed"
            || text("executor_device_id")? != device
            || text("claim_token")? != token
        {
            return None;
        }
        let plan: orbit_blueprint::ActionInvocation =
            serde_json::from_value(body.get("execution_plan")?.clone()).ok()?;
        Some(Self {
            command_id: text("command_id")?.to_owned(),
            item_id: text("item_id")?.to_owned(),
            action_id: text("action_id")?.to_owned(),
            capability_id: text("capability_id")?.to_owned(),
            from: text("from")?.to_owned(),
            to: text("to")?.to_owned(),
            plan,
        })
    }
}

// ---------------------------------------------------------------------------
// Transport: the connected CLI's existing credential and URL resolution.
// ---------------------------------------------------------------------------

struct Transport {
    base: String,
    token: String,
    device: String,
    client: Client,
}

/// A refusal from the connected authority. The message is the server's own
/// fixed text, never a local paraphrase, so a caller can branch on it.
struct Failure {
    status: Option<u16>,
    message: String,
}

impl Failure {
    fn into_cli(self, operation: &str) -> CliError {
        match self.status {
            Some(status) => CliError::Usage(format!(
                "capability authority rejected {operation} (HTTP {status}): {}",
                self.message
            )),
            None => CliError::Usage(format!(
                "capability authority unreachable for {operation}: {}",
                self.message
            )),
        }
    }
}

impl Transport {
    fn connect(server: &str) -> Result<Self, CliError> {
        let base = super::connected_blueprint::server_url(server)?;
        let token = super::connected_blueprint::bearer_token()?;
        let device = super::connected_blueprint::device_identity()?;
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| CliError::Usage(error.to_string()))?;
        Ok(Self {
            base,
            token,
            device,
            client,
        })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/v1/authoritative/{path}", self.base)
    }

    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, Failure> {
        let response = self
            .client
            .get(self.endpoint(path))
            .query(query)
            .bearer_auth(&self.token)
            .header("x-orbit-device-id", self.device.as_str())
            .send()
            .map_err(|error| Failure {
                status: None,
                message: format!("transport: {error}"),
            })?;
        decode(response)
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value, Failure> {
        let response = self
            .client
            .post(self.endpoint(path))
            .json(body)
            .bearer_auth(&self.token)
            .header("x-orbit-device-id", self.device.as_str())
            .send()
            .map_err(|error| Failure {
                status: None,
                message: format!("transport: {error}"),
            })?;
        decode(response)
    }
}

fn decode(response: reqwest::blocking::Response) -> Result<Value, Failure> {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if !status.is_success() {
        return Err(Failure {
            status: Some(status.as_u16()),
            message: server_text(&body),
        });
    }
    serde_json::from_str(&body).map_err(|error| Failure {
        status: Some(status.as_u16()),
        message: format!("unreadable response: {error}"),
    })
}

/// The server answers refusals with `{"error": "<fixed text>"}`. Quote that text
/// verbatim; fall back to a bounded, control-character-free rendering of the
/// body so a proxy page can never be replayed as a reason.
fn server_text(body: &str) -> String {
    if let Some(message) = serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }) {
        return message;
    }
    let cleaned: String = body
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(300)
        .collect();
    cleaned.trim().to_owned()
}

// ---------------------------------------------------------------------------
// Local provider dispatch.
// ---------------------------------------------------------------------------

/// What the local provider did, in this process.
#[derive(Debug)]
struct LocalRun {
    outcome: CapabilityOutcome,
    /// The honest description of where the provider actually wrote. It always
    /// says it was the scratch authority, and whether a declared effect was
    /// submitted with this report.
    effect: Value,
    /// The declared effect payload to submit, present only when the provider
    /// really produced one that matches its contract's declared kind.
    declared: Option<Value>,
    /// Why the provider did not run, when it did not.
    refusal: Option<String>,
}

/// What the provider produced, split into the description a reader sees and the
/// payload the authority may persist.
struct ScratchEffect {
    /// The receipt-detail document. Never claims the write reached the authority.
    document: Value,
    /// The declared, schema-shaped payload for the result body, if any.
    declared: Option<Value>,
}

/// The seam between the serve loop and provider dispatch, so the loop's
/// refusal and reporting rules can be exercised without a real provider.
trait PlanRunner {
    fn run(&mut self, composition: &mut TrustedComposition, claim: &Claim) -> LocalRun;
}

/// The real runner: the local `TrustedComposition` provider, nothing else.
struct LocalProviderRunner;

impl PlanRunner for LocalProviderRunner {
    fn run(&mut self, composition: &mut TrustedComposition, claim: &Claim) -> LocalRun {
        match run_local_plan(composition, claim) {
            Ok(run) => run,
            // Nothing executed, so this is a definite `failed`: an automatic
            // retry is not allowed, but a reconcile is not needed either.
            Err(reason) => LocalRun {
                outcome: CapabilityOutcome::Failed,
                effect: json!({
                    "scope": "executor_process",
                    "submitted_to_authority": false,
                    "recorded": Value::Null,
                    "reason": "the provider did not run, so it produced no effect",
                }),
                declared: None,
                refusal: Some(reason),
            },
        }
    }
}

/// Dispatch one claimed plan through the local provider.
///
/// The provider's contract needs a host, and the executor is not the authority,
/// so the plan is replayed against a scratch authority built from the claim. The
/// plan is compared before dispatch: an executor that cannot reproduce the
/// frozen plan refuses instead of running something else.
fn run_local_plan(composition: &mut TrustedComposition, claim: &Claim) -> Result<LocalRun, String> {
    if claim.plan.command_id != claim.command_id
        || claim.plan.action_id != claim.action_id
        || claim.plan.capability != claim.capability_id
    {
        return Err("claimed execution plan does not describe the claimed request".to_owned());
    }
    let at_ms = now_ms();
    let mut host = orbit_runtime::AuthoritativeHost::open(":memory:")
        .map_err(|_| "executor scratch authority unavailable".to_owned())?;
    let project = ProjectId::new(SCRATCH_PROJECT)
        .map_err(|_| "invalid scratch project identifier".to_owned())?;
    // A scratch project in this process only. It exists to give the provider the
    // item, transition and grant its own contract reads; nothing here is ever
    // sent to the connected authority.
    let activity = ActivityId::new(generate_id("capability-executor-activity"))
        .map_err(|_| "invalid scratch activity identifier".to_owned())?;
    let (revision, _, _) = host
        .create_project_accepted(
            SCRATCH_ACTOR,
            &generate_id("capability-executor"),
            0,
            CreateProjectWithActivityCommand {
                project: CreateProjectCommand {
                    id: project.clone(),
                    name: "Capability executor scratch".into(),
                },
                activity_id: activity,
                origin: ActivityOrigin::Human,
                summary: "capability executor scratch project".into(),
            },
        )
        .map_err(|_| "executor scratch authority refused its project".to_owned())?;
    let installed = std::collections::BTreeSet::from([claim.capability_id.clone()]);
    let manifest = scratch_manifest(claim)?;
    host.publish_project_blueprint(&manifest, &installed)
        .map_err(|_| "executor scratch authority refused the mirrored workflow".to_owned())?;
    host.set_project_workflow_accepted(
        SCRATCH_ACTOR,
        &generate_id("capability-executor"),
        revision,
        project,
        SCRATCH_BLUEPRINT_ID.to_owned(),
        Some(1),
    )
    .map_err(|_| "executor scratch authority refused the mirrored binding".to_owned())?;
    host.install_trusted_capability(composition, &claim.capability_id, at_ms)
        .map_err(|_| "executor scratch authority refused the mirrored installation".to_owned())?;
    host.authorize_trusted_capability(&claim.capability_id, SCRATCH_PROJECT, SCRATCH_ACTOR, at_ms)
        .map_err(|_| "executor scratch authority refused the mirrored grant".to_owned())?;
    host.create_project_work_item(
        &claim.item_id,
        SCRATCH_PROJECT,
        SCRATCH_KIND,
        std::collections::BTreeMap::new(),
    )
    .map_err(|_| "executor scratch authority refused the mirrored item".to_owned())?;
    let stored = host
        .request_capability_action(
            composition,
            CapabilityActionRequest {
                item_id: claim.item_id.clone(),
                expected_revision: 0,
                action_id: claim.action_id.clone(),
                command_id: claim.command_id.clone(),
                inputs: claim.plan.parameters.clone(),
            },
        )
        .map_err(|_| "the claimed action could not be reproduced locally".to_owned())?;
    let local_plan: orbit_blueprint::ActionInvocation = serde_json::from_str(&stored.plan_json)
        .map_err(|_| "the local action plan is unreadable".to_owned())?;
    if local_plan != claim.plan {
        return Err("the claimed execution plan does not match this build's local plan".to_owned());
    }
    let receipt = host
        .execute_capability_action(composition, &claim.command_id, at_ms)
        .map_err(|_| "the local provider could not be dispatched".to_owned())?;
    let outcome = match receipt.outcome.as_str() {
        "succeeded" => CapabilityOutcome::Succeeded,
        "failed" => CapabilityOutcome::Failed,
        // Anything else is ambiguous by construction: never guess `succeeded`.
        _ => CapabilityOutcome::Uncertain,
    };
    let effect = scratch_effect(&host, claim);
    Ok(LocalRun {
        outcome,
        effect: effect.document,
        declared: effect.declared,
        refusal: None,
    })
}

/// A minimal workflow that mirrors exactly the claimed transition: the same
/// states, the same edge, the same action bound to the same capability.
fn scratch_manifest(claim: &Claim) -> Result<orbit_blueprint::Blueprint, String> {
    let document = json!({
        "id": SCRATCH_BLUEPRINT_ID,
        "version": 1,
        "name": "Capability executor mirror",
        "description": "Executor-local projection of one claimed capability action",
        "required_capabilities": [claim.capability_id],
        "work_items": [{
            "id": SCRATCH_KIND,
            "name": "Mirror",
            "initial_state": claim.from,
            "states": [claim.from, claim.to],
            "fields": [],
            "transitions": [{
                "from": claim.from,
                "to": claim.to,
                "actions": [{
                    "id": claim.action_id,
                    "capability": claim.capability_id,
                    "description": "Mirrored capability action",
                    "parameters": {},
                }],
            }],
        }],
    });
    let text = serde_json::to_string(&document)
        .map_err(|_| "the mirrored workflow is not serializable".to_owned())?;
    orbit_blueprint::Blueprint::from_json(
        &text,
        &std::collections::BTreeSet::from([claim.capability_id.clone()]),
    )
    .map_err(|_| "the claimed transition cannot be mirrored as a workflow".to_owned())
}

/// What the provider produced in the scratch authority, in the shape the
/// authority may persist.
///
/// The declared kind comes from the capability's compile-time contract, never
/// from the claim and never from a default. A capability that declares no effect
/// submits nothing; a provider that recorded nothing submits nothing; the honest
/// reason goes in the receipt detail instead of a fabricated payload.
fn scratch_effect(host: &orbit_runtime::AuthoritativeHost, claim: &Claim) -> ScratchEffect {
    let Some(declared) = orbit_runtime::capability_contracts::contract(&claim.capability_id)
        .and_then(|contract| contract.effect)
    else {
        return ScratchEffect {
            document: json!({
                "scope": "executor_process",
                "submitted_to_authority": false,
                "recorded": Value::Null,
                "reason": "this capability's contract declares no effect; only its outcome travelled",
            }),
            declared: None,
        };
    };
    // Only one effect kind is readable out of this process. An unknown kind is
    // reported, not guessed at.
    if declared.kind != orbit_runtime::capability_contracts::REFERENCE_EFFECT_KIND {
        return ScratchEffect {
            document: json!({
                "scope": "executor_process",
                "submitted_to_authority": false,
                "recorded": Value::Null,
                "reason": format!(
                    "this runner cannot read the declared effect kind {}",
                    declared.kind
                ),
            }),
            declared: None,
        };
    }
    let notes = match host.reference_work_item_notes(&claim.item_id) {
        Ok(notes) => notes,
        Err(_) => {
            return ScratchEffect {
                document: json!({
                    "scope": "executor_scratch_authority",
                    "submitted_to_authority": false,
                    "recorded": Value::Null,
                    "reason": "the scratch authority could not report the effect",
                }),
                declared: None,
            };
        }
    };
    // One claim is one command id, so only this command's own row can be this
    // action's effect. Anything else belongs to a different request.
    let Some(note) = notes
        .iter()
        .find(|note| note.command_id == claim.command_id)
    else {
        return ScratchEffect {
            document: json!({
                "scope": "executor_scratch_authority",
                "submitted_to_authority": false,
                "recorded": Value::Null,
                "reason": "the provider recorded no readable effect in this process",
            }),
            declared: None,
        };
    };
    let mut payload = json!({
        "kind": declared.kind,
        "item_id": claim.item_id,
        "note": note.note,
    });
    if let Some(tag) = &note.tag {
        payload["tag"] = json!(tag);
    }
    ScratchEffect {
        document: json!({
            "scope": "executor_scratch_authority",
            "submitted_to_authority": true,
            "kind": declared.kind,
            "item_id": claim.item_id,
            "recorded": notes
                .iter()
                .map(|note| json!({
                    "command_id": note.command_id,
                    "note": bounded_text(&note.note),
                    "tag": note.tag,
                }))
                .collect::<Vec<_>>(),
        }),
        declared: Some(payload),
    }
}

fn bounded_text(text: &str) -> String {
    let mut bounded: String = text.chars().take(EFFECT_TEXT_BUDGET).collect();
    if text.chars().count() > EFFECT_TEXT_BUDGET {
        bounded.push('…');
    }
    bounded
}

/// The durable receipt detail: the executor's own account of its local run. The
/// authority adds its `effect.applied` statement to this document, so a reader
/// can tell a scratch write from a durable one.
fn receipt_detail(claim: &Claim, device: &str, run: &LocalRun) -> String {
    let document = json!({
        "executor": device,
        "capability": claim.capability_id,
        "action_id": claim.action_id,
        "command_id": claim.command_id,
        "provider_outcome": run.outcome.as_str(),
        "effect": run.effect,
        "refusal": run.refusal,
    });
    match serde_json::to_string(&document) {
        Ok(rendered) if rendered.chars().count() <= DETAIL_BUDGET => rendered,
        // A detail the authority would reject is worse than a short one. The
        // effect payload itself travels in its own field, so dropping the quoted
        // rows here never loses the durable part.
        _ => json!({
            "executor": device,
            "capability": claim.capability_id,
            "action_id": claim.action_id,
            "command_id": claim.command_id,
            "provider_outcome": run.outcome.as_str(),
            "effect": {
                "scope": "executor_process",
                "submitted_to_authority": run.declared.is_some(),
                "recorded": Value::Null,
                "truncated": true,
            },
            "refusal": run.refusal,
        })
        .to_string(),
    }
}

fn local_summary(run: &LocalRun) -> Value {
    json!({
        "outcome": run.outcome.as_str(),
        "effect": run.effect,
        "submitted_effect": run.declared,
        "refusal": run.refusal,
    })
}

// ---------------------------------------------------------------------------
// The serve loop.
// ---------------------------------------------------------------------------

/// The capabilities this build can actually execute: an enabled descriptor in
/// the local `TrustedComposition` **and** a registered provider for it. Nothing
/// is hardcoded, so a capability that gains or loses a provider changes the
/// announcement without a second list to maintain.
fn executable_capabilities(composition: &TrustedComposition) -> Vec<String> {
    let mut ids: Vec<String> = composition
        .capabilities()
        .filter(|capability| {
            capability.descriptor.enabled && composition.has_executor(&capability.descriptor.id)
        })
        .map(|capability| capability.descriptor.id.clone())
        .collect();
    ids.sort();
    ids
}

/// Announce, claim at most one pending request, run it locally, report once.
fn serve_once(
    transport: &Transport,
    announced: &[String],
    composition: &mut TrustedComposition,
    runner: &mut dyn PlanRunner,
) -> Result<Value, CliError> {
    let announcement = transport
        .post(
            "capability-executors/announce",
            &json!({ "capability_ids": announced }),
        )
        .map_err(|failure| failure.into_cli("the executor announcement"))?;
    if announcement.get("device_id").and_then(Value::as_str) != Some(transport.device.as_str()) {
        return Err(CliError::Usage(
            "the connected authority announced a different device than this executor".into(),
        ));
    }
    let pending = transport
        .get("capability-actions/list", &[("state", "pending")])
        .map_err(|failure| failure.into_cli("the pending request list"))?;
    let requests = pending
        .get("requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let candidates: Vec<Value> = requests
        .into_iter()
        .filter(|request| {
            request
                .get("capability_id")
                .and_then(Value::as_str)
                .is_some_and(|capability| announced.iter().any(|known| known == capability))
        })
        .collect();
    if candidates.is_empty() {
        return Ok(json!({
            "status": "idle",
            "command": "capability.serve",
            "device_id": transport.device,
            "announced": announced,
            "reason": "no pending capability action is claimable by this executor",
        }));
    }
    // A contender that already holds a request wins the race: keep looking, but
    // never mint a second token for a request this invocation lost.
    let mut refused: Vec<Value> = Vec::new();
    let mut claimed: Option<(Claim, String)> = None;
    for candidate in &candidates {
        let Some(command_id) = candidate.get("command_id").and_then(Value::as_str) else {
            continue;
        };
        let token = generate_id("claim");
        let claim = transport.post(
            "capability-actions/claim",
            &json!({ "command_id": command_id, "claim_token": token }),
        );
        match claim {
            Ok(body) => match Claim::parse(&body, &token, &transport.device) {
                Some(claim) => {
                    claimed = Some((claim, token));
                    break;
                }
                // The server answered something other than "this invocation
                // holds the claim". Running it would execute an intent that is
                // not ours, so stop with the fact instead.
                None => {
                    return Ok(json!({
                        "status": "claim_mismatch",
                        "command": "capability.serve",
                        "device_id": transport.device,
                        "command_id": command_id,
                        "reason": "the claim was not bound to this executor and this invocation; nothing was executed",
                    }));
                }
            },
            Err(failure) if failure.status == Some(409) => {
                refused.push(json!({
                    "command_id": command_id,
                    "reason": failure.message,
                }));
            }
            Err(failure) => return Err(failure.into_cli("the capability claim")),
        }
    }
    let Some((claim, token)) = claimed else {
        return Ok(json!({
            "status": "contended",
            "command": "capability.serve",
            "device_id": transport.device,
            "announced": announced,
            "claims_refused": refused,
            "reason": "another executor claimed every pending request this executor could run",
        }));
    };
    let run = runner.run(composition, &claim);
    let detail = receipt_detail(&claim, &transport.device, &run);
    // The effect travels as its own field, not inside `detail`: the authority
    // validates it against the capability's contract and persists it. When the
    // provider produced nothing readable the field is absent, and `detail` says
    // why.
    let mut report_body = json!({
        "command_id": claim.command_id,
        "claim_token": token,
        "outcome": run.outcome.as_str(),
        "detail": detail,
    });
    if let Some(effect) = &run.declared {
        report_body["effect"] = effect.clone();
    }
    let report = transport.post("capability-actions/result", &report_body);
    let identity = json!({
        "command_id": claim.command_id,
        "item_id": claim.item_id,
        "capability_id": claim.capability_id,
        "action_id": claim.action_id,
    });
    match report {
        Ok(body) => Ok(json!({
            "status": "reported",
            "command": "capability.serve",
            "device_id": transport.device,
            "outcome": run.outcome.as_str(),
            "action": identity,
            "state": body.get("request").and_then(|request| request.get("state")).cloned(),
            "receipt": body.get("receipt").cloned(),
            "local": local_summary(&run),
            "claims_refused": refused,
        })),
        Err(failure) => {
            // The report was not confirmed. One read tells the truth about the
            // durable state; nothing is re-sent, because a second claim or a
            // second result could duplicate the effect.
            let observed = transport
                .get(
                    "capability-actions/show",
                    &[("command_id", claim.command_id.as_str())],
                )
                .ok();
            let receipt = observed
                .as_ref()
                .and_then(|body| body.get("receipt"))
                .filter(|receipt| !receipt.is_null())
                .cloned();
            // A durable receipt, when there is one, is the outcome. Without one
            // the honest answer is `uncertain`, whatever the provider said.
            let durable_outcome = receipt
                .as_ref()
                .and_then(|receipt| receipt.get("outcome"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            Ok(json!({
                "status": if receipt.is_some() { "confirmed_by_read" } else { "unconfirmed" },
                "command": "capability.serve",
                "device_id": transport.device,
                "outcome": durable_outcome
                    .unwrap_or_else(|| CapabilityOutcome::Uncertain.as_str().to_owned()),
                "provider_outcome": run.outcome.as_str(),
                "action": identity,
                "reason": failure.message,
                "durable_receipt": receipt,
                "local": local_summary(&run),
                "claims_refused": refused,
                "next": "do not retry: an operator reconciles the request explicitly",
            }))
        }
    }
}

/// `orbit capability …`: the connected deployment's capability **executor**.
/// The local authority executes in-process and has no use for a runner, so this
/// command exists only to be pointed at a connected deployment.
#[derive(Debug, Args)]
pub struct CapabilityRunnerCommand {
    #[command(subcommand)]
    pub command: CapabilityRunnerSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum CapabilityRunnerSubcommand {
    /// Announce the capabilities this build can execute, claim one pending
    /// request, run it through the local provider, report the outcome, and exit.
    ///
    /// The connected authority never dispatches provider code: this process is
    /// the executor, and it is the only place a provider runs.
    Serve {
        /// Handle at most one pending request. A second request needs a second
        /// process; no invocation ever claims twice.
        #[arg(long)]
        once: bool,
    },
}

/// Entry point for `orbit capability serve`. Connected deployment only.
pub(super) fn run(server: &str, command: CapabilityRunnerCommand) -> Result<Response, CliError> {
    let CapabilityRunnerSubcommand::Serve { once } = command.command;
    if !once {
        return Err(CliError::Usage(
            "capability serve requires --once: one process handles at most one claimed request"
                .into(),
        ));
    }
    let transport = Transport::connect(server)?;
    let mut composition =
        orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
    let announced = executable_capabilities(&composition);
    let summary = serve_once(
        &transport,
        &announced,
        &mut composition,
        &mut LocalProviderRunner,
    )?;
    Ok(Response::Authority { result: summary })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connected_test_authority::{Call, Stub};
    use serde_json::Value;

    const HABIT_CAPABILITY: &str = "orbit.habit.recurrence";
    const REFERENCE_CAPABILITY: &str = "orbit.reference.work-item-note";
    const TOKEN: &str = "executor-secret-0123456789abcdefghij";
    const DEVICE: &str = "exec-1";

    fn transport(base: &str) -> Transport {
        Transport {
            base: base.to_owned(),
            token: TOKEN.to_owned(),
            device: DEVICE.to_owned(),
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("test client"),
        }
    }

    fn habit_parameters() -> Value {
        json!({
            "cadence": {"kind": "daily"},
            "periods": [{"start_ms": 0, "end_ms": 86_400_000}],
            "occurrences": [],
        })
    }

    fn pending(command_id: &str, capability: &str, action_id: &str) -> Value {
        json!({
            "command_id": command_id,
            "item_id": "item-1",
            "item_revision": 0,
            "action_id": action_id,
            "capability_id": capability,
            "from": "open",
            "to": "closed",
            "state": "pending",
            "inputs": habit_parameters(),
            "created_at_ms": 10,
            "executor_device_id": Value::Null,
            "claim_token": Value::Null,
        })
    }

    fn announcement() -> Value {
        json!({
            "device_id": DEVICE,
            "capabilities": [{"capability_id": HABIT_CAPABILITY, "announced_at_ms": 1}],
        })
    }

    /// A claim response that binds the request to this invocation, echoing the
    /// token the runner minted — the runner must never reuse a token it did not
    /// mint.
    fn claim_response(call: &Call) -> Value {
        let command_id = call.body["command_id"].as_str().unwrap_or_default();
        let token = call.body["claim_token"].as_str().unwrap_or_default();
        let mut request = pending(command_id, HABIT_CAPABILITY, "evaluate_periods");
        request["state"] = json!("claimed");
        request["executor_device_id"] = json!(DEVICE);
        request["claim_token"] = json!(token);
        json!({
            "request": request,
            "execution_plan": {
                "action_id": "evaluate_periods",
                "capability": HABIT_CAPABILITY,
                "parameters": habit_parameters(),
                "command_id": command_id,
            },
        })
    }

    /// A claim response for the reference capability, so the real local provider
    /// can be dispatched through the loop and its effect observed on the wire.
    fn reference_claim_response(call: &Call) -> Value {
        let command_id = call.body["command_id"].as_str().unwrap_or_default();
        let token = call.body["claim_token"].as_str().unwrap_or_default();
        json!({
            "request": {
                "command_id": command_id,
                "item_id": "item-1",
                "item_revision": 0,
                "action_id": "append_note",
                "capability_id": REFERENCE_CAPABILITY,
                "from": "open",
                "to": "closed",
                "state": "claimed",
                "inputs": {"note": "hello"},
                "created_at_ms": 10,
                "executor_device_id": DEVICE,
                "claim_token": token,
            },
            "execution_plan": {
                "action_id": "append_note",
                "capability": REFERENCE_CAPABILITY,
                "parameters": {"note": "hello"},
                "command_id": command_id,
            },
        })
    }

    fn succeeded_result(call: &Call) -> Value {
        let command_id = call.body["command_id"].as_str().unwrap_or_default();
        let mut request = pending(command_id, HABIT_CAPABILITY, "evaluate_periods");
        request["state"] = json!("succeeded");
        request["executor_device_id"] = json!(DEVICE);
        json!({
            "request": request,
            "receipt": {
                "outcome": "succeeded",
                "detail": {"state": "succeeded"},
                "reported_at_ms": 20,
                "reported_by": DEVICE,
            },
        })
    }

    /// A runner double: proves what the loop does with a given provider outcome
    /// without needing a provider that can produce it.
    struct StubRunner {
        outcome: CapabilityOutcome,
        runs: usize,
        /// What the double claims the provider produced.
        declared: Option<Value>,
    }

    impl PlanRunner for StubRunner {
        fn run(&mut self, _composition: &mut TrustedComposition, _claim: &Claim) -> LocalRun {
            self.runs += 1;
            LocalRun {
                outcome: self.outcome,
                effect: json!({
                    "scope": "stub",
                    "submitted_to_authority": self.declared.is_some(),
                }),
                declared: self.declared.clone(),
                refusal: None,
            }
        }
    }

    fn run_loop(stub: &Stub, runner: &mut dyn PlanRunner) -> Value {
        let mut composition =
            orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
        let announced = executable_capabilities(&composition);
        assert!(
            announced.iter().any(|id| id == HABIT_CAPABILITY),
            "the built-in composition advertises the habit capability: {announced:?}"
        );
        serve_once(
            &transport(&stub.base()),
            &announced,
            &mut composition,
            runner,
        )
        .expect("serve loop")
    }

    #[test]
    fn announces_claims_runs_the_local_provider_and_reports_once() {
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                assert!(call.query.contains("state=pending"), "{}", call.query);
                return (
                    200,
                    json!({"requests": [pending("cap-1", HABIT_CAPABILITY, "evaluate_periods")]}),
                );
            }
            if path.ends_with("/capability-actions/claim") {
                return (200, claim_response(call));
            }
            if path.ends_with("/capability-actions/result") {
                return (200, succeeded_result(call));
            }
            (404, json!({"error": "route absent"}))
        });
        let summary = run_loop(&stub, &mut LocalProviderRunner);
        assert_eq!(summary["status"], "reported", "{summary}");
        assert_eq!(summary["outcome"], "succeeded", "{summary}");
        assert_eq!(summary["action"]["command_id"], "cap-1", "{summary}");
        assert_eq!(summary["state"], "succeeded", "{summary}");
        assert_eq!(summary["receipt"]["reported_by"], DEVICE, "{summary}");
        // The local run is reported, and it says plainly where the provider
        // actually wrote: the scratch authority, with nothing submitted because
        // this capability declares no effect.
        assert_eq!(summary["local"]["outcome"], "succeeded", "{summary}");
        assert_eq!(
            summary["local"]["effect"]["submitted_to_authority"],
            json!(false),
            "{summary}"
        );
        assert!(summary["local"]["submitted_effect"].is_null(), "{summary}");
        // Announce, list, claim, result — exactly once, in that order.
        let calls = stub.calls();
        assert_eq!(calls[0].method, "POST", "the announcement is a write");
        assert_eq!(calls[1].method, "GET", "the pending list is a read");
        let paths: Vec<String> = calls
            .into_iter()
            .map(|call| call.path.rsplit('/').next().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            paths,
            vec!["announce", "list", "claim", "result"],
            "{paths:?}"
        );
        let result = stub.calls_to("/capability-actions/result");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].body["outcome"], "succeeded");
        // The habit capability declares no effect, so nothing is submitted and
        // the report body has no `effect` field at all.
        assert_eq!(
            result[0].body.get("effect"),
            None,
            "a capability with no declared effect submits none: {}",
            result[0].body
        );
        let detail = result[0].body["detail"].as_str().expect("detail string");
        let detail: Value = serde_json::from_str(detail).expect("detail is JSON");
        assert_eq!(detail["capability"], HABIT_CAPABILITY);
        assert_eq!(detail["command_id"], "cap-1");
        assert_eq!(detail["effect"]["submitted_to_authority"], json!(false));
        assert!(
            detail["effect"]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("declares no effect")),
            "{detail}"
        );
        // No token in the report, and no token anywhere but the claim body.
        assert!(!detail.to_string().contains(TOKEN));
    }

    #[test]
    fn the_loop_reports_the_effect_the_provider_really_produced() {
        // The whole chain: the real provider writes in the scratch authority, the
        // runner reads its own row back, and the loop puts that payload in the
        // result body the authority will validate and persist.
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                return (
                    200,
                    json!({"requests": [pending("cap-ref", REFERENCE_CAPABILITY, "append_note")]}),
                );
            }
            if path.ends_with("/capability-actions/claim") {
                return (200, reference_claim_response(call));
            }
            if path.ends_with("/capability-actions/result") {
                return (200, succeeded_result(call));
            }
            (404, json!({"error": "route absent"}))
        });
        let summary = run_loop(&stub, &mut LocalProviderRunner);
        assert_eq!(summary["status"], "reported", "{summary}");
        assert_eq!(
            summary["local"]["submitted_effect"],
            json!({"kind": "work_item_note", "item_id": "item-1", "note": "hello"}),
            "{summary}"
        );
        let results = stub.calls_to("/capability-actions/result");
        assert_eq!(results.len(), 1);
        // The declared effect is its own field, not a string buried in `detail`.
        assert_eq!(
            results[0].body["effect"],
            json!({"kind": "work_item_note", "item_id": "item-1", "note": "hello"}),
            "{}",
            results[0].body
        );
        let detail: Value =
            serde_json::from_str(results[0].body["detail"].as_str().expect("detail"))
                .expect("detail JSON");
        assert_eq!(detail["effect"]["submitted_to_authority"], json!(true));
        assert_eq!(detail["effect"]["recorded"][0]["note"], "hello");
    }

    #[test]
    fn nothing_to_claim_is_an_idle_exit_without_touching_a_request() {
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                return (200, json!({"requests": []}));
            }
            (404, json!({"error": "route absent"}))
        });
        let summary = run_loop(&stub, &mut LocalProviderRunner);
        assert_eq!(summary["status"], "idle", "{summary}");
        assert_eq!(summary["announced"][0], HABIT_CAPABILITY);
        assert!(stub.calls_to("/capability-actions/claim").is_empty());
        assert!(stub.calls_to("/capability-actions/result").is_empty());
    }

    #[test]
    fn a_provider_uncertain_outcome_is_reported_as_uncertain_and_never_retried() {
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                return (
                    200,
                    json!({"requests": [pending("cap-1", HABIT_CAPABILITY, "x")]}),
                );
            }
            if path.ends_with("/capability-actions/claim") {
                return (200, claim_response(call));
            }
            if path.ends_with("/capability-actions/result") {
                return (
                    200,
                    json!({
                        "request": {"state": "uncertain", "command_id": "cap-1"},
                        "receipt": {
                            "outcome": "uncertain",
                            "detail": {"state": "uncertain"},
                            "reported_at_ms": 20,
                            "reported_by": DEVICE,
                        },
                    }),
                );
            }
            (404, json!({"error": "route absent"}))
        });
        let mut runner = StubRunner {
            outcome: CapabilityOutcome::Uncertain,
            runs: 0,
            declared: None,
        };
        let summary = run_loop(&stub, &mut runner);
        assert_eq!(runner.runs, 1);
        assert_eq!(summary["status"], "reported", "{summary}");
        assert_eq!(summary["outcome"], "uncertain", "{summary}");
        assert_eq!(
            stub.calls_to("/capability-actions/result")[0].body["outcome"],
            "uncertain"
        );
        assert_eq!(stub.calls_to("/capability-actions/claim").len(), 1);
        // An uncertain outcome is never retried by this process.
        assert!(summary["next"].is_null(), "{summary}");
    }

    #[test]
    fn an_unconfirmed_report_is_uncertain_and_never_reclaimed() {
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                return (
                    200,
                    json!({"requests": [pending("cap-1", HABIT_CAPABILITY, "x")]}),
                );
            }
            if path.ends_with("/capability-actions/claim") {
                return (200, claim_response(call));
            }
            if path.ends_with("/capability-actions/result") {
                return (500, json!({"error": "capability storage error"}));
            }
            if path.ends_with("/capability-actions/show") {
                return (
                    200,
                    json!({"request": {"state": "claimed"}, "receipt": Value::Null}),
                );
            }
            (404, json!({"error": "route absent"}))
        });
        let mut runner = StubRunner {
            outcome: CapabilityOutcome::Succeeded,
            runs: 0,
            declared: None,
        };
        let summary = run_loop(&stub, &mut runner);
        assert_eq!(summary["status"], "unconfirmed", "{summary}");
        assert_eq!(summary["outcome"], "uncertain", "{summary}");
        assert_eq!(summary["provider_outcome"], "succeeded", "{summary}");
        assert_eq!(
            summary["reason"], "capability storage error",
            "the server's fixed text is quoted verbatim: {summary}"
        );
        // Exactly one claim and one report: an unconfirmed report is never
        // re-sent and the request is never re-claimed.
        assert_eq!(stub.calls_to("/capability-actions/claim").len(), 1);
        assert_eq!(stub.calls_to("/capability-actions/result").len(), 1);
        assert!(
            summary["next"]
                .as_str()
                .is_some_and(|next| next.contains("do not retry"))
        );
    }

    #[test]
    fn a_lost_report_is_confirmed_by_the_durable_receipt_it_left_behind() {
        // The result POST answers 409 because the settlement had already been
        // written; the durable receipt is the truth and must be reported as such.
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                return (
                    200,
                    json!({"requests": [pending("cap-1", HABIT_CAPABILITY, "x")]}),
                );
            }
            if path.ends_with("/capability-actions/claim") {
                return (200, claim_response(call));
            }
            if path.ends_with("/capability-actions/result") {
                return (409, json!({"error": "capability action already settled"}));
            }
            if path.ends_with("/capability-actions/show") {
                return (
                    200,
                    json!({
                        "request": {"state": "succeeded"},
                        "receipt": {
                            "outcome": "succeeded",
                            "detail": {"state": "succeeded"},
                            "reported_at_ms": 20,
                            "reported_by": DEVICE,
                        },
                    }),
                );
            }
            (404, json!({"error": "route absent"}))
        });
        let mut runner = StubRunner {
            outcome: CapabilityOutcome::Succeeded,
            runs: 0,
            declared: None,
        };
        let summary = run_loop(&stub, &mut runner);
        assert_eq!(summary["status"], "confirmed_by_read", "{summary}");
        assert_eq!(summary["outcome"], "succeeded", "{summary}");
        assert_eq!(summary["durable_receipt"]["reported_by"], DEVICE);
        assert_eq!(stub.calls_to("/capability-actions/result").len(), 1);
        assert_eq!(stub.calls_to("/capability-actions/claim").len(), 1);
    }

    #[test]
    fn a_claim_lost_to_another_executor_moves_to_the_next_candidate() {
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                return (
                    200,
                    json!({"requests": [
                        pending("cap-first", HABIT_CAPABILITY, "evaluate_periods"),
                        pending("cap-second", HABIT_CAPABILITY, "evaluate_periods"),
                    ]}),
                );
            }
            if path.ends_with("/capability-actions/claim") {
                if call.body["command_id"] == "cap-first" {
                    return (409, json!({"error": "capability action already claimed"}));
                }
                return (200, claim_response(call));
            }
            if path.ends_with("/capability-actions/result") {
                return (200, succeeded_result(call));
            }
            (404, json!({"error": "route absent"}))
        });
        let mut runner = StubRunner {
            outcome: CapabilityOutcome::Succeeded,
            runs: 0,
            declared: None,
        };
        let summary = run_loop(&stub, &mut runner);
        assert_eq!(summary["status"], "reported", "{summary}");
        assert_eq!(summary["action"]["command_id"], "cap-second", "{summary}");
        assert_eq!(
            summary["claims_refused"][0]["command_id"], "cap-first",
            "{summary}"
        );
        assert_eq!(
            summary["claims_refused"][0]["reason"],
            "capability action already claimed"
        );
        // Only the request this invocation claimed was executed.
        assert_eq!(runner.runs, 1);
        let results = stub.calls_to("/capability-actions/result");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].body["command_id"], "cap-second");
    }

    #[test]
    fn a_claim_not_bound_to_this_executor_is_never_executed() {
        let stub = Stub::start(|call| {
            let path = call.path.as_str();
            if path.ends_with("/capability-executors/announce") {
                return (200, announcement());
            }
            if path.ends_with("/capability-actions/list") {
                return (
                    200,
                    json!({"requests": [pending("cap-1", HABIT_CAPABILITY, "x")]}),
                );
            }
            if path.ends_with("/capability-actions/claim") {
                // Another executor's claim, handed back to us.
                let mut response = claim_response(call);
                response["request"]["executor_device_id"] = json!("exec-other");
                return (200, response);
            }
            (404, json!({"error": "route absent"}))
        });
        let mut runner = StubRunner {
            outcome: CapabilityOutcome::Succeeded,
            runs: 0,
            declared: None,
        };
        let summary = run_loop(&stub, &mut runner);
        assert_eq!(summary["status"], "claim_mismatch", "{summary}");
        assert_eq!(runner.runs, 0);
        assert!(stub.calls_to("/capability-actions/result").is_empty());
    }

    #[test]
    fn the_announcement_is_derived_from_the_local_composition() {
        let composition =
            orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
        let announced = executable_capabilities(&composition);
        let mut expected: Vec<String> = composition
            .capabilities()
            .map(|capability| capability.descriptor.id.clone())
            .collect();
        expected.sort();
        assert_eq!(announced, expected);
        assert!(announced.contains(&HABIT_CAPABILITY.to_owned()));
        assert!(
            announced.contains(
                &orbit_runtime::capability_composition::REFERENCE_CAPABILITY_ID.to_owned()
            )
        );
    }

    #[test]
    fn the_reference_provider_sends_the_effect_it_produced() {
        use orbit_blueprint::ActionInvocation;
        use orbit_runtime::capability_composition::REFERENCE_CAPABILITY_ID;
        use std::collections::BTreeMap;
        let mut composition =
            orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
        let claim = Claim {
            command_id: "cap-ref".into(),
            item_id: "item-1".into(),
            action_id: "append_note".into(),
            capability_id: REFERENCE_CAPABILITY_ID.into(),
            from: "open".into(),
            to: "closed".into(),
            plan: ActionInvocation {
                action_id: "append_note".into(),
                capability: REFERENCE_CAPABILITY_ID.into(),
                parameters: BTreeMap::from([
                    ("note".to_owned(), json!("written by the runner")),
                    ("tag".to_owned(), json!("demo")),
                ]),
                command_id: "cap-ref".into(),
            },
        };
        // The provider really ran in this process: it recorded its note in the
        // scratch authority, and the outcome is the provider's own.
        let run = run_local_plan(&mut composition, &claim).expect("local provider run");
        assert_eq!(run.outcome, CapabilityOutcome::Succeeded);
        assert!(run.refusal.is_none());
        assert_eq!(run.effect["submitted_to_authority"], json!(true));
        assert_eq!(run.effect["scope"], "executor_scratch_authority");
        assert_eq!(
            run.effect["recorded"][0]["note"], "written by the runner",
            "the provider's own output is what the receipt describes: {run:?}"
        );
        // The payload is the declaration's shape, and it is exactly what the
        // provider recorded — not a template and not a guess.
        assert_eq!(
            run.declared,
            Some(json!({
                "kind": "work_item_note",
                "item_id": "item-1",
                "note": "written by the runner",
                "tag": "demo",
            })),
            "{run:?}"
        );
        // The receipt detail describes the scratch run; the authority adds its
        // own `effect.applied` statement to the same document.
        let detail: Value =
            serde_json::from_str(&receipt_detail(&claim, DEVICE, &run)).expect("detail JSON");
        assert_eq!(detail["executor"], DEVICE);
        assert_eq!(detail["provider_outcome"], "succeeded");
        assert_eq!(
            detail["effect"]["recorded"][0]["note"],
            "written by the runner"
        );
        assert_eq!(detail["effect"]["submitted_to_authority"], json!(true));
        assert_eq!(detail["effect"]["kind"], "work_item_note");
    }

    #[test]
    fn a_provider_that_recorded_nothing_submits_nothing() {
        use orbit_blueprint::ActionInvocation;
        let mut composition =
            orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
        // A habit claim really runs, but its contract declares no effect, so the
        // runner must not invent one for it.
        let claim = Claim {
            command_id: "cap-habit".into(),
            item_id: "item-1".into(),
            action_id: "evaluate_periods".into(),
            capability_id: HABIT_CAPABILITY.into(),
            from: "open".into(),
            to: "closed".into(),
            plan: ActionInvocation {
                action_id: "evaluate_periods".into(),
                capability: HABIT_CAPABILITY.into(),
                parameters: serde_json::from_value(habit_parameters()).expect("parameters"),
                command_id: "cap-habit".into(),
            },
        };
        let run = run_local_plan(&mut composition, &claim).expect("local provider run");
        assert!(run.declared.is_none(), "{run:?}");
        assert_eq!(run.effect["submitted_to_authority"], json!(false));
        assert!(
            run.effect["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("declares no effect")),
            "{run:?}"
        );
    }

    #[test]
    fn the_effect_payload_is_the_claimed_command_s_own_note_or_nothing() {
        use orbit_blueprint::ActionInvocation;
        use orbit_runtime::capability_composition::REFERENCE_CAPABILITY_ID;
        use std::collections::BTreeMap;
        let mut composition =
            orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
        let claim = Claim {
            command_id: "cap-ref".into(),
            item_id: "item-1".into(),
            action_id: "append_note".into(),
            capability_id: REFERENCE_CAPABILITY_ID.into(),
            from: "open".into(),
            to: "closed".into(),
            plan: ActionInvocation {
                action_id: "append_note".into(),
                capability: REFERENCE_CAPABILITY_ID.into(),
                parameters: BTreeMap::from([("note".to_owned(), json!("mine"))]),
                command_id: "cap-ref".into(),
            },
        };
        // A scratch authority with no row for this command yields no payload at
        // all: the runner reads, it never writes one of its own.
        let empty = orbit_runtime::AuthoritativeHost::open(":memory:").expect("scratch host");
        let effect = scratch_effect(&empty, &claim);
        assert!(effect.declared.is_none());
        assert_eq!(effect.document["submitted_to_authority"], json!(false));
        assert!(
            effect.document["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("no readable effect")),
            "{:?}",
            effect.document
        );
        // After the real run the payload is exactly the provider's own row.
        let run = run_local_plan(&mut composition, &claim).expect("local provider run");
        assert_eq!(
            run.declared,
            Some(json!({
                "kind": "work_item_note",
                "item_id": "item-1",
                "note": "mine",
            })),
            "the run's own note is what travels: {run:?}"
        );
    }

    #[test]
    fn an_unreproducible_transition_is_refused_without_running_the_provider() {
        use orbit_blueprint::ActionInvocation;
        use std::collections::BTreeMap;
        let mut composition =
            orbit_runtime::capability_composition::TrustedComposition::with_builtin_reference();
        let claim = Claim {
            command_id: "cap-odd".into(),
            item_id: "item-1".into(),
            action_id: "evaluate_periods".into(),
            capability_id: HABIT_CAPABILITY.into(),
            // A transition to its own state cannot be mirrored as a workflow.
            from: "open".into(),
            to: "open".into(),
            plan: ActionInvocation {
                action_id: "evaluate_periods".into(),
                capability: HABIT_CAPABILITY.into(),
                parameters: BTreeMap::new(),
                command_id: "cap-odd".into(),
            },
        };
        let run = run_local_plan(&mut composition, &claim).expect_err("mirror refusal");
        assert!(run.contains("cannot be mirrored"), "{run}");
    }

    #[test]
    fn serve_without_once_refuses_before_any_network_call() {
        let error = run(
            "http://localhost:1",
            CapabilityRunnerCommand {
                command: CapabilityRunnerSubcommand::Serve { once: false },
            },
        )
        .unwrap_err();
        assert!(matches!(error, CliError::Usage(_)), "{error:?}");
        assert!(error.to_string().contains("requires --once"), "{error}");
    }
}
