//! Opt-in trusted capability package for habit recurrence and streak evaluation.
//!
//! Orbit core keeps only habit value types and their validation. Everything that
//! turns caller-supplied periods plus recorded occurrences into a streak summary
//! lives here and is reachable only after the trusted composition installs the
//! `orbit.habit.recurrence` capability.
//!
//! The package deliberately depends only on `orbit-domain` (value types and
//! validation errors), `serde` and `serde_json`. It contains no storage, UI,
//! runtime or transport code. The trusted executor lives in `orbit-runtime`.

use orbit_domain::{
    DomainError, HabitCadence, HabitId, HabitOccurrence, HabitOccurrenceId, HabitPeriod,
};
use serde::{Deserialize, Serialize, Serializer, de::Error as _};
use serde_json::{Value, json};
use std::fmt;

/// Stable, versioned capability identifier.
pub const CAPABILITY_ID: &str = "orbit.habit.recurrence";
/// The single action exposed by the capability.
pub const ACTION_EVALUATE_PERIODS: &str = "evaluate_periods";
/// Contract version carried in the input schema.
pub const CONTRACT_VERSION: u32 = 1;

/// Per-period recurrence result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitPeriodResult {
    pub period: HabitPeriod,
    pub completions: u32,
    pub fulfilled: bool,
    pub duration_minutes: u32,
}

/// The recurring summary returned by [`evaluate_habit_periods`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitStreakSummary {
    pub periods: Vec<HabitPeriodResult>,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub total_completions: u32,
    pub total_duration_minutes: u32,
}

/// Period results serialize as flat records so the JSON contract is stable and
/// independent of the domain value type's internal shape:
/// `{start_ms, end_ms, completions, fulfilled, duration_minutes}`.
impl Serialize for HabitPeriodResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire {
            start_ms: i64,
            end_ms: i64,
            completions: u32,
            fulfilled: bool,
            duration_minutes: u32,
        }
        Wire {
            start_ms: self.period.start_ms,
            end_ms: self.period.end_ms,
            completions: self.completions,
            fulfilled: self.fulfilled,
            duration_minutes: self.duration_minutes,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for HabitPeriodResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            start_ms: i64,
            end_ms: i64,
            completions: u32,
            fulfilled: bool,
            duration_minutes: u32,
        }
        let wire = Wire::deserialize(deserializer)?;
        let period = HabitPeriod::new(wire.start_ms, wire.end_ms).map_err(D::Error::custom)?;
        Ok(Self {
            period,
            completions: wire.completions,
            fulfilled: wire.fulfilled,
            duration_minutes: wire.duration_minutes,
        })
    }
}

/// The recurrence/streak algorithm. Periods must be supplied by the caller and
/// must be ordered and non-overlapping; no timezone or calendar expansion is
/// assumed. `current_streak` counts fulfilled periods from the most recent end,
/// while `longest_streak` is the longest consecutive fulfilled run.
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

/// Whether the recurrence capability may be used in this process context.
///
/// The trusted host resolves this from durable installation (and, where a
/// project is in scope, from that project's grant). Passing a status into an
/// application use case is what makes recurrence opt-in: without
/// [`HabitRecurrenceStatus::Available`] no streak is computed anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HabitRecurrenceStatus {
    Available,
    Unavailable {
        capability_id: String,
        reason: String,
    },
}

impl HabitRecurrenceStatus {
    pub fn available() -> Self {
        Self::Available
    }

    /// The capability is not durably installed.
    pub fn not_installed() -> Self {
        Self::Unavailable {
            capability_id: CAPABILITY_ID.to_owned(),
            reason: format!("{CAPABILITY_ID} is not installed"),
        }
    }

    /// The capability is installed but not authorized for the requested project.
    pub fn not_authorized(project_id: &str) -> Self {
        Self::Unavailable {
            capability_id: CAPABILITY_ID.to_owned(),
            reason: format!("{CAPABILITY_ID} is not authorized for project {project_id}"),
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            capability_id: CAPABILITY_ID.to_owned(),
            reason: reason.into(),
        }
    }

    pub fn from_installed(installed: bool) -> Self {
        if installed {
            Self::Available
        } else {
            Self::not_installed()
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn unavailable_parts(&self) -> Option<(&str, &str)> {
        match self {
            Self::Available => None,
            Self::Unavailable {
                capability_id,
                reason,
            } => Some((capability_id, reason)),
        }
    }
}

/// Typed failures from the capability contract: invalid JSON input or a domain
/// validation failure (overlapping periods, invalid cadence/duration).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HabitRecurrenceError {
    InvalidInput(String),
    Domain(DomainError),
}

impl fmt::Display for HabitRecurrenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid recurrence input: {message}"),
            Self::Domain(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for HabitRecurrenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidInput(_) => None,
            Self::Domain(error) => Some(error),
        }
    }
}

impl From<DomainError> for HabitRecurrenceError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}

/// Cadence as accepted by the wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HabitCadenceInput {
    Daily,
    Weekly { target_per_week: u8 },
}

impl TryFrom<HabitCadenceInput> for HabitCadence {
    type Error = DomainError;

    fn try_from(value: HabitCadenceInput) -> Result<Self, Self::Error> {
        match value {
            HabitCadenceInput::Daily => HabitCadence::Daily.validate(),
            HabitCadenceInput::Weekly { target_per_week } => {
                HabitCadence::Weekly { target_per_week }.validate()
            }
        }
    }
}

impl From<HabitCadence> for HabitCadenceInput {
    fn from(value: HabitCadence) -> Self {
        match value {
            HabitCadence::Daily => Self::Daily,
            HabitCadence::Weekly { target_per_week } => Self::Weekly { target_per_week },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitPeriodInput {
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitOccurrenceInput {
    pub id: String,
    pub habit_id: String,
    pub occurred_at_ms: i64,
    #[serde(default)]
    pub duration_minutes: Option<u32>,
}

/// Input document for `orbit.habit.recurrence/evaluate_periods`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatePeriodsInput {
    pub cadence: HabitCadenceInput,
    pub periods: Vec<HabitPeriodInput>,
    pub occurrences: Vec<HabitOccurrenceInput>,
}

impl EvaluatePeriodsInput {
    fn domain_parts(
        &self,
    ) -> Result<(HabitCadence, Vec<HabitPeriod>, Vec<HabitOccurrence>), HabitRecurrenceError> {
        let cadence = HabitCadence::try_from(self.cadence)?;
        let periods = self
            .periods
            .iter()
            .map(|period| HabitPeriod::new(period.start_ms, period.end_ms))
            .collect::<Result<Vec<_>, _>>()?;
        let occurrences = self
            .occurrences
            .iter()
            .map(|occurrence| {
                let id = HabitOccurrenceId::new(occurrence.id.clone())?;
                let habit_id = HabitId::new(occurrence.habit_id.clone())?;
                HabitOccurrence::new(
                    id,
                    habit_id,
                    occurrence.occurred_at_ms,
                    occurrence.duration_minutes,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((cadence, periods, occurrences))
    }
}

/// Evaluate the wire input document.
pub fn evaluate_periods_input(
    input: &EvaluatePeriodsInput,
) -> Result<HabitStreakSummary, HabitRecurrenceError> {
    let (cadence, periods, occurrences) = input.domain_parts()?;
    Ok(evaluate_habit_periods(cadence, &periods, &occurrences)?)
}

/// Parse a JSON input document and return the JSON result document.
///
/// Result shape:
/// `{"periods":[{"start_ms","end_ms","completions","fulfilled",
/// "duration_minutes"}],"current_streak","longest_streak",
/// "total_completions","total_duration_minutes"}`.
pub fn evaluate_periods_json(input: &Value) -> Result<Value, HabitRecurrenceError> {
    let input: EvaluatePeriodsInput = serde_json::from_value(input.clone())
        .map_err(|error| HabitRecurrenceError::InvalidInput(error.to_string()))?;
    let summary = evaluate_periods_input(&input)?;
    serde_json::to_value(summary)
        .map_err(|error| HabitRecurrenceError::InvalidInput(error.to_string()))
}

/// JSON-schema subset advertised for the capability action. The host validator
/// only enforces top-level string/integer/boolean types; the nested shape is the
/// documented contract and is validated again when the executor parses it.
pub fn input_schema() -> Value {
    json!({
        "type": "object",
        "version": CONTRACT_VERSION,
        "properties": {
            "cadence": {
                "type": "object",
                "description": "Daily or weekly recurrence cadence",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {"kind": {"const": "daily"}},
                        "required": ["kind"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "kind": {"const": "weekly"},
                            "target_per_week": {
                                "type": "integer",
                                "minimum": 1,
                                "maximum": 7
                            }
                        },
                        "required": ["kind", "target_per_week"]
                    }
                ]
            },
            "periods": {
                "type": "array",
                "description": "Ordered, non-overlapping half-open periods",
                "items": {
                    "type": "object",
                    "properties": {
                        "start_ms": {"type": "integer"},
                        "end_ms": {"type": "integer"}
                    },
                    "required": ["start_ms", "end_ms"]
                }
            },
            "occurrences": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string", "minLength": 1},
                        "habit_id": {"type": "string", "minLength": 1},
                        "occurred_at_ms": {"type": "integer"},
                        "duration_minutes": {"type": "integer", "minimum": 1}
                    },
                    "required": ["id", "habit_id", "occurred_at_ms"]
                }
            }
        },
        "required": ["cadence", "periods", "occurrences"]
    })
}

/// Documented JSON result schema.
pub fn result_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "periods": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "start_ms": {"type": "integer"},
                        "end_ms": {"type": "integer"},
                        "completions": {"type": "integer"},
                        "fulfilled": {"type": "boolean"},
                        "duration_minutes": {"type": "integer"}
                    },
                    "required": [
                        "start_ms",
                        "end_ms",
                        "completions",
                        "fulfilled",
                        "duration_minutes"
                    ]
                }
            },
            "current_streak": {"type": "integer"},
            "longest_streak": {"type": "integer"},
            "total_completions": {"type": "integer"},
            "total_duration_minutes": {"type": "integer"}
        },
        "required": [
            "periods",
            "current_streak",
            "longest_streak",
            "total_completions",
            "total_duration_minutes"
        ]
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

    #[test]
    fn overlapping_periods_are_rejected() {
        let periods = vec![
            HabitPeriod::new(0, 100).unwrap(),
            HabitPeriod::new(90, 200).unwrap(),
        ];
        assert_eq!(
            evaluate_habit_periods(HabitCadence::Daily, &periods, &[]),
            Err(DomainError::OverlappingHabitPeriods)
        );
    }

    #[test]
    fn json_contract_round_trips_summary_and_rejects_invalid_input() {
        let input = json!({
            "cadence": {"kind": "daily"},
            "periods": [
                {"start_ms": 0, "end_ms": 100},
                {"start_ms": 100, "end_ms": 200}
            ],
            "occurrences": [
                {"id": "o1", "habit_id": "habit-1", "occurred_at_ms": 10, "duration_minutes": 20},
                {"id": "o2", "habit_id": "habit-1", "occurred_at_ms": 120}
            ]
        });
        let result = evaluate_periods_json(&input).unwrap();
        assert_eq!(result["current_streak"], 2);
        assert_eq!(result["longest_streak"], 2);
        assert_eq!(result["total_completions"], 2);
        assert_eq!(result["total_duration_minutes"], 20);
        assert_eq!(result["periods"][0]["start_ms"], 0);
        assert_eq!(result["periods"][0]["end_ms"], 100);
        assert_eq!(result["periods"][0]["completions"], 1);
        assert_eq!(result["periods"][0]["fulfilled"], true);

        let parsed: HabitStreakSummary = serde_json::from_value(result.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), result);

        let overlapping = json!({
            "cadence": {"kind": "daily"},
            "periods": [
                {"start_ms": 0, "end_ms": 100},
                {"start_ms": 90, "end_ms": 200}
            ],
            "occurrences": []
        });
        assert!(matches!(
            evaluate_periods_json(&overlapping),
            Err(HabitRecurrenceError::Domain(
                DomainError::OverlappingHabitPeriods
            ))
        ));

        let invalid_cadence = json!({
            "cadence": {"kind": "weekly", "target_per_week": 0},
            "periods": [],
            "occurrences": []
        });
        assert!(matches!(
            evaluate_periods_json(&invalid_cadence),
            Err(HabitRecurrenceError::Domain(
                DomainError::InvalidHabitCadence
            ))
        ));
    }

    #[test]
    fn contract_is_versioned_and_documents_the_expected_shape() {
        let schema = input_schema();
        assert_eq!(schema["version"], CONTRACT_VERSION);
        assert_eq!(schema["type"], "object");
        assert_eq!(
            schema["required"],
            json!(["cadence", "periods", "occurrences"])
        );
        assert_eq!(schema["properties"]["cadence"]["type"], "object");
        assert_eq!(schema["properties"]["periods"]["type"], "array");
        assert_eq!(schema["properties"]["occurrences"]["type"], "array");
        let result = result_schema();
        for field in [
            "periods",
            "current_streak",
            "longest_streak",
            "total_completions",
            "total_duration_minutes",
        ] {
            assert!(result["properties"].get(field).is_some(), "missing {field}");
        }
    }
}
