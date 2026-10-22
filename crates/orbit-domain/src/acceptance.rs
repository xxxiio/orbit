use crate::{AcceptanceCriterionId, DomainError};

/// Maximum number of ordered acceptance criteria on one task.
pub const MAX_ACCEPTANCE_CRITERIA: usize = 100;
/// Maximum text length of one acceptance criterion.
pub const MAX_ACCEPTANCE_CRITERION_CHARS: usize = 2_000;

/// One ordered, independently checkable task acceptance criterion.
///
/// Completion is recorded as an explicit timestamp instead of overloading the
/// task status: checking every criterion never completes the task implicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceCriterion {
    id: AcceptanceCriterionId,
    text: String,
    completed_at_ms: Option<i64>,
}

impl AcceptanceCriterion {
    pub fn new(id: AcceptanceCriterionId, text: impl Into<String>) -> Result<Self, DomainError> {
        let text = validate_text(text.into())?;
        Ok(Self {
            id,
            text,
            completed_at_ms: None,
        })
    }

    /// Rehydrate a criterion from durable state, preserving completion.
    pub fn restored(
        id: AcceptanceCriterionId,
        text: impl Into<String>,
        completed_at_ms: Option<i64>,
    ) -> Result<Self, DomainError> {
        let text = validate_text(text.into())?;
        if matches!(completed_at_ms, Some(value) if value < 0) {
            return Err(DomainError::InvalidAcceptanceCriterionOrder);
        }
        Ok(Self {
            id,
            text,
            completed_at_ms,
        })
    }

    pub fn id(&self) -> &AcceptanceCriterionId {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn completed_at_ms(&self) -> Option<i64> {
        self.completed_at_ms
    }

    pub fn is_completed(&self) -> bool {
        self.completed_at_ms.is_some()
    }

    pub fn set_text(&mut self, text: impl Into<String>) -> Result<(), DomainError> {
        self.text = validate_text(text.into())?;
        Ok(())
    }

    /// Sets or clears completion. Callers provide the timestamp so the domain
    /// stays clock-free.
    pub fn set_completed(&mut self, completed_at_ms: Option<i64>) {
        self.completed_at_ms = completed_at_ms;
    }
}

fn validate_text(text: String) -> Result<String, DomainError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyAcceptanceCriterionText);
    }
    if trimmed.chars().count() > MAX_ACCEPTANCE_CRITERION_CHARS {
        return Err(DomainError::AcceptanceCriterionTextTooLong);
    }
    if trimmed.contains('\0') {
        return Err(DomainError::EmptyAcceptanceCriterionText);
    }
    Ok(trimmed.to_owned())
}

/// Ordered collection of acceptance criteria.
///
/// Order is the vector position; identifiers stay stable across reordering so
/// external clients can address one criterion reliably.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AcceptanceCriteria {
    items: Vec<AcceptanceCriterion>,
}

impl AcceptanceCriteria {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a validated collection from persisted rows. Duplicate identifiers
    /// are rejected rather than silently collapsing distinct criteria.
    pub fn from_ordered(
        items: impl IntoIterator<Item = AcceptanceCriterion>,
    ) -> Result<Self, DomainError> {
        let items: Vec<AcceptanceCriterion> = items.into_iter().collect();
        if items.len() > MAX_ACCEPTANCE_CRITERIA {
            return Err(DomainError::TooManyAcceptanceCriteria);
        }
        let mut seen = std::collections::BTreeSet::new();
        for item in &items {
            if !seen.insert(item.id().as_str().to_owned()) {
                return Err(DomainError::DuplicateAcceptanceCriterionId);
            }
        }
        Ok(Self { items })
    }

    pub fn items(&self) -> &[AcceptanceCriterion] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn completed_count(&self) -> usize {
        self.items.iter().filter(|item| item.is_completed()).count()
    }

    /// True when at least one criterion exists and all of them are checked.
    /// This is informational only; task completion remains an explicit action.
    pub fn all_completed(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(AcceptanceCriterion::is_completed)
    }

    pub fn get(&self, id: &AcceptanceCriterionId) -> Option<&AcceptanceCriterion> {
        self.items.iter().find(|item| item.id() == id)
    }

    pub fn add(
        &mut self,
        id: AcceptanceCriterionId,
        text: impl Into<String>,
    ) -> Result<&AcceptanceCriterion, DomainError> {
        if self.items.len() >= MAX_ACCEPTANCE_CRITERIA {
            return Err(DomainError::TooManyAcceptanceCriteria);
        }
        if self.get(&id).is_some() {
            return Err(DomainError::DuplicateAcceptanceCriterionId);
        }
        self.items.push(AcceptanceCriterion::new(id, text)?);
        Ok(self.items.last().expect("criterion was just pushed"))
    }

    pub fn set_text(
        &mut self,
        id: &AcceptanceCriterionId,
        text: impl Into<String>,
    ) -> Result<(), DomainError> {
        self.get_mut(id)?.set_text(text)
    }

    pub fn set_completed(
        &mut self,
        id: &AcceptanceCriterionId,
        completed_at_ms: Option<i64>,
    ) -> Result<(), DomainError> {
        self.get_mut(id)?.set_completed(completed_at_ms);
        Ok(())
    }

    /// Moves one criterion to `new_index`, shifting the others. The remaining
    /// order is preserved.
    pub fn reorder(
        &mut self,
        id: &AcceptanceCriterionId,
        new_index: usize,
    ) -> Result<(), DomainError> {
        if new_index >= self.items.len() {
            return Err(DomainError::InvalidAcceptanceCriterionOrder);
        }
        let current = self
            .items
            .iter()
            .position(|item| item.id() == id)
            .ok_or(DomainError::UnknownAcceptanceCriterion)?;
        if current == new_index {
            return Ok(());
        }
        let item = self.items.remove(current);
        self.items.insert(new_index, item);
        Ok(())
    }

    pub fn remove(
        &mut self,
        id: &AcceptanceCriterionId,
    ) -> Result<AcceptanceCriterion, DomainError> {
        let index = self
            .items
            .iter()
            .position(|item| item.id() == id)
            .ok_or(DomainError::UnknownAcceptanceCriterion)?;
        Ok(self.items.remove(index))
    }

    fn get_mut(
        &mut self,
        id: &AcceptanceCriterionId,
    ) -> Result<&mut AcceptanceCriterion, DomainError> {
        self.items
            .iter_mut()
            .find(|item| item.id() == id)
            .ok_or(DomainError::UnknownAcceptanceCriterion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> AcceptanceCriterionId {
        AcceptanceCriterionId::new(value).expect("valid criterion id")
    }

    #[test]
    fn criteria_are_ordered_and_reorderable_with_stable_ids() {
        let mut criteria = AcceptanceCriteria::new();
        criteria.add(id("ac-1"), "First").expect("add");
        criteria.add(id("ac-2"), "Second").expect("add");
        criteria.add(id("ac-3"), "Third").expect("add");

        criteria.reorder(&id("ac-3"), 0).expect("reorder");

        let texts: Vec<&str> = criteria.items().iter().map(|c| c.text()).collect();
        assert_eq!(texts, vec!["Third", "First", "Second"]);
        assert_eq!(criteria.items()[0].id(), &id("ac-3"));
    }

    #[test]
    fn completion_is_independent_from_task_status() {
        let mut criteria = AcceptanceCriteria::new();
        criteria.add(id("ac-1"), "One").expect("add");
        criteria.add(id("ac-2"), "Two").expect("add");
        assert!(!criteria.all_completed());

        criteria
            .set_completed(&id("ac-1"), Some(10))
            .expect("complete");
        assert!(!criteria.all_completed());
        criteria
            .set_completed(&id("ac-2"), Some(11))
            .expect("complete");
        assert!(criteria.all_completed());
        assert_eq!(criteria.completed_count(), 2);

        criteria.set_completed(&id("ac-2"), None).expect("reopen");
        assert!(!criteria.all_completed());
    }

    #[test]
    fn duplicate_ids_and_unknown_criteria_are_rejected() {
        let mut criteria = AcceptanceCriteria::new();
        criteria.add(id("ac-1"), "One").expect("add");
        assert_eq!(
            criteria.add(id("ac-1"), "Duplicate"),
            Err(DomainError::DuplicateAcceptanceCriterionId)
        );
        assert_eq!(
            criteria.remove(&id("missing")),
            Err(DomainError::UnknownAcceptanceCriterion)
        );
        assert_eq!(
            criteria.reorder(&id("ac-1"), 5),
            Err(DomainError::InvalidAcceptanceCriterionOrder)
        );
    }

    #[test]
    fn blank_text_is_rejected() {
        assert_eq!(
            AcceptanceCriterion::new(id("ac-1"), "   "),
            Err(DomainError::EmptyAcceptanceCriterionText)
        );
    }
}
