use crate::app::{BeamView, LogViewState};
use aurora_core::scheduler::BeamStatus;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render_log_panel(f: &mut Frame, beam: &BeamView, log_state: &LogViewState, area: Rect, focused: bool) {
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

    if lines.is_empty() {
        let placeholder = match beam.status {
            BeamStatus::Pending => "(en attente de démarrage)",
            BeamStatus::Running => "(pas encore de sortie)",
            _ => "(aucune sortie)",
        };
        lines.push(Line::from(Span::styled(
            placeholder,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let auto_indicator = if log_state.scroll_locked {
        " [scroll manuel]"
    } else {
        " [auto]"
    };
    let title = format!(" {} — Logs{} ", beam.name, auto_indicator);

    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .wrap(Wrap { trim: false })
        .scroll((log_state.scroll, 0));
    f.render_widget(paragraph, area);
}
