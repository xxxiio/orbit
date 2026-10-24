//! Ratatui renderer for the project-centric shell. All data arrives through
//! `App` and resolved `orbit-ui` models; this module never queries storage.
//!
//! The renderer is split by responsibility: [`layout`] owns the horizontal
//! budget and rect arithmetic, [`shell`] the frame chrome, [`components`] the
//! workspace bodies and [`modals`] every overlay.
mod components;
mod layout;
mod modals;
mod shell;

use crate::app::App;
use modals::draw_modal;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};
use shell::{draw_body, draw_header, draw_status};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.set_viewport(area.width);
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);
    draw_header(frame, root[0], app);
    draw_body(frame, root[1], app);
    draw_status(frame, root[2], app);
    draw_modal(frame, area, app);
}
