use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub enum HelpContext {
    Picker,
    Execution,
    LogView,
}

pub fn render_help_popup(f: &mut Frame, area: Rect, ctx: HelpContext) {
    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 16u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let lines = match ctx {
        HelpContext::Picker => vec![
            Line::from(Span::styled(
                " Picker",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" ↑↓ / jk    Navigate"),
            Line::from(" Space      Select/deselect"),
            Line::from(" d          Show dependencies"),
            Line::from(" Enter      Run the selected beam"),
            Line::from(" Esc / q    Quit"),
            Line::from(""),
            Line::from(Span::styled(
                " Search",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" (type)     Filter by name/description"),
            Line::from(" Backspace  Delete a character"),
        ],
        HelpContext::Execution => vec![
            Line::from(Span::styled(
                " Execution",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" ↑↓ / jk    Navigate beams (or scroll logs if logs focused)"),
            Line::from(" ←→ / Tab   Toggle focus beams / logs"),
            Line::from(" PgUp/Dn    Scroll the logs by page"),
            Line::from(" Ctrl+U/D   Half-page up / down"),
            Line::from(" g / G      Top / bottom of logs"),
            Line::from(" /          Filter the beams (beams focused) / search (logs focused)"),
            Line::from(" n / N      Next / previous match"),
            Line::from(" y          Copy the logs to the clipboard"),
            Line::from(" d          Show / hide the dependencies"),
            Line::from(" r          Rerun the beam (if Failed/Cancelled)"),
            Line::from(" q          Cancel the selected beam (if running)"),
            Line::from(" ?          Close this help"),
            Line::from(" Esc        Quit"),
        ],
        HelpContext::LogView => vec![
            Line::from(Span::styled(
                " Log view",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" ↑↓ / jk    Scroll line by line"),
            Line::from(" PgUp/Dn    Scroll by page"),
            Line::from(" G          Go to the bottom (resumes auto-scroll)"),
            Line::from(" y          Copy the logs"),
            Line::from(" Esc        Back to the split view"),
            Line::from(" q          Quit"),
        ],
    };

    let popup = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help: ? to close "),
        )
        .alignment(Alignment::Left);
    f.render_widget(popup, popup_area);
}
