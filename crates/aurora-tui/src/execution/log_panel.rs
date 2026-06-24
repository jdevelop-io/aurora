use crate::app::{wrap_log_line, BeamView, LogKind, LogSearch, LogViewState};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_log_panel(
    f: &mut Frame,
    beam: &BeamView,
    log_state: &LogViewState,
    search: Option<&LogSearch>,
    area: Rect,
    focused: bool,
) {
    let needle = search
        .filter(|s| !s.query.is_empty())
        .map(|s| s.query.to_lowercase());
    let current_line = search.and_then(|s| s.current_line());

    // Wrap manuel (par caractères) : on construit les lignes visuelles nous-mêmes
    // pour que l'offset de scroll corresponde exactement aux index logiques.
    let width = area.width.saturating_sub(2);
    let mut lines: Vec<Line> = Vec::new();
    for (idx, (text, kind)) in beam.iter_log_lines().enumerate() {
        let base = match kind {
            LogKind::Stdout => Style::default(),
            LogKind::Stderr | LogKind::Separator => Style::default().fg(Color::Red),
            LogKind::Placeholder => Style::default().fg(Color::DarkGray),
        };
        let highlightable = matches!(kind, LogKind::Stdout | LogKind::Stderr);
        for segment in wrap_log_line(text, width) {
            let line = match &needle {
                Some(n) if highlightable => {
                    let ranges = match_ranges(segment, n);
                    if ranges.is_empty() {
                        Line::from(Span::styled(segment.to_string(), base))
                    } else {
                        let hl = if current_line == Some(idx) {
                            Style::default().fg(Color::Black).bg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                        };
                        Line::from(highlight_spans(segment, &ranges, base, hl))
                    }
                }
                _ => Line::from(Span::styled(segment.to_string(), base)),
            };
            lines.push(line);
        }
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
        .scroll((log_state.scroll, 0));
    f.render_widget(paragraph, area);
}

/// Plages d'octets des occurrences de `needle_lower` (déjà en minuscules) dans
/// `haystack`, casse insensible. Si la mise en minuscules change la longueur en
/// octets (cas Unicode rares), surligne la ligne entière par sécurité.
fn match_ranges(haystack: &str, needle_lower: &str) -> Vec<(usize, usize)> {
    let hay_lower = haystack.to_lowercase();
    if hay_lower.len() != haystack.len() {
        return if hay_lower.contains(needle_lower) {
            vec![(0, haystack.len())]
        } else {
            vec![]
        };
    }
    hay_lower
        .match_indices(needle_lower)
        .map(|(start, m)| (start, start + m.len()))
        .collect()
}

fn highlight_spans(
    text: &str,
    ranges: &[(usize, usize)],
    base: Style,
    hl: Style,
) -> Vec<Span<'static>> {
    let mut spans = vec![];
    let mut last = 0;
    for &(start, end) in ranges {
        if start > last {
            spans.push(Span::styled(text[last..start].to_string(), base));
        }
        spans.push(Span::styled(text[start..end].to_string(), hl));
        last = end;
    }
    if last < text.len() {
        spans.push(Span::styled(text[last..].to_string(), base));
    }
    spans
}
