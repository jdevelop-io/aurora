use crate::app::PickerState;
use crate::picker::fuzzy::match_indices;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn render_picker(f: &mut Frame, state: &PickerState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    // Zone centrale : panneau des beams (+ panel deps si show_deps).
    let (main_area, deps_area) = if state.show_deps {
        let sub = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);
        (sub[0], Some(sub[1]))
    } else {
        (chunks[0], None)
    };

    // Panneau des beams : titre « Aurora » identique au runner. La recherche est
    // une ligne fine à l'intérieur, plus d'encart bordé dédié. Le récap de
    // sélection vit dans le footer (ligne d'état), pas dans le titre.
    let selected_count = state.checked.iter().filter(|&&c| c).count();
    // Panneau actif : même style que le panneau des beams « focus » du runner
    // (bordure et titre en jaune), pour une couleur de titre identique.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(
            Line::from(Span::styled(" Aurora ", Style::default().fg(Color::Yellow))).left_aligned(),
        );
    let list_area = block.inner(main_area);
    f.render_widget(block, main_area);

    // La recherche n'a plus d'encart en haut de la liste : elle vit dans le
    // footer (invite `/`), comme la recherche de logs du runner.
    let filtered = state.filtered();

    if filtered.is_empty() {
        let message = if state.search.is_empty() {
            "Aucun beam disponible".to_string()
        } else {
            format!("Aucun beam ne correspond à « {} »", state.search)
        };
        let empty = Paragraph::new(message)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(empty, list_area);
    } else {
        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(display_i, (orig_idx, beam, _score))| {
                let is_selected = display_i == state.selected;
                let is_checked = state.checked[*orig_idx];
                let checkbox = if is_checked { "[x] " } else { "[ ] " };
                let prefix = if is_selected { "▶ " } else { "  " };

                let name_spans = if !state.search.is_empty() {
                    let indices = match_indices(&state.search, &beam.name);
                    highlight_name(&beam.name, &indices, is_selected)
                } else {
                    vec![Span::styled(
                        beam.name.clone(),
                        if is_selected {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    )]
                };

                let mut spans = vec![Span::raw(format!("{}{}", prefix, checkbox))];
                spans.extend(name_spans);
                if let Some(desc) = &beam.description {
                    spans.push(Span::styled(
                        format!("  {}", desc),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items);
        f.render_widget(list, list_area);
    }

    if let Some(area) = deps_area {
        crate::picker::deps_panel::render_deps_panel(f, state, area);
    }

    // Barre de raccourcis : même rendu que l'écran d'exécution.
    let deps_hint = if state.show_deps {
        ("d", "fermer deps")
    } else {
        ("d", "deps")
    };
    let hints = [
        ("/", "filtrer"),
        ("↑↓", "nav"),
        ("Space", "sélec"),
        deps_hint,
        ("Enter", "lancer"),
        ("Esc", "quitter"),
    ];

    // Footer sur 2 lignes, comme l'écran d'exécution : ligne d'état puis
    // raccourcis (ou invite de recherche `/` si le filtre est en saisie).
    let footer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(chunks[1]);

    let status = status_line(state, &filtered, selected_count);
    f.render_widget(Paragraph::new(status), footer[0]);
    if state.search_input {
        f.render_widget(search_bar(&state.search), footer[1]);
    } else {
        crate::widgets::status_bar::render_hints(f, footer[1], &hints);
    }
}

/// Invite de recherche affichée dans le footer pendant la saisie du filtre.
/// Calquée sur la barre de recherche des logs du runner.
fn search_bar(query: &str) -> Paragraph<'static> {
    let text = format!(" /{}   (Entrée valider, Esc effacer) ", query);
    Paragraph::new(text).style(Style::default().fg(Color::Yellow))
}

/// Ligne d'état du footer : nombre de résultats, sélection et aperçu de l'action
/// Entrée. Fait écho à la ligne d'état du runner.
fn status_line(
    state: &PickerState,
    filtered: &[(usize, &crate::app::PickerBeam, u32)],
    selected_count: usize,
) -> Line<'static> {
    let total = state.beams.len();
    let count_text = if state.search.is_empty() {
        format!("{} beams", total)
    } else {
        format!(
            "« {} » · {} / {} beams",
            state.search,
            filtered.len(),
            total
        )
    };

    let action_text = if filtered.is_empty() {
        "aucun résultat".to_string()
    } else if selected_count > 0 {
        let s = if selected_count > 1 { "s" } else { "" };
        format!(
            "{} sélectionné{s} · Entrée lance {} beam{s}",
            selected_count, selected_count
        )
    } else {
        let name = filtered
            .get(state.selected)
            .map(|(_, b, _)| b.name.clone())
            .unwrap_or_default();
        format!("Entrée lance « {} »", name)
    };

    Line::from(vec![
        Span::styled(" ◆ ", Style::default().fg(Color::Cyan)),
        Span::styled(count_text, Style::default().fg(Color::Gray)),
        Span::styled(" · ", Style::default().fg(Color::DarkGray)),
        Span::styled(action_text, Style::default().fg(Color::Gray)),
    ])
}

fn highlight_name(name: &str, indices: &[usize], selected: bool) -> Vec<Span<'static>> {
    let base_style = if selected {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let highlight_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let mut spans = vec![];
    for (i, ch) in name.char_indices() {
        let style = if indices.contains(&i) {
            highlight_style
        } else {
            base_style
        };
        spans.push(Span::styled(ch.to_string(), style));
    }
    spans
}
