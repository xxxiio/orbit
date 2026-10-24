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
            "done" | "completed" | "mastered" | "reported" | "active" | "succeeded" => {
                self.success()
            }
            "blocked" | "failed" | "unreachable" | "cancelled" | "error" => self.danger(),
            "ready" | "review" | "waiting" | "waiting_input" | "launching" | "in_progress"
            | "investigating" | "studying" | "practicing" | "analysis" | "running" => {
                self.warning()
            }
            "inbox" | "planned" | "proposed" | "backlog" | "designed" | "due" => self.info(),
            _ => self.text(),
        }
    }

    pub fn status_glyph(status: &str) -> &'static str {
        match status {
            "done" | "completed" | "mastered" | "reported" | "succeeded" => "✓",
            "blocked" | "failed" | "unreachable" | "cancelled" => "!",
            "ready" | "review" | "waiting" | "waiting_input" | "launching" => "◐",
            "in_progress" | "investigating" | "studying" | "practicing" | "analysis"
            | "running" => "▶",
            "inbox" | "proposed" | "planned" | "backlog" | "designed" | "due" => "○",
            "deferred" | "paused" | "on_hold" => "~",
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
