use crate::*;
use ratatui::{Terminal, backend::TestBackend};

fn app() -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m28-layout-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    let mut app = App::open(path.to_string_lossy().into_owned()).unwrap();
    let project = app
        .client
        .create_project(orbit_client::CreateProjectRequest {
            id: None,
            name: "Layout".into(),
        })
        .unwrap();
    app.client.install_bundled_blueprint("development").unwrap();
    app.client
        .apply_workflow(project.id.clone(), "development".to_owned(), None)
        .unwrap();
    for (title, statuses) in [
        ("Inbox task", vec![]),
        ("Ready task", vec!["ready"]),
        ("Progress task", vec!["ready", "in_progress"]),
        ("Review task", vec!["ready", "in_progress", "review"]),
        ("Done task", vec!["ready", "in_progress", "done"]),
    ] {
        let task = app
            .client
            .create_task(orbit_client::CreateTaskRequest {
                id: None,
                title: title.into(),
                project_id: Some(project.id.clone()),
                milestone_id: None,
            })
            .unwrap();
        for status in statuses {
            app.client
                .set_task_status(orbit_client::SetTaskStatusRequest {
                    id: task.id.clone(),
                    status: status.into(),
                })
                .unwrap();
        }
    }
    app.refresh_workspace();
    app.open_project(&project.id);
    let board = app
        .project
        .as_ref()
        .unwrap()
        .views
        .iter()
        .position(|view| view.id == "board")
        .unwrap();
    app.switch_view_by_index(board);
    app
}

fn text(app: &mut App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn common_terminal_sizes_remain_readable_and_keep_quit_visible() {
    let mut app = app();
    for (width, height) in [(60, 18), (80, 24), (100, 30), (120, 35), (160, 45)] {
        let rendered = text(&mut app, width, height);
        assert!(
            rendered.contains("Ctrl+Q quit"),
            "{width}x{height}: {rendered}"
        );
        assert!(rendered.contains("Task board"), "{width}x{height}");
        assert!(rendered.contains("Inbox"), "{width}x{height}");
        if width >= 120 {
            assert!(rendered.contains("Details"), "{width}x{height}");
        }
    }
}

#[test]
fn kanban_uses_a_readable_window_instead_of_nine_slivers() {
    let mut app = app();
    let rendered = text(&mut app, 120, 35);
    assert!(rendered.contains("Planned"), "{rendered}");
    assert!(rendered.contains("Ready"), "{rendered}");
    assert!(
        rendered.contains("Inbox task") || rendered.contains("Ready task"),
        "{rendered}"
    );
    assert!(!rendered.contains("╭ Inb╮"), "{rendered}");
    let wide = text(&mut app, 160, 45);
    assert!(wide.contains("columns 1-"), "{wide}");
}
