use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

const BAR_WIDTH: usize = 16;

// Palette : touche en accent, libellé lisible, séparateur discret.
const KEY: Color = Color::Cyan;
const LABEL: Color = Color::Gray;
const SEP: Color = Color::DarkGray;

pub enum StatusContext {
    Picker,
    Execution { done: Option<bool>, done_count: usize, total: usize },
    LogView,
}

/// Ratio (plein, vide) de la barre de progression, ou `None` si `total == 0`.
pub fn progress_fill(done: usize, total: usize) -> Option<(usize, usize)> {
    if total == 0 {
        return None;
    }
    let filled = (done * BAR_WIDTH) / total;
    Some((filled, BAR_WIDTH - filled))
}

/// Texte concaténé des raccourcis : « Tab focus · / cherche · q quitter ».
pub fn hint_text(hints: &[(&str, &str)]) -> String {
    hints
        .iter()
        .map(|(k, l)| format!("{} {}", k, l))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Spans colorés des raccourcis : touche en cyan, libellé en gris clair,
/// séparateur discret.
fn hint_spans(hints: &[(&str, &str)]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(SEP)));
        }
        spans.push(Span::styled(format!("{} ", key), Style::default().fg(KEY)));
        spans.push(Span::styled(label.to_string(), Style::default().fg(LABEL)));
    }
    spans
}

/// Spans de la barre de progression : compteur, puis barre (plein en couleur
/// sémantique, vide discret). Vide si `total == 0`.
fn progress_spans(done: usize, total: usize, color: Color) -> Vec<Span<'static>> {
    match progress_fill(done, total) {
        None => vec![],
        Some((filled, empty)) => vec![
            Span::styled(format!("{}/{} ", done, total), Style::default().fg(LABEL)),
            Span::styled("[", Style::default().fg(SEP)),
            Span::styled("█".repeat(filled), Style::default().fg(color)),
            Span::styled("░".repeat(empty), Style::default().fg(SEP)),
            Span::styled("]", Style::default().fg(SEP)),
        ],
    }
}

pub fn render_status_bar(f: &mut Frame, area: Rect, ctx: StatusContext) {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];

    match ctx {
        StatusContext::Picker => spans.extend(hint_spans(&[
            ("↑↓", "nav"),
            ("Space", "sélec"),
            ("Enter", "lancer"),
            ("?", "aide"),
            ("q", "quitter"),
        ])),
        StatusContext::LogView => spans.extend(hint_spans(&[
            ("↑↓", "scroll"),
            ("/", "cherche"),
            ("?", "aide"),
            ("q", "quitter"),
        ])),
        StatusContext::Execution { done, done_count, total } => {
            let (symbol, word, color, hints): (&str, &str, Color, Vec<(&str, &str)>) = match done {
                None => (
                    "⣦",
                    "Running",
                    Color::Yellow,
                    vec![
                        ("↑↓", "beam"),
                        ("Tab", "focus"),
                        ("/", "cherche"),
                        ("?", "aide"),
                        ("q", "annuler"),
                    ],
                ),
                Some(true) => (
                    "✔",
                    "Done",
                    Color::Green,
                    vec![
                        ("↑↓", "beam"),
                        ("/", "cherche"),
                        ("r", "relancer"),
                        ("?", "aide"),
                        ("q", "quitter"),
                    ],
                ),
                Some(false) => (
                    "✕",
                    "Failed",
                    Color::Red,
                    vec![
                        ("↑↓", "beam"),
                        ("/", "cherche"),
                        ("r", "relancer"),
                        ("?", "aide"),
                        ("q", "quitter"),
                    ],
                ),
            };
            spans.push(Span::styled(
                format!("{} {}  ", symbol, word),
                Style::default().fg(color),
            ));
            let pspans = progress_spans(done_count, total, color);
            if !pspans.is_empty() {
                spans.extend(pspans);
                spans.push(Span::raw("   "));
            }
            spans.extend(hint_spans(&hints));
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
