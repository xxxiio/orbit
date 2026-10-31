//! Unified search: grouping, escaping, ordering, truncation and scoping.
use super::*;
use crate::domain_commands::{
    ProjectCommand, ProjectCommandAction, ProjectCommandRequest, TaskAction, TaskCommand,
};
use crate::search::{SearchKind, SearchResponse};

fn host() -> AuthoritativeHost {
    AuthoritativeHost::open(":memory:").expect("in-memory authority")
}

fn project(host: &mut AuthoritativeHost, id: &str, name: &str) {
    let revision = host.projects().expect("projects").0;
    host.project_command(
        "device-a",
        ProjectCommandRequest {
            command_id: format!("create-{id}"),
            expected_revision: revision,
            action: ProjectCommandAction::Create,
            project_id: id.into(),
            name: name.into(),
            status: None,
            workflow_id: None,
        },
    )
    .expect("project accepted");
}

fn task(host: &mut AuthoritativeHost, id: &str, title: &str, project_id: Option<&str>) {
    let revision = host.projects().expect("projects").0;
    host.task_command(
        "device-a",
        TaskCommand {
            command_id: format!("create-{id}"),
            expected_revision: revision,
            action: TaskAction::Create {
                task_id: id.into(),
                title: title.into(),
                project_id: project_id.map(str::to_owned),
                milestone_id: None,
                description: String::new(),
                status: None,
                priority: None,
                estimated_minutes: None,
                deadline_at_ms: None,
                earliest_start_at_ms: None,
                acceptance_criteria: Vec::new(),
                prerequisite_ids: Vec::new(),
            },
        },
    )
    .expect("task accepted");
}

fn folders(host: &AuthoritativeHost, paths: &[&str]) {
    for path in paths {
        host.knowledge_vault
            .vault()
            .create_folder(path)
            .expect("folder created");
    }
}

fn group(response: &SearchResponse, kind: SearchKind) -> &crate::search::SearchGroup {
    response
        .groups
        .iter()
        .find(|group| group.kind == kind)
        .expect("every group is always present")
}

#[test]
fn groups_are_ordered_and_carry_server_composed_subtitles() {
    let mut host = host();
    project(&mut host, "guide-core", "Guide Core");
    project(&mut host, "guide-web", "guide web");
    project(&mut host, "elsewhere", "Elsewhere");
    task(
        &mut host,
        "task-a",
        "Fix the Guide login",
        Some("guide-web"),
    );
    task(&mut host, "task-b", "Guide metrics", None);
    task(&mut host, "task-c", "Write the docs", Some("guide-core"));
    folders(&host, &["docs", "docs/guides"]);
    host.stage_markdown_create(
        "note-guide",
        "note-guide",
        "docs/guides/guide.md",
        "# Guide\nA guide to the flow\n",
    )
    .expect("note written");
    host.stage_markdown_create(
        "note-other",
        "note-other",
        "docs/guides/other.md",
        "# Other\nUnrelated\n",
    )
    .expect("note written");

    // "guide" is absent from the note identity frontmatter, so only the note
    // whose title/body actually matches is returned.
    let response = host.search("  guide  ", 10, None).expect("search");
    assert_eq!(response.query, "guide", "the query is echoed trimmed");
    assert_eq!(response.limit, 10);
    assert_eq!(response.revision, host.projects().expect("projects").0);
    assert_eq!(
        response
            .groups
            .iter()
            .map(|group| group.kind)
            .collect::<Vec<_>>(),
        vec![SearchKind::Project, SearchKind::Task, SearchKind::Note],
        "group order is fixed"
    );

    let projects = group(&response, SearchKind::Project);
    assert!(!projects.truncated);
    // Case-insensitive match, ordered by name then id.
    assert_eq!(
        projects
            .results
            .iter()
            .map(|result| result.id.as_str())
            .collect::<Vec<_>>(),
        vec!["guide-core", "guide-web"]
    );
    assert_eq!(projects.results[0].title, "Guide Core");
    assert_eq!(
        projects.results[0].subtitle, "proposed",
        "a project subtitle is its status token"
    );
    assert_eq!(
        projects.results[0].project_id.as_deref(),
        Some("guide-core")
    );
    assert_eq!(projects.results[0].path, None);

    let tasks = group(&response, SearchKind::Task);
    assert_eq!(
        tasks
            .results
            .iter()
            .map(|result| result.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-a", "task-b"],
        "tasks are ordered by title"
    );
    assert_eq!(tasks.results[0].subtitle, "inbox · guide web");
    assert_eq!(tasks.results[0].project_id.as_deref(), Some("guide-web"));
    assert_eq!(
        tasks.results[1].subtitle, "inbox",
        "an unassigned task has only its status line"
    );
    assert_eq!(tasks.results[1].project_id, None);

    let notes = group(&response, SearchKind::Note);
    assert_eq!(notes.results.len(), 1);
    assert_eq!(notes.results[0].id, "note-guide");
    assert_eq!(notes.results[0].subtitle, "docs/guides/guide.md");
    assert_eq!(
        notes.results[0].path.as_deref(),
        Some("docs/guides/guide.md")
    );
    assert_eq!(notes.results[0].project_id, None);
}

#[test]
fn a_project_name_is_not_searched_for_tasks() {
    let mut host = host();
    project(&mut host, "orbit", "Orbit");
    task(&mut host, "task-a", "Unrelated title", Some("orbit"));
    let response = host.search("orbit", 10, None).expect("search");
    assert_eq!(group(&response, SearchKind::Project).results.len(), 1);
    assert!(
        group(&response, SearchKind::Task).results.is_empty(),
        "only task titles are matched, the joined project name is display data"
    );
}

#[test]
fn like_wildcards_in_the_query_match_literally() {
    let mut host = host();
    project(&mut host, "percent", "100% done");
    project(&mut host, "plain", "100x done");
    project(&mut host, "underscore", "file_name");
    project(&mut host, "dash", "fileXname");

    let percent = host.search("0% d", 10, None).expect("search");
    assert_eq!(
        percent
            .groups
            .iter()
            .flat_map(|group| group.results.iter())
            .map(|result| result.id.as_str())
            .collect::<Vec<_>>(),
        vec!["percent"],
        "`%` must not act as a wildcard"
    );

    let underscore = host.search("file_", 10, None).expect("search");
    assert_eq!(
        underscore
            .groups
            .iter()
            .flat_map(|group| group.results.iter())
            .map(|result| result.id.as_str())
            .collect::<Vec<_>>(),
        vec!["underscore"],
        "`_` must not act as a single-character wildcard"
    );

    // A backslash is the escape character itself and must also be literal.
    let response = host.search("\\", 10, None).expect("search");
    assert!(response.groups.iter().all(|group| group.results.is_empty()));
}

#[test]
fn truncation_uses_one_extra_row_and_the_limit_is_clamped() {
    let mut host = host();
    for index in 0..3 {
        project(
            &mut host,
            &format!("orbit-{index}"),
            &format!("Orbit {index}"),
        );
    }
    let response = host.search("orbit", 2, None).expect("search");
    let projects = group(&response, SearchKind::Project);
    assert!(projects.truncated, "three matches cannot fit a page of two");
    assert_eq!(projects.results.len(), 2);
    assert_eq!(
        projects
            .results
            .iter()
            .map(|result| result.id.as_str())
            .collect::<Vec<_>>(),
        vec!["orbit-0", "orbit-1"]
    );

    let exact = host.search("orbit", 3, None).expect("search");
    assert!(!group(&exact, SearchKind::Project).truncated);

    // A zero limit is clamped up, an oversized one down to the contract window.
    assert_eq!(host.search("orbit", 0, None).expect("search").limit, 1);
    assert_eq!(host.search("orbit", 99, None).expect("search").limit, 20);
}

#[test]
fn the_project_filter_narrows_projects_and_tasks_but_not_notes() {
    let mut host = host();
    project(&mut host, "orbit", "Orbit");
    project(&mut host, "orbit-archive", "Orbit archive");
    task(&mut host, "task-a", "Orbit one", Some("orbit"));
    task(&mut host, "task-b", "Orbit two", Some("orbit-archive"));
    folders(&host, &["notes"]);
    host.stage_markdown_create("note-1", "note-1", "notes/orbit.md", "# Orbit\nOrbit\n")
        .expect("note written");

    let response = host.search("orbit", 10, Some("orbit")).expect("search");
    assert_eq!(
        group(&response, SearchKind::Project)
            .results
            .iter()
            .map(|result| result.id.as_str())
            .collect::<Vec<_>>(),
        vec!["orbit"]
    );
    assert_eq!(
        group(&response, SearchKind::Task)
            .results
            .iter()
            .map(|result| result.id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-a"]
    );
    assert_eq!(
        group(&response, SearchKind::Note).results.len(),
        1,
        "notes are never narrowed by project in this iteration"
    );
}

#[test]
fn empty_and_oversized_queries_are_refusals() {
    let host = host();
    assert!(matches!(
        host.search("", 10, None),
        Err(AuthorityError::Invalid("search query is required"))
    ));
    assert!(matches!(
        host.search("   ", 10, None),
        Err(AuthorityError::Invalid("search query is required"))
    ));
    let oversized = "a".repeat(crate::search::MAX_QUERY_CHARS + 1);
    assert!(matches!(
        host.search(&oversized, 10, None),
        Err(AuthorityError::Invalid("search query is too long"))
    ));
    // The boundary itself is accepted.
    let exactly = "a".repeat(crate::search::MAX_QUERY_CHARS);
    let response = host.search(&exactly, 10, None).expect("boundary accepted");
    assert!(response.groups.iter().all(|group| group.results.is_empty()));
}

#[test]
fn the_legacy_project_payload_still_creates() {
    // Old transports keep sending the five-field value through the same method.
    let mut host = host();
    let revision = host.projects().expect("projects").0;
    host.project_command(
        "device-a",
        ProjectCommand {
            command_id: "legacy".into(),
            expected_revision: revision,
            project_id: "legacy".into(),
            name: "Legacy".into(),
            workflow_id: None,
        },
    )
    .expect("legacy create");
    let response = host.search("legacy", 10, None).expect("search");
    assert_eq!(group(&response, SearchKind::Project).results.len(), 1);
}
