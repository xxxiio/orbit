use crate::{DomainError, KnowledgeItemId, KnowledgeResourceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeResource {
    id: KnowledgeResourceId,
    knowledge_id: KnowledgeItemId,
    display_name: String,
    media_type: Option<String>,
    size_bytes: Option<u64>,
    content_hash: Option<String>,
    created_at_ms: i64,
}

impl KnowledgeResource {
    pub fn new(
        id: KnowledgeResourceId,
        knowledge_id: KnowledgeItemId,
        display_name: impl Into<String>,
        media_type: Option<String>,
        size_bytes: Option<u64>,
        content_hash: Option<String>,
        created_at_ms: i64,
    ) -> Result<Self, DomainError> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(DomainError::EmptyKnowledgeResourceName);
        }
        if media_type
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(DomainError::EmptyKnowledgeResourceMediaType);
        }
        if content_hash
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(DomainError::EmptyKnowledgeResourceHash);
        }
        Ok(Self {
            id,
            knowledge_id,
            display_name,
            media_type,
            size_bytes,
            content_hash,
            created_at_ms,
        })
    }

    pub fn id(&self) -> &KnowledgeResourceId {
        &self.id
    }
    pub fn knowledge_id(&self) -> &KnowledgeItemId {
        &self.knowledge_id
    }
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
    pub fn size_bytes(&self) -> Option<u64> {
        self.size_bytes
    }
    pub fn content_hash(&self) -> Option<&str> {
        self.content_hash.as_deref()
    }
    pub fn created_at_ms(&self) -> i64 {
        self.created_at_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalResourceLocator {
    resource_id: KnowledgeResourceId,
    path: String,
}

impl LocalResourceLocator {
    pub fn new(
        resource_id: KnowledgeResourceId,
        path: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let path = path.into();
        if path.trim().is_empty() {
            return Err(DomainError::EmptyLocalResourcePath);
        }
        Ok(Self { resource_id, path })
    }
    pub fn resource_id(&self) -> &KnowledgeResourceId {
        &self.resource_id
    }
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_metadata_is_separate_from_device_local_path() {
        let resource = KnowledgeResource::new(
            KnowledgeResourceId::new("resource-1").unwrap(),
            KnowledgeItemId::new("knowledge-1").unwrap(),
            "design.pdf",
            Some("application/pdf".into()),
            Some(123),
            None,
            10,
        )
        .unwrap();
        let locator = LocalResourceLocator::new(resource.id().clone(), "/tmp/design.pdf").unwrap();
        assert_eq!(resource.display_name(), "design.pdf");
        assert_eq!(locator.path(), "/tmp/design.pdf");
    }
}
