use ratatui::{layout::Rect, style::{Color, Style}, widgets::Paragraph, Frame};

const BAR_WIDTH: usize = 16;

pub enum StatusContext {
    Picker,
    Execution { done: Option<bool>, done_count: usize, total: usize },
    LogView,
}

/// Génère une barre ASCII de progression. Retourne "" si total == 0.
pub fn build_progress_bar(done_count: usize, total: usize) -> String {
    if total == 0 {
        return String::new();
    }
    let filled = (done_count * BAR_WIDTH) / total;
    let empty = BAR_WIDTH - filled;
    format!(
        "[{}{}] {}/{}",
        "█".repeat(filled),
        "░".repeat(empty),
        done_count,
        total
    )
}

pub fn render_status_bar(f: &mut Frame, area: Rect, ctx: StatusContext) {
    let (text, color) = match ctx {
        StatusContext::Picker => (
            " [↑↓] nav  [Space] sélec  [Tab] deps  [Enter] lancer  [?] aide  [q] quitter ".to_string(),
            Color::DarkGray,
        ),
        StatusContext::Execution { done: None, done_count, total } => {
            let bar = build_progress_bar(done_count, total);
            let prefix = if bar.is_empty() {
                " ⣦ Running... ".to_string()
            } else {
                format!(" ⣦ Running...  {} ", bar)
            };
            (
                format!("{}[↑↓/jk] beam  [Tab] focus  [PgUp/Dn] scroll  [G] bas  [y] copier  [?] aide  [q] annuler ", prefix),
                Color::DarkGray,
            )
        }
        StatusContext::Execution { done: Some(true), done_count, total } => {
            let bar = build_progress_bar(done_count, total);
            let prefix = if bar.is_empty() {
                " ✔ Done ".to_string()
            } else {
                format!(" ✔ Done  {} ", bar)
            };
            (
                format!("{}[↑↓/jk] beam  [y] copier  [?] aide  [q] quitter ", prefix),
                Color::Green,
            )
        }
        StatusContext::Execution { done: Some(false), done_count, total } => {
            let bar = build_progress_bar(done_count, total);
            let prefix = if bar.is_empty() {
                " ✕ Failed ".to_string()
            } else {
                format!(" ✕ Failed  {} ", bar)
            };
            (
                format!("{}[↑↓/jk] beam  [y] copier  [?] aide  [q] quitter ", prefix),
                Color::Red,
            )
        }
        StatusContext::LogView => (
            " [Esc] retour  [↑↓/PgUp/Dn] scroll  [G] bas  [y] copier  [q] quitter ".to_string(),
            Color::DarkGray,
        ),
    };
    let bar = Paragraph::new(text.as_str()).style(Style::default().fg(color));
    f.render_widget(bar, area);
}
