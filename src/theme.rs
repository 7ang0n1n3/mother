use ratatui::{
    style::{Color, Modifier, Style},
    widgets::BorderType,
};

// ── Green phosphor CRT palette ──────────────────────────────────────────────
pub const BG:            Color = Color::Rgb(0,   8,  2);
pub const GREEN_BRIGHT:  Color = Color::Rgb(57, 255, 20);  // selected, open ports
pub const GREEN_NORMAL:  Color = Color::Rgb(0,  200, 55);  // standard text
pub const GREEN_DIM:     Color = Color::Rgb(0,  100, 30);  // borders, secondary
pub const GREEN_FAINT:   Color = Color::Rgb(0,   40, 12);  // placeholder, very dim

pub const BORDER: BorderType = BorderType::Double;

// ── Style constructors ───────────────────────────────────────────────────────

pub fn normal() -> Style {
    Style::default().fg(GREEN_NORMAL).bg(BG)
}

pub fn bright() -> Style {
    Style::default().fg(GREEN_BRIGHT).bg(BG).add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(GREEN_DIM).bg(BG)
}

pub fn faint() -> Style {
    Style::default().fg(GREEN_FAINT).bg(BG)
}

/// Highlighted list item (inverted)
pub fn selected() -> Style {
    Style::default()
        .fg(BG)
        .bg(GREEN_BRIGHT)
        .add_modifier(Modifier::BOLD)
}

pub fn border() -> Style {
    Style::default().fg(GREEN_DIM).bg(BG)
}

pub fn border_active() -> Style {
    Style::default().fg(GREEN_BRIGHT).bg(BG)
}

pub fn error() -> Style {
    Style::default()
        .fg(GREEN_BRIGHT)
        .bg(BG)
        .add_modifier(Modifier::BOLD | Modifier::RAPID_BLINK)
}
