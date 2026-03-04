use crate::app::{BeamView, LogViewState};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render_log_panel(f: &mut Frame, beam: &BeamView, log_state: &LogViewState, area: Rect) {
    let mut lines: Vec<Line> = beam
        .stdout
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();
    if !beam.stderr.is_empty() {
        lines.push(Line::from(Span::styled(
            "── stderr ──",
            Style::default().fg(Color::Red),
        )));
        lines.extend(beam.stderr.iter().map(|l| {
            Line::from(Span::styled(l.as_str(), Style::default().fg(Color::Red)))
        }));
    }

    let auto_indicator = if log_state.scroll_locked {
        " [scroll manuel]"
    } else {
        " [auto]"
    };
    let title = format!(" {} — Logs{} ", beam.name, auto_indicator);

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false })
        .scroll((log_state.scroll, 0));
    f.render_widget(paragraph, area);
}
