//! Optional Herdr CLI adapter. Invoke on a trusted executor machine, never in
//! the server HTTP request handler. No shell interpolation or implicit SSH.
use orbit_application::agent_runtime::{
    AgentLaunchOutcome, AgentLaunchRequest, AgentMessageOutcome, AgentMessenger, AgentRuntime,
    AgentSessionObservation,
};
use serde_json::Value;
use std::process::Command;

pub trait HerdrCommandRunner {
    fn run(&mut self, args: &[String]) -> Result<Value, String>;
}

#[derive(Default)]
pub struct HerdrCli;
impl HerdrCommandRunner for HerdrCli {
    fn run(&mut self, args: &[String]) -> Result<Value, String> {
        let output = Command::new("herdr")
            .args(args)
            .output()
            .map_err(|e| format!("herdr transport: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "herdr exited {}: {}",
                output.status,
                stderr.chars().take(1000).collect::<String>()
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|e| format!("herdr JSON response: {e}"))
    }
}

pub struct HerdrRuntime<R = HerdrCli> {
    runner: R,
}
impl Default for HerdrRuntime<HerdrCli> {
    fn default() -> Self {
        Self { runner: HerdrCli }
    }
}
impl<R: HerdrCommandRunner> HerdrRuntime<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
    /// Send a user-authorized continuation to an existing named Herdr agent.
    /// This does not create a pane, restart the agent, or mutate an Orbit task.
    /// An error is delivery-ambiguous: callers must NOT retry automatically.
    pub fn continue_existing(
        &mut self,
        machine: &str,
        session_ref: &str,
        message: &str,
    ) -> Result<Value, String> {
        if machine.trim().is_empty()
            || machine.starts_with('-')
            || machine.chars().any(|c| c.is_control())
        {
            return Err("invalid Herdr machine reference".into());
        }
        if !session_ref.starts_with("orbit")
            || session_ref.len() != 21
            || !session_ref[5..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("session is not an Orbit-managed Herdr agent".into());
        }
        if message.trim().is_empty() || message.len() > 32768 {
            return Err("message must be 1..=32768 bytes".into());
        }
        self.invoke(machine, &["agent", "prompt", session_ref, message])
    }
    fn invoke(&mut self, machine: &str, args: &[&str]) -> Result<Value, String> {
        let mut command = Vec::<String>::new();
        if machine != "local" {
            if machine.trim().is_empty() {
                return Err("machine reference required".into());
            }
            command.extend(["--machine".to_string(), machine.to_string()]);
        }
        command.extend(args.iter().map(|s| s.to_string()));
        self.runner.run(&command)
    }
}
/// Read the optional objective out of the frozen task brief. A brief without an
/// objective (or with a blank one) yields `None`, so the launch prompt stays
/// byte-identical for every pre-objective caller.
fn objective_paragraph(brief_json: &str) -> Option<String> {
    let brief: Value = serde_json::from_str(brief_json).ok()?;
    let objective = brief.get("objective")?.as_str()?.trim();
    if objective.is_empty() {
        return None;
    }
    Some(objective.to_owned())
}

fn agent_name(job_id: &str) -> String {
    // Deterministic, name-safe, collision-resistant for normal job ids. Names
    // are NOT the durable identity; Orbit retains the launch claim and pane.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in job_id.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("orbit{h:016x}")
}
impl<R: HerdrCommandRunner> AgentRuntime for HerdrRuntime<R> {
    fn launch(&mut self, request: &AgentLaunchRequest) -> Result<AgentLaunchOutcome, String> {
        if !matches!(request.agent_kind.as_str(), "codex" | "opencode") {
            return Ok(AgentLaunchOutcome::Rejected {
                diagnostic: "agent kind not allowlisted".into(),
            });
        }
        if request.repository_ref.trim().is_empty()
            || !std::path::Path::new(&request.repository_ref).is_absolute()
        {
            return Ok(AgentLaunchOutcome::Rejected {
                diagnostic: "repository_ref must be an absolute path on the target machine".into(),
            });
        }
        let name = agent_name(&request.claim_token);
        // Once workspace creation is attempted any error is ambiguous: remote
        // execution may have happened before a transport/response failure.
        let branch = format!("orbit/{}", name);
        let created = if request.worktree {
            self.invoke(
                &request.machine_ref,
                &[
                    "worktree",
                    "create",
                    "--cwd",
                    &request.repository_ref,
                    "--branch",
                    &branch,
                    "--label",
                    &name,
                    "--no-focus",
                ],
            )
        } else {
            self.invoke(
                &request.machine_ref,
                &[
                    "workspace",
                    "create",
                    "--cwd",
                    &request.repository_ref,
                    "--label",
                    &name,
                    "--no-focus",
                ],
            )
        };
        let created = match created {
            Ok(v) => v,
            Err(e) => return Ok(AgentLaunchOutcome::Ambiguous { diagnostic: e }),
        };
        let Some(pane) = created
            .pointer("/result/root_pane/pane_id")
            .and_then(Value::as_str)
        else {
            return Ok(AgentLaunchOutcome::Ambiguous {
                diagnostic: "workspace created but no pane ID returned".into(),
            });
        };
        let pane = pane.to_string();
        if let Err(e) = self.invoke(
            &request.machine_ref,
            &[
                "agent",
                "start",
                &name,
                "--kind",
                &request.agent_kind,
                "--pane",
                &pane,
            ],
        ) {
            return Ok(AgentLaunchOutcome::Ambiguous {
                diagnostic: format!("agent start uncertain: {e}"),
            });
        }
        // Prompt submission may have happened even if acknowledgment is lost.
        let mut prompt = format!(
            "Orbit job {}. Implement the following authority-captured task brief. Report changes and validation; do not assume task completion.\n{}",
            request.job_id, request.brief_json
        );
        // The objective is an optional, additive part of the frozen brief. When it
        // is absent the prompt must stay byte-identical to the pre-objective
        // shape; when present it is a separate paragraph, never shell argv.
        if let Some(objective) = objective_paragraph(&request.brief_json) {
            prompt.push_str("\n\nObjective: ");
            prompt.push_str(&objective);
        }
        if let Err(e) = self.invoke(&request.machine_ref, &["agent", "prompt", &name, &prompt]) {
            return Ok(AgentLaunchOutcome::Ambiguous {
                diagnostic: format!("prompt delivery uncertain: {e}"),
            });
        }
        Ok(AgentLaunchOutcome::Confirmed { session_ref: name })
    }
    fn inspect(
        &mut self,
        machine_ref: &str,
        claim_token: &str,
    ) -> Result<AgentSessionObservation, String> {
        // The token is a durable job identifier to this adapter; it is never
        // interpreted as a shell expression or a raw session selector.
        let target = agent_name(claim_token);
        match self.invoke(machine_ref, &["agent", "get", &target]) {
            Ok(v) => {
                let status = v
                    .pointer("/result/agent/status")
                    .or_else(|| v.pointer("/result/status"))
                    .or_else(|| v.pointer("/result/agent/state"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                Ok(AgentSessionObservation::Present {
                    session_ref: target,
                    runtime_state: status.to_string(),
                })
            }
            // Herdr errors are not proof of absence (machine disconnected,
            // agent renamed, session destroyed, transport timeout, etc.).
            Err(_) => Ok(AgentSessionObservation::Unreachable),
        }
    }
}

impl<R: HerdrCommandRunner> AgentMessenger for HerdrRuntime<R> {
    fn send_message(
        &mut self,
        machine_ref: &str,
        session_ref: &str,
        message: &str,
    ) -> Result<AgentMessageOutcome, String> {
        // Herdr transport errors are not evidence that the prompt was not sent.
        // Invalid input is reported by the concrete adapter as an error.
        if machine_ref.trim().is_empty()
            || message.trim().is_empty()
            || !session_ref.starts_with("orbit")
            || session_ref.len() != 21
        {
            return Err("invalid continuation request".into());
        }
        match self.continue_existing(machine_ref, session_ref, message) {
            Ok(_) => Ok(AgentMessageOutcome::Delivered),
            Err(diagnostic) => Ok(AgentMessageOutcome::Ambiguous { diagnostic }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Mock {
        calls: Vec<Vec<String>>,
        responses: Vec<Result<Value, String>>,
    }
    impl HerdrCommandRunner for Mock {
        fn run(&mut self, args: &[String]) -> Result<Value, String> {
            if self.responses.is_empty() {
                return Err("mock exhausted: no scripted response".into());
            }
            self.calls.push(args.to_vec());
            self.responses.remove(0)
        }
    }
    fn request() -> AgentLaunchRequest {
        AgentLaunchRequest {
            job_id: "task-1".into(),
            claim_token: "launch-1".into(),
            agent_kind: "codex".into(),
            machine_ref: "buildbox".into(),
            repository_ref: "/srv/repo".into(),
            brief_json: "{\"title\":\"ship\"}".into(),
            worktree: false,
        }
    }
    #[test]
    fn launches_on_explicit_remote_machine_and_prompts() {
        let mock = Mock {
            calls: vec![],
            responses: vec![
                Ok(serde_json::json!({"result":{"root_pane":{"pane_id":"w1:p1"}}})),
                Ok(serde_json::json!({"result":{}})),
                Ok(serde_json::json!({"result":{}})),
            ],
        };
        let mut runtime = HerdrRuntime::new(mock);
        assert!(matches!(
            runtime.launch(&request()).unwrap(),
            AgentLaunchOutcome::Confirmed { .. }
        ));
        assert_eq!(&runtime.runner.calls[0][0..2], ["--machine", "buildbox"]);
        assert!(runtime.runner.calls[1].contains(&"w1:p1".to_string()));
        assert!(runtime.runner.calls[2].iter().any(|s| s.contains("ship")));
    }
    #[test]
    fn worktree_uses_isolated_herdr_workspace_and_deterministic_branch() {
        let mock = Mock {
            calls: vec![],
            responses: vec![
                Ok(serde_json::json!({"result":{"root_pane":{"pane_id":"w2:p1"}}})),
                Ok(serde_json::json!({"result":{}})),
                Ok(serde_json::json!({"result":{}})),
            ],
        };
        let mut runtime = HerdrRuntime::new(mock);
        let mut r = request();
        r.worktree = true;
        assert!(matches!(
            runtime.launch(&r).unwrap(),
            AgentLaunchOutcome::Confirmed { .. }
        ));
        let args = &runtime.runner.calls[0];
        assert!(args.windows(2).any(|w| w == ["worktree", "create"]));
        let branch = args.iter().position(|a| a == "--branch").unwrap();
        assert!(args[branch + 1].starts_with("orbit/orbit"));
    }
    #[test]
    fn transport_loss_does_not_trigger_relaunch() {
        let mock = Mock {
            calls: vec![],
            responses: vec![Err("connection lost".into())],
        };
        let mut runtime = HerdrRuntime::new(mock);
        assert!(matches!(
            runtime.launch(&request()).unwrap(),
            AgentLaunchOutcome::Ambiguous { .. }
        ));
        assert_eq!(runtime.runner.calls.len(), 1);
    }
    #[test]
    fn rejects_unsupported_kind_before_side_effects() {
        let mock = Mock {
            calls: vec![],
            responses: vec![],
        };
        let mut runtime = HerdrRuntime::new(mock);
        let mut r = request();
        r.agent_kind = "arbitrary-shell".into();
        assert!(matches!(
            runtime.launch(&r).unwrap(),
            AgentLaunchOutcome::Rejected { .. }
        ));
        assert!(runtime.runner.calls.is_empty());
    }
    /// The prompt is the last call's final argument; tests quote it verbatim.
    fn prompt_of(call: &[String]) -> String {
        call.last().cloned().unwrap_or_default()
    }
    fn launch_prompt(brief_json: &str) -> String {
        let mock = Mock {
            calls: vec![],
            responses: vec![
                Ok(serde_json::json!({"result":{"root_pane":{"pane_id":"w1:p1"}}})),
                Ok(serde_json::json!({"result":{}})),
                Ok(serde_json::json!({"result":{}})),
            ],
        };
        let mut runtime = HerdrRuntime::new(mock);
        let mut r = request();
        r.brief_json = brief_json.into();
        assert!(matches!(
            runtime.launch(&r).unwrap(),
            AgentLaunchOutcome::Confirmed { .. }
        ));
        prompt_of(&runtime.runner.calls[2])
    }
    #[test]
    fn prompt_is_unchanged_without_an_objective() {
        let prompt = launch_prompt("{\"title\":\"ship\"}");
        assert_eq!(
            prompt,
            "Orbit job task-1. Implement the following authority-captured task brief. \
             Report changes and validation; do not assume task completion.\n{\"title\":\"ship\"}"
        );
        // A blank objective is the same as an absent one: no paragraph is added,
        // and the brief text is passed through unchanged.
        let blank = launch_prompt("{\"title\":\"ship\",\"objective\":\"   \"}");
        assert_eq!(
            blank,
            "Orbit job task-1. Implement the following authority-captured task brief. \
             Report changes and validation; do not assume task completion.\n\
             {\"title\":\"ship\",\"objective\":\"   \"}"
        );
        assert!(!blank.contains("Objective:"));
    }
    #[test]
    fn prompt_appends_the_objective_as_its_own_paragraph() {
        let prompt = launch_prompt(
            "{\"title\":\"ship\",\"objective\":\"Fix the login redirect and add a regression test.\"}",
        );
        assert!(
            prompt.ends_with("\n\nObjective: Fix the login redirect and add a regression test.")
        );
        // The brief paragraph itself is untouched.
        assert!(prompt.contains("\n{\"title\":\"ship\",\"objective\":\"Fix the login"));
    }
    #[test]
    fn continuation_routes_to_named_remote_machine_without_new_session() {
        let mock = Mock {
            calls: vec![],
            responses: vec![Ok(serde_json::json!({"result":{}}))],
        };
        let mut runtime = HerdrRuntime::new(mock);
        let session = agent_name("launch-1");
        assert!(
            runtime
                .continue_existing("buildbox", &session, "Apply requested changes")
                .is_ok()
        );
        assert_eq!(
            runtime.runner.calls[0],
            vec![
                "--machine",
                "buildbox",
                "agent",
                "prompt",
                session.as_str(),
                "Apply requested changes"
            ]
        );
        assert_eq!(runtime.runner.calls.len(), 1);
        assert!(
            runtime
                .continue_existing("--machine", &session, "bad")
                .is_err()
        );
    }
    #[test]
    fn continuation_only_prompts_existing_orbit_agent() {
        let mock = Mock {
            calls: vec![],
            responses: vec![Ok(serde_json::json!({"result":{}}))],
        };
        let mut runtime = HerdrRuntime::new(mock);
        let session = agent_name("launch-1");
        assert!(
            runtime
                .continue_existing("local", &session, "Continue from review")
                .is_ok()
        );
        assert_eq!(runtime.runner.calls.len(), 1);
        assert_eq!(
            runtime.runner.calls[0][0..3],
            ["agent", "prompt", session.as_str()]
        );
        assert!(
            runtime
                .continue_existing("remote", &session, "prompt")
                .is_err()
        ); // mock has no second response
        assert!(
            runtime
                .continue_existing("local", "other-agent", "prompt")
                .is_err()
        );
        assert_eq!(runtime.runner.calls.len(), 1);
    }
}
