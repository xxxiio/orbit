//! Atomic metric writes with the same semantic outbox and replay path as projects.
use crate::{
    ApplicationError, Clock, MetricEvidenceRepository, MetricObservation, MetricTargetRecord,
    PendingSemanticChange, SemanticEntityKind, UnitOfWork, enqueue_if_sync_enabled, sync_field,
};

pub fn set_metric_target<R: UnitOfWork>(
    repository: &mut R,
    clock: &impl Clock,
    target: MetricTargetRecord,
) -> Result<(), ApplicationError> {
    repository.transact(|tx| {
        let before = MetricEvidenceRepository::get_metric_target(tx, &target.metric_id)?;
        if before.as_ref() == Some(&target) {
            return Ok(());
        }
        MetricEvidenceRepository::put_metric_target(tx, &target)?;
        let direction = match target.direction {
            orbit_progress::MetricTargetDirection::AtLeast => "at_least",
            orbit_progress::MetricTargetDirection::AtMost => "at_most",
        };
        let value = format!("{}:{}:{}", target.baseline, target.target, direction);
        enqueue_if_sync_enabled(
            tx,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Metric,
                target.metric_id,
                vec![sync_field("target", Some(value))?],
            )?,
        )?;
        Ok(())
    })
}

pub fn record_metric_observation<R: UnitOfWork>(
    repository: &mut R,
    clock: &impl Clock,
    observation: MetricObservation,
) -> Result<(), ApplicationError> {
    repository.transact(|tx| {
        // Reject an unknown target and conflicting duplicate timestamp; exact retries are idempotent.
        if MetricEvidenceRepository::get_metric_target(tx, &observation.metric_id)?.is_none() {
            return Err(ApplicationError::InconsistentReference {
                relation: "metric_target",
            });
        }
        let existing = MetricEvidenceRepository::latest_metric_observation(
            tx,
            &observation.metric_id,
            observation.observed_at_ms,
        )?;
        if let Some(previous) = existing.filter(|m| m.observed_at_ms == observation.observed_at_ms)
        {
            if previous.value == observation.value {
                return Ok(());
            }
            return Err(ApplicationError::InconsistentReference {
                relation: "metric_observation_timestamp",
            });
        }
        MetricEvidenceRepository::append_metric_observation(tx, &observation)?;
        enqueue_if_sync_enabled(
            tx,
            clock.now_ms(),
            PendingSemanticChange::new(
                SemanticEntityKind::Metric,
                observation.metric_id,
                vec![sync_field(
                    format!("observation:{}", observation.observed_at_ms),
                    Some(observation.value.to_string()),
                )?],
            )?,
        )?;
        Ok(())
    })
}
