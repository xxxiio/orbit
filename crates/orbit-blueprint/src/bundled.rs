//! Trusted, optional example blueprint packages. Nothing is installed implicitly.
//! All packages use the same public manifest format as user-authored definitions.
use crate::{Blueprint, BlueprintError};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledBlueprintDescriptor {
    pub id: &'static str,
    pub version: u32,
    pub name: &'static str,
    pub description: &'static str,
}

pub const BUNDLED_BLUEPRINTS: &[BundledBlueprintDescriptor] = &[
    BundledBlueprintDescriptor {
        id: "development",
        version: 2,
        name: "Development",
        description: "Software-development work items",
    },
    BundledBlueprintDescriptor {
        id: "learning",
        version: 2,
        name: "Learning",
        description: "Lessons and assessments",
    },
    BundledBlueprintDescriptor {
        id: "habits",
        version: 2,
        name: "Habits",
        description: "Optional activity and check-in workflows",
    },
    BundledBlueprintDescriptor {
        id: "research",
        version: 2,
        name: "Research",
        description: "Hypotheses and experiments",
    },
];

pub fn bundled_manifest(id: &str) -> Result<&'static str, BlueprintError> {
    match id {
        "development" => Ok(include_str!("../../../blueprints/development.json")),
        "learning" => Ok(include_str!("../../../blueprints/learning.json")),
        "habits" => Ok(include_str!("../../../blueprints/habits.json")),
        "research" => Ok(include_str!("../../../blueprints/research.json")),
        _ => Err(BlueprintError::NotFound(format!(
            "unknown bundled blueprint: {id}"
        ))),
    }
}

pub fn bundled_blueprint(id: &str) -> Result<Blueprint, BlueprintError> {
    Blueprint::from_json(bundled_manifest(id)?, &BTreeSet::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_bundled_manifest_is_valid_and_versioned() {
        for descriptor in BUNDLED_BLUEPRINTS {
            let blueprint = bundled_blueprint(descriptor.id).unwrap();
            assert_eq!(blueprint.id, descriptor.id);
            assert_eq!(blueprint.version, descriptor.version);
            assert!(!blueprint.work_items.is_empty());
            for template in &blueprint.templates {
                let first = blueprint
                    .plan_template(&template.id, "example", "example")
                    .unwrap();
                assert_eq!(
                    first,
                    blueprint
                        .plan_template(&template.id, "example", "example")
                        .unwrap()
                );
            }
        }
    }
    #[test]
    fn unknown_package_is_not_an_implicit_installation() {
        assert!(bundled_blueprint("unknown").is_err());
    }
}
