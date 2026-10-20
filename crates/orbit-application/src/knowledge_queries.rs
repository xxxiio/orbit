//! Navigation-oriented knowledge query projections.

/// A term occurrence resolved from physical knowledge content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeTermOccurrence {
    pub item_id: orbit_domain::KnowledgeItemId,
    pub field: &'static str,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}
