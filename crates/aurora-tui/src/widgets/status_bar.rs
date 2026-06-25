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

// Jeux de raccourcis : complet si la largeur le permet, sinon essentiel.
const RUNNING_FULL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("←→", "focus"),
    ("PgUp/Dn", "scroll"),
    ("g/G", "haut/bas"),
    ("/", "cherche"),
    ("y", "copier"),
    ("?", "aide"),
    ("q", "annuler"),
];
const RUNNING_ESSENTIAL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("←→", "focus"),
    ("/", "cherche"),
    ("?", "aide"),
    ("q", "annuler"),
];
const DONE_FULL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("←→", "focus"),
    ("PgUp/Dn", "scroll"),
    ("g/G", "haut/bas"),
    ("/", "cherche"),
    ("y", "copier"),
    ("r", "relancer"),
    ("?", "aide"),
    ("q", "quitter"),
];
const DONE_ESSENTIAL: &[(&str, &str)] = &[
    ("↑↓", "beam"),
    ("/", "cherche"),
    ("r", "relancer"),
    ("?", "aide"),
    ("q", "quitter"),
];

/// Ratio (plein, vide) de la barre, largeur fixe par défaut.
pub fn progress_fill(done: usize, total: usize) -> Option<(usize, usize)> {
    progress_fill_width(done, total, BAR_WIDTH)
}

/// Ratio (plein, vide) sur une largeur donnée, ou `None` si total ou largeur nuls.
pub fn progress_fill_width(done: usize, total: usize, width: usize) -> Option<(usize, usize)> {
    if total == 0 || width == 0 {
        return None;
    }
    let filled = (done * width) / total;
    Some((filled, width - filled))
}

/// Texte concaténé des raccourcis : « Tab focus · / cherche · q quitter ».
pub fn hint_text(hints: &[(&str, &str)]) -> String {
    hints
        .iter()
        .map(|(k, l)| format!("{} {}", k, l))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Renvoie le jeu complet s'il tient dans `width` colonnes, sinon l'essentiel.
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

/// Largeurs des séparateurs pour justifier `n` éléments de largeur cumulée
/// `content` sur `target` colonnes (séparateur minimal 3 : « · » entouré
/// d'espaces). `None` si un seul élément ou si ça ne tient pas.
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

/// Séparateur de largeur `width` (>= 3) avec le point centré.
fn separator(width: usize) -> String {
    let left = width / 2;
    let right = width - 1 - left;
    format!("{}·{}", " ".repeat(left), " ".repeat(right))
}

/// Largeur d'affichage d'un couple (touche, libellé) : « touche libellé ».
fn pair_width(key: &str, label: &str) -> usize {
    key.chars().count() + 1 + label.chars().count()
}

fn key_label_spans(key: &str, label: &str) -> [Span<'static>; 2] {
    [
        Span::styled(format!("{} ", key), Style::default().fg(KEY)),
        Span::styled(label.to_string(), Style::default().fg(LABEL)),
    ]
}

/// Raccourcis alignés à gauche, séparateur minimal.
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

/// Raccourcis justifiés sur `width` colonnes si possible, sinon alignés à gauche.
fn hint_spans_justified(hints: &[(&str, &str)], width: usize) -> Vec<Span<'static>> {
    let content: usize = hints.iter().map(|(k, l)| pair_width(k, l)).sum();
    // 1 colonne de marge à gauche et à droite.
    match justify_gaps(content, hints.len(), width.saturating_sub(2)) {
        Some(gaps) => {
            let mut spans = Vec::new();
            for (i, (key, label)) in hints.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled(separator(gaps[i - 1]), Style::default().fg(SEP)));
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

/// Décompte des beams terminés par statut, pour le détail de la ligne de
/// progression. `success` inclut les succès en cache, `failed` inclut les annulés.
pub struct StatusBreakdown {
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Spans du détail « (✔n ✕n ◌n) », uniquement les catégories non nulles, avec
/// leur largeur d'affichage cumulée. Vide si aucun beam terminé.
fn breakdown_spans(b: &StatusBreakdown) -> (Vec<Span<'static>>, usize) {
    let parts = [
        (b.success, "✔", Color::Green),
        (b.failed, "✕", Color::Red),
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
        let text = format!("{}{}", sym, n);
        width += text.chars().count();
        spans.push(Span::styled(text, Style::default().fg(*color)));
    }
    spans.push(Span::styled(") ", Style::default().fg(SEP)));
    width += 2;
    (spans, width)
}

/// Ligne 1 du footer : état + compteur global + détail par statut + barre de
/// progression proportionnelle.
pub fn render_progress_line(
    f: &mut Frame,
    area: Rect,
    done: Option<bool>,
    done_count: usize,
    total: usize,
    breakdown: &StatusBreakdown,
) {
    let (symbol, word, color) = semantic(done);
    let left = format!(" {} {}   ", symbol, word);
    let mut spans = vec![Span::styled(left.clone(), Style::default().fg(color))];

    if total > 0 {
        let count = format!("{}/{} ", done_count, total);
        let (detail, detail_w) = breakdown_spans(breakdown);
        // Largeur restante pour la barre : largeur totale - gauche - compteur -
        // détail - crochets - marge droite.
        let used = left.chars().count() + count.chars().count() + detail_w + 3;
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

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Ligne 2 du footer : raccourcis (complets si ça tient, sinon essentiels).
pub fn render_hints_line(f: &mut Frame, area: Rect, done: Option<bool>) {
    let (full, essential) = match done {
        None => (RUNNING_FULL, RUNNING_ESSENTIAL),
        _ => (DONE_FULL, DONE_ESSENTIAL),
    };
    let hints = fit_hints(full, essential, area.width as usize);
    render_hints(f, area, hints);
}

/// Rend une ligne de raccourcis avec la palette commune (touche en accent,
/// libellé lisible, séparateur discret), justifiée sur la largeur disponible.
/// Point d'entrée partagé entre l'écran d'exécution et le picker.
pub fn render_hints(f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(hint_spans_justified(hints, area.width as usize));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakdown_empty_when_all_zero() {
        let (spans, w) = breakdown_spans(&StatusBreakdown { success: 0, failed: 0, skipped: 0 });
        assert!(spans.is_empty());
        assert_eq!(w, 0);
    }

    #[test]
    fn breakdown_only_non_zero_categories() {
        // success + skipped actifs, failed omis : "(", "✔6", " ", "◌1", ") ".
        let (spans, _w) = breakdown_spans(&StatusBreakdown { success: 6, failed: 0, skipped: 1 });
        assert_eq!(spans.len(), 5);
    }

    #[test]
    fn breakdown_all_three_categories() {
        // "(", "✔6", " ", "✕1", " ", "◌1", ") ".
        let (spans, _w) = breakdown_spans(&StatusBreakdown { success: 6, failed: 1, skipped: 1 });
        assert_eq!(spans.len(), 7);
    }
}
