use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Renders the dependency panel shared by the picker and the execution view:
/// the beam name, its direct dependencies as a tree, then the beams that
/// depend on it ("Required by"). Only how each caller obtains the selected
/// beam and computes its dependents differs; the rendering lives here so the
/// two cannot drift.
pub(crate) fn render_deps(
    f: &mut Frame,
    area: Rect,
    name: &str,
    depends_on: &[String],
    dependents: &[&str],
) {
    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {}", name),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if depends_on.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let last = depends_on.len() - 1;
        for (i, dep) in depends_on.iter().enumerate() {
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

    render_panel(f, area, lines);
}

/// Renders the panel chrome (bordered "Dependencies" block) around `lines`.
/// Also used to draw the empty panel when nothing is selected.
pub(crate) fn render_panel(f: &mut Frame, area: Rect, lines: Vec<Line>) {
    let panel = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Dependencies "),
    );
    f.render_widget(panel, area);
}
