use crate::app::{FocusPane, Screen};
use crate::*;
use orbit_ui::model::{ItemKind, TimelineEventModel};
use ratatui::{Terminal, backend::TestBackend};

fn app() -> App {
    let path = std::env::temp_dir().join(format!(
        "orbit-tui-m28-focus-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    App::open(path.to_string_lossy().into_owned()).unwrap()
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
fn tab_cycles_sidebar_and_workspace_below_wide_breakpoint() {
    let mut app = app();
    app.set_viewport(100);
    app.focus = FocusPane::Workspace;
    app.focus_next(false);
    assert_eq!(app.focus, FocusPane::Sidebar);
    app.focus_next(false);
    assert_eq!(app.focus, FocusPane::Workspace);
}

#[test]
fn timeline_selection_stays_visible_at_60x18() {
    let mut app = app();
    let events = (0..12)
        .map(|index| TimelineEventModel {
            id: format!("event-{index}"),
            kind: ItemKind::Schedule,
            title: format!("Timeline event {index}"),
            when: format!("2026-10-22 {index:02}:00Z"),
            sort_key: index,
            status: "planned".into(),
            detail: "detail".into(),
        })
        .collect::<Vec<_>>();
    app.home.components = vec![orbit_ui::ResolvedComponent {
        id: "timeline".into(),
        title: "Timeline".into(),
        component_type: "timeline".into(),
        status: orbit_ui::LoadStatus::Ready,
        body: orbit_ui::ComponentBody::Timeline { events },
        selection: (0..12)
            .map(|index| orbit_ui::SelectionItem {
                id: format!("event-{index}"),
                kind: ItemKind::Schedule,
                title: format!("Timeline event {index}"),
                subtitle: String::new(),
                status: "planned".into(),
                fields: Vec::new(),
                allowed_statuses: Vec::new(),
                allowed_transitions: Vec::new(),
                revision: None,
            })
            .collect(),
    }];
    app.screen = Screen::Home;
    app.active_component = 0;
    app.selected = 11;
    app.focus = FocusPane::Workspace;
    let rendered = text(&mut app, 60, 18);
    assert!(rendered.contains("Timeline event 11"), "{rendered}");
    assert!(
        rendered
            .lines()
            .any(|line| line.contains("▸") && line.contains("event 11")),
        "{rendered}"
    );
}
