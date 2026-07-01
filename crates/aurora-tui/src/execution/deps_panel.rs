use crate::app::ExecutionState;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Dependency panel for the beam selected in the runner. Replicates the
/// rendering of the picker (`picker::deps_panel`): direct dependencies as a
/// tree, then the beams that depend on the current beam ("Required by").
pub fn render_deps_panel(f: &mut Frame, state: &ExecutionState, area: Rect) {
    let content = if let Some(beam) = state.beams.get(state.selected) {
        let mut lines = vec![
            Line::from(Span::styled(
                format!(" {}", beam.name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        if beam.depends_on.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (none)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            let last = beam.depends_on.len() - 1;
            for (i, dep) in beam.depends_on.iter().enumerate() {
                let prefix = if i == last {
                    "  └── "
                } else {
                    "  ├── "
                };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", prefix, dep),
                    Style::default().fg(Color::Cyan),
                )));
            }
        }

        // Beams that depend on this beam.
        let dependents: Vec<&str> = state
            .beams
            .iter()
            .filter(|b| b.depends_on.iter().any(|d| d == &beam.name))
            .map(|b| b.name.as_str())
            .collect();

        if !dependents.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                " Required by:",
                Style::default().fg(Color::White),
            )));
            for dep in dependents {
                lines.push(Line::from(Span::styled(
                    format!("  → {}", dep),
                    Style::default().fg(Color::Magenta),
                )));
            }
        }

        lines
    } else {
        vec![Line::from("")]
    };

    let panel = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Dependencies "),
    );
    f.render_widget(panel, area);
}
