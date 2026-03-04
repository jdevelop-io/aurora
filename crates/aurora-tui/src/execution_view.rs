use crate::app::{App, AppMode};
use aurora_core::scheduler::BeamStatus;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

const SPINNER_FRAMES: &[&str] = &["⣇", "⣦", "⣴", "⣸", "⢹", "⠻", "⠟", "⡏"];

pub fn render_execution(f: &mut Frame, app: &App, tick: u64) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    if app.mode == AppMode::LogView {
        render_log_view(f, app, chunks[0]);
    } else {
        render_beam_list(f, app, tick, chunks[0]);
    }
    render_status_bar(f, app, chunks[1]);
}

fn render_beam_list(f: &mut Frame, app: &App, tick: u64, area: Rect) {
    let items: Vec<ListItem> = app
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
            let duration_str = match &beam.status {
                BeamStatus::Success { duration, .. } => {
                    format!(" [{:.2}s]", duration.as_secs_f32())
                }
                BeamStatus::Failed { duration, .. } => {
                    format!(" [{:.2}s]", duration.as_secs_f32())
                }
                BeamStatus::Running => {
                    if let Some(t) = beam.started_at {
                        format!(" [{:.0}s]", t.elapsed().as_secs_f32())
                    } else {
                        String::new()
                    }
                }
                _ => String::new(),
            };

            let line = Line::from(vec![
                Span::styled(format!("  {}  ", symbol), Style::default().fg(color)),
                Span::styled(
                    format!("{:<20}", beam.name),
                    Style::default()
                        .fg(if i == app.selected {
                            Color::White
                        } else {
                            Color::Gray
                        })
                        .add_modifier(if i == app.selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(duration_str, Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = match &app.mode {
        AppMode::Done { success: true } => " Aurora ✔ Done ",
        AppMode::Done { success: false } => " Aurora ✕ Failed ",
        _ => " Aurora  Running... ",
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(list, area);
}

fn render_log_view(f: &mut Frame, app: &App, area: Rect) {
    let beam = &app.beams[app.selected];
    let mut lines: Vec<Line> = beam.stdout.iter().map(|l| Line::from(l.as_str())).collect();
    if !beam.stderr.is_empty() {
        lines.push(Line::from(Span::styled(
            "── stderr ──",
            Style::default().fg(Color::Red),
        )));
        lines.extend(beam.stderr.iter().map(|l| {
            Line::from(Span::styled(l.as_str(), Style::default().fg(Color::Red)))
        }));
    }
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} — Logs ", beam.name)),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.log_scroll, 0));
    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        AppMode::LogView => " [Esc] retour  [↑↓] scroll  [q] quitter ",
        AppMode::Done { .. } => " [↑↓] naviguer  [Enter] logs  [q] quitter ",
        AppMode::Running => " [↑↓] naviguer  [Enter] logs  [q] annuler ",
    };
    let bar = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    f.render_widget(bar, area);
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
