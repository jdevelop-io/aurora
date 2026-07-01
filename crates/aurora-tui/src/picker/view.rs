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

    // Central area: beams panel (+ deps panel if show_deps).
    let (main_area, deps_area) = if state.show_deps {
        let sub = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);
        (sub[0], Some(sub[1]))
    } else {
        (chunks[0], None)
    };

    // Beams panel: title "Aurora" identical to the runner. The search is
    // a thin line inside it, no more dedicated bordered box. The selection
    // recap lives in the footer (status line), not in the title.
    let selected_count = state.checked.iter().filter(|&&c| c).count();
    // Active panel: same style as the runner's "focused" beams panel
    // (border and title in yellow), for an identical title color.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(
            Line::from(Span::styled(" Aurora ", Style::default().fg(Color::Yellow))).left_aligned(),
        );
    let list_area = block.inner(main_area);
    f.render_widget(block, main_area);

    // The search no longer has a box above the list: it lives in the
    // footer (`/` prompt), like the runner's log search.
    let filtered = state.filtered();

    if filtered.is_empty() {
        let message = if state.search.is_empty() {
            "No beam available".to_string()
        } else {
            format!("No beam matches « {} »", state.search)
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

    // Hints bar: same rendering as the execution screen.
    let deps_hint = if state.show_deps {
        ("d", "close deps")
    } else {
        ("d", "deps")
    };
    let hints = [
        ("/", "filter"),
        ("↑↓", "nav"),
        ("Space", "sel"),
        deps_hint,
        ("Enter", "run"),
        ("Esc", "quit"),
    ];

    // Footer on 2 lines, like the execution screen: status line then
    // hints (or the `/` search prompt if the filter is being typed).
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

/// Search prompt shown in the footer while typing the filter.
/// Modeled on the runner's log search bar.
fn search_bar(query: &str) -> Paragraph<'static> {
    let text = format!(" /{}   (Enter confirm, Esc clear) ", query);
    Paragraph::new(text).style(Style::default().fg(Color::Yellow))
}

/// Footer status line: number of results, selection and a preview of the
/// Enter action. Echoes the runner's status line.
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
        "no results".to_string()
    } else if selected_count > 0 {
        let s = if selected_count > 1 { "s" } else { "" };
        format!(
            "{} selected{s} · Enter runs {} beam{s}",
            selected_count, selected_count
        )
    } else {
        let name = filtered
            .get(state.selected)
            .map(|(_, b, _)| b.name.clone())
            .unwrap_or_default();
        format!("Enter runs « {} »", name)
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
