//! Blueprint MCP tools use the same host services and HTTP API as the orbit CLI.
//! No second persistence authority and no implicit external capability grants.
use super::*;

fn required_version(args: &Value, key: &str) -> Result<u32, String> {
    let value = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing or invalid {key}"))?;
    u32::try_from(value).map_err(|_| format!("{key} exceeds u32"))
}
fn revision(args: &Value) -> Result<u64, String> {
    args.get("expected_revision")
        .and_then(Value::as_u64)
        .ok_or("missing or invalid expected_revision".into())
}
fn map_properties(
    args: &Value,
    key: &str,
) -> Result<std::collections::BTreeMap<String, Value>, String> {
    let v = args.get(key).cloned().unwrap_or_else(|| json!({}));
    serde_json::from_value(v).map_err(debug_err)
}
fn query(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn workspace(policy: &AgentPolicy) -> Result<(), String> {
    if !policy.projects.is_empty() || !policy.allow_unscoped_execution {
        return Err("blueprint registry requires unrestricted workspace scope".into());
    }
    Ok(())
}
fn project(policy: &AgentPolicy, project_id: &str) -> Result<(), String> {
    if project_id.is_empty()
        || (!policy.projects.is_empty() && !policy.projects.contains(project_id))
    {
        return Err("project outside agent scope".into());
    }
    Ok(())
}
impl Server {
    pub(super) fn blueprint_dispatch(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "blueprint.list" => {
                workspace(&self.policy)?;
                serde_json::to_value(
                    self.authority
                        .available_blueprint_versions(args.get("id").and_then(Value::as_str))
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.show" => {
                workspace(&self.policy)?;
                serde_json::to_value(
                    self.authority
                        .project_blueprint(
                            &required_str(args, "id")?,
                            required_version(args, "version")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.binding" => {
                let id = required_str(args, "project_id")?;
                project(&self.policy, &id)?;
                serde_json::to_value(
                    self.authority
                        .active_project_blueprint(&id)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.item" => {
                let id = required_str(args, "id")?;
                let item = self
                    .authority
                    .project_work_item(&id)
                    .map_err(debug_err)?
                    .ok_or("work item not found")?;
                project(&self.policy, &item.project_id)?;
                serde_json::to_value(item).map_err(debug_err)
            }
            "blueprint.publish" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                let definition: orbit_blueprint::Blueprint = serde_json::from_value(
                    args.get("definition").ok_or("missing definition")?.clone(),
                )
                .map_err(debug_err)?;
                self.authority
                    .publish_project_blueprint(&definition, &BTreeSet::new())
                    .map_err(debug_err)?;
                serde_json::to_value(definition).map_err(debug_err)
            }
            "blueprint.install" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                serde_json::to_value(
                    self.authority
                        .install_bundled_project_blueprint(&required_str(args, "id")?)
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.bind" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                serde_json::to_value(
                    self.authority
                        .activate_project_blueprint(
                            &project_id,
                            &required_str(args, "id")?,
                            required_version(args, "version")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.create_item" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                serde_json::to_value(
                    self.authority
                        .create_project_work_item(
                            &required_str(args, "id")?,
                            &project_id,
                            &required_str(args, "kind")?,
                            map_properties(args, "properties")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            "blueprint.transition" | "blueprint.properties" => {
                self.policy.write_allowed()?;
                let id = required_str(args, "id")?;
                let item = self
                    .authority
                    .project_work_item(&id)
                    .map_err(debug_err)?
                    .ok_or("work item not found")?;
                project(&self.policy, &item.project_id)?;
                if name == "blueprint.transition" {
                    serde_json::to_value(
                        self.authority
                            .transition_project_work_item(
                                &id,
                                &required_str(args, "to")?,
                                revision(args)?,
                                &BTreeSet::new(),
                            )
                            .map_err(debug_err)?,
                    )
                    .map_err(debug_err)
                } else {
                    serde_json::to_value(
                        self.authority
                            .update_project_work_item_properties(
                                &id,
                                revision(args)?,
                                map_properties(args, "changes")?,
                            )
                            .map_err(debug_err)?,
                    )
                    .map_err(debug_err)
                }
            }
            "blueprint.template" => {
                self.policy.write_allowed()?;
                let project_id = required_str(args, "project_id")?;
                project(&self.policy, &project_id)?;
                serde_json::to_value(
                    self.authority
                        .apply_project_blueprint_template(
                            &project_id,
                            &required_str(args, "template_id")?,
                            &required_str(args, "namespace")?,
                        )
                        .map_err(debug_err)?,
                )
                .map_err(debug_err)
            }
            _ => Err("unknown blueprint tool".into()),
        }
    }
}
impl ConnectedMcp {
    pub(super) fn blueprint_dispatch(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        let base = "/v1/authoritative/blueprints";
        let (path, body) = match name {
            "blueprint.list" => {
                workspace(&self.policy)?;
                (
                    format!(
                        "{base}{}",
                        args.get("id")
                            .and_then(Value::as_str)
                            .map(|id| format!("?id={}", query(id)))
                            .unwrap_or_default()
                    ),
                    None,
                )
            }
            "blueprint.show" => {
                workspace(&self.policy)?;
                (
                    format!(
                        "{base}/show?id={}&version={}",
                        query(&required_str(args, "id")?),
                        required_version(args, "version")?
                    ),
                    None,
                )
            }
            "blueprint.binding" => {
                let id = required_str(args, "project_id")?;
                project(&self.policy, &id)?;
                (format!("{base}/binding?project_id={}", query(&id)), None)
            }
            "blueprint.item" => {
                let id = required_str(args, "id")?;
                let item = self.blueprint_http_get(&format!("{base}/items?id={}", query(&id)))?;
                project(
                    &self.policy,
                    item.get("project_id")
                        .and_then(Value::as_str)
                        .ok_or("work item lacks project scope")?,
                )?;
                return Ok(item);
            }
            "blueprint.publish" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                (
                    base.to_owned(),
                    Some(json!({"definition":args.get("definition").ok_or("missing definition")?})),
                )
            }
            "blueprint.install" => {
                self.policy.write_allowed()?;
                workspace(&self.policy)?;
                (
                    format!("{base}/install"),
                    Some(json!({"id":required_str(args,"id")?})),
                )
            }
            "blueprint.bind" => {
                self.policy.write_allowed()?;
                project(&self.policy, &required_str(args, "project_id")?)?;
                (
                    format!("{base}/bind"),
                    Some(
                        json!({"project_id":required_str(args,"project_id")?,"id":required_str(args,"id")?,"version":required_version(args,"version")?}),
                    ),
                )
            }
            "blueprint.create_item" => {
                self.policy.write_allowed()?;
                project(&self.policy, &required_str(args, "project_id")?)?;
                (
                    format!("{base}/items"),
                    Some(
                        json!({"project_id":required_str(args,"project_id")?,"id":required_str(args,"id")?,"kind":required_str(args,"kind")?,"properties":map_properties(args,"properties")?}),
                    ),
                )
            }
            "blueprint.transition" | "blueprint.properties" => {
                self.policy.write_allowed()?;
                let id = required_str(args, "id")?;
                let item = self.blueprint_http_get(&format!("{base}/items?id={}", query(&id)))?;
                project(
                    &self.policy,
                    item.get("project_id")
                        .and_then(Value::as_str)
                        .ok_or("work item lacks project scope")?,
                )?;
                if name == "blueprint.transition" {
                    (
                        format!("{base}/transition"),
                        Some(
                            json!({"id":id,"to":required_str(args,"to")?,"expected_revision":revision(args)?}),
                        ),
                    )
                } else {
                    (
                        format!("{base}/properties"),
                        Some(
                            json!({"id":id,"changes":map_properties(args,"changes")?,"expected_revision":revision(args)?}),
                        ),
                    )
                }
            }
            "blueprint.template" => {
                self.policy.write_allowed()?;
                project(&self.policy, &required_str(args, "project_id")?)?;
                (
                    format!("{base}/template"),
                    Some(
                        json!({"project_id":required_str(args,"project_id")?,"template_id":required_str(args,"template_id")?,"namespace":required_str(args,"namespace")?}),
                    ),
                )
            }
            _ => return Err("unknown blueprint tool".into()),
        };
        let http = match &self.client {
            AuthorityConnection::Remote {
                http: Some(http), ..
            } => http,
            _ => {
                return Err(
                    "blueprint management requires an online authenticated authority".into(),
                );
            }
        };
        match body {
            Some(body) => http.post(&path, &body).map_err(debug_err),
            None => http.get(&path).map_err(debug_err),
        }
    }
    fn blueprint_http_get(&self, path: &str) -> Result<Value, String> {
        match &self.client {
            AuthorityConnection::Remote {
                http: Some(http), ..
            } => http.get(path).map_err(debug_err),
            _ => Err("blueprint management requires an online authenticated authority".into()),
        }
    }
}
