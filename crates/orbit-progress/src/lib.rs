//! Deterministic progress, variance, blocker exposure, and forecasting metrics.
//!
//! This crate intentionally exposes quantitative evidence rather than opaque
//! health opinions. Higher-level clients may map these facts to labels, but the
//! underlying calculations remain deterministic and inspectable.

use orbit_domain::ProgressBasisPoints;

const MAX_PROGRESS_BP: u32 = 10_000;
const MINUTE_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressError {
    EmptyRollup,
    ZeroWeight,
    InvalidPlannedActual,
    InvalidScheduleActual,
    InvalidObservationSeries,
    NoProgressRate,
    ArithmeticOverflow,
}

/// One progress contribution to a weighted rollup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressContribution {
    pub progress: ProgressBasisPoints,
    pub weight: u32,
}

impl ProgressContribution {
    pub fn new(progress: ProgressBasisPoints, weight: u32) -> Result<Self, ProgressError> {
        if weight == 0 {
            return Err(ProgressError::ZeroWeight);
        }
        Ok(Self { progress, weight })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressRollup {
    pub progress: ProgressBasisPoints,
    pub item_count: u32,
    pub total_weight: u64,
}

/// Weighted rollup using integer arithmetic and half-up rounding.
///
/// Equal weights produce a normal arithmetic mean. Callers can use estimates,
/// milestone importance, or another explicit deterministic policy as weights.
pub fn weighted_rollup(items: &[ProgressContribution]) -> Result<ProgressRollup, ProgressError> {
    if items.is_empty() {
        return Err(ProgressError::EmptyRollup);
    }

    let mut weighted_sum = 0u128;
    let mut total_weight = 0u64;
    for item in items {
        if item.weight == 0 {
            return Err(ProgressError::ZeroWeight);
        }
        weighted_sum = weighted_sum
            .checked_add(u128::from(item.progress.value()) * u128::from(item.weight))
            .ok_or(ProgressError::ArithmeticOverflow)?;
        total_weight = total_weight
            .checked_add(u64::from(item.weight))
            .ok_or(ProgressError::ArithmeticOverflow)?;
    }

    let denominator = u128::from(total_weight);
    let rounded = weighted_sum
        .checked_add(denominator / 2)
        .ok_or(ProgressError::ArithmeticOverflow)?
        / denominator;
    let progress =
        ProgressBasisPoints::new(rounded as u16).map_err(|_| ProgressError::ArithmeticOverflow)?;

    Ok(ProgressRollup {
        progress,
        item_count: items.len() as u32,
        total_weight,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedVsActual {
    pub planned: ProgressBasisPoints,
    pub actual: ProgressBasisPoints,
    /// `actual - planned` in basis points. Negative means behind plan.
    pub variance_bp: i32,
}

impl PlannedVsActual {
    pub fn new(planned: ProgressBasisPoints, actual: ProgressBasisPoints) -> Self {
        Self {
            planned,
            actual,
            variance_bp: i32::from(actual.value()) - i32::from(planned.value()),
        }
    }

    pub fn is_behind(self) -> bool {
        self.variance_bp < 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleVariance {
    pub planned_minutes: u32,
    pub actual_minutes: u32,
    /// `actual - planned`. Positive means more effort than planned.
    pub variance_minutes: i64,
    /// Actual/planned ratio in basis points. 10_000 means exactly on estimate.
    /// `None` when no planned minutes exist.
    pub actual_to_planned_bp: Option<u32>,
}

impl ScheduleVariance {
    pub fn new(planned_minutes: u32, actual_minutes: u32) -> Result<Self, ProgressError> {
        let variance_minutes = i64::from(actual_minutes) - i64::from(planned_minutes);
        let actual_to_planned_bp = if planned_minutes == 0 {
            None
        } else {
            Some(
                u32::try_from(
                    u64::from(actual_minutes)
                        .checked_mul(u64::from(MAX_PROGRESS_BP))
                        .ok_or(ProgressError::ArithmeticOverflow)?
                        .checked_add(u64::from(planned_minutes) / 2)
                        .ok_or(ProgressError::ArithmeticOverflow)?
                        .checked_div(u64::from(planned_minutes))
                        .ok_or(ProgressError::InvalidScheduleActual)?,
                )
                .map_err(|_| ProgressError::ArithmeticOverflow)?,
            )
        };
        Ok(Self {
            planned_minutes,
            actual_minutes,
            variance_minutes,
            actual_to_planned_bp,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockerExposure {
    pub open_blockers: u32,
    pub blocked_tasks: u32,
    pub blocked_milestones: u32,
    pub oldest_open_blocker_age_minutes: Option<u64>,
}

impl BlockerExposure {
    pub fn has_exposure(self) -> bool {
        self.open_blockers > 0 || self.blocked_tasks > 0 || self.blocked_milestones > 0
    }

    pub fn total_blocked_entities(self) -> u32 {
        self.blocked_tasks.saturating_add(self.blocked_milestones)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressObservation {
    pub observed_at_ms: i64,
    pub progress: ProgressBasisPoints,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionForecast {
    pub latest_progress: ProgressBasisPoints,
    /// Average positive progress rate over the accepted observation span.
    pub basis_points_per_day: u32,
    pub projected_finish_at_ms: i64,
    pub observation_span_minutes: u64,
}

/// Forecast completion from the first and last observations using a transparent
/// average progress rate. The series must be strictly time-ordered and
/// non-decreasing in progress. Regressions fail closed instead of producing an
/// optimistic projection.
pub fn forecast_completion(
    observations: &[ProgressObservation],
) -> Result<CompletionForecast, ProgressError> {
    if observations.len() < 2 {
        return Err(ProgressError::InvalidObservationSeries);
    }

    for pair in observations.windows(2) {
        if pair[1].observed_at_ms <= pair[0].observed_at_ms || pair[1].progress < pair[0].progress {
            return Err(ProgressError::InvalidObservationSeries);
        }
    }

    let first = observations[0];
    let last = observations[observations.len() - 1];
    if last.progress.value() >= ProgressBasisPoints::MAX {
        return Ok(CompletionForecast {
            latest_progress: last.progress,
            basis_points_per_day: 0,
            projected_finish_at_ms: last.observed_at_ms,
            observation_span_minutes: ((last.observed_at_ms - first.observed_at_ms) / MINUTE_MS)
                as u64,
        });
    }

    let progress_delta = u64::from(last.progress.value() - first.progress.value());
    if progress_delta == 0 {
        return Err(ProgressError::NoProgressRate);
    }

    let elapsed_ms = last
        .observed_at_ms
        .checked_sub(first.observed_at_ms)
        .ok_or(ProgressError::ArithmeticOverflow)?;
    let elapsed_minutes = elapsed_ms / MINUTE_MS;
    if elapsed_minutes <= 0 {
        return Err(ProgressError::InvalidObservationSeries);
    }

    let minutes_per_day = 24u64 * 60;
    let rate_bp_per_day = progress_delta
        .checked_mul(minutes_per_day)
        .ok_or(ProgressError::ArithmeticOverflow)?
        .checked_add((elapsed_minutes as u64) / 2)
        .ok_or(ProgressError::ArithmeticOverflow)?
        / elapsed_minutes as u64;
    if rate_bp_per_day == 0 {
        return Err(ProgressError::NoProgressRate);
    }

    let remaining_bp = u64::from(ProgressBasisPoints::MAX - last.progress.value());
    let remaining_minutes = remaining_bp
        .checked_mul(minutes_per_day)
        .ok_or(ProgressError::ArithmeticOverflow)?
        .checked_add(rate_bp_per_day - 1)
        .ok_or(ProgressError::ArithmeticOverflow)?
        / rate_bp_per_day;
    let finish_delta_ms = i64::try_from(remaining_minutes)
        .map_err(|_| ProgressError::ArithmeticOverflow)?
        .checked_mul(MINUTE_MS)
        .ok_or(ProgressError::ArithmeticOverflow)?;
    let projected_finish_at_ms = last
        .observed_at_ms
        .checked_add(finish_delta_ms)
        .ok_or(ProgressError::ArithmeticOverflow)?;

    Ok(CompletionForecast {
        latest_progress: last.progress,
        basis_points_per_day: u32::try_from(rate_bp_per_day)
            .map_err(|_| ProgressError::ArithmeticOverflow)?,
        projected_finish_at_ms,
        observation_span_minutes: elapsed_minutes as u64,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastVariance {
    pub target_finish_at_ms: i64,
    pub projected_finish_at_ms: i64,
    /// Positive means the forecast is later than target.
    pub variance_minutes: i64,
}

impl ForecastVariance {
    pub fn new(
        target_finish_at_ms: i64,
        projected_finish_at_ms: i64,
    ) -> Result<Self, ProgressError> {
        let variance_ms = projected_finish_at_ms
            .checked_sub(target_finish_at_ms)
            .ok_or(ProgressError::ArithmeticOverflow)?;
        Ok(Self {
            target_finish_at_ms,
            projected_finish_at_ms,
            variance_minutes: variance_ms / MINUTE_MS,
        })
    }

    pub fn is_projected_late(self) -> bool {
        self.variance_minutes > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bp(value: u16) -> ProgressBasisPoints {
        ProgressBasisPoints::new(value).expect("valid basis points")
    }

    #[test]
    fn weighted_rollup_supports_equal_and_explicit_weights() {
        let equal = weighted_rollup(&[
            ProgressContribution::new(bp(2_000), 1).unwrap(),
            ProgressContribution::new(bp(6_000), 1).unwrap(),
        ])
        .unwrap();
        assert_eq!(equal.progress.value(), 4_000);

        let weighted = weighted_rollup(&[
            ProgressContribution::new(bp(2_000), 1).unwrap(),
            ProgressContribution::new(bp(6_000), 3).unwrap(),
        ])
        .unwrap();
        assert_eq!(weighted.progress.value(), 5_000);
        assert_eq!(weighted.total_weight, 4);
    }

    #[test]
    fn empty_or_zero_weight_rollups_fail_closed() {
        assert_eq!(weighted_rollup(&[]), Err(ProgressError::EmptyRollup));
        assert_eq!(
            ProgressContribution::new(bp(5_000), 0),
            Err(ProgressError::ZeroWeight)
        );
    }

    #[test]
    fn planned_vs_actual_preserves_signed_variance() {
        let metric = PlannedVsActual::new(bp(7_000), bp(5_500));
        assert_eq!(metric.variance_bp, -1_500);
        assert!(metric.is_behind());
    }

    #[test]
    fn schedule_variance_reports_overrun_ratio() {
        let metric = ScheduleVariance::new(60, 90).unwrap();
        assert_eq!(metric.variance_minutes, 30);
        assert_eq!(metric.actual_to_planned_bp, Some(15_000));
    }

    #[test]
    fn blocker_exposure_is_transparent_not_a_health_label() {
        let exposure = BlockerExposure {
            open_blockers: 2,
            blocked_tasks: 1,
            blocked_milestones: 1,
            oldest_open_blocker_age_minutes: Some(120),
        };
        assert!(exposure.has_exposure());
        assert_eq!(exposure.total_blocked_entities(), 2);
    }

    #[test]
    fn forecast_uses_observed_progress_rate() {
        let day = 24 * 60 * MINUTE_MS;
        let observations = [
            ProgressObservation {
                observed_at_ms: 0,
                progress: bp(2_000),
            },
            ProgressObservation {
                observed_at_ms: 2 * day,
                progress: bp(4_000),
            },
        ];
        let forecast = forecast_completion(&observations).unwrap();
        assert_eq!(forecast.basis_points_per_day, 1_000);
        assert_eq!(forecast.projected_finish_at_ms, 8 * day);
    }

    #[test]
    fn forecast_rejects_regression_or_no_progress() {
        let day = 24 * 60 * MINUTE_MS;
        assert_eq!(
            forecast_completion(&[
                ProgressObservation {
                    observed_at_ms: 0,
                    progress: bp(5_000)
                },
                ProgressObservation {
                    observed_at_ms: day,
                    progress: bp(4_000)
                },
            ]),
            Err(ProgressError::InvalidObservationSeries)
        );
        assert_eq!(
            forecast_completion(&[
                ProgressObservation {
                    observed_at_ms: 0,
                    progress: bp(5_000)
                },
                ProgressObservation {
                    observed_at_ms: day,
                    progress: bp(5_000)
                },
            ]),
            Err(ProgressError::NoProgressRate)
        );
    }

    #[test]
    fn completed_progress_forecasts_completion_at_latest_observation() {
        let forecast = forecast_completion(&[
            ProgressObservation {
                observed_at_ms: 0,
                progress: bp(9_000),
            },
            ProgressObservation {
                observed_at_ms: 60_000,
                progress: bp(10_000),
            },
        ])
        .unwrap();
        assert_eq!(forecast.projected_finish_at_ms, 60_000);
    }

    #[test]
    fn forecast_variance_reports_lateness_in_minutes() {
        let variance = ForecastVariance::new(1_000_000, 1_600_000).unwrap();
        assert_eq!(variance.variance_minutes, 10);
        assert!(variance.is_projected_late());
    }
}

/// Transparent evidence bundle used by higher-level project health projections.
///
/// This intentionally contains no composite score. Each field is independently
/// inspectable and can be rendered/explained by clients without reverse-
/// engineering a hidden weighting model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectProgressEvidence {
    pub actual_progress: ProgressBasisPoints,
    pub planned_vs_actual: Option<PlannedVsActual>,
    pub schedule_variance: Option<ScheduleVariance>,
    pub blocker_exposure: BlockerExposure,
    pub forecast: Option<CompletionForecast>,
    pub forecast_variance: Option<ForecastVariance>,
}

impl ProjectProgressEvidence {
    pub fn new(
        actual_progress: ProgressBasisPoints,
        planned_progress: Option<ProgressBasisPoints>,
        schedule_variance: Option<ScheduleVariance>,
        blocker_exposure: BlockerExposure,
        forecast: Option<CompletionForecast>,
        target_finish_at_ms: Option<i64>,
    ) -> Result<Self, ProgressError> {
        let planned_vs_actual =
            planned_progress.map(|planned| PlannedVsActual::new(planned, actual_progress));
        let forecast_variance = match (forecast, target_finish_at_ms) {
            (Some(forecast), Some(target)) => Some(ForecastVariance::new(
                target,
                forecast.projected_finish_at_ms,
            )?),
            _ => None,
        };

        Ok(Self {
            actual_progress,
            planned_vs_actual,
            schedule_variance,
            blocker_exposure,
            forecast,
            forecast_variance,
        })
    }

    pub fn is_behind_plan(self) -> bool {
        self.planned_vs_actual
            .is_some_and(PlannedVsActual::is_behind)
    }

    pub fn is_projected_late(self) -> bool {
        self.forecast_variance
            .is_some_and(ForecastVariance::is_projected_late)
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;

    fn bp(value: u16) -> ProgressBasisPoints {
        ProgressBasisPoints::new(value).expect("valid basis points")
    }

    #[test]
    fn project_evidence_preserves_each_independent_signal() {
        let day = 24 * 60 * MINUTE_MS;
        let forecast = CompletionForecast {
            latest_progress: bp(5_000),
            basis_points_per_day: 500,
            projected_finish_at_ms: 10 * day,
            observation_span_minutes: 2 * 24 * 60,
        };
        let evidence = ProjectProgressEvidence::new(
            bp(5_000),
            Some(bp(6_000)),
            Some(ScheduleVariance::new(100, 130).unwrap()),
            BlockerExposure {
                open_blockers: 1,
                blocked_tasks: 1,
                blocked_milestones: 0,
                oldest_open_blocker_age_minutes: Some(90),
            },
            Some(forecast),
            Some(8 * day),
        )
        .unwrap();

        assert!(evidence.is_behind_plan());
        assert!(evidence.blocker_exposure.has_exposure());
        assert!(evidence.is_projected_late());
        assert_eq!(evidence.schedule_variance.unwrap().variance_minutes, 30);
    }

    #[test]
    fn project_evidence_does_not_require_plan_or_forecast() {
        let evidence = ProjectProgressEvidence::new(
            bp(2_500),
            None,
            None,
            BlockerExposure::default(),
            None,
            None,
        )
        .unwrap();

        assert!(!evidence.is_behind_plan());
        assert!(!evidence.is_projected_late());
        assert_eq!(evidence.planned_vs_actual, None);
        assert_eq!(evidence.forecast_variance, None);
    }
}

/// Named weighting policy for canonical task/milestone rollups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollupWeightPolicy {
    Equal,
    EstimateMinutes,
}

/// Canonical task progress input derived from an Orbit task plus an optional
/// normalized progress source. Cancelled tasks are excluded from rollups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalTaskProgress {
    pub status: orbit_domain::TaskStatus,
    pub estimated_minutes: Option<u32>,
    pub explicit_progress: Option<ProgressBasisPoints>,
}

impl CanonicalTaskProgress {
    pub fn from_task(
        task: &orbit_domain::Task,
        explicit_progress: Option<ProgressBasisPoints>,
    ) -> Self {
        Self {
            status: task.status(),
            estimated_minutes: task.planning().estimated_minutes(),
            explicit_progress,
        }
    }

    fn normalized(self) -> Option<ProgressBasisPoints> {
        match self.status {
            orbit_domain::TaskStatus::Cancelled => None,
            orbit_domain::TaskStatus::Done => {
                ProgressBasisPoints::new(ProgressBasisPoints::MAX).ok()
            }
            _ => self
                .explicit_progress
                .or_else(|| ProgressBasisPoints::new(0).ok()),
        }
    }

    fn weight(self, policy: RollupWeightPolicy) -> u32 {
        match policy {
            RollupWeightPolicy::Equal => 1,
            RollupWeightPolicy::EstimateMinutes => self.estimated_minutes.unwrap_or(1).max(1),
        }
    }
}

/// Canonical milestone progress result. A completed milestone is always 100%,
/// a cancelled milestone is excluded, and active milestones roll up their
/// non-cancelled tasks using the selected explicit weighting policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalMilestoneProgress {
    pub progress: Option<ProgressBasisPoints>,
    pub included_tasks: u32,
    pub total_weight: u64,
}

pub fn canonical_milestone_rollup(
    milestone: &orbit_domain::Milestone,
    tasks: &[CanonicalTaskProgress],
    policy: RollupWeightPolicy,
) -> Result<CanonicalMilestoneProgress, ProgressError> {
    use orbit_domain::MilestoneStatus;

    if milestone.status() == MilestoneStatus::Cancelled {
        return Ok(CanonicalMilestoneProgress {
            progress: None,
            included_tasks: 0,
            total_weight: 0,
        });
    }
    if milestone.status() == MilestoneStatus::Completed {
        return Ok(CanonicalMilestoneProgress {
            progress: Some(
                ProgressBasisPoints::new(ProgressBasisPoints::MAX)
                    .map_err(|_| ProgressError::ArithmeticOverflow)?,
            ),
            included_tasks: tasks
                .iter()
                .filter(|task| task.status != orbit_domain::TaskStatus::Cancelled)
                .count() as u32,
            total_weight: tasks
                .iter()
                .copied()
                .filter(|task| task.status != orbit_domain::TaskStatus::Cancelled)
                .map(|task| u64::from(task.weight(policy)))
                .sum(),
        });
    }

    let contributions: Vec<_> = tasks
        .iter()
        .copied()
        .filter_map(|task| {
            task.normalized().map(|progress| ProgressContribution {
                progress,
                weight: task.weight(policy),
            })
        })
        .collect();

    if contributions.is_empty() {
        return Ok(CanonicalMilestoneProgress {
            progress: Some(
                ProgressBasisPoints::new(0).map_err(|_| ProgressError::ArithmeticOverflow)?,
            ),
            included_tasks: 0,
            total_weight: 0,
        });
    }

    let rollup = weighted_rollup(&contributions)?;
    Ok(CanonicalMilestoneProgress {
        progress: Some(rollup.progress),
        included_tasks: rollup.item_count,
        total_weight: rollup.total_weight,
    })
}

/// One project-level progress contribution. Milestones and direct project tasks
/// share the same normalized scale but remain visibly distinguishable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectProgressContribution {
    Milestone {
        progress: Option<ProgressBasisPoints>,
        weight: u32,
    },
    DirectTask(CanonicalTaskProgress),
}

/// Canonical project rollup. Completed projects are 100%; cancelled/archived
/// projects still report the evidence supplied rather than inventing completion.
pub fn canonical_project_rollup(
    project: &orbit_domain::Project,
    items: &[ProjectProgressContribution],
    task_weight_policy: RollupWeightPolicy,
) -> Result<ProgressRollup, ProgressError> {
    if project.status() == orbit_domain::ProjectStatus::Completed {
        return Ok(ProgressRollup {
            progress: ProgressBasisPoints::new(ProgressBasisPoints::MAX)
                .map_err(|_| ProgressError::ArithmeticOverflow)?,
            item_count: items.len() as u32,
            total_weight: items.len().max(1) as u64,
        });
    }

    let contributions: Vec<_> = items
        .iter()
        .filter_map(|item| match *item {
            ProjectProgressContribution::Milestone { progress, weight } => {
                progress.map(|progress| ProgressContribution {
                    progress,
                    weight: weight.max(1),
                })
            }
            ProjectProgressContribution::DirectTask(task) => {
                task.normalized().map(|progress| ProgressContribution {
                    progress,
                    weight: task.weight(task_weight_policy),
                })
            }
        })
        .collect();

    if contributions.is_empty() {
        return Ok(ProgressRollup {
            progress: ProgressBasisPoints::new(0).map_err(|_| ProgressError::ArithmeticOverflow)?,
            item_count: 0,
            total_weight: 0,
        });
    }
    weighted_rollup(&contributions)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressTrendDirection {
    Improving,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressTrend {
    pub start_progress: ProgressBasisPoints,
    pub end_progress: ProgressBasisPoints,
    pub delta_bp: u16,
    pub span_minutes: u64,
    pub basis_points_per_day: u32,
    pub direction: ProgressTrendDirection,
}

pub fn progress_trend(
    observations: &[ProgressObservation],
) -> Result<ProgressTrend, ProgressError> {
    if observations.len() < 2 {
        return Err(ProgressError::InvalidObservationSeries);
    }
    for pair in observations.windows(2) {
        if pair[1].observed_at_ms <= pair[0].observed_at_ms || pair[1].progress < pair[0].progress {
            return Err(ProgressError::InvalidObservationSeries);
        }
    }
    let first = observations[0];
    let last = observations[observations.len() - 1];
    let elapsed_minutes = last
        .observed_at_ms
        .checked_sub(first.observed_at_ms)
        .ok_or(ProgressError::ArithmeticOverflow)?
        / MINUTE_MS;
    if elapsed_minutes <= 0 {
        return Err(ProgressError::InvalidObservationSeries);
    }
    let delta = last.progress.value() - first.progress.value();
    let rate = u64::from(delta)
        .checked_mul(24 * 60)
        .ok_or(ProgressError::ArithmeticOverflow)?
        .checked_add((elapsed_minutes as u64) / 2)
        .ok_or(ProgressError::ArithmeticOverflow)?
        / elapsed_minutes as u64;
    Ok(ProgressTrend {
        start_progress: first.progress,
        end_progress: last.progress,
        delta_bp: delta,
        span_minutes: elapsed_minutes as u64,
        basis_points_per_day: u32::try_from(rate).map_err(|_| ProgressError::ArithmeticOverflow)?,
        direction: if delta == 0 {
            ProgressTrendDirection::Flat
        } else {
            ProgressTrendDirection::Improving
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecentActivityEvidence {
    pub activity_count: u32,
    pub last_activity_at_ms: Option<i64>,
    pub last_activity_age_minutes: Option<u64>,
    pub lookback_minutes: u64,
}

pub fn recent_activity_evidence(
    now_ms: i64,
    activity_timestamps_ms: &[i64],
    lookback_minutes: u64,
) -> Result<RecentActivityEvidence, ProgressError> {
    let lookback_ms = i64::try_from(lookback_minutes)
        .map_err(|_| ProgressError::ArithmeticOverflow)?
        .checked_mul(MINUTE_MS)
        .ok_or(ProgressError::ArithmeticOverflow)?;
    let window_start = now_ms
        .checked_sub(lookback_ms)
        .ok_or(ProgressError::ArithmeticOverflow)?;
    let mut count = 0u32;
    let mut last = None;
    for &timestamp in activity_timestamps_ms {
        if timestamp > now_ms {
            return Err(ProgressError::InvalidObservationSeries);
        }
        last = Some(last.map_or(timestamp, |current: i64| current.max(timestamp)));
        if timestamp >= window_start {
            count = count
                .checked_add(1)
                .ok_or(ProgressError::ArithmeticOverflow)?;
        }
    }
    let age = last.map(|timestamp| ((now_ms - timestamp) / MINUTE_MS) as u64);
    Ok(RecentActivityEvidence {
        activity_count: count,
        last_activity_at_ms: last,
        last_activity_age_minutes: age,
        lookback_minutes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthPolicy {
    pub behind_plan_warning_bp: u16,
    pub effort_overrun_warning_bp: u32,
    pub stale_activity_minutes: u64,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            behind_plan_warning_bp: 500,
            effort_overrun_warning_bp: 12_000,
            stale_activity_minutes: 7 * 24 * 60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectHealthReason {
    BehindPlan,
    ProjectedLate,
    OpenBlockers,
    BlockedEntities,
    EffortOverrun,
    NoRecentActivity,
    ProgressStalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectHealthState {
    OnTrack,
    NeedsAttention,
    AtRisk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHealthProjection {
    pub state: ProjectHealthState,
    pub reasons: Vec<ProjectHealthReason>,
}

/// Deterministic, explainable health projection. The policy and every reason are
/// returned explicitly; no hidden score is calculated.
pub fn project_health_projection(
    evidence: ProjectProgressEvidence,
    trend: Option<ProgressTrend>,
    activity: Option<RecentActivityEvidence>,
    policy: HealthPolicy,
) -> ProjectHealthProjection {
    let mut reasons = Vec::new();
    if evidence
        .planned_vs_actual
        .is_some_and(|variance| variance.variance_bp < -i32::from(policy.behind_plan_warning_bp))
    {
        reasons.push(ProjectHealthReason::BehindPlan);
    }
    if evidence.is_projected_late() {
        reasons.push(ProjectHealthReason::ProjectedLate);
    }
    if evidence.blocker_exposure.open_blockers > 0 {
        reasons.push(ProjectHealthReason::OpenBlockers);
    }
    if evidence.blocker_exposure.total_blocked_entities() > 0 {
        reasons.push(ProjectHealthReason::BlockedEntities);
    }
    if evidence
        .schedule_variance
        .and_then(|variance| variance.actual_to_planned_bp)
        .is_some_and(|ratio| ratio > policy.effort_overrun_warning_bp)
    {
        reasons.push(ProjectHealthReason::EffortOverrun);
    }
    if activity
        .and_then(|activity| activity.last_activity_age_minutes)
        .is_some_and(|age| age > policy.stale_activity_minutes)
    {
        reasons.push(ProjectHealthReason::NoRecentActivity);
    }
    if trend.is_some_and(|trend| trend.direction == ProgressTrendDirection::Flat)
        && evidence.actual_progress.value() < ProgressBasisPoints::MAX
    {
        reasons.push(ProjectHealthReason::ProgressStalled);
    }

    let severe = reasons.contains(&ProjectHealthReason::ProjectedLate)
        || (reasons.contains(&ProjectHealthReason::OpenBlockers)
            && reasons.contains(&ProjectHealthReason::ProgressStalled));
    let state = if severe {
        ProjectHealthState::AtRisk
    } else if reasons.is_empty() {
        ProjectHealthState::OnTrack
    } else {
        ProjectHealthState::NeedsAttention
    };

    ProjectHealthProjection { state, reasons }
}

#[cfg(test)]
mod canonical_health_tests {
    use super::*;
    use orbit_domain::{
        Milestone, MilestoneId, MilestoneStatus, Project, ProjectId, ProjectStatus, Task, TaskId,
        TaskPlanning, TaskScope, TaskStatus,
    };

    fn bp(value: u16) -> ProgressBasisPoints {
        ProgressBasisPoints::new(value).unwrap()
    }

    fn active_task(id: &str, estimate: Option<u32>) -> Task {
        let mut task = Task::new(TaskId::new(id).unwrap(), id, TaskScope::standalone()).unwrap();
        task.transition_to(TaskStatus::Ready).unwrap();
        task.set_planning(TaskPlanning::new(estimate, None, None).unwrap());
        task
    }

    #[test]
    fn canonical_milestone_rollup_excludes_cancelled_and_honors_estimates() {
        let mut milestone = Milestone::new(
            MilestoneId::new("m1").unwrap(),
            ProjectId::new("p1").unwrap(),
            "M1",
        )
        .unwrap();
        milestone
            .transition_to(MilestoneStatus::InProgress)
            .unwrap();

        let mut done = active_task("done", Some(30));
        done.transition_to(TaskStatus::InProgress).unwrap();
        done.transition_to(TaskStatus::Done).unwrap();
        let active = active_task("active", Some(90));
        let mut cancelled = active_task("cancelled", Some(1000));
        cancelled.transition_to(TaskStatus::Cancelled).unwrap();

        let result = canonical_milestone_rollup(
            &milestone,
            &[
                CanonicalTaskProgress::from_task(&done, None),
                CanonicalTaskProgress::from_task(&active, Some(bp(5_000))),
                CanonicalTaskProgress::from_task(&cancelled, None),
            ],
            RollupWeightPolicy::EstimateMinutes,
        )
        .unwrap();

        assert_eq!(result.included_tasks, 2);
        assert_eq!(result.progress.unwrap().value(), 6_250);
    }

    #[test]
    fn completed_project_reports_complete_progress() {
        let mut project = Project::new(ProjectId::new("p1").unwrap(), "P1").unwrap();
        project.transition_to(ProjectStatus::Active).unwrap();
        project.transition_to(ProjectStatus::Completed).unwrap();
        let rollup = canonical_project_rollup(&project, &[], RollupWeightPolicy::Equal).unwrap();
        assert_eq!(rollup.progress.value(), 10_000);
    }

    #[test]
    fn trend_and_recent_activity_are_quantitative() {
        let day = 24 * 60 * MINUTE_MS;
        let trend = progress_trend(&[
            ProgressObservation {
                observed_at_ms: 0,
                progress: bp(2_000),
            },
            ProgressObservation {
                observed_at_ms: 2 * day,
                progress: bp(4_000),
            },
        ])
        .unwrap();
        assert_eq!(trend.delta_bp, 2_000);
        assert_eq!(trend.basis_points_per_day, 1_000);

        let activity =
            recent_activity_evidence(10 * MINUTE_MS, &[MINUTE_MS, 9 * MINUTE_MS], 5).unwrap();
        assert_eq!(activity.activity_count, 1);
        assert_eq!(activity.last_activity_age_minutes, Some(1));
    }

    #[test]
    fn health_projection_returns_reasons_not_a_hidden_score() {
        let evidence = ProjectProgressEvidence::new(
            bp(4_000),
            Some(bp(6_000)),
            Some(ScheduleVariance::new(100, 140).unwrap()),
            BlockerExposure {
                open_blockers: 1,
                blocked_tasks: 1,
                blocked_milestones: 0,
                oldest_open_blocker_age_minutes: Some(120),
            },
            Some(CompletionForecast {
                latest_progress: bp(4_000),
                basis_points_per_day: 500,
                projected_finish_at_ms: 20 * MINUTE_MS,
                observation_span_minutes: 60,
            }),
            Some(10 * MINUTE_MS),
        )
        .unwrap();
        let projection = project_health_projection(
            evidence,
            Some(ProgressTrend {
                start_progress: bp(4_000),
                end_progress: bp(4_000),
                delta_bp: 0,
                span_minutes: 60,
                basis_points_per_day: 0,
                direction: ProgressTrendDirection::Flat,
            }),
            Some(RecentActivityEvidence {
                activity_count: 0,
                last_activity_at_ms: Some(0),
                last_activity_age_minutes: Some(20_000),
                lookback_minutes: 10_080,
            }),
            HealthPolicy::default(),
        );
        assert_eq!(projection.state, ProjectHealthState::AtRisk);
        assert!(
            projection
                .reasons
                .contains(&ProjectHealthReason::BehindPlan)
        );
        assert!(
            projection
                .reasons
                .contains(&ProjectHealthReason::ProjectedLate)
        );
        assert!(
            projection
                .reasons
                .contains(&ProjectHealthReason::OpenBlockers)
        );
        assert!(
            projection
                .reasons
                .contains(&ProjectHealthReason::ProgressStalled)
        );
        assert!(!projection.reasons.is_empty());
    }
}

/// Mode-specific evidence calculators. These deliberately do not infer a project-to-habit
/// association: callers must supply evidence from explicitly bound sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitConsistencyEvidence {
    pub fulfilled_periods: u32,
    pub evaluated_periods: u32,
    pub consistency: Option<ProgressBasisPoints>,
}

/// Compute the share of elapsed, evaluated habit periods fulfilled. An empty window is
/// unknown rather than 0%: it must not silently penalize a newly linked habit.
pub fn habit_consistency_evidence(
    periods: &[orbit_domain::HabitPeriodResult],
) -> Result<HabitConsistencyEvidence, ProgressError> {
    let evaluated_periods =
        u32::try_from(periods.len()).map_err(|_| ProgressError::ArithmeticOverflow)?;
    let fulfilled_periods = u32::try_from(periods.iter().filter(|p| p.fulfilled).count())
        .map_err(|_| ProgressError::ArithmeticOverflow)?;
    let consistency = if evaluated_periods == 0 {
        None
    } else {
        let bp = (u64::from(fulfilled_periods) * 10_000 + u64::from(evaluated_periods) / 2)
            / u64::from(evaluated_periods);
        Some(ProgressBasisPoints::new(bp as u16).map_err(|_| ProgressError::ArithmeticOverflow)?)
    };
    Ok(HabitConsistencyEvidence {
        fulfilled_periods,
        evaluated_periods,
        consistency,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricTargetDirection {
    AtLeast,
    AtMost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricTargetEvidence {
    pub starting_value: i64,
    pub current_value: i64,
    pub target_value: i64,
    pub reached: bool,
    pub progress: ProgressBasisPoints,
}

/// A bounded target measure on an explicit baseline-to-target interval. Movement away
/// from the target clamps to 0%, overshoot to 100%; direction and target must agree.
/// A starting value equal to the target is rejected since no interval exists.
pub fn metric_target_evidence(
    starting_value: i64,
    current_value: i64,
    target_value: i64,
    direction: MetricTargetDirection,
) -> Result<MetricTargetEvidence, ProgressError> {
    let (distance, travelled, reached) = match direction {
        MetricTargetDirection::AtLeast if target_value > starting_value => (
            i128::from(target_value) - i128::from(starting_value),
            i128::from(current_value) - i128::from(starting_value),
            current_value >= target_value,
        ),
        MetricTargetDirection::AtMost if target_value < starting_value => (
            i128::from(starting_value) - i128::from(target_value),
            i128::from(starting_value) - i128::from(current_value),
            current_value <= target_value,
        ),
        _ => return Err(ProgressError::InvalidObservationSeries),
    };
    let clamped = travelled.clamp(0, distance);
    let bp = (clamped * 10_000 + distance / 2) / distance;
    let progress =
        ProgressBasisPoints::new(bp as u16).map_err(|_| ProgressError::ArithmeticOverflow)?;
    Ok(MetricTargetEvidence {
        starting_value,
        current_value,
        target_value,
        reached,
        progress,
    })
}

#[cfg(test)]
mod mode_evidence_tests {
    use super::*;
    use orbit_domain::HabitPeriod;

    #[test]
    fn habit_evidence_is_unknown_without_evaluated_periods() {
        let value = habit_consistency_evidence(&[]).unwrap();
        assert_eq!(value.consistency, None);
        assert_eq!(value.evaluated_periods, 0);
    }

    #[test]
    fn habit_evidence_counts_fulfilled_periods_not_raw_occurrences() {
        let periods = [true, false, true]
            .into_iter()
            .enumerate()
            .map(|(i, fulfilled)| orbit_domain::HabitPeriodResult {
                period: HabitPeriod::new(i as i64, i as i64 + 1).unwrap(),
                completions: if fulfilled { 2 } else { 0 },
                fulfilled,
                duration_minutes: 0,
            })
            .collect::<Vec<_>>();
        let result = habit_consistency_evidence(&periods).unwrap();
        assert_eq!(result.fulfilled_periods, 2);
        assert_eq!(result.consistency.unwrap().value(), 6667);
    }

    #[test]
    fn metric_targets_handle_both_directions_and_clamping() {
        use MetricTargetDirection::{AtLeast, AtMost};
        assert_eq!(
            metric_target_evidence(20, 60, 100, AtLeast)
                .unwrap()
                .progress
                .value(),
            5000
        );
        assert_eq!(
            metric_target_evidence(100, 60, 20, AtMost)
                .unwrap()
                .progress
                .value(),
            5000
        );
        assert_eq!(
            metric_target_evidence(20, 10, 100, AtLeast)
                .unwrap()
                .progress
                .value(),
            0
        );
        let exceeded = metric_target_evidence(100, 0, 20, AtMost).unwrap();
        assert_eq!(exceeded.progress.value(), 10_000);
        assert!(exceeded.reached);
        assert!(metric_target_evidence(20, 40, 10, AtLeast).is_err());
        assert!(metric_target_evidence(20, 40, 20, AtLeast).is_err());
    }

    #[test]
    fn metric_target_handles_full_i64_range_without_overflow() {
        let result =
            metric_target_evidence(i64::MIN, 0, i64::MAX, MetricTargetDirection::AtLeast).unwrap();
        assert_eq!(result.progress.value(), 5000);
    }
}
