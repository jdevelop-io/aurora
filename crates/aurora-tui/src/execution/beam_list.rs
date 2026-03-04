use crate::app::{BeamView, ExecutionState};
use aurora_core::scheduler::BeamStatus;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

const SPINNER_FRAMES: &[&str] = &["⣇", "⣦", "⣴", "⣸", "⢹", "⠻", "⠟", "⡏"];

pub fn render_beam_list(f: &mut Frame, state: &ExecutionState, tick: u64, area: Rect, focused: bool) {
    let title = " Aurora ";

    let items: Vec<ListItem> = state
        .beams
        .iter()
        .enumerate()
        .map(|(i, beam)| {
            let symbol = match &beam.status {
                BeamStatus::Running => {
                    SPINNER_FRAMES[(tick / 2 % SPINNER_FRAMES.len() as u64) as usize]
                }
                _ => beam.status_symbol(),
            };
            let color = status_color(&beam.status);
            let duration_str = duration_label(&beam.status, beam);
            let line = Line::from(vec![
                Span::styled(format!("  {}  ", symbol), Style::default().fg(color)),
                Span::styled(
                    format!("{:<20}", beam.name),
                    if i == state.selected {
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ),
                Span::styled(duration_str, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style),
    );
    f.render_widget(list, area);
}

fn duration_label(status: &BeamStatus, beam: &BeamView) -> String {
    match status {
        BeamStatus::Success { duration, .. } => format!(" [{:.2}s]", duration.as_secs_f32()),
        BeamStatus::Failed { duration, .. } => format!(" [{:.2}s]", duration.as_secs_f32()),
        BeamStatus::Running => {
            if let Some(t) = beam.started_at {
                format!(" [{:.0}s]", t.elapsed().as_secs_f32())
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

fn status_color(status: &BeamStatus) -> Color {
    match status {
        BeamStatus::Success { .. } => Color::Green,
        BeamStatus::Skipped { .. } => Color::Cyan,
        BeamStatus::Failed { .. } => Color::Red,
        BeamStatus::Cancelled => Color::Magenta,
        BeamStatus::Running => Color::Yellow,
        BeamStatus::Pending => Color::DarkGray,
    }
}
