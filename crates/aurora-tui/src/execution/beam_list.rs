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

pub fn render_beam_list(
    f: &mut Frame,
    state: &ExecutionState,
    tick: u64,
    area: Rect,
    focused: bool,
) {
    // Titre : « Aurora », complété par le filtre actif le cas échéant.
    let title = if state.beam_filter.is_empty() {
        " Aurora ".to_string()
    } else {
        format!(" Aurora — /{} ", state.beam_filter)
    };

    let items: Vec<ListItem> = state
        .visible_indices()
        .into_iter()
        .map(|i| {
            let beam = &state.beams[i];
            let symbol = match &beam.status {
                BeamStatus::Running => {
                    SPINNER_FRAMES[(tick / 2 % SPINNER_FRAMES.len() as u64) as usize]
                }
                _ => beam.status_symbol(),
            };
            let color = status_color(&beam.status);
            let duration_str = duration_label(&beam.status, beam);

            // Largeur intérieure du panneau : on réserve la durée à droite et on
            // ajuste le nom pour que la ligne ne déborde jamais de la bordure.
            let inner = area.width.saturating_sub(2) as usize;
            let prefix = format!("  {}  ", symbol);
            let prefix_w = prefix.chars().count();
            let dur_w = duration_str.chars().count();
            let name_budget = inner.saturating_sub(prefix_w + dur_w);
            let name = fit_name(&beam.name, name_budget);

            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(color)),
                Span::styled(
                    name,
                    if i == state.selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
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
        BeamStatus::Success { duration, .. }
        | BeamStatus::Failed { duration, .. }
        | BeamStatus::FailedAllowed { duration, .. } => {
            format!(" [{}]", compact_duration(duration.as_secs_f64(), true))
        }
        BeamStatus::Running => {
            if let Some(t) = beam.started_at {
                format!(" [{}]", compact_duration(t.elapsed().as_secs_f64(), false))
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// Format compact et borné d'une durée : sous la minute « 12.34s » (ou « 12s »
/// sans décimales), puis « 1m02s », puis « 1h05m ». Largeur bornée à ~7 colonnes.
fn compact_duration(secs: f64, decimals: bool) -> String {
    if secs < 60.0 {
        if decimals {
            format!("{:.2}s", secs)
        } else {
            format!("{:.0}s", secs)
        }
    } else if secs < 3600.0 {
        let m = (secs / 60.0).floor() as u64;
        let s = (secs % 60.0).floor() as u64;
        format!("{}m{:02}s", m, s)
    } else {
        let h = (secs / 3600.0).floor() as u64;
        let m = ((secs % 3600.0) / 60.0).floor() as u64;
        format!("{}h{:02}m", h, m)
    }
}

/// Ajuste un nom de beam à `budget` colonnes : complète par des espaces s'il est
/// plus court, le tronque avec « … » s'il est plus long.
fn fit_name(name: &str, budget: usize) -> String {
    let count = name.chars().count();
    if budget == 0 {
        String::new()
    } else if count <= budget {
        let mut s = name.to_string();
        s.push_str(&" ".repeat(budget - count));
        s
    } else {
        let mut s: String = name.chars().take(budget - 1).collect();
        s.push('…');
        s
    }
}

fn status_color(status: &BeamStatus) -> Color {
    match status {
        BeamStatus::Success { .. } => Color::Green,
        BeamStatus::Skipped { .. } => Color::Cyan,
        BeamStatus::Failed { .. } => Color::Red,
        BeamStatus::FailedAllowed { .. } => Color::Yellow,
        BeamStatus::Cancelled => Color::Magenta,
        BeamStatus::Running => Color::Yellow,
        BeamStatus::Pending => Color::DarkGray,
    }
}

#[cfg(test)]
mod tests {
    use super::{compact_duration, fit_name};

    #[test]
    fn compact_sub_minute_with_decimals() {
        assert_eq!(compact_duration(0.0, true), "0.00s");
        assert_eq!(compact_duration(12.34, true), "12.34s");
        assert_eq!(compact_duration(59.99, true), "59.99s");
    }

    #[test]
    fn compact_sub_minute_without_decimals() {
        assert_eq!(compact_duration(3.0, false), "3s");
        assert_eq!(compact_duration(59.0, false), "59s");
    }

    #[test]
    fn compact_minutes() {
        assert_eq!(compact_duration(60.0, true), "1m00s");
        assert_eq!(compact_duration(62.0, true), "1m02s");
        assert_eq!(compact_duration(3599.0, true), "59m59s");
    }

    #[test]
    fn compact_hours() {
        assert_eq!(compact_duration(3600.0, true), "1h00m");
        assert_eq!(compact_duration(3900.0, true), "1h05m");
        assert_eq!(compact_duration(90000.0, true), "25h00m");
    }

    #[test]
    fn pads_short_name_to_budget() {
        assert_eq!(fit_name("fmt", 6), "fmt   ");
    }

    #[test]
    fn name_exactly_budget_is_unchanged() {
        assert_eq!(fit_name("coverage", 8), "coverage");
    }

    #[test]
    fn truncates_long_name_with_ellipsis() {
        // budget 6 : 5 chars + …
        assert_eq!(fit_name("node_modules", 6), "node_…");
        assert_eq!(fit_name("node_modules", 6).chars().count(), 6);
    }

    #[test]
    fn budget_zero_is_empty() {
        assert_eq!(fit_name("anything", 0), "");
    }
}
