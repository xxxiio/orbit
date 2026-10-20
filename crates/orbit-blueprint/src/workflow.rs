//! Deterministic blueprint work-item property and lifecycle rules.
//! This module has no storage, runtime, or plugin execution dependencies.
use crate::{Blueprint, BlueprintError, FieldKind, TransitionGuard, WorkItemKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlueprintWorkItem {
    pub id: String,
    pub project_id: String,
    pub kind: String,
    pub state: String,
    pub properties: BTreeMap<String, Value>,
    pub revision: u64,
}

fn error(message: impl Into<String>) -> BlueprintError {
    BlueprintError::Invalid(message.into())
}

impl Blueprint {
    pub fn item_kind(&self, kind: &str) -> Result<&WorkItemKind, BlueprintError> {
        self.work_items
            .iter()
            .find(|item| item.id == kind)
            .ok_or_else(|| error(format!("unknown work-item kind: {kind}")))
    }

    /// Normalize explicitly supplied properties using schema defaults.
    /// Unknown properties and type mismatches are rejected (fail closed).
    pub fn normalize_properties(
        &self,
        kind: &str,
        mut values: BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, BlueprintError> {
        let definition = self.item_kind(kind)?;
        for field in &definition.fields {
            if !values.contains_key(&field.id)
                && let Some(default) = &field.default
            {
                values.insert(field.id.clone(), default.clone());
            }
            if field.required && !values.contains_key(&field.id) {
                return Err(error(format!("missing required property: {}", field.id)));
            }
            if let Some(value) = values.get(&field.id) {
                let valid = match &field.kind {
                    FieldKind::Text => value.is_string(),
                    FieldKind::Boolean => value.is_boolean(),
                    FieldKind::Integer => value.as_i64().is_some(),
                    FieldKind::Choice { options } => value
                        .as_str()
                        .is_some_and(|text| options.iter().any(|v| v == text)),
                };
                if !valid {
                    return Err(error(format!("invalid property value: {}", field.id)));
                }
            }
        }
        for key in values.keys() {
            if !definition.fields.iter().any(|field| field.id == *key) {
                return Err(error(format!("undeclared property: {key}")));
            }
        }
        Ok(values)
    }

    pub fn instantiate(
        &self,
        id: &str,
        project_id: &str,
        kind: &str,
        properties: BTreeMap<String, Value>,
    ) -> Result<BlueprintWorkItem, BlueprintError> {
        if id.trim().is_empty() || project_id.trim().is_empty() {
            return Err(error("item and project ID required"));
        }
        let definition = self.item_kind(kind)?;
        Ok(BlueprintWorkItem {
            id: id.into(),
            project_id: project_id.into(),
            kind: kind.into(),
            state: definition.initial_state.clone(),
            properties: self.normalize_properties(kind, properties)?,
            revision: 0,
        })
    }

    /// Authorize an explicit transition. A capability declaration is not a
    /// permission grant: caller-supplied capabilities must come from trusted host composition.
    /// No external action is executed as part of this state change.
    pub fn transition(
        &self,
        item: &BlueprintWorkItem,
        to: &str,
        expected_revision: u64,
        granted: &BTreeSet<String>,
    ) -> Result<BlueprintWorkItem, BlueprintError> {
        if item.revision != expected_revision {
            return Err(BlueprintError::Conflict("stale work-item revision".into()));
        }
        let definition = self.item_kind(&item.kind)?;
        if !definition.states.contains(&item.state) {
            return Err(error("invalid current state"));
        }
        let edge = definition
            .transitions
            .iter()
            .find(|edge| edge.from == item.state && edge.to == to)
            .ok_or_else(|| error(format!("transition not allowed: {} -> {to}", item.state)))?;
        if edge
            .required_capability
            .as_ref()
            .is_some_and(|capability| !granted.contains(capability))
        {
            return Err(error("transition requires unavailable capability"));
        }
        self.normalize_properties(&item.kind, item.properties.clone())?;
        for guard in &edge.guards {
            match guard {
                TransitionGuard::PropertyEquals { field, value }
                    if item.properties.get(field) != Some(value) =>
                {
                    return Err(error(format!("property guard failed: {field}")));
                }
                TransitionGuard::PropertyPresent { field }
                    if !item.properties.contains_key(field) =>
                {
                    return Err(error(format!("required guard property absent: {field}")));
                }
                // Authoritative storage checks relationship guards before writing.
                TransitionGuard::AllPrerequisitesInState { .. } => (),
                _ => (),
            }
        }
        let mut next = item.clone();
        next.state = to.into();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| error("revision overflow"))?;
        Ok(next)
    }

    pub fn update_properties(
        &self,
        item: &BlueprintWorkItem,
        expected_revision: u64,
        patch: BTreeMap<String, Value>,
    ) -> Result<BlueprintWorkItem, BlueprintError> {
        if item.revision != expected_revision {
            return Err(BlueprintError::Conflict("stale work-item revision".into()));
        }
        let mut props = item.properties.clone();
        props.extend(patch);
        let mut next = item.clone();
        next.properties = self.normalize_properties(&item.kind, props)?;
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| error("revision overflow"))?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Blueprint {
        serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap()
    }
    #[test]
    fn validates_properties_and_revision() {
        let schema = fixture();
        let item = schema
            .instantiate("i1", "p1", "feature", BTreeMap::new())
            .unwrap();
        assert_eq!(item.properties["priority"], "medium");
        assert!(
            schema
                .instantiate(
                    "i2",
                    "p1",
                    "feature",
                    BTreeMap::from([("wrong".into(), Value::Bool(true))])
                )
                .is_err()
        );
        let next = schema
            .transition(&item, "ready", 0, &BTreeSet::new())
            .unwrap();
        assert_eq!(next.revision, 1);
        assert!(
            schema
                .transition(&next, "review", 1, &BTreeSet::new())
                .is_err()
        );
        assert!(
            schema
                .transition(&next, "implementing", 0, &BTreeSet::new())
                .is_err()
        );
    }
    #[test]
    fn guards_are_authoritative_and_updates_are_versioned() {
        let mut schema = fixture();
        schema.required_capabilities.push("approval".into());
        schema.work_items[0].transitions[0].required_capability = Some("approval".into());
        let item = schema
            .instantiate("i1", "p1", "feature", BTreeMap::new())
            .unwrap();
        assert!(
            schema
                .transition(&item, "ready", 0, &BTreeSet::new())
                .is_err()
        );
        assert!(
            schema
                .transition(&item, "ready", 0, &BTreeSet::from(["approval".into()]))
                .is_ok()
        );
        let updated = schema
            .update_properties(
                &item,
                0,
                BTreeMap::from([("priority".into(), Value::String("high".into()))]),
            )
            .unwrap();
        assert_eq!(updated.revision, 1);
        assert!(
            schema
                .update_properties(&updated, 0, BTreeMap::new())
                .is_err()
        );
    }
}

#[cfg(test)]
mod declarative_guard_tests {
    use super::*;
    use crate::TransitionGuard;

    #[test]
    fn property_guards_are_deterministic_and_fail_closed() {
        let mut schema: Blueprint =
            serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap();
        let feature = schema
            .work_items
            .iter_mut()
            .find(|i| i.id == "feature")
            .unwrap();
        let edge = feature
            .transitions
            .iter_mut()
            .find(|e| e.from == "backlog" && e.to == "ready")
            .unwrap();
        edge.guards.push(TransitionGuard::PropertyEquals {
            field: "priority".into(),
            value: Value::String("high".into()),
        });
        schema.validate(&BTreeSet::new()).unwrap();
        let initial = schema
            .instantiate("one", "project", "feature", BTreeMap::new())
            .unwrap();
        assert!(
            schema
                .transition(&initial, "ready", 0, &BTreeSet::new())
                .is_err()
        );
        let revised = schema
            .update_properties(
                &initial,
                0,
                BTreeMap::from([("priority".into(), Value::String("high".into()))]),
            )
            .unwrap();
        assert!(
            schema
                .transition(&revised, "ready", 1, &BTreeSet::new())
                .is_ok()
        );
    }

    #[test]
    fn invalid_guard_fields_are_rejected_at_publish_time() {
        let mut schema: Blueprint =
            serde_json::from_str(include_str!("../../../blueprints/development.json")).unwrap();
        schema.work_items[0].transitions[0]
            .guards
            .push(TransitionGuard::PropertyPresent {
                field: "not_declared".into(),
            });
        assert!(schema.validate(&BTreeSet::new()).is_err());
    }
}
