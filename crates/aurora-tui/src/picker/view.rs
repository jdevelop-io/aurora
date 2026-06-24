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
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let selected_count = state.checked.iter().filter(|&&c| c).count();
    let title = if selected_count > 0 {
        format!(" Aurora — Choisir un beam ({} sélectionnés) ", selected_count)
    } else {
        " Aurora — Choisir un beam ".to_string()
    };

    let search = Paragraph::new(format!(" {} ", state.search))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(search, chunks[0]);

    // Zone centrale : liste + panel deps si show_deps
    let (list_area, deps_area) = if state.show_deps {
        let sub = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[1]);
        (sub[0], Some(sub[1]))
    } else {
        (chunks[1], None)
    };

    // Liste
    let filtered = state.filtered();

    if filtered.is_empty() {
        let message = if state.search.is_empty() {
            "Aucun beam disponible".to_string()
        } else {
            format!("Aucun beam ne correspond à « {} »", state.search)
        };
        let empty = Paragraph::new(message)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
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
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
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

        let list = List::new(items).block(Block::default().borders(Borders::ALL));
        f.render_widget(list, list_area);
    }

    if let Some(area) = deps_area {
        crate::picker::deps_panel::render_deps_panel(f, state, area);
    }

    // Help bar
    let help = if state.show_deps {
        " [↑↓] nav  [Space] sélec  [Tab] fermer deps  [Enter] lancer  [q] quitter "
    } else {
        " [↑↓] nav  [Space] sélec  [Tab] deps  [Enter] lancer  [q] quitter "
    };
    let bar = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    f.render_widget(bar, chunks[2]);
}

fn highlight_name(name: &str, indices: &[usize], selected: bool) -> Vec<Span<'static>> {
    let base_style = if selected {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let highlight_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
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
