//! Restrained terminal design tokens.
//!
//! The palette is deliberately small: one accent (cyan), one quiet grey, and
//! three semantic signals (success/warning/danger). Colour is never the only
//! state signal, so `NO_COLOR` and `TERM=dumb` sessions keep every distinction:
//!
//! * status always carries a glyph (`✓ ! ◐ ▶ ○ ~`);
//! * selection always carries a `▸` rail and a filled row;
//! * focus always carries a heavy border and a bright title.
use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub color: bool,
}

impl Theme {
    pub fn detect() -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some()
            || std::env::var("TERM")
                .map(|term| term == "dumb")
                .unwrap_or(false);
        Self { color: !no_color }
    }

    /// Body copy.
    pub fn text(&self) -> Style {
        Style::default()
    }

    /// Secondary copy: timestamps, counts, hints.
    pub fn muted(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::DIM);
        if self.color {
            style.fg(Color::DarkGray)
        } else {
            style
        }
    }

    /// The single accent colour, used for key hints, markers and focused titles.
    pub fn accent(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::Cyan)
        } else {
            style.add_modifier(Modifier::UNDERLINED)
        }
    }

    /// Emphasis used for the item the user is looking at (not selection).
    pub fn title(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::White)
        } else {
            style
        }
    }

    /// Quiet section label inside a panel, e.g. `PROPERTIES`.
    pub fn section(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::Gray)
        } else {
            style.add_modifier(Modifier::UNDERLINED)
        }
    }

    /// Table header row and kanban column headings.
    pub fn column_header(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::DarkGray)
        } else {
            style
        }
    }

    /// Explanatory copy for empty panes.
    pub fn empty(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::ITALIC);
        if self.color {
            style.fg(Color::DarkGray)
        } else {
            style.add_modifier(Modifier::DIM)
        }
    }

    /// The whole-row highlight for the selected list/table card.
    pub fn selected(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::Black).bg(Color::Cyan)
        } else {
            style.add_modifier(Modifier::REVERSED)
        }
    }

    /// Trailing padding that extends a selected row to the pane edge. Kept
    /// separate from [`Self::selected`] so cells may keep their own foreground
    /// colour without breaking the row bar.
    pub fn selected_fill(&self) -> Style {
        if self.color {
            Style::default().bg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::REVERSED)
        }
    }

    /// Border of the pane that owns the keyboard.
    pub fn focused_border(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }

    /// Border of every other pane.
    pub fn border(&self) -> Style {
        if self.color {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        }
    }

    pub fn success(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Green)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }

    pub fn warning(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }

    pub fn danger(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
        }
    }

    pub fn info(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Blue)
        } else {
            Style::default()
        }
    }

    pub fn status(&self, status: &str) -> Style {
        match status {
            "done" | "completed" | "mastered" | "reported" | "active" | "succeeded"
            | "supported" | "evaluated" | "recorded" => self.success(),
            "blocked" | "failed" | "unreachable" | "cancelled" | "error" | "rejected" => {
                self.danger()
            }
            "ready" | "review" | "waiting" | "waiting_input" | "launching" | "in_progress"
            | "investigating" | "studying" | "practicing" | "analysis" | "running"
            | "implementing" | "preparing" | "assessment" => self.warning(),
            "inbox" | "planned" | "proposed" | "backlog" | "designed" | "due" | "queued" => {
                self.info()
            }
            _ => self.text(),
        }
    }

    pub fn status_glyph(status: &str) -> &'static str {
        match status {
            "done" | "completed" | "mastered" | "reported" | "succeeded" | "supported"
            | "evaluated" | "recorded" => "✓",
            "blocked" | "failed" | "unreachable" | "cancelled" | "interrupted" => "!",
            "rejected" => "✕",
            "ready" | "review" | "waiting" | "waiting_input" | "launching" | "preparing" => "◐",
            "in_progress" | "investigating" | "studying" | "practicing" | "analysis"
            | "running" | "implementing" => "▶",
            "inbox" | "proposed" | "planned" | "backlog" | "designed" | "due" | "queued" => "○",
            "deferred" | "paused" | "on_hold" => "~",
            "active" => "●",
            "archived" => "▪",
            "assessment" => "◇",
            _ => "·",
        }
    }
}

/// Compact sidebar width used from the medium breakpoint upwards.
pub const SIDEBAR_WIDTH: u16 = 22;
/// Comfortable detail width on medium terminals.
pub const DETAIL_WIDTH: u16 = 30;
/// The sidebar never shrinks below its marker + navigation label.
pub const SIDEBAR_MIN: u16 = 18;
/// ... and never grows into a second workspace, even with long project names.
pub const SIDEBAR_MAX: u16 = 26;
/// The detail pane keeps property keys and a wrapped value readable.
pub const DETAIL_MIN: u16 = 26;
/// Wider terminals may give the detail pane more room, up to this width.
pub const DETAIL_MAX: u16 = 40;
/// Below this width the detail pane is opened on demand instead of docked.
pub const WIDE_BREAKPOINT: u16 = 120;
/// Below this width only one pane is visible at a time.
pub const MEDIUM_BREAKPOINT: u16 = 76;
/// One row is reserved above a component's content for its legend/hints.
pub const LEGEND_HEIGHT: u16 = 1;

#[cfg(test)]
mod tests {
    use super::Theme;
    use std::collections::BTreeSet;

    const TASK_STATUSES: &[&str] = &[
        "inbox",
        "planned",
        "ready",
        "in_progress",
        "waiting",
        "blocked",
        "review",
        "deferred",
        "done",
        "cancelled",
    ];
    const PROJECT_STATUSES: &[&str] = &[
        "proposed",
        "active",
        "on_hold",
        "completed",
        "cancelled",
        "archived",
    ];
    const AGENT_STATUSES: &[&str] = &[
        "queued",
        "preparing",
        "launching",
        "running",
        "waiting_input",
        "blocked",
        "completed",
        "failed",
        "interrupted",
        "unreachable",
    ];

    #[test]
    fn every_known_status_has_a_non_default_glyph() {
        let mut statuses: BTreeSet<String> = TASK_STATUSES
            .iter()
            .chain(PROJECT_STATUSES)
            .chain(AGENT_STATUSES)
            .map(|status| (*status).to_owned())
            .collect();
        for descriptor in orbit_blueprint::bundled::BUNDLED_BLUEPRINTS {
            let blueprint = orbit_blueprint::bundled::bundled_blueprint(descriptor.id).unwrap();
            for item in &blueprint.work_items {
                statuses.extend(item.states.iter().cloned());
            }
        }
        for status in statuses {
            let glyph = Theme::status_glyph(&status);
            assert_ne!(glyph, "·", "{status} has no glyph");
            assert_eq!(
                glyph.chars().count(),
                1,
                "{status} glyph must occupy one cell"
            );
            // Every status also has a meaningful style decision.
            let _ = Theme { color: true }.status(&status);
        }
    }

    #[test]
    fn project_statuses_never_share_a_glyph() {
        let glyphs: BTreeSet<&str> = PROJECT_STATUSES
            .iter()
            .map(|status| Theme::status_glyph(status))
            .collect();
        assert_eq!(glyphs.len(), PROJECT_STATUSES.len(), "{glyphs:?}");
    }
}
