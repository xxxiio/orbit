//! Round-trips for rich task details, labels, and ordered acceptance criteria.
use super::*;
use orbit_domain::{AcceptanceCriteria, AcceptanceCriterion, TaskDetails, TaskPriority};

fn task_id(value: &str) -> TaskId {
    TaskId::new(value).expect("valid task id")
}

#[test]
fn task_details_and_labels_round_trip() {
    let mut store = SqliteStore::in_memory().expect("open database");
    let mut task = Task::new(
        task_id("task-details"),
        "Rich task",
        TaskScope::standalone(),
    )
    .expect("valid task");
    let details = TaskDetails::new(
        "A detailed description",
        Some(TaskPriority::High),
        vec!["beta".to_owned(), "alpha".to_owned(), "beta".to_owned()],
    )
    .expect("valid details");
    task.set_details(details);
    TaskRepository::save(&mut store, task).expect("save task");

    let loaded = TaskRepository::get(&store, &task_id("task-details"))
        .expect("load task")
        .expect("task exists");
    assert_eq!(loaded.details().description(), "A detailed description");
    assert_eq!(loaded.details().priority(), Some(TaskPriority::High));
    assert_eq!(
        loaded.details().labels(),
        ["alpha".to_owned(), "beta".to_owned()]
    );

    let replacement =
        TaskDetails::new("Updated", None, vec!["gamma".to_owned()]).expect("valid replacement");
    let mut loaded = loaded;
    loaded.set_details(replacement);
    TaskRepository::save(&mut store, loaded).expect("update task");

    let loaded = TaskRepository::get(&store, &task_id("task-details"))
        .expect("load task")
        .expect("task exists");
    assert_eq!(loaded.details().description(), "Updated");
    assert_eq!(loaded.details().priority(), None);
    assert_eq!(loaded.details().labels(), ["gamma".to_owned()]);
    let label_count = store
        .query_row("SELECT COUNT(*) FROM task_labels", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("label count");
    assert_eq!(label_count, 1);
}

#[test]
fn ordered_acceptance_criteria_round_trip_preserves_position_and_completion() {
    let mut store = SqliteStore::in_memory().expect("open database");
    let task = Task::new(
        task_id("task-criteria"),
        "Acceptance task",
        TaskScope::standalone(),
    )
    .expect("valid task");
    TaskRepository::save(&mut store, task).expect("save task");

    let first = AcceptanceCriterion::restored(
        AcceptanceCriterionId::new("ac-1").expect("valid criterion id"),
        "First",
        Some(100),
    )
    .expect("valid criterion");
    let second = AcceptanceCriterion::new(
        AcceptanceCriterionId::new("ac-2").expect("valid criterion id"),
        "Second",
    )
    .expect("valid criterion");
    let third = AcceptanceCriterion::restored(
        AcceptanceCriterionId::new("ac-3").expect("valid criterion id"),
        "Third",
        None,
    )
    .expect("valid criterion");
    let criteria =
        AcceptanceCriteria::from_ordered(vec![third, first, second]).expect("valid criteria");
    AcceptanceCriteriaRepository::replace_acceptance_criteria(
        &mut store,
        &task_id("task-criteria"),
        &criteria,
    )
    .expect("replace criteria");

    let loaded =
        AcceptanceCriteriaRepository::list_acceptance_criteria(&store, &task_id("task-criteria"))
            .expect("list criteria");
    let texts: Vec<&str> = loaded.iter().map(|item| item.text()).collect();
    assert_eq!(texts, ["Third", "First", "Second"]);
    assert_eq!(loaded[1].completed_at_ms(), Some(100));
    assert_eq!(loaded[0].completed_at_ms(), None);

    // Replacing the list preserves the new order and stable criterion ids.
    let reordered =
        AcceptanceCriteria::from_ordered(loaded.into_iter().rev()).expect("valid reorder");
    AcceptanceCriteriaRepository::replace_acceptance_criteria(
        &mut store,
        &task_id("task-criteria"),
        &reordered,
    )
    .expect("reorder criteria");
    let loaded =
        AcceptanceCriteriaRepository::list_acceptance_criteria(&store, &task_id("task-criteria"))
            .expect("list criteria");
    let ids: Vec<&str> = loaded.iter().map(|item| item.id().as_str()).collect();
    assert_eq!(ids, ["ac-2", "ac-1", "ac-3"]);
    assert_eq!(loaded[1].completed_at_ms(), Some(100));
}
