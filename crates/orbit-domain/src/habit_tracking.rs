use crate::{DomainError, HabitCadence, HabitId, HabitOccurrenceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitOccurrence {
    id: HabitOccurrenceId,
    habit_id: HabitId,
    occurred_at_ms: i64,
    duration_minutes: Option<u32>,
}

impl HabitOccurrence {
    pub fn new(
        id: HabitOccurrenceId,
        habit_id: HabitId,
        occurred_at_ms: i64,
        duration_minutes: Option<u32>,
    ) -> Result<Self, DomainError> {
        if matches!(duration_minutes, Some(0)) {
            return Err(DomainError::InvalidHabitOccurrenceDuration);
        }
        Ok(Self {
            id,
            habit_id,
            occurred_at_ms,
            duration_minutes,
        })
    }

    pub fn id(&self) -> &HabitOccurrenceId {
        &self.id
    }
    pub fn habit_id(&self) -> &HabitId {
        &self.habit_id
    }
    pub fn occurred_at_ms(&self) -> i64 {
        self.occurred_at_ms
    }
    pub fn duration_minutes(&self) -> Option<u32> {
        self.duration_minutes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitPeriod {
    pub start_ms: i64,
    pub end_ms: i64,
}

impl HabitPeriod {
    pub fn new(start_ms: i64, end_ms: i64) -> Result<Self, DomainError> {
        if start_ms >= end_ms {
            return Err(DomainError::InvalidHabitPeriod);
        }
        Ok(Self { start_ms, end_ms })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitPeriodResult {
    pub period: HabitPeriod,
    pub completions: u32,
    pub fulfilled: bool,
    pub duration_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitStreakSummary {
    pub periods: Vec<HabitPeriodResult>,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub total_completions: u32,
    pub total_duration_minutes: u32,
}

pub fn evaluate_habit_periods(
    cadence: HabitCadence,
    periods: &[HabitPeriod],
    occurrences: &[HabitOccurrence],
) -> Result<HabitStreakSummary, DomainError> {
    for pair in periods.windows(2) {
        if pair[0].end_ms > pair[1].start_ms {
            return Err(DomainError::OverlappingHabitPeriods);
        }
    }

    let target = match cadence {
        HabitCadence::Daily => 1_u32,
        HabitCadence::Weekly { target_per_week } => u32::from(target_per_week),
    };

    let mut results = Vec::with_capacity(periods.len());
    for period in periods {
        let matching: Vec<_> = occurrences
            .iter()
            .filter(|occurrence| {
                occurrence.occurred_at_ms() >= period.start_ms
                    && occurrence.occurred_at_ms() < period.end_ms
            })
            .collect();
        let completions = matching.len() as u32;
        let duration_minutes = matching.iter().filter_map(|o| o.duration_minutes()).sum();
        results.push(HabitPeriodResult {
            period: *period,
            completions,
            fulfilled: completions >= target,
            duration_minutes,
        });
    }

    let mut longest = 0_u32;
    let mut run = 0_u32;
    for result in &results {
        if result.fulfilled {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    let current = results
        .iter()
        .rev()
        .take_while(|result| result.fulfilled)
        .count() as u32;

    Ok(HabitStreakSummary {
        total_completions: occurrences.len() as u32,
        total_duration_minutes: occurrences
            .iter()
            .filter_map(HabitOccurrence::duration_minutes)
            .sum(),
        periods: results,
        current_streak: current,
        longest_streak: longest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occurrence(id: &str, at: i64, duration: Option<u32>) -> HabitOccurrence {
        HabitOccurrence::new(
            HabitOccurrenceId::new(id).unwrap(),
            HabitId::new("habit-1").unwrap(),
            at,
            duration,
        )
        .unwrap()
    }

    #[test]
    fn daily_streaks_are_computed_from_caller_supplied_periods() {
        let periods = vec![
            HabitPeriod::new(0, 100).unwrap(),
            HabitPeriod::new(100, 200).unwrap(),
            HabitPeriod::new(200, 300).unwrap(),
        ];
        let occurrences = vec![
            occurrence("o1", 10, Some(20)),
            occurrence("o2", 120, Some(30)),
            occurrence("o3", 250, None),
        ];
        let summary = evaluate_habit_periods(HabitCadence::Daily, &periods, &occurrences).unwrap();
        assert_eq!(summary.current_streak, 3);
        assert_eq!(summary.longest_streak, 3);
        assert_eq!(summary.total_duration_minutes, 50);
    }

    #[test]
    fn weekly_target_must_be_met_within_each_period() {
        let periods = vec![
            HabitPeriod::new(0, 100).unwrap(),
            HabitPeriod::new(100, 200).unwrap(),
        ];
        let occurrences = vec![
            occurrence("o1", 10, None),
            occurrence("o2", 20, None),
            occurrence("o3", 120, None),
        ];
        let summary = evaluate_habit_periods(
            HabitCadence::Weekly { target_per_week: 2 },
            &periods,
            &occurrences,
        )
        .unwrap();
        assert!(summary.periods[0].fulfilled);
        assert!(!summary.periods[1].fulfilled);
        assert_eq!(summary.current_streak, 0);
        assert_eq!(summary.longest_streak, 1);
    }
}
