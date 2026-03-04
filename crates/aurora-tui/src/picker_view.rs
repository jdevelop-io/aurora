use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub struct PickerState {
    pub beams: Vec<PickerBeam>,
    pub selected: usize,
    pub search: String,
    pub show_deps: bool,
}

pub struct PickerBeam {
    pub name: String,
    pub description: Option<String>,
    pub depends_on: Vec<String>,
}

impl PickerState {
    pub fn filtered(&self) -> Vec<&PickerBeam> {
        if self.search.is_empty() {
            self.beams.iter().collect()
        } else {
            self.beams
                .iter()
                .filter(|b| {
                    b.name.contains(&self.search)
                        || b.description
                            .as_deref()
                            .map(|d| d.contains(&self.search))
                            .unwrap_or(false)
                })
                .collect()
        }
    }

    pub fn selected_beam(&self) -> Option<&PickerBeam> {
        self.filtered().into_iter().nth(self.selected)
    }
}

pub fn render_picker(f: &mut Frame, state: &PickerState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let search = Paragraph::new(format!(" {} ", state.search))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Aurora — Choisir un beam "),
        );
    f.render_widget(search, chunks[0]);

    let filtered = state.filtered();
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, beam)| {
            let selected = i == state.selected;
            let prefix = if selected { "▶ " } else { "  " };
            let desc = beam.description.as_deref().unwrap_or("");
            let line = Line::from(vec![Span::styled(
                format!("{}{:<20}  {}", prefix, beam.name, desc),
                if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                },
            )]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL));
    f.render_widget(list, chunks[1]);

    let help = Paragraph::new(
        " [↑↓] naviguer  [/] rechercher  [Tab] dépendances  [Enter] lancer  [q] quitter ",
    )
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, chunks[2]);
}
