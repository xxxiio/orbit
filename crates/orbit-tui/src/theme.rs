//! Restrained terminal design tokens. Color is never the only state signal:
//! every status also has a glyph and every focus change has a border/marker.
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

    pub fn text(&self) -> Style {
        Style::default()
    }
    pub fn muted(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::DIM);
        if self.color {
            style.fg(Color::DarkGray)
        } else {
            style
        }
    }
    pub fn accent(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::Cyan)
        } else {
            style.add_modifier(Modifier::UNDERLINED)
        }
    }
    pub fn title(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::White)
        } else {
            style
        }
    }
    pub fn selected(&self) -> Style {
        let style = Style::default().add_modifier(Modifier::BOLD);
        if self.color {
            style.fg(Color::Black).bg(Color::Cyan)
        } else {
            style.add_modifier(Modifier::REVERSED)
        }
    }
    pub fn focused_border(&self) -> Style {
        if self.color {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        }
    }
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

pub const SIDEBAR_WIDTH: u16 = 22;
pub const DETAIL_WIDTH: u16 = 34;
pub const WIDE_BREAKPOINT: u16 = 104;
pub const MEDIUM_BREAKPOINT: u16 = 76;
