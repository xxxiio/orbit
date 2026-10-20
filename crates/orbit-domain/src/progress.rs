use crate::DomainError;

/// Normalized progress value in basis points, where 10_000 == 100%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgressBasisPoints(u16);

impl ProgressBasisPoints {
    pub const MAX: u16 = 10_000;

    pub fn new(value: u16) -> Result<Self, DomainError> {
        if value > Self::MAX {
            return Err(DomainError::ProgressOutOfRange);
        }
        Ok(Self(value))
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Domain-level progress sources. External systems and agents normalize their
/// observations before they cross into the Orbit domain.
#[derive(Debug, Clone, PartialEq)]
pub enum ProgressSource {
    TaskDerived {
        completed: u32,
        total: u32,
    },
    NumericMetric {
        current: f64,
        target: f64,
    },
    Manual {
        progress: ProgressBasisPoints,
    },
    External {
        provider: String,
        progress: ProgressBasisPoints,
    },
    AgentReported {
        agent: String,
        progress: ProgressBasisPoints,
    },
}

impl ProgressSource {
    pub fn task_derived(completed: u32, total: u32) -> Result<Self, DomainError> {
        if total == 0 || completed > total {
            return Err(DomainError::InvalidTaskDerivedProgress);
        }
        Ok(Self::TaskDerived { completed, total })
    }

    pub fn numeric_metric(current: f64, target: f64) -> Result<Self, DomainError> {
        if !current.is_finite() || !target.is_finite() || target <= 0.0 || current < 0.0 {
            return Err(DomainError::InvalidNumericProgress);
        }
        Ok(Self::NumericMetric { current, target })
    }

    pub fn external(
        provider: impl Into<String>,
        progress: ProgressBasisPoints,
    ) -> Result<Self, DomainError> {
        let provider = provider.into();
        if provider.trim().is_empty() {
            return Err(DomainError::EmptyProgressSourceName);
        }
        Ok(Self::External { provider, progress })
    }

    pub fn agent_reported(
        agent: impl Into<String>,
        progress: ProgressBasisPoints,
    ) -> Result<Self, DomainError> {
        let agent = agent.into();
        if agent.trim().is_empty() {
            return Err(DomainError::EmptyProgressSourceName);
        }
        Ok(Self::AgentReported { agent, progress })
    }

    pub fn normalized(&self) -> ProgressBasisPoints {
        match self {
            Self::TaskDerived { completed, total } => {
                let value =
                    ((*completed as u64 * ProgressBasisPoints::MAX as u64) / *total as u64) as u16;
                ProgressBasisPoints(value)
            }
            Self::NumericMetric { current, target } => {
                let ratio = (*current / *target).clamp(0.0, 1.0);
                ProgressBasisPoints((ratio * ProgressBasisPoints::MAX as f64).round() as u16)
            }
            Self::Manual { progress }
            | Self::External { progress, .. }
            | Self::AgentReported { progress, .. } => *progress,
        }
    }
}

/// Extension contract for custom progress calculators that can be implemented
/// outside `orbit-domain` while returning the same normalized value type.
pub trait ProgressCalculator {
    type Input;

    fn calculate(&self, input: &Self::Input) -> Result<ProgressBasisPoints, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_points_reject_values_above_one_hundred_percent() {
        assert_eq!(
            ProgressBasisPoints::new(10_001),
            Err(DomainError::ProgressOutOfRange)
        );
    }

    #[test]
    fn task_derived_progress_normalizes_deterministically() {
        let progress = ProgressSource::task_derived(3, 4).expect("valid task progress");
        assert_eq!(progress.normalized().value(), 7_500);
    }

    #[test]
    fn invalid_task_derived_progress_is_rejected() {
        assert_eq!(
            ProgressSource::task_derived(1, 0),
            Err(DomainError::InvalidTaskDerivedProgress)
        );
        assert_eq!(
            ProgressSource::task_derived(5, 4),
            Err(DomainError::InvalidTaskDerivedProgress)
        );
    }

    #[test]
    fn numeric_progress_clamps_above_target() {
        let progress =
            ProgressSource::numeric_metric(120.0, 100.0).expect("valid numeric progress");
        assert_eq!(progress.normalized().value(), 10_000);
    }

    #[test]
    fn external_and_agent_sources_require_names() {
        let value = ProgressBasisPoints::new(5_000).expect("valid progress");
        assert_eq!(
            ProgressSource::external("   ", value),
            Err(DomainError::EmptyProgressSourceName)
        );
        assert_eq!(
            ProgressSource::agent_reported("", value),
            Err(DomainError::EmptyProgressSourceName)
        );
    }

    #[test]
    fn all_required_source_variants_normalize() {
        let value = ProgressBasisPoints::new(4_200).expect("valid progress");
        let sources = [
            ProgressSource::task_derived(42, 100).expect("task-derived"),
            ProgressSource::numeric_metric(42.0, 100.0).expect("numeric"),
            ProgressSource::Manual { progress: value },
            ProgressSource::external("fitness-api", value).expect("external"),
            ProgressSource::agent_reported("coding-agent", value).expect("agent"),
        ];

        for source in sources {
            assert_eq!(source.normalized().value(), 4_200);
        }
    }
}
