//! Authority-level attachment and rich task-command coverage.
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use orbit_application::{
    AcceptanceCriteriaChange, AcceptanceCriteriaCommand, ApplicationError,
    AssociateAttachmentCommand, CreateTaskCommand, CreateTaskWithActivityCommand,
    ImportAttachmentCommand, RemoveAttachmentCommand, UpdateTaskDetailsCommand,
};
use orbit_domain::{
    AcceptanceCriterionId, ActivityId, ActivityOrigin, AttachmentId, AttachmentOwner,
    AttachmentOwnerKind, DomainError, ProjectId, TaskId, TaskPriority, TaskScope,
};

use crate::AuthoritativeHost;
use crate::AuthorityError;
use crate::attachment_store::AttachmentObjectStore;

static NEXT: AtomicU64 = AtomicU64::new(1);

fn scratch_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "orbit-attachment-authority-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn import_command(
    id: &str,
    owner: &str,
    filename: &str,
    content: &[u8],
    activity: &str,
) -> ImportAttachmentCommand {
    ImportAttachmentCommand {
        id: AttachmentId::new(id).expect("valid attachment id"),
        owner: AttachmentOwner::task(owner).expect("valid owner"),
        filename: filename.to_owned(),
        content_type: None,
        content: content.to_vec(),
        description: Some("Evidence".to_owned()),
        activity_id: ActivityId::new(activity).expect("valid activity id"),
        origin: ActivityOrigin::Human,
    }
}

fn create_task(host: &mut AuthoritativeHost) -> TaskId {
    let task_id = TaskId::new("task-1").expect("valid task id");
    host.create_task_accepted(
        "user",
        "task-create",
        0,
        CreateTaskWithActivityCommand {
            task: CreateTaskCommand {
                id: task_id.clone(),
                title: "Attachment task".to_owned(),
                scope: TaskScope::standalone(),
            },
            activity_id: ActivityId::new("task-activity").expect("valid activity id"),
            origin: ActivityOrigin::Human,
            summary: "Created task".to_owned(),
        },
    )
    .expect("create task");
    task_id
}

#[test]
fn import_list_read_remove_and_content_gc() {
    let mut host = AuthoritativeHost::open(":memory:").expect("open host");
    let root = host.attachment_store_root().to_path_buf();
    let owner = AttachmentOwner::task("task-1").expect("valid owner");
    let command = import_command("att-1", "task-1", "note.txt", b"hello", "import-activity");

    let (revision, duplicate, id) = host
        .import_attachment_accepted("user", "import-1", 0, command.clone())
        .expect("import attachment");
    assert_eq!((revision, duplicate), (1, false));
    assert_eq!(id.as_str(), "att-1");

    let listed = host.list_attachments(&owner).expect("list attachments");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].filename(), "note.txt");
    assert_eq!(listed[0].size_bytes(), 5);

    let (attachment, content) = host.attachment_content(&id).expect("read content");
    assert_eq!(content, b"hello");
    assert_eq!(attachment.checksum(), attachment.object());
    let object_path = root
        .join(&attachment.object()[..2])
        .join(attachment.object());
    assert!(object_path.is_file());

    let (duplicate_revision, duplicate, _) = host
        .import_attachment_accepted("user", "import-1", 0, command)
        .expect("duplicate import returns receipt");
    assert_eq!((duplicate_revision, duplicate), (1, true));
    assert_eq!(host.list_attachments(&owner).expect("list").len(), 1);
    assert_eq!(host.application_queries().activities().unwrap().len(), 1);

    let (revision, duplicate, content_removed) = host
        .remove_attachment_accepted(
            "user",
            "remove-1",
            1,
            RemoveAttachmentCommand {
                id: id.clone(),
                activity_id: ActivityId::new("remove-activity").expect("valid activity id"),
                origin: ActivityOrigin::Human,
            },
        )
        .expect("remove attachment");
    assert_eq!((revision, duplicate, content_removed), (2, false, true));
    assert!(!object_path.exists());
    assert!(host.list_attachments(&owner).expect("list").is_empty());
    assert!(host.attachment_content(&id).is_err());
    assert_eq!(host.purge_attachment_content().expect("purge"), 0);
    assert_eq!(host.application_queries().activities().unwrap().len(), 2);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn import_ledger_payload_and_history_are_metadata_only() {
    let mut host = AuthoritativeHost::open(":memory:").expect("open host");
    let root = host.attachment_store_root().to_path_buf();
    let content = vec![b'x'; 256 * 1024];
    let command = import_command("att-large", "task-1", "large.bin", &content, "import-large");

    let (revision, duplicate, id) = host
        .import_attachment_accepted("user", "import-large", 0, command.clone())
        .expect("import large attachment");
    assert_eq!((revision, duplicate), (1, false));
    assert_eq!(id.as_str(), "att-large");
    assert_eq!(
        host.list_attachments(&AttachmentOwner::task("task-1").unwrap())
            .unwrap()[0]
            .size_bytes(),
        256 * 1024
    );

    let payload: String = host
        .authority
        .conn
        .query_row(
            "SELECT payload FROM authority_commands WHERE command_id='import-large'",
            [],
            |row| row.get(0),
        )
        .expect("stored payload");
    let wire: String = host
        .authority
        .conn
        .query_row(
            "SELECT wire FROM authority_history WHERE kind='change' ORDER BY cursor DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("stored history wire");
    assert!(
        payload.len() < 8 * 1024,
        "import payload must stay metadata-only, got {} bytes",
        payload.len()
    );
    assert!(
        wire.len() < 8 * 1024,
        "history wire must stay metadata-only, got {} bytes",
        wire.len()
    );
    assert!(payload.contains("\"size_bytes\":262144"), "{payload}");
    assert!(!payload.contains(&"x".repeat(64)), "{payload}");
    assert!(!wire.contains(&"x".repeat(64)), "{wire}");

    // A lost-ack retry still returns the original receipt.
    let (duplicate_revision, duplicate, duplicate_id) = host
        .import_attachment_accepted("user", "import-large", 0, command.clone())
        .expect("duplicate import");
    assert_eq!((duplicate_revision, duplicate), (1, true));
    assert_eq!(duplicate_id.as_str(), "att-large");
    assert_eq!(
        host.list_attachments(&AttachmentOwner::task("task-1").unwrap())
            .unwrap()
            .len(),
        1
    );

    // A different file under the same command id must conflict because the
    // checksum is bound to the immutable command payload.
    let mut changed = command;
    changed.content = b"different bytes".to_vec();
    assert!(matches!(
        host.import_attachment_accepted("user", "import-large", 0, changed),
        Err(AuthorityError::ImmutableConflict)
    ));

    // Application rejections keep their typed reason instead of flattening to
    // a generic invalid request.
    let fresh = host.import_attachment_accepted(
        "user",
        "import-fresh",
        1,
        import_command(
            "att-large",
            "task-1",
            "large.bin",
            &content,
            "import-large-activity",
        ),
    );
    assert!(matches!(
        fresh,
        Err(AuthorityError::Application(
            ApplicationError::AlreadyExists { .. }
        ))
    ));
    assert_eq!(host.snapshot().unwrap().0, 1);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn oversize_import_is_rejected_before_metadata_or_history() {
    let mut host = AuthoritativeHost::open(":memory:").expect("open host");
    let original_root = host.attachment_store_root().to_path_buf();
    let small_root = scratch_dir("oversize");
    host.attachment_store = AttachmentObjectStore::open(&small_root, 4).expect("open small store");

    let result = host.import_attachment_accepted(
        "user",
        "oversize",
        0,
        import_command("att-big", "task-1", "big.txt", b"12345", "big-activity"),
    );
    match result {
        Err(AuthorityError::Application(ApplicationError::AttachmentTooLarge { max_bytes })) => {
            assert_eq!(max_bytes, 4)
        }
        other => panic!("expected preserved oversize application error, got {other:?}"),
    }
    assert_eq!(host.snapshot().unwrap().0, 0);
    let owner = AttachmentOwner::task("task-1").expect("valid owner");
    assert!(host.list_attachments(&owner).expect("list").is_empty());
    assert!(host.application_queries().activities().unwrap().is_empty());

    let _ = fs::remove_dir_all(original_root);
    let _ = fs::remove_dir_all(small_root);
}

#[test]
fn invalid_filename_is_rejected_without_writing_metadata() {
    let mut host = AuthoritativeHost::open(":memory:").expect("open host");
    let root = host.attachment_store_root().to_path_buf();
    let error = host
        .application_services()
        .import_attachment(import_command(
            "att-bad",
            "task-1",
            "../escape.txt",
            b"payload",
            "bad-activity",
        ))
        .expect_err("invalid filename must be rejected");
    assert_eq!(
        error,
        orbit_application::ApplicationError::Domain(DomainError::InvalidAttachmentFilename)
    );
    let owner = AttachmentOwner::task("task-1").expect("valid owner");
    assert!(host.list_attachments(&owner).expect("list").is_empty());
    assert!(host.application_queries().activities().unwrap().is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn association_shares_one_object_until_the_last_reference_is_removed() {
    let mut host = AuthoritativeHost::open(":memory:").expect("open host");
    let root = host.attachment_store_root().to_path_buf();
    let first_owner = AttachmentOwner::task("task-1").expect("valid owner");
    let second_owner = AttachmentOwner::task("task-2").expect("valid owner");
    let (_, _, first_id) = host
        .import_attachment_accepted(
            "user",
            "import-1",
            0,
            import_command("att-1", "task-1", "note.txt", b"shared", "import-activity"),
        )
        .expect("import attachment");

    let (revision, duplicate, second_id) = host
        .associate_attachment_accepted(
            "user",
            "assoc-1",
            1,
            AssociateAttachmentCommand {
                source_id: first_id.clone(),
                new_id: AttachmentId::new("att-2").expect("valid attachment id"),
                owner: second_owner.clone(),
                activity_id: ActivityId::new("assoc-activity").expect("valid activity id"),
                origin: ActivityOrigin::Human,
            },
        )
        .expect("associate attachment");
    assert_eq!((revision, duplicate), (2, false));
    assert_eq!(host.list_attachments(&first_owner).expect("list").len(), 1);
    assert_eq!(host.list_attachments(&second_owner).expect("list").len(), 1);
    let object = host
        .attachment_content(&first_id)
        .expect("first content")
        .0
        .object()
        .to_owned();
    assert_eq!(
        host.attachment_content(&second_id)
            .expect("second content")
            .0
            .object(),
        object
    );

    let (_, _, content_removed) = host
        .remove_attachment_accepted(
            "user",
            "remove-first",
            2,
            RemoveAttachmentCommand {
                id: first_id,
                activity_id: ActivityId::new("remove-first-activity").expect("valid activity id"),
                origin: ActivityOrigin::Human,
            },
        )
        .expect("remove first association");
    assert!(!content_removed);
    assert_eq!(
        host.attachment_content(&second_id)
            .expect("content survives")
            .1,
        b"shared"
    );

    let (_, _, content_removed) = host
        .remove_attachment_accepted(
            "user",
            "remove-second",
            3,
            RemoveAttachmentCommand {
                id: second_id,
                activity_id: ActivityId::new("remove-second-activity").expect("valid activity id"),
                origin: ActivityOrigin::Human,
            },
        )
        .expect("remove second association");
    assert!(content_removed);
    assert!(!root.join(&object[..2]).join(&object).exists());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn rich_task_detail_and_acceptance_commands_are_duplicate_safe() {
    let mut host = AuthoritativeHost::open(":memory:").expect("open host");
    let root = host.attachment_store_root().to_path_buf();
    let task_id = create_task(&mut host);

    let update = UpdateTaskDetailsCommand {
        task_id: task_id.clone(),
        description: "Detailed".to_owned(),
        priority: Some(TaskPriority::High),
        labels: vec!["backend".to_owned()],
        activity_id: ActivityId::new("details-activity").expect("valid activity id"),
        origin: ActivityOrigin::Human,
    };
    let (revision, duplicate, result_id) = host
        .update_task_details_accepted("user", "details-1", 1, update.clone())
        .expect("update details");
    assert_eq!((revision, duplicate), (2, false));
    assert_eq!(result_id, "task-1");
    let (revision, duplicate, _) = host
        .update_task_details_accepted("user", "details-1", 1, update)
        .expect("duplicate details receipt");
    assert_eq!((revision, duplicate), (2, true));
    assert_eq!(host.application_queries().activities().unwrap().len(), 2);
    assert!(
        host.application_queries()
            .task(&task_id)
            .unwrap()
            .details()
            .priority()
            == Some(TaskPriority::High)
    );

    let (revision, duplicate, _) = host
        .apply_acceptance_criteria_accepted(
            "user",
            "criteria-1",
            2,
            AcceptanceCriteriaCommand {
                task_id: task_id.clone(),
                change: AcceptanceCriteriaChange::Add {
                    criterion_id: AcceptanceCriterionId::new("ac-1").expect("valid criterion id"),
                    text: "Works offline".to_owned(),
                },
                activity_id: ActivityId::new("criteria-activity").expect("valid activity id"),
                origin: ActivityOrigin::Human,
            },
        )
        .expect("add criterion");
    assert_eq!((revision, duplicate), (3, false));
    let criteria = host
        .acceptance_criteria(&task_id)
        .expect("read acceptance criteria");
    assert_eq!(criteria.len(), 1);
    assert_eq!(criteria.items()[0].text(), "Works offline");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn attachment_owner_existence_and_project_scope_cover_every_kind() {
    let mut host = AuthoritativeHost::open(":memory:").expect("open host");
    let root = host.attachment_store_root().to_path_buf();

    let project = ProjectId::new("project-1").expect("valid project id");
    host.project_command(
        "user",
        crate::domain_commands::ProjectCommand {
            command_id: "project-create".to_owned(),
            expected_revision: 0,
            project_id: project.as_str().to_owned(),
            name: "Attachment owners".to_owned(),
            workflow_id: None,
        },
    )
    .expect("create project");

    host.create_task_accepted(
        "user",
        "task-create",
        1,
        CreateTaskWithActivityCommand {
            task: CreateTaskCommand {
                id: TaskId::new("task-1").expect("valid task id"),
                title: "In project".to_owned(),
                scope: TaskScope::in_project(project.clone()),
            },
            activity_id: ActivityId::new("task-create-activity").expect("valid activity id"),
            origin: ActivityOrigin::Human,
            summary: "Created task".to_owned(),
        },
    )
    .expect("create project task");

    host.create_task_accepted(
        "user",
        "standalone-create",
        2,
        CreateTaskWithActivityCommand {
            task: CreateTaskCommand {
                id: TaskId::new("task-standalone").expect("valid task id"),
                title: "Standalone".to_owned(),
                scope: TaskScope::standalone(),
            },
            activity_id: ActivityId::new("standalone-activity").expect("valid activity id"),
            origin: ActivityOrigin::Human,
            summary: "Created standalone task".to_owned(),
        },
    )
    .expect("create standalone task");

    // Blueprint work items and agent jobs are schema-owned tables; insert the
    // minimal valid rows so the owner lookup is exercised against real SQL.
    host.authority
        .conn
        .execute(
            "INSERT INTO orbit_blueprints(id, version, manifest_json) VALUES('bp-attachment', 1, '{}')",
            [],
        )
        .expect("insert blueprint");
    host.authority
        .conn
        .execute(
            "INSERT INTO orbit_blueprint_work_items(id, project_id, kind, state, properties_json, revision, blueprint_id, blueprint_version) \
             VALUES('item-1','project-1','task','todo','{}',0,'bp-attachment',1)",
            [],
        )
        .expect("insert work item");
    host.authority
        .conn
        .execute(
            "INSERT INTO agent_jobs(id, request_id, project_id, task_id, launch_revision, agent_kind, \
             machine_ref, runtime_kind, repository_ref, state, revision, created_at_ms, updated_at_ms) \
             VALUES('job-1','req-1','project-1','task-1',0,'agent','machine-1','runtime-1','repo-1','queued',0,1,1)",
            [],
        )
        .expect("insert agent job");

    host.stage_markdown_create("op-knowledge", "note-1", "note.md", "# Note\n")
        .expect("stage knowledge note");

    let task = AttachmentOwner::task("task-1").expect("owner");
    let standalone = AttachmentOwner::task("task-standalone").expect("owner");
    let project_owner = AttachmentOwner::project("project-1").expect("owner");
    let work_item = AttachmentOwner::new(AttachmentOwnerKind::WorkItem, "item-1").expect("owner");
    let knowledge = AttachmentOwner::new(AttachmentOwnerKind::Knowledge, "note-1").expect("owner");
    let agent_job = AttachmentOwner::new(AttachmentOwnerKind::AgentJob, "job-1").expect("owner");

    // Every supported kind resolves to its own table.
    for owner in [
        &task,
        &standalone,
        &project_owner,
        &work_item,
        &knowledge,
        &agent_job,
    ] {
        assert!(
            host.attachment_owner_exists(owner).expect("owner lookup"),
            "{} {} should exist",
            owner.kind().as_str(),
            owner.id()
        );
    }

    // Missing owners in every kind are reported as absent, not as errors.
    for (kind, id) in [
        (AttachmentOwnerKind::Task, "task-missing"),
        (AttachmentOwnerKind::Project, "project-missing"),
        (AttachmentOwnerKind::WorkItem, "item-missing"),
        (AttachmentOwnerKind::Knowledge, "note-missing"),
        (AttachmentOwnerKind::AgentJob, "job-missing"),
    ] {
        let owner = AttachmentOwner::new(kind, id).expect("owner");
        assert!(
            !host.attachment_owner_exists(&owner).expect("owner lookup"),
            "{} {id} should be missing",
            owner.kind().as_str()
        );
        assert!(
            host.attachment_owner_project(&owner)
                .expect("project lookup")
                .is_none(),
            "missing owners have no project scope"
        );
    }

    // Project scope is derived from the stored owner row.
    assert_eq!(
        host.attachment_owner_project(&task).expect("task project"),
        Some(project.clone())
    );
    assert_eq!(
        host.attachment_owner_project(&project_owner)
            .expect("project project"),
        Some(project.clone())
    );
    assert_eq!(
        host.attachment_owner_project(&work_item)
            .expect("work item project"),
        Some(project.clone())
    );
    assert_eq!(
        host.attachment_owner_project(&agent_job)
            .expect("agent job project"),
        Some(project.clone())
    );
    assert_eq!(
        host.attachment_owner_project(&standalone)
            .expect("standalone project"),
        None
    );
    assert_eq!(
        host.attachment_owner_project(&knowledge)
            .expect("knowledge project"),
        None
    );

    let _ = fs::remove_dir_all(root);
}
