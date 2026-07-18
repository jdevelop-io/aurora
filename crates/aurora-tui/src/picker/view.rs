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
                // A beam requiring arguments has no way to receive them here
                // (no value input in the picker): its row is dimmed to signal
                // it cannot be launched or checked from this view.
                let dimmed = beam.requires_args;
                let checkbox = if is_checked { "[x] " } else { "[ ] " };
                let prefix = if is_selected { "▶ " } else { "  " };
                let prefix_style = if dimmed {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default()
                };

                let name_spans = if !state.search.is_empty() {
                    let indices = match_indices(&state.search, &beam.name);
                    highlight_name(&beam.name, &indices, is_selected, dimmed)
                } else {
                    vec![Span::styled(
                        beam.name.clone(),
                        if dimmed {
                            Style::default().fg(Color::DarkGray)
                        } else if is_selected {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    )]
                };

                let mut spans = vec![Span::styled(
                    format!("{}{}", prefix, checkbox),
                    prefix_style,
                )];
                spans.extend(name_spans);
                // Param signature suffix (e.g. " <version> [env=staging]"):
                // informational, dimmed, and not part of the fuzzy-matched name.
                let suffix = &beam.signature[beam.name.len()..];
                if !suffix.is_empty() {
                    spans.push(Span::styled(
                        suffix.to_string(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
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
    // The notice (a parameterized beam refused at Enter/Space) takes priority
    // for the one frame it is shown: it is a direct answer to the key the
    // user just pressed, ahead of the filter prompt or the static hints.
    if let Some(notice) = &state.notice {
        f.render_widget(notice_bar(notice), footer[1]);
    } else if state.search_input {
        f.render_widget(search_bar(&state.search), footer[1]);
    } else {
        crate::widgets::status_bar::render_hints(f, footer[1], &hints);
    }
}

/// Advisory line shown when a parameterized beam is refused at Enter/Space.
/// Same footer slot and color family as the search prompt, so it reads as
/// part of the same status area rather than an unrelated overlay.
fn notice_bar(message: &str) -> Paragraph<'static> {
    Paragraph::new(format!(" ⚠ {} ", message)).style(Style::default().fg(Color::Yellow))
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
        match filtered.get(state.selected).map(|(_, b, _)| *b) {
            // A beam that requires arguments cannot be launched here: Enter only
            // surfaces its usage notice, so the preview says so rather than
            // promising a run that will not happen.
            Some(beam) if beam.requires_args => "Enter shows usage".to_string(),
            Some(beam) => format!("Enter runs « {} »", beam.name),
            None => String::new(),
        }
    };

    Line::from(vec![
        Span::styled(" ◆ ", Style::default().fg(Color::Cyan)),
        Span::styled(count_text, Style::default().fg(Color::Gray)),
        Span::styled(" · ", Style::default().fg(Color::DarkGray)),
        Span::styled(action_text, Style::default().fg(Color::Gray)),
    ])
}

fn highlight_name(
    name: &str,
    indices: &[usize],
    selected: bool,
    dimmed: bool,
) -> Vec<Span<'static>> {
    // A dimmed row (a beam that requires arguments) reads as uniformly
    // DarkGray: the match highlight is suppressed too, so the whole row keeps
    // the same "cannot launch from here" look whether or not the filter is
    // active.
    if dimmed {
        return name
            .chars()
            .map(|ch| Span::styled(ch.to_string(), Style::default().fg(Color::DarkGray)))
            .collect();
    }
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
