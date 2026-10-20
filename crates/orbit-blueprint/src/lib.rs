//! Declarative, headless project blueprints. No database or execution dependencies.
//! Published versions are immutable and project bindings are version-pinned.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    pub id: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub work_items: Vec<WorkItemKind>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub templates: Vec<ProjectTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItemKind {
    pub id: String,
    pub name: String,
    pub initial_state: String,
    pub states: Vec<String>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldKind {
    Text,
    Boolean,
    Integer,
    Choice { options: Vec<String> },
}

/// Deterministic transition predicates over the current work item.
/// Dependencies are evaluated by the authoritative repository, never by callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TransitionGuard {
    PropertyEquals {
        field: String,
        value: serde_json::Value,
    },
    PropertyPresent {
        field: String,
    },
    AllPrerequisitesInState {
        state: String,
    },
}

/// An action offered to a user after a permitted transition. Merely declaring it
/// never triggers a side effect or grants the associated capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowAction {
    pub id: String,
    pub capability: String,
    /// Requires an explicit authorized invocation; automatic execution is not supported.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub required_capability: Option<String>,
    #[serde(default)]
    pub guards: Vec<TransitionGuard>,
    #[serde(default)]
    pub actions: Vec<WorkflowAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlueprintBinding {
    pub project_id: String,
    pub blueprint_id: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlueprintError {
    Invalid(String),
    Conflict(String),
    NotFound(String),
}
impl std::fmt::Display for BlueprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) | Self::Conflict(s) | Self::NotFound(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for BlueprintError {}

fn ident(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.' || b == b'-'
        })
}
fn invalid(msg: impl Into<String>) -> BlueprintError {
    BlueprintError::Invalid(msg.into())
}
fn unique<'a>(ids: impl IntoIterator<Item = &'a str>, context: &str) -> Result<(), BlueprintError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !ident(id) {
            return Err(invalid(format!("invalid {context} identifier: {id}")));
        }
        if !seen.insert(id) {
            return Err(invalid(format!("duplicate {context} identifier: {id}")));
        }
    }
    Ok(())
}

impl Blueprint {
    pub fn validate(&self, installed: &BTreeSet<String>) -> Result<(), BlueprintError> {
        if !ident(&self.id) || self.version == 0 || self.name.trim().is_empty() {
            return Err(invalid(
                "blueprint needs a valid id, positive version and name",
            ));
        }
        if self.work_items.is_empty() {
            return Err(invalid("at least one work-item kind is required"));
        }
        unique(
            self.required_capabilities.iter().map(String::as_str),
            "capability",
        )?;
        for capability in &self.required_capabilities {
            if !installed.contains(capability) {
                return Err(invalid(format!("unavailable capability: {capability}")));
            }
        }
        unique(self.work_items.iter().map(|x| x.id.as_str()), "work-item")?;
        self.validate_templates()?;
        for item in &self.work_items {
            if item.name.trim().is_empty() || item.states.is_empty() {
                return Err(invalid(format!(
                    "work-item {} needs name and states",
                    item.id
                )));
            }
            unique(item.states.iter().map(String::as_str), "state")?;
            unique(item.fields.iter().map(|x| x.id.as_str()), "field")?;
            if !item.states.contains(&item.initial_state) {
                return Err(invalid(format!("missing initial state for {}", item.id)));
            }
            for field in &item.fields {
                if let FieldKind::Choice { options } = &field.kind {
                    if options.is_empty() {
                        return Err(invalid(format!("empty choices: {}", field.id)));
                    }
                    unique(options.iter().map(String::as_str), "choice")?;
                }
                if let Some(value) = &field.default {
                    let valid = match &field.kind {
                        FieldKind::Text => value.is_string(),
                        FieldKind::Boolean => value.is_boolean(),
                        FieldKind::Integer => value.as_i64().is_some(),
                        FieldKind::Choice { options } => value
                            .as_str()
                            .is_some_and(|s| options.iter().any(|o| o == s)),
                    };
                    if !valid {
                        return Err(invalid(format!("invalid field default: {}", field.id)));
                    }
                }
            }
            let mut edges = BTreeSet::new();
            for t in &item.transitions {
                if !item.states.contains(&t.from) || !item.states.contains(&t.to) || t.from == t.to
                {
                    return Err(invalid(format!(
                        "invalid transition in {}: {} -> {}",
                        item.id, t.from, t.to
                    )));
                }
                if !edges.insert((&t.from, &t.to)) {
                    return Err(invalid("duplicate transition"));
                }
                if let Some(c) = &t.required_capability
                    && !self.required_capabilities.contains(c)
                {
                    return Err(invalid(format!("undeclared transition capability: {c}")));
                }
                unique(t.actions.iter().map(|a| a.id.as_str()), "transition action")?;
                for action in &t.actions {
                    if !ident(&action.capability)
                        || !self.required_capabilities.contains(&action.capability)
                    {
                        return Err(invalid(format!(
                            "undeclared action capability: {}",
                            action.capability
                        )));
                    }
                    if action.parameters.keys().any(|key| !ident(key)) {
                        return Err(invalid(format!(
                            "invalid action parameter in {}",
                            action.id
                        )));
                    }
                }
                for guard in &t.guards {
                    match guard {
                        TransitionGuard::PropertyEquals { field, value } => {
                            let definition =
                                item.fields.iter().find(|f| &f.id == field).ok_or_else(|| {
                                    invalid(format!("undeclared guard field: {field}"))
                                })?;
                            let valid = match &definition.kind {
                                FieldKind::Text => value.is_string(),
                                FieldKind::Boolean => value.is_boolean(),
                                FieldKind::Integer => value.as_i64().is_some(),
                                FieldKind::Choice { options } => value
                                    .as_str()
                                    .is_some_and(|s| options.iter().any(|o| o == s)),
                            };
                            if !valid {
                                return Err(invalid(format!("invalid guard value: {field}")));
                            }
                        }
                        TransitionGuard::PropertyPresent { field } => {
                            if !item.fields.iter().any(|f| &f.id == field) {
                                return Err(invalid(format!("undeclared guard field: {field}")));
                            }
                        }
                        TransitionGuard::AllPrerequisitesInState { state } => {
                            if !item.states.contains(state) {
                                return Err(invalid(format!(
                                    "invalid prerequisite target state: {state}"
                                )));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn from_json(json: &str, installed: &BTreeSet<String>) -> Result<Self, BlueprintError> {
        let parsed: Self = serde_json::from_str(json).map_err(|e| invalid(e.to_string()))?;
        parsed.validate(installed)?;
        Ok(parsed)
    }
    pub fn from_yaml(yaml: &str, installed: &BTreeSet<String>) -> Result<Self, BlueprintError> {
        let parsed: Self = serde_yaml::from_str(yaml).map_err(|e| invalid(e.to_string()))?;
        parsed.validate(installed)?;
        Ok(parsed)
    }
    pub fn to_yaml(&self) -> Result<String, BlueprintError> {
        serde_yaml::to_string(self).map_err(|e| invalid(e.to_string()))
    }
    pub fn to_json(&self) -> Result<String, BlueprintError> {
        serde_json::to_string_pretty(self).map_err(|e| invalid(e.to_string()))
    }
}

pub mod actions;
pub mod bundled;
pub mod capabilities;
pub mod templates;
pub mod workflow;
pub use actions::{ActionInvocation, ActionOffer, ActionPlan, ActionSelection};
pub use capabilities::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityPermission, CapabilityStatus,
};
pub use templates::{ProjectTemplate, TemplateItem, TemplatePlan};
pub use workflow::BlueprintWorkItem;
#[derive(Default)]
pub struct BlueprintRegistry {
    published: BTreeMap<(String, u32), Blueprint>,
}
impl BlueprintRegistry {
    pub fn publish(
        &mut self,
        manifest: Blueprint,
        installed: &BTreeSet<String>,
    ) -> Result<(), BlueprintError> {
        manifest.validate(installed)?;
        let key = (manifest.id.clone(), manifest.version);
        match self.published.get(&key) {
            Some(existing) if existing == &manifest => Ok(()),
            Some(_) => Err(BlueprintError::Conflict(
                "published blueprint version is immutable".into(),
            )),
            None => {
                self.published.insert(key, manifest);
                Ok(())
            }
        }
    }
    pub fn get(&self, id: &str, version: u32) -> Result<&Blueprint, BlueprintError> {
        self.published
            .get(&(id.to_owned(), version))
            .ok_or_else(|| {
                BlueprintError::NotFound(format!("blueprint {id}@{version} not installed"))
            })
    }
    pub fn bind(
        &self,
        project_id: &str,
        id: &str,
        version: u32,
    ) -> Result<BlueprintBinding, BlueprintError> {
        if project_id.trim().is_empty() {
            return Err(invalid("project ID is empty"));
        }
        self.get(id, version)?;
        Ok(BlueprintBinding {
            project_id: project_id.into(),
            blueprint_id: id.into(),
            version,
        })
    }
    pub fn versions(&self, id: &str) -> Vec<u32> {
        self.published
            .keys()
            .filter(|(name, _)| name == id)
            .map(|(_, v)| *v)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Blueprint {
        Blueprint {
            id: "development".into(),
            version: 1,
            name: "Development".into(),
            description: String::new(),
            required_capabilities: vec![],
            templates: vec![],
            work_items: vec![WorkItemKind {
                id: "feature".into(),
                name: "Feature".into(),
                initial_state: "backlog".into(),
                states: vec!["backlog".into(), "review".into()],
                fields: vec![Field {
                    id: "priority".into(),
                    kind: FieldKind::Choice {
                        options: vec!["low".into(), "high".into()],
                    },
                    required: true,
                    default: Some(serde_json::json!("low")),
                }],
                transitions: vec![Transition {
                    from: "backlog".into(),
                    to: "review".into(),
                    required_capability: None,
                    guards: vec![],
                    actions: vec![],
                }],
            }],
        }
    }
    #[test]
    fn roundtrip_and_immutable_publish() {
        let caps = BTreeSet::new();
        let m = sample();
        let json = m.to_json().unwrap();
        assert_eq!(Blueprint::from_json(&json, &caps).unwrap(), m);
        let mut r = BlueprintRegistry::default();
        r.publish(m.clone(), &caps).unwrap();
        r.publish(m.clone(), &caps).unwrap();
        assert_eq!(r.bind("project-1", "development", 1).unwrap().version, 1);
        let mut changed = m;
        changed.name = "Changed".into();
        assert!(matches!(
            r.publish(changed, &caps),
            Err(BlueprintError::Conflict(_))
        ));
    }
    #[test]
    fn rejects_bad_references_and_defaults() {
        let caps = BTreeSet::new();
        let mut m = sample();
        m.work_items[0].initial_state = "unknown".into();
        assert!(m.validate(&caps).is_err());
        m = sample();
        m.work_items[0].fields[0].default = Some(serde_json::json!(9));
        assert!(m.validate(&caps).is_err());
        m = sample();
        m.required_capabilities.push("agent.launch".into());
        assert!(m.validate(&caps).is_err());
        m = sample();
        m.work_items[0].transitions[0].to = "unknown".into();
        assert!(m.validate(&caps).is_err());
    }
}

#[cfg(test)]
mod registry_contract_tests {
    use super::*;
    #[test]
    fn published_revisions_are_independent_and_explicitly_pinned() {
        let caps = BTreeSet::new();
        let first =
            Blueprint::from_json(include_str!("../../../blueprints/development.json"), &caps)
                .unwrap();
        let mut registry = BlueprintRegistry::default();
        registry.publish(first.clone(), &caps).unwrap();
        let mut second = first.clone();
        second.version = 2;
        second.description = "Newer version".to_owned();
        registry.publish(second, &caps).unwrap();
        let binding = registry.bind("existing-project", "development", 1).unwrap();
        assert_eq!(binding.version, 1);
        assert_eq!(registry.versions("development"), vec![1, 2]);
        assert_eq!(registry.get("development", 1).unwrap(), &first);
        assert!(registry.bind("p", "development", 3).is_err());
    }
}

#[cfg(test)]
mod yaml_tests {
    use super::*;
    #[test]
    fn json_yaml_equivalence() {
        let installed = BTreeSet::new();
        let json = include_str!("../../../blueprints/development.json");
        let blueprint = Blueprint::from_json(json, &installed).unwrap();
        let yaml = blueprint.to_yaml().unwrap();
        assert_eq!(Blueprint::from_yaml(&yaml, &installed).unwrap(), blueprint);
    }
}
