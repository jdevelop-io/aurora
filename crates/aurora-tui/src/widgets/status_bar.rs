use ratatui::{layout::Rect, style::{Color, Style}, widgets::Paragraph, Frame};

pub enum StatusContext {
    Picker,
    Execution { done: bool },
    LogView,
}

pub fn render_status_bar(f: &mut Frame, area: Rect, ctx: StatusContext) {
    let help = match ctx {
        StatusContext::Picker => {
            " [↑↓] nav  [Space] sélec  [Tab] deps  [Enter] lancer  [?] aide  [q] quitter "
        }
        StatusContext::Execution { done: false } => {
            " [↑↓/jk] beam  [PgUp/Dn] scroll  [G] bas  [y] copier  [?] aide  [q] annuler "
        }
        StatusContext::Execution { done: true } => {
            " [↑↓/jk] beam  [r] re-run  [y] copier  [?] aide  [q] quitter "
        }
        StatusContext::LogView => {
            " [Esc] retour  [↑↓/PgUp/Dn] scroll  [G] bas  [y] copier  [q] quitter "
        }
    };
    let bar = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    f.render_widget(bar, area);
}
