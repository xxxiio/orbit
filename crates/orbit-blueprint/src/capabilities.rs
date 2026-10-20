//! Registration is distinct from authorization. A blueprint cannot grant its own permissions.
use crate::{Blueprint, BlueprintError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityPermission {
    Read,
    Mutate,
    ExternalEffect,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub id: String,
    pub provider: String,
    pub permission: CapabilityPermission,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityStatus {
    Available,
    Disabled,
    Missing,
}
#[derive(Default)]
pub struct CapabilityCatalog {
    providers: BTreeMap<String, CapabilityDescriptor>,
}
impl CapabilityCatalog {
    pub fn register(&mut self, capability: CapabilityDescriptor) -> Result<(), BlueprintError> {
        if !valid(&capability.id) || !valid(&capability.provider) {
            return Err(BlueprintError::Invalid(
                "invalid capability/provider id".into(),
            ));
        }
        match self.providers.get(&capability.id) {
            Some(existing) if existing == &capability => Ok(()),
            Some(_) => Err(BlueprintError::Conflict(
                "capability already registered by a different provider or contract".into(),
            )),
            None => {
                self.providers.insert(capability.id.clone(), capability);
                Ok(())
            }
        }
    }
    pub fn status(&self, id: &str) -> CapabilityStatus {
        match self.providers.get(id) {
            None => CapabilityStatus::Missing,
            Some(c) if !c.enabled => CapabilityStatus::Disabled,
            Some(_) => CapabilityStatus::Available,
        }
    }
    pub fn available(&self) -> BTreeSet<String> {
        self.providers
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(k, _)| k.clone())
            .collect()
    }
    /// This checks declared availability only. Permission grants must be evaluated by the host.
    pub fn validate_blueprint(&self, blueprint: &Blueprint) -> Result<(), BlueprintError> {
        blueprint.validate(&self.available())
    }
    pub fn authorize(
        &self,
        id: &str,
        host_grants: &BTreeSet<String>,
    ) -> Result<(), BlueprintError> {
        match self.status(id) {
            CapabilityStatus::Available if host_grants.contains(id) => Ok(()),
            _ => Err(BlueprintError::Invalid(format!(
                "capability not available or authorized: {id}"
            ))),
        }
    }
}
fn valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_'
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_not_a_permission_grant() {
        let mut catalog = CapabilityCatalog::default();
        catalog
            .register(CapabilityDescriptor {
                id: "agent.launch".into(),
                provider: "herdr".into(),
                permission: CapabilityPermission::ExternalEffect,
                enabled: true,
            })
            .unwrap();
        assert_eq!(catalog.status("agent.launch"), CapabilityStatus::Available);
        assert!(catalog.authorize("agent.launch", &BTreeSet::new()).is_err());
        assert!(
            catalog
                .authorize("agent.launch", &BTreeSet::from(["agent.launch".into()]))
                .is_ok()
        );
    }
}
