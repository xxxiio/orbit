//! Attachment use cases shared by the TUI, CLI, MCP and server adapters.
//!
//! Attachment content is stored as an immutable, content-addressed object and
//! metadata is persisted through the attachment repository. Import is ordered
//! so a crash can only leave an unreferenced object (recoverable by garbage
//! collection), never a metadata row whose content is missing.
use orbit_domain::{
    ActivityId, ActivityKind, ActivityOrigin, ActivityRecord, ActivitySubject, Attachment,
    AttachmentId, AttachmentOwner, AttachmentOwnerKind, KnowledgeItemId, ProjectId, TaskId,
    content_type_for_filename, validate_attachment_content_type, validate_attachment_filename,
};

use crate::{
    ActivityRepository, ApplicationError, AttachmentContentStore, AttachmentObjectRecord,
    AttachmentRepository, AttachmentStoreError, Clock, UnitOfWork,
};

impl From<AttachmentStoreError> for ApplicationError {
    fn from(error: AttachmentStoreError) -> Self {
        match error {
            AttachmentStoreError::TooLarge { max_bytes } => {
                ApplicationError::AttachmentTooLarge { max_bytes }
            }
            AttachmentStoreError::Unavailable => {
                ApplicationError::AttachmentStore("storage unavailable".to_owned())
            }
            AttachmentStoreError::Io(detail) => ApplicationError::AttachmentStore(detail),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportAttachmentCommand {
    pub id: AttachmentId,
    pub owner: AttachmentOwner,
    pub filename: String,
    pub content_type: Option<String>,
    /// Raw content bytes read by the caller from an explicitly authorised path.
    pub content: Vec<u8>,
    pub description: Option<String>,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
}

/// Store content and metadata for a new attachment association.
pub fn import_attachment_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    content_store: &mut impl AttachmentContentStore,
    clock: &impl Clock,
    command: ImportAttachmentCommand,
) -> Result<Attachment, ApplicationError> {
    let ImportAttachmentCommand {
        id,
        owner,
        filename,
        content_type,
        content,
        description,
        activity_id,
        origin,
    } = command;

    // Validate caller-controlled display metadata before touching the object
    // store so invalid names never create files.
    validate_attachment_filename(&filename)?;
    let content_type = match content_type {
        Some(value) => validate_attachment_content_type(&value)?,
        None => content_type_for_filename(&filename).to_owned(),
    };
    let max_bytes = content_store.max_bytes();
    if content.len() as u64 > max_bytes {
        return Err(ApplicationError::AttachmentTooLarge { max_bytes });
    }
    if unit_of_work_exists(unit_of_work, &id)? {
        return Err(ApplicationError::AlreadyExists {
            entity: "attachment",
            id: id.to_string(),
        });
    }

    let stored = content_store.store(&content)?;
    let attachment = Attachment::new(
        id,
        owner,
        filename,
        content_type,
        stored.size_bytes,
        stored.checksum,
        stored.object,
        stored.created_at_ms,
        description,
    )?;

    unit_of_work.transact(|repositories| {
        AttachmentRepository::record_attachment_object(
            repositories,
            &AttachmentObjectRecord {
                object_hash: attachment.object().to_owned(),
                size_bytes: attachment.size_bytes(),
                created_at_ms: attachment.created_at_ms(),
            },
        )?;
        AttachmentRepository::save_attachment(repositories, &attachment)?;
        append_attachment_activity(
            repositories,
            clock,
            &attachment,
            activity_id,
            ActivityKind::AttachmentAdded,
            origin,
            format!(
                "Attachment {} added to {} {}",
                attachment.filename(),
                attachment.owner().kind().as_str(),
                attachment.owner().id()
            ),
        )?;
        Ok(attachment)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveAttachmentCommand {
    pub id: AttachmentId,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentRemoval {
    pub attachment: Attachment,
    /// True when the underlying content object was no longer referenced and was
    /// removed from the managed store.
    pub content_removed: bool,
}

/// Delete one association, then delete its content object only when no other
/// attachment references it. Store failures leave an orphan for a later purge
/// rather than losing an association that another entity still owns.
pub fn remove_attachment_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    content_store: &mut impl AttachmentContentStore,
    clock: &impl Clock,
    command: RemoveAttachmentCommand,
) -> Result<AttachmentRemoval, ApplicationError> {
    unit_of_work
        .transact(|repositories| {
            let attachment = AttachmentRepository::get_attachment(repositories, &command.id)?
                .ok_or_else(|| ApplicationError::NotFound {
                    entity: "attachment",
                    id: command.id.to_string(),
                })?;
            let object = attachment.object().to_owned();
            if !AttachmentRepository::delete_attachment(repositories, &command.id)? {
                return Err(ApplicationError::NotFound {
                    entity: "attachment",
                    id: command.id.to_string(),
                });
            }
            append_attachment_activity(
                repositories,
                clock,
                &attachment,
                command.activity_id,
                ActivityKind::AttachmentRemoved,
                command.origin,
                format!(
                    "Attachment {} removed from {} {}",
                    attachment.filename(),
                    attachment.owner().kind().as_str(),
                    attachment.owner().id()
                ),
            )?;
            let unreferenced =
                !AttachmentRepository::attachment_object_referenced(repositories, &object)?;
            if unreferenced {
                AttachmentRepository::delete_attachment_object(repositories, &object)?;
            }
            Ok((attachment, unreferenced))
        })
        .map(|(attachment, unreferenced)| {
            let content_removed = if unreferenced {
                content_store.remove(attachment.object()).unwrap_or(false)
            } else {
                false
            };
            AttachmentRemoval {
                attachment,
                content_removed,
            }
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociateAttachmentCommand {
    pub source_id: AttachmentId,
    pub new_id: AttachmentId,
    pub owner: AttachmentOwner,
    pub activity_id: ActivityId,
    pub origin: ActivityOrigin,
}

/// Attach existing deduplicated content to another owner without duplicating
/// the underlying object or losing either association.
pub fn associate_existing_attachment_with_activity(
    unit_of_work: &mut impl UnitOfWork,
    content_store: &impl AttachmentContentStore,
    clock: &impl Clock,
    command: AssociateAttachmentCommand,
) -> Result<Attachment, ApplicationError> {
    let source = AttachmentRepository::get_attachment(unit_of_work, &command.source_id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "attachment",
            id: command.source_id.to_string(),
        })?;
    if !content_store.contains(source.object())? {
        return Err(ApplicationError::AttachmentContentUnavailable {
            object: source.object().to_owned(),
        });
    }
    if AttachmentRepository::get_attachment(unit_of_work, &command.new_id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "attachment",
            id: command.new_id.to_string(),
        });
    }
    let created_at_ms = clock.now_ms();
    let attachment = Attachment::new(
        command.new_id,
        command.owner,
        source.filename(),
        source.content_type(),
        source.size_bytes(),
        source.checksum(),
        source.object(),
        created_at_ms,
        source.description().map(str::to_owned),
    )?;
    unit_of_work.transact(|repositories| {
        AttachmentRepository::record_attachment_object(
            repositories,
            &AttachmentObjectRecord {
                object_hash: attachment.object().to_owned(),
                size_bytes: attachment.size_bytes(),
                created_at_ms,
            },
        )?;
        AttachmentRepository::save_attachment(repositories, &attachment)?;
        append_attachment_activity(
            repositories,
            clock,
            &attachment,
            command.activity_id,
            ActivityKind::AttachmentAdded,
            command.origin,
            format!(
                "Attachment {} linked to {} {}",
                attachment.filename(),
                attachment.owner().kind().as_str(),
                attachment.owner().id()
            ),
        )?;
        Ok(attachment)
    })
}

/// Read attachment content after integrity verification. The caller is
/// responsible for authorising entity access before calling this function.
pub fn read_attachment_content(
    repository: &impl AttachmentRepository,
    content_store: &impl AttachmentContentStore,
    id: &AttachmentId,
) -> Result<(Attachment, Vec<u8>), ApplicationError> {
    let attachment = repository
        .get_attachment(id)?
        .ok_or_else(|| ApplicationError::NotFound {
            entity: "attachment",
            id: id.to_string(),
        })?;
    let content = content_store.read(attachment.object())?;
    Ok((attachment, content))
}

/// Remove every content object no attachment references any more. Returns the
/// number of cleared objects.
pub fn purge_unreferenced_attachment_content(
    unit_of_work: &mut impl UnitOfWork,
    content_store: &mut impl AttachmentContentStore,
) -> Result<usize, ApplicationError> {
    let objects = unit_of_work.unreferenced_attachment_objects()?;
    let mut removed = 0;
    for object in objects {
        let deleted = content_store.remove(&object.object_hash).unwrap_or(false);
        if deleted {
            unit_of_work.delete_attachment_object(&object.object_hash)?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn unit_of_work_exists(
    repository: &impl AttachmentRepository,
    id: &AttachmentId,
) -> Result<bool, ApplicationError> {
    Ok(repository.get_attachment(id)?.is_some())
}

fn append_attachment_activity(
    repositories: &mut impl UnitOfWork,
    clock: &impl Clock,
    attachment: &Attachment,
    activity_id: ActivityId,
    kind: ActivityKind,
    origin: ActivityOrigin,
    summary: String,
) -> Result<(), ApplicationError> {
    let Some(subject) = activity_subject(attachment.owner()) else {
        // Work items and agent jobs have no activity subject today; attachment
        // metadata is still recorded on the attachment itself.
        return Ok(());
    };
    if ActivityRepository::get(repositories, &activity_id)?.is_some() {
        return Err(ApplicationError::AlreadyExists {
            entity: "activity",
            id: activity_id.to_string(),
        });
    }
    let activity =
        ActivityRecord::new(activity_id, subject, kind, origin, clock.now_ms(), summary)?;
    ActivityRepository::append(repositories, activity)?;
    Ok(())
}

fn activity_subject(owner: &AttachmentOwner) -> Option<ActivitySubject> {
    match owner.kind() {
        AttachmentOwnerKind::Task => TaskId::new(owner.id()).ok().map(ActivitySubject::Task),
        AttachmentOwnerKind::Project => ProjectId::new(owner.id())
            .ok()
            .map(ActivitySubject::Project),
        AttachmentOwnerKind::Knowledge => KnowledgeItemId::new(owner.id())
            .ok()
            .map(ActivitySubject::Knowledge),
        AttachmentOwnerKind::WorkItem | AttachmentOwnerKind::AgentJob => None,
    }
}
