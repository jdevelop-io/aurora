use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub enum HelpContext {
    Picker,
    Execution,
    LogView,
}

pub fn render_help_popup(f: &mut Frame, area: Rect, ctx: HelpContext) {
    let popup_width = 60u16.min(area.width.saturating_sub(4));
    let popup_height = 16u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let lines = match ctx {
        HelpContext::Picker => vec![
            Line::from(Span::styled(
                " Picker",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" ↑↓ / jk    Naviguer"),
            Line::from(" Space      Sélectionner/désélectionner"),
            Line::from(" Tab        Afficher les dépendances"),
            Line::from(" Enter      Lancer le beam sélectionné"),
            Line::from(" Esc / q    Quitter"),
            Line::from(""),
            Line::from(Span::styled(
                " Recherche",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" (typer)    Filtrer par nom/description"),
            Line::from(" Backspace  Effacer un caractère"),
        ],
        HelpContext::Execution => vec![
            Line::from(Span::styled(
                " Exécution",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" ↑↓ / jk    Naviguer beams (ou scroller logs si focus logs)"),
            Line::from(" Tab        Basculer focus beams / logs"),
            Line::from(" PgUp/Dn    Scroller les logs par page"),
            Line::from(" G          Aller au bas des logs"),
            Line::from(" /          Chercher dans les logs"),
            Line::from(" n / N      Correspondance suivante / précédente"),
            Line::from(" y          Copier les logs dans le clipboard"),
            Line::from(" r          Re-lancer le beam (si Failed/Cancelled)"),
            Line::from(" ?          Fermer cette aide"),
            Line::from(" q          Annuler et quitter"),
        ],
        HelpContext::LogView => vec![
            Line::from(Span::styled(
                " Vue logs",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" ↑↓ / jk    Scroller ligne par ligne"),
            Line::from(" PgUp/Dn    Scroller par page"),
            Line::from(" G          Aller au bas (reprend l'auto-scroll)"),
            Line::from(" y          Copier les logs"),
            Line::from(" Esc        Retour à la vue split"),
            Line::from(" q          Quitter"),
        ],
    };

    let popup = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Aide — ? pour fermer "),
        )
        .alignment(Alignment::Left);
    f.render_widget(popup, popup_area);
}
