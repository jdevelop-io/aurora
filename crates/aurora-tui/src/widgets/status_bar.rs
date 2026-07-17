use aurora_core::events::BeamStatus;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

const BAR_WIDTH: usize = 16;

// Palette: key in accent color, readable label, discreet separator.
const KEY: Color = Color::Cyan;
const LABEL: Color = Color::Gray;
const SEP: Color = Color::DarkGray;

// Hint sets: full if the width allows it, otherwise essential.
const RUNNING_FULL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("←→", "focus"),
    ("PgUp/Dn", "scroll"),
    ("g/G", "top/bottom"),
    ("/", "search"),
    ("y", "copy"),
    ("d", "deps"),
    ("?", "help"),
    ("q", "cancel"),
];
const RUNNING_ESSENTIAL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("←→", "focus"),
    ("/", "search"),
    ("?", "help"),
    ("q", "cancel"),
];
const DONE_FULL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("←→", "focus"),
    ("PgUp/Dn", "scroll"),
    ("g/G", "top/bottom"),
    ("/", "search"),
    ("y", "copy"),
    ("r", "rerun"),
    ("d", "deps"),
    ("w", "watch"),
    ("?", "help"),
    ("q", "quit"),
];
const DONE_ESSENTIAL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("/", "search"),
    ("r", "rerun"),
    ("?", "help"),
    ("q", "quit"),
];

/// Ratio (filled, empty) of the bar, default fixed width.
pub fn progress_fill(done: usize, total: usize) -> Option<(usize, usize)> {
    progress_fill_width(done, total, BAR_WIDTH)
}

/// Ratio (filled, empty) over a given width, or `None` if total or width is zero.
pub fn progress_fill_width(done: usize, total: usize, width: usize) -> Option<(usize, usize)> {
    if total == 0 || width == 0 {
        return None;
    }
    let filled = (done * width) / total;
    Some((filled, width - filled))
}

/// Concatenated hint text: « Tab focus · / search · q quit ».
pub fn hint_text(hints: &[(&str, &str)]) -> String {
    hints
        .iter()
        .map(|(k, l)| format!("{} {}", k, l))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Returns the full set if it fits within `width` columns, otherwise the essential one.
pub fn fit_hints<'a>(
    full: &'a [(&str, &str)],
    essential: &'a [(&str, &str)],
    width: usize,
) -> &'a [(&'a str, &'a str)] {
    if hint_text(full).chars().count() < width {
        full
    } else {
        essential
    }
}

/// Separator widths to justify `n` items with cumulative width
/// `content` over `target` columns (minimum separator 3: « · » surrounded
/// by spaces). `None` if there is a single item or if it does not fit.
pub fn justify_gaps(content: usize, n: usize, target: usize) -> Option<Vec<usize>> {
    if n <= 1 {
        return None;
    }
    let gaps = n - 1;
    if target < content + gaps * 3 {
        return None;
    }
    let total = target - content;
    let base = total / gaps;
    let rem = total % gaps;
    Some((0..gaps).map(|i| base + usize::from(i < rem)).collect())
}

/// Separator of width `width` (>= 3) with a centered dot.
fn separator(width: usize) -> String {
    let left = width / 2;
    let right = width - 1 - left;
    format!("{}·{}", " ".repeat(left), " ".repeat(right))
}

/// Display width of a (key, label) pair: « key label ».
fn pair_width(key: &str, label: &str) -> usize {
    key.chars().count() + 1 + label.chars().count()
}

fn key_label_spans(key: &str, label: &str) -> [Span<'static>; 2] {
    [
        Span::styled(format!("{} ", key), Style::default().fg(KEY)),
        Span::styled(label.to_string(), Style::default().fg(LABEL)),
    ]
}

/// Hints aligned to the left, minimal separator.
fn hint_spans(hints: &[(&str, &str)]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(SEP)));
        }
        spans.extend(key_label_spans(key, label));
    }
    spans
}

/// Hints justified over `width` columns if possible, otherwise aligned to the left.
fn hint_spans_justified(hints: &[(&str, &str)], width: usize) -> Vec<Span<'static>> {
    let content: usize = hints.iter().map(|(k, l)| pair_width(k, l)).sum();
    // 1 column of margin on the left and right.
    match justify_gaps(content, hints.len(), width.saturating_sub(2)) {
        Some(gaps) => {
            let mut spans = Vec::new();
            for (i, (key, label)) in hints.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled(
                        separator(gaps[i - 1]),
                        Style::default().fg(SEP),
                    ));
                }
                spans.extend(key_label_spans(key, label));
            }
            spans
        }
        None => hint_spans(hints),
    }
}

fn semantic(done: Option<bool>) -> (&'static str, &'static str, Color) {
    match done {
        None => ("⣦", "Running", Color::Yellow),
        Some(true) => ("✔", "Done", Color::Green),
        Some(false) => ("✕", "Failed", Color::Red),
    }
}

/// Count of finished beams by status, for the breakdown in the progress
/// line. `success` includes cached successes, `warning` counts tolerated
/// failures (allow_failure), `cancelled` counts cancelled beams (neutral category,
/// distinct from failures).
pub struct StatusBreakdown {
    pub success: usize,
    pub warning: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cancelled: usize,
}

impl StatusBreakdown {
    /// Counts finished beams by status. Unfinished statuses (Pending,
    /// Running) are ignored. `Cancelled` is counted separately, never as a failure.
    pub fn from_statuses<'a>(statuses: impl Iterator<Item = &'a BeamStatus>) -> Self {
        let mut b = StatusBreakdown {
            success: 0,
            warning: 0,
            failed: 0,
            skipped: 0,
            cancelled: 0,
        };
        for s in statuses {
            match s {
                BeamStatus::Success { .. } => b.success += 1,
                BeamStatus::FailedAllowed { .. } => b.warning += 1,
                BeamStatus::Failed { .. } => b.failed += 1,
                BeamStatus::Skipped { .. } => b.skipped += 1,
                BeamStatus::Cancelled => b.cancelled += 1,
                BeamStatus::Pending | BeamStatus::Running => {}
            }
        }
        b
    }

    /// Total number of finished beams (all categories counted here).
    pub fn done_count(&self) -> usize {
        self.success + self.warning + self.failed + self.skipped + self.cancelled
    }
}

/// Spans of the breakdown « (✔ n ✕ n ◌ n) », only the non-zero categories, with
/// their cumulative display width. Empty if no beam has finished.
fn breakdown_spans(b: &StatusBreakdown) -> (Vec<Span<'static>>, usize) {
    let parts = [
        (b.success, "✔", Color::Green),
        (b.warning, "⚠", Color::Yellow),
        (b.failed, "✕", Color::Red),
        (b.cancelled, "⊘", Color::Magenta),
        (b.skipped, "◌", Color::Cyan),
    ];
    let active: Vec<_> = parts.iter().filter(|(n, _, _)| *n > 0).collect();
    if active.is_empty() {
        return (Vec::new(), 0);
    }

    let mut spans = vec![Span::styled("(", Style::default().fg(SEP))];
    let mut width = 1;
    for (i, (n, sym, color)) in active.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ", Style::default().fg(SEP)));
            width += 1;
        }
        let text = format!("{} {}", sym, n);
        width += text.chars().count();
        spans.push(Span::styled(text, Style::default().fg(*color)));
    }
    spans.push(Span::styled(") ", Style::default().fg(SEP)));
    width += 2;
    (spans, width)
}

/// Footer line 1: status + global count + breakdown by status + proportional
/// progress bar + an optional discreet watch label on the right.
#[allow(clippy::too_many_arguments)]
pub fn render_progress_line(
    f: &mut Frame,
    area: Rect,
    done: Option<bool>,
    done_count: usize,
    total: usize,
    breakdown: &StatusBreakdown,
    watch_label: Option<&str>,
) {
    let (symbol, word, color) = semantic(done);
    let left = format!(" {} {}   ", symbol, word);
    let mut spans = vec![Span::styled(left.clone(), Style::default().fg(color))];

    // Reserve room for the watch label on the right so the bar, which is
    // otherwise sized to fill the rest of the line, does not push it off
    // screen.
    let label_w = watch_label.map_or(0, |l| l.chars().count() + 2);

    if total > 0 {
        let count = format!("{}/{} ", done_count, total);
        let (detail, detail_w) = breakdown_spans(breakdown);
        // Remaining width for the bar: total width - left - count -
        // breakdown - brackets - right margin - watch label.
        let used = left.chars().count() + count.chars().count() + detail_w + 3 + label_w;
        let bar_w = (area.width as usize).saturating_sub(used);
        spans.push(Span::styled(count, Style::default().fg(LABEL)));
        spans.extend(detail);
        if let Some((filled, empty)) = progress_fill_width(done_count, total, bar_w) {
            spans.push(Span::styled("[", Style::default().fg(SEP)));
            spans.push(Span::styled("█".repeat(filled), Style::default().fg(color)));
            spans.push(Span::styled("░".repeat(empty), Style::default().fg(SEP)));
            spans.push(Span::styled("]", Style::default().fg(SEP)));
        }
    }

    if let Some(label) = watch_label {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(label.to_string(), Style::default().fg(SEP)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Footer line 2: hints (full if it fits, otherwise essential).
pub fn render_hints_line(f: &mut Frame, area: Rect, done: Option<bool>) {
    let (full, essential) = match done {
        None => (RUNNING_FULL, RUNNING_ESSENTIAL),
        _ => (DONE_FULL, DONE_ESSENTIAL),
    };
    let hints = fit_hints(full, essential, area.width as usize);
    render_hints(f, area, hints);
}

/// Renders a hints line with the common palette (key in accent color,
/// readable label, discreet separator), justified over the available width.
/// Shared entry point between the execution screen and the picker.
pub fn render_hints(f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(hint_spans_justified(hints, area.width as usize));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The status-bar watch indicator, or `None` when watch is off. "watching" when
/// armed and idle; the longer form when a change was seen but the current run
/// has not finished yet.
pub fn watch_status_label(armed: bool, pending: bool) -> Option<&'static str> {
    match (armed, pending) {
        (false, _) => None,
        (true, false) => Some("watching"),
        (true, true) => Some("change detected, waiting for run to finish"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakdown_empty_when_all_zero() {
        let (spans, w) = breakdown_spans(&StatusBreakdown {
            success: 0,
            warning: 0,
            failed: 0,
            skipped: 0,
            cancelled: 0,
        });
        assert!(spans.is_empty());
        assert_eq!(w, 0);
    }

    #[test]
    fn breakdown_only_non_zero_categories() {
        // success + skipped active, failed omitted: "(", "✔ 6", " ", "◌ 1", ") ".
        let (spans, _w) = breakdown_spans(&StatusBreakdown {
            success: 6,
            warning: 0,
            failed: 0,
            skipped: 1,
            cancelled: 0,
        });
        assert_eq!(spans.len(), 5);
    }

    #[test]
    fn breakdown_all_three_categories() {
        // "(", "✔ 6", " ", "✕ 1", " ", "◌ 1", ") ".
        let (spans, _w) = breakdown_spans(&StatusBreakdown {
            success: 6,
            warning: 0,
            failed: 1,
            skipped: 1,
            cancelled: 0,
        });
        assert_eq!(spans.len(), 7);
    }

    #[test]
    fn breakdown_includes_warning_category() {
        // success + warning + failed + skipped all active:
        // "(", "✔ 6", " ", "⚠ 2", " ", "✕ 1", " ", "◌ 1", ") " => 9 spans.
        let (spans, _w) = breakdown_spans(&StatusBreakdown {
            success: 6,
            warning: 2,
            failed: 1,
            skipped: 1,
            cancelled: 0,
        });
        assert_eq!(spans.len(), 9);
    }

    #[test]
    fn breakdown_includes_cancelled_category() {
        // success + cancelled active: "(", "✔ 6", " ", "⊘ 2", ") " => 5 spans.
        let (spans, _w) = breakdown_spans(&StatusBreakdown {
            success: 6,
            warning: 0,
            failed: 0,
            skipped: 0,
            cancelled: 2,
        });
        assert_eq!(spans.len(), 5);
    }

    #[test]
    fn from_statuses_counts_cancelled_apart_from_failed() {
        use std::time::Duration;
        let statuses = [
            BeamStatus::Success {
                duration: Duration::ZERO,
                cached: false,
            },
            BeamStatus::Failed {
                exit_code: 1,
                duration: Duration::ZERO,
            },
            BeamStatus::Cancelled,
            BeamStatus::Cancelled,
            BeamStatus::Running,
            BeamStatus::Pending,
        ];
        let b = StatusBreakdown::from_statuses(statuses.iter());
        assert_eq!(b.success, 1);
        assert_eq!(
            b.failed, 1,
            "a Cancelled beam must not be counted as a failure"
        );
        assert_eq!(b.cancelled, 2);
        // Running and Pending are not finished: 1 + 1 + 2 = 4.
        assert_eq!(b.done_count(), 4);
    }
}

#[cfg(test)]
mod watch_label_tests {
    use super::watch_status_label;

    #[test]
    fn label_reflects_watch_state() {
        assert_eq!(watch_status_label(false, false), None);
        assert_eq!(watch_status_label(true, false), Some("watching"));
        assert_eq!(
            watch_status_label(true, true),
            Some("change detected, waiting for run to finish")
        );
    }
}
