use crate::app::{wrap_log_line, BeamView, LogKind, LogSearch, LogViewState};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
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

    let total_visual = lines.len() as u16;
    let inner_height = area.height.saturating_sub(2);

    let auto_indicator = if log_state.scroll_locked {
        " [scroll manuel]"
    } else {
        " [auto]"
    };
    // Indicateur de position en lignes logiques : on affiche la dernière ligne
    // visible (bas du panneau), bornée au contenu. Ainsi, collé au bas (auto),
    // l'indicateur atteint N/N.
    let position = if beam.stdout.is_empty() && beam.stderr.is_empty() {
        String::new()
    } else {
        let last_visual = total_visual.saturating_sub(1);
        let bottom_visual = log_state
            .scroll
            .saturating_add(inner_height.saturating_sub(1))
            .min(last_visual);
        let bottom = beam.logical_line_at_visual(bottom_visual, width) + 1;
        format!("  {}/{}", bottom, beam.log_line_count())
    };
    // Signale que les logs sont rejoués depuis le cache (dernière exécution),
    // sinon rien ne les distinguait d'une exécution fraîche.
    let cache_marker = if beam.is_cached() { " (cache)" } else { "" };
    let title = format!(
        " {} — Logs{}{}{} ",
        beam.name, cache_marker, auto_indicator, position
    );

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

    // Scrollbar verticale, seulement si le contenu déborde du panneau.
    if total_visual > inner_height {
        let mut sb_state = ScrollbarState::new(total_visual as usize)
            .viewport_content_length(inner_height as usize)
            .position(log_state.scroll as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        f.render_stateful_widget(scrollbar, area, &mut sb_state);
    }
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
