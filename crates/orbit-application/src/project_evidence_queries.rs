//! Read-only, source-backed project evidence: missing data is never zero progress.
use crate::{
    ApplicationError, HabitOccurrenceRepository, HabitRepository, MetricEvidenceRepository,
    ProjectEvidenceBinding, ProjectEvidenceRepository, ProjectRepository,
};
use orbit_domain::{HabitId, HabitPeriod, ProgressBasisPoints, ProjectId};
use orbit_habit_capability::{HabitRecurrenceStatus, evaluate_habit_periods};
use orbit_progress::habit_consistency_evidence;
use orbit_progress::{MetricTargetEvidence, metric_target_evidence};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundHabitEvidence {
    pub habit_id: HabitId,
    pub available: bool,
    pub fulfilled_periods: u32,
    pub evaluated_periods: u32,
    pub consistency: Option<ProgressBasisPoints>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundMetricEvidence {
    pub metric_id: String,
    pub evaluation: Option<MetricTargetEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedProjectEvidenceReport {
    pub habits: ProjectEvidenceReport,
    pub metrics: Vec<BoundMetricEvidence>,
    /// Equal-weight score from known habit and metric values; None means no evidence.
    pub evidence_progress: Option<ProgressBasisPoints>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEvidenceReport {
    pub project_id: ProjectId,
    pub habits: Vec<BoundHabitEvidence>,
    /// Bound metric identifiers have no value/target repository yet. Never invent observations.
    pub metrics_awaiting_source: Vec<String>,
    /// Equal-weight mean of habits with evaluated periods only. None means unknown.
    pub habit_consistency: Option<ProgressBasisPoints>,
    /// Whether recurrence evaluation was available for this report. When this is
    /// `Unavailable`, bound habits keep `available: false` and no streak or
    /// consistency value is computed in core.
    pub habit_recurrence: HabitRecurrenceStatus,
}

/// Explicit caller-supplied periods permit daily or weekly cadence windows, with no
/// assumed timezone or fabricated historical records. Periods are validated by domain.
/// Recurrence itself is delegated to the opt-in `orbit.habit.recurrence` capability:
/// without an available capability, habits are reported as unevaluated rather than
/// silently computed in core.
pub fn get_project_evidence_report<R>(
    repository: &R,
    project_id: &ProjectId,
    periods: &[HabitPeriod],
    recurrence: &HabitRecurrenceStatus,
) -> Result<ProjectEvidenceReport, ApplicationError>
where
    R: ProjectRepository + ProjectEvidenceRepository + HabitRepository + HabitOccurrenceRepository,
{
    if ProjectRepository::get(repository, project_id)?.is_none() {
        return Err(ApplicationError::NotFound {
            entity: "project",
            id: project_id.to_string(),
        });
    }
    let mut habits = Vec::new();
    let mut metrics_awaiting_source = Vec::new();
    for binding in ProjectEvidenceRepository::list_project_evidence(repository, project_id)? {
        match binding {
            ProjectEvidenceBinding::Metric(id) => metrics_awaiting_source.push(id),
            ProjectEvidenceBinding::Habit(id) => {
                let Some(habit) = HabitRepository::get(repository, &id)? else {
                    habits.push(BoundHabitEvidence {
                        habit_id: id,
                        available: false,
                        fulfilled_periods: 0,
                        evaluated_periods: 0,
                        consistency: None,
                    });
                    continue;
                };
                if !recurrence.is_available() {
                    habits.push(BoundHabitEvidence {
                        habit_id: id,
                        available: false,
                        fulfilled_periods: 0,
                        evaluated_periods: 0,
                        consistency: None,
                    });
                    continue;
                }
                let occurrences = HabitOccurrenceRepository::list_for_habit(repository, &id)?;
                let evaluation = evaluate_habit_periods(habit.cadence(), periods, &occurrences)?;
                let fulfilled: Vec<bool> = evaluation
                    .periods
                    .iter()
                    .map(|period| period.fulfilled)
                    .collect();
                let consistency = habit_consistency_evidence(&fulfilled)?;
                habits.push(BoundHabitEvidence {
                    habit_id: id,
                    available: true,
                    fulfilled_periods: consistency.fulfilled_periods,
                    evaluated_periods: consistency.evaluated_periods,
                    consistency: consistency.consistency,
                });
            }
        }
    }
    let habit_consistency = average_known_habit_consistency(&habits)?;
    Ok(ProjectEvidenceReport {
        project_id: project_id.clone(),
        habits,
        metrics_awaiting_source,
        habit_consistency,
        habit_recurrence: recurrence.clone(),
    })
}

fn average_known_habit_consistency(
    habits: &[BoundHabitEvidence],
) -> Result<Option<ProgressBasisPoints>, ApplicationError> {
    let known: Vec<u16> = habits
        .iter()
        .filter_map(|h| h.consistency.map(|p| p.value()))
        .collect();
    if known.is_empty() {
        return Ok(None);
    }
    let sum: u64 = known.iter().map(|&v| u64::from(v)).sum();
    let mean = (sum + known.len() as u64 / 2) / known.len() as u64;
    Ok(Some(ProgressBasisPoints::new(mean as u16)?))
}

/// Read-only as-of snapshot: no future measurement can leak into a historical report.
pub fn get_combined_project_evidence_report<R>(
    repository: &R,
    project_id: &ProjectId,
    periods: &[HabitPeriod],
    as_of_ms: i64,
    recurrence: &HabitRecurrenceStatus,
) -> Result<CombinedProjectEvidenceReport, ApplicationError>
where
    R: ProjectRepository
        + ProjectEvidenceRepository
        + HabitRepository
        + HabitOccurrenceRepository
        + MetricEvidenceRepository,
{
    let habits = get_project_evidence_report(repository, project_id, periods, recurrence)?;
    let mut values: Vec<u16> = habits
        .habits
        .iter()
        .filter_map(|h| h.consistency.map(|v| v.value()))
        .collect();
    let mut metrics = Vec::new();
    for metric_id in &habits.metrics_awaiting_source {
        let evaluation = match (
            repository.get_metric_target(metric_id)?,
            repository.latest_metric_observation(metric_id, as_of_ms)?,
        ) {
            (Some(target), Some(observation)) => Some(metric_target_evidence(
                target.baseline,
                observation.value,
                target.target,
                target.direction,
            )?),
            _ => None,
        };
        if let Some(value) = evaluation {
            values.push(value.progress.value());
        }
        metrics.push(BoundMetricEvidence {
            metric_id: metric_id.clone(),
            evaluation,
        });
    }
    let evidence_progress = if values.is_empty() {
        None
    } else {
        let sum: u64 = values.iter().map(|v| u64::from(*v)).sum();
        Some(ProgressBasisPoints::new(
            ((sum + values.len() as u64 / 2) / values.len() as u64) as u16,
        )?)
    };
    Ok(CombinedProjectEvidenceReport {
        habits,
        metrics,
        evidence_progress,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn habit(name: &str, value: Option<u16>) -> BoundHabitEvidence {
        BoundHabitEvidence {
            habit_id: HabitId::new(name).unwrap(),
            available: value.is_some(),
            fulfilled_periods: 0,
            evaluated_periods: 0,
            consistency: value.map(|v| ProgressBasisPoints::new(v).unwrap()),
        }
    }
    #[test]
    fn missing_evidence_does_not_turn_into_zero() {
        assert_eq!(
            average_known_habit_consistency(&[habit("missing", None)]).unwrap(),
            None
        );
        assert_eq!(
            average_known_habit_consistency(&[habit("a", Some(5000)), habit("missing", None)])
                .unwrap()
                .unwrap()
                .value(),
            5000
        );
    }
    #[test]
    fn equally_weights_only_evaluated_habits() {
        assert_eq!(
            average_known_habit_consistency(&[habit("a", Some(10000)), habit("b", Some(5000))])
                .unwrap()
                .unwrap()
                .value(),
            7500
        );
    }
}
