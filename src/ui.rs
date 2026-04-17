use chrono::Local;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Mode};
use crate::modules::{OutputLine, MODULES};
use crate::theme;

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Vertical regions: header(3) | main(fill) | input(3) | status(1)
    let [header, main, input, status] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(area);

    // Horizontal split of main: modules(22) | output(fill)
    let [modules_col, output_col] = Layout::horizontal([
        Constraint::Length(22),
        Constraint::Fill(1),
    ])
    .areas(main);

    draw_header(frame, header);
    draw_modules(frame, modules_col, app);
    draw_output(frame, output_col, app);
    draw_input(frame, input, app);
    draw_status(frame, status, app);
}

// ── Header ───────────────────────────────────────────────────────────────────

fn draw_header(frame: &mut Frame, area: Rect) {
    let date = Local::now().format("%Y.%m.%d").to_string();
    let title = format!(
        " MU/TH/UR 6000  \u{25c6}  WEYLAND-YUTANI CORP  \u{25c6}  NETWORK INTEL  \u{25c6}  {} ",
        date
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(theme::border())
        .style(Style::default().bg(theme::BG));

    let p = Paragraph::new(Line::from(Span::styled(title, theme::bright())))
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(p, area);
}

// ── Module list ──────────────────────────────────────────────────────────────

fn draw_modules(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .title(Span::styled(" MODULES ", theme::dim()))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(theme::border())
        .style(Style::default().bg(theme::BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner: list | description footer
    let [list_area, desc_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(inner);

    // Module list
    let items: Vec<ListItem> = MODULES
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == app.selected {
                theme::selected()
            } else {
                theme::normal()
            };
            let prefix = if i == app.selected { "\u{25b6} " } else { "  " };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}", prefix, m.name),
                style,
            )))
        })
        .collect();

    let list = List::new(items);
    frame.render_stateful_widget(list, list_area, &mut app.list_state);

    // Description of selected module
    let desc = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            MODULES[app.selected].description,
            theme::dim(),
        )),
    ]))
    .wrap(Wrap { trim: true });
    frame.render_widget(desc, desc_area);
}

// ── Output panel ─────────────────────────────────────────────────────────────

pub fn draw_output(frame: &mut Frame, area: Rect, app: &mut App) {
    let is_running = app.mode == Mode::Running;
    let border_style = if is_running {
        theme::border_active()
    } else {
        theme::border()
    };

    let title_text = if is_running {
        " OUTPUT  \u{25cf} RUNNING "
    } else {
        " OUTPUT "
    };

    let block = Block::default()
        .title(Span::styled(title_text, if is_running { theme::bright() } else { theme::dim() }))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(border_style)
        .style(Style::default().bg(theme::BG));

    let inner = block.inner(area);
    let viewport_h = inner.height.saturating_sub(0) as usize;

    // Update app's viewport_height so the app can compute max scroll
    app.viewport_height = viewport_h as u16;

    // Compute effective scroll offset
    let total = app.output.len();
    let max_scroll = total.saturating_sub(viewport_h) as u16;
    let scroll_y = if app.auto_scroll {
        max_scroll
    } else {
        app.scroll.min(max_scroll)
    };
    // Keep app.scroll in sync
    app.scroll = scroll_y;

    // Build lines from output vec
    let lines: Vec<Line> = app.output.iter().map(output_line_to_ratatui).collect();

    let p = Paragraph::new(Text::from(lines))
        .block(block)
        .scroll((scroll_y, 0));

    frame.render_widget(p, area);
}

fn output_line_to_ratatui(ol: &OutputLine) -> Line<'_> {
    match ol {
        OutputLine::Normal(s) => Line::from(Span::styled(s.as_str(), theme::normal())),
        OutputLine::Bright(s) => Line::from(Span::styled(s.as_str(), theme::bright())),
        OutputLine::Dim(s)    => Line::from(Span::styled(s.as_str(), theme::dim())),
        OutputLine::Error(s)  => Line::from(Span::styled(s.as_str(), theme::error())),
        OutputLine::Done      => Line::raw(""),
    }
}

// ── Input bar ────────────────────────────────────────────────────────────────

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let active = app.mode == Mode::Input;

    let (b_style, t_style) = if active {
        (theme::border_active(), theme::bright())
    } else {
        (theme::border(), theme::dim())
    };

    let module_name = MODULES[app.selected].name;
    let title = format!(" TARGET  \u{25c6}  {} ", module_name);

    let block = Block::default()
        .title(Span::styled(title, t_style))
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(b_style)
        .style(Style::default().bg(theme::BG));

    // Cursor blink: block char visible on even ticks
    let cursor = if app.tick % 10 < 5 { "\u{2588}" } else { " " };

    let line: Line = if app.input.is_empty() {
        if active {
            // Just cursor
            Line::from(Span::styled(cursor, theme::bright()))
        } else {
            // Show hint
            Line::from(Span::styled(MODULES[app.selected].hint, theme::faint()))
        }
    } else if active {
        // Input + cursor
        Line::from(vec![
            Span::styled(app.input.as_str(), theme::bright()),
            Span::styled(
                cursor,
                Style::default().fg(theme::GREEN_BRIGHT).add_modifier(Modifier::RAPID_BLINK),
            ),
        ])
    } else {
        Line::from(Span::styled(app.input.as_str(), theme::normal()))
    };

    let p = Paragraph::new(line).block(block);
    frame.render_widget(p, area);
}

// ── Status bar ───────────────────────────────────────────────────────────────

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let scroll_indicator = if app.auto_scroll {
        "AUTO"
    } else {
        "MANUAL"
    };

    let hint = match app.mode {
        Mode::Browse  => "[\u{2191}\u{2193}/JK] SELECT  [ENTER/TAB] INPUT  [PGUP/PGDN] SCROLL  [Q] QUIT",
        Mode::Input   => "[ENTER] EXECUTE  [ESC/TAB] CANCEL  [CTRL+U] CLEAR",
        Mode::Running => "[PGUP/PGDN] SCROLL  [Q] QUIT",
    };

    let text = format!(
        " \u{25c6} {}  \u{25c6}  SCROLL:{}  \u{25c6}  {} ",
        app.status, scroll_indicator, hint
    );

    let p = Paragraph::new(Span::styled(text, theme::dim()))
        .style(Style::default().bg(theme::BG));

    frame.render_widget(p, area);
}
