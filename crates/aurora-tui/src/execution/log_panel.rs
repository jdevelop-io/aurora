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

    // Manual wrap (by character): we build the visual lines ourselves
    // so the scroll offset corresponds exactly to the logical indices.
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
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD)
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
        " [manual scroll]"
    } else {
        " [auto]"
    };
    // Position indicator in logical lines: shows the last visible
    // line (bottom of the panel), bounded by the content. Thus, stuck to the
    // bottom (auto), the indicator reaches N/N.
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
    // Flags that the logs are replayed from the cache (last run),
    // otherwise nothing would distinguish them from a fresh run.
    let cache_marker = if beam.is_cached() { " (cache)" } else { "" };
    let title = format!(
        " {}: Logs{}{}{} ",
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

    // Vertical scrollbar, only if the content overflows the panel.
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

/// Byte ranges (into `haystack`) of the case-insensitive occurrences of
/// `needle_lower` (already lowercase).
///
/// Lowercasing can change byte lengths (`K` U+212A -> `k`, `İ` -> `i̇`), and
/// even when the total length is preserved the internal offsets shift, so a
/// range computed on the lowercased text is not valid on the original.
/// We therefore lowercase char by char while recording, for each lowered
/// byte, the start offset of the original char it came from; matches found in
/// the lowered text then map back to original char boundaries.
fn match_ranges(haystack: &str, needle_lower: &str) -> Vec<(usize, usize)> {
    if needle_lower.is_empty() {
        return vec![];
    }

    let mut lower = String::with_capacity(haystack.len());
    // `lower_to_orig[i]` is the byte offset in `haystack` of the original char
    // that produced lowered byte `i`; the final entry maps the end of the
    // lowered text to the end of the original.
    let mut lower_to_orig = Vec::with_capacity(haystack.len() + 1);
    let mut encode = [0u8; 4];
    for (orig_off, ch) in haystack.char_indices() {
        for lc in ch.to_lowercase() {
            let encoded = lc.encode_utf8(&mut encode);
            for _ in 0..encoded.len() {
                lower_to_orig.push(orig_off);
            }
            lower.push_str(encoded);
        }
    }
    lower_to_orig.push(haystack.len());

    lower
        .match_indices(needle_lower)
        .map(|(start, m)| (lower_to_orig[start], lower_to_orig[start + m.len()]))
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
