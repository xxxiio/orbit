//! Declarative templates produce a deterministic plan, not storage side effects.
use crate::{Blueprint, BlueprintError, BlueprintWorkItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTemplate {
    pub id: String,
    pub name: String,
    pub items: Vec<TemplateItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateItem {
    /// Template-local key, not a globally allocated database ID.
    pub key: String,
    pub kind: String,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplatePlan {
    pub items: Vec<BlueprintWorkItem>,
    /// prerequisite edges (item_id, prerequisite_id), in deterministic order.
    pub prerequisites: Vec<(String, String)>,
}
impl Blueprint {
    pub fn validate_templates(&self) -> Result<(), BlueprintError> {
        let mut names = BTreeSet::new();
        for template in &self.templates {
            if !valid_key(&template.id)
                || !names.insert(&template.id)
                || template.name.trim().is_empty()
            {
                return Err(BlueprintError::Invalid(format!(
                    "invalid/duplicate template: {}",
                    template.id
                )));
            }
            let keys: BTreeSet<_> = template.items.iter().map(|i| i.key.as_str()).collect();
            if keys.len() != template.items.len() {
                return Err(BlueprintError::Invalid(
                    "duplicate template item key".into(),
                ));
            }
            for item in &template.items {
                if !valid_key(&item.key) {
                    return Err(BlueprintError::Invalid("invalid template item key".into()));
                }
                self.normalize_properties(&item.kind, item.properties.clone())?;
                let mut prerequisites = BTreeSet::new();
                for other in &item.prerequisites {
                    if other == &item.key
                        || !keys.contains(other.as_str())
                        || !prerequisites.insert(other)
                    {
                        return Err(BlueprintError::Invalid(format!(
                            "invalid prerequisite for {}",
                            item.key
                        )));
                    }
                }
            }
            // Kahn traversal rejects cycles; stable sorted keys ensure deterministic behavior.
            let mut completed = BTreeSet::new();
            loop {
                let before = completed.len();
                for item in &template.items {
                    if item
                        .prerequisites
                        .iter()
                        .all(|p| completed.contains(p.as_str()))
                    {
                        completed.insert(item.key.as_str());
                    }
                }
                if before == completed.len() {
                    break;
                }
            }
            if completed.len() != template.items.len() {
                return Err(BlueprintError::Invalid(
                    "template prerequisite cycle".into(),
                ));
            }
        }
        Ok(())
    }
    /// Creates a pure plan; caller supplies a unique namespace and commits atomically.
    pub fn plan_template(
        &self,
        template_id: &str,
        project_id: &str,
        namespace: &str,
    ) -> Result<TemplatePlan, BlueprintError> {
        if !valid_key(namespace) || project_id.trim().is_empty() {
            return Err(BlueprintError::Invalid(
                "invalid template namespace or project".into(),
            ));
        }
        self.validate_templates()?;
        let template = self
            .templates
            .iter()
            .find(|t| t.id == template_id)
            .ok_or_else(|| BlueprintError::NotFound(format!("unknown template: {template_id}")))?;
        let mut items = Vec::new();
        let mut edges = Vec::new();
        for item in &template.items {
            let id = format!("{namespace}.{}", item.key);
            items.push(self.instantiate(&id, project_id, &item.kind, item.properties.clone())?);
            for prior in &item.prerequisites {
                edges.push((id.clone(), format!("{namespace}.{prior}")));
            }
        }
        edges.sort();
        Ok(TemplatePlan {
            items,
            prerequisites: edges,
        })
    }
}
fn valid_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Blueprint {
        serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap()
    }
    #[test]
    fn builds_isolated_deterministic_plan() {
        let mut schema = fixture();
        schema.templates.push(ProjectTemplate {
            id: "starter".into(),
            name: "Starter".into(),
            items: vec![
                TemplateItem {
                    key: "design".into(),
                    kind: "feature".into(),
                    properties: BTreeMap::new(),
                    prerequisites: vec![],
                },
                TemplateItem {
                    key: "build".into(),
                    kind: "feature".into(),
                    properties: BTreeMap::new(),
                    prerequisites: vec!["design".into()],
                },
            ],
        });
        let plan = schema.plan_template("starter", "p", "instance1").unwrap();
        assert_eq!(
            plan.prerequisites,
            vec![("instance1.build".into(), "instance1.design".into())]
        );
        assert_eq!(plan.items[0].properties["priority"], "medium");
        assert_eq!(
            plan,
            schema.plan_template("starter", "p", "instance1").unwrap()
        );
    }
    #[test]
    fn rejects_cycle_and_missing_reference() {
        let mut schema = fixture();
        schema.templates.push(ProjectTemplate {
            id: "bad".into(),
            name: "Bad".into(),
            items: vec![
                TemplateItem {
                    key: "one".into(),
                    kind: "feature".into(),
                    properties: BTreeMap::new(),
                    prerequisites: vec!["two".into()],
                },
                TemplateItem {
                    key: "two".into(),
                    kind: "feature".into(),
                    properties: BTreeMap::new(),
                    prerequisites: vec!["one".into()],
                },
            ],
        });
        assert!(schema.validate_templates().is_err());
        schema.templates[0].items[1].prerequisites = vec!["missing".into()];
        assert!(schema.validate_templates().is_err());
    }
}
