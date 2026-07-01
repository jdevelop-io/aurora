use crate::app::{ExecutionState, FocusPanel, LogSearch, LogViewState};
use crate::execution::{beam_list, deps_panel, log_panel};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::Paragraph,
    Frame,
};

pub fn render_execution(
    f: &mut Frame,
    exec: &ExecutionState,
    log_state: &LogViewState,
    search: &LogSearch,
    tick: u64,
    show_help: bool,
) {
    let area = f.area();
    // Footer on 2 lines: status + bar, then hints (or search).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    // Horizontal split: beams / logs (30/70), with a dependency panel
    // inserted (25/25/50) when `show_deps` is active (key « d »).
    let beams_focused = exec.focus == FocusPanel::Beams;
    let beam = &exec.beams[log_state.beam_index];

    if exec.show_deps {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(50),
            ])
            .split(outer[0]);

        beam_list::render_beam_list(f, exec, tick, split[0], beams_focused);
        deps_panel::render_deps_panel(f, exec, split[1]);
        log_panel::render_log_panel(f, beam, log_state, Some(search), split[2], !beams_focused);
    } else {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(outer[0]);

        beam_list::render_beam_list(f, exec, tick, split[0], beams_focused);
        log_panel::render_log_panel(f, beam, log_state, Some(search), split[1], !beams_focused);
    }

    let footer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(outer[1]);

    let total = exec.beams.len();
    // Breakdown by status: success (cache included), warnings (tolerated failures),
    // failures, cancelled (neutral category, distinct from failures), skipped.
    let breakdown = crate::widgets::status_bar::StatusBreakdown::from_statuses(
        exec.beams.iter().map(|b| &b.status),
    );
    let done_count = breakdown.done_count();

    // Line 1: always the status + the bar (stays visible even while searching).
    crate::widgets::status_bar::render_progress_line(
        f, footer[0], exec.done, done_count, total, &breakdown,
    );

    // Line 2: beam filter prompt, or log search prompt, or hints.
    // The filter takes priority while its input is active.
    if exec.filter_input {
        f.render_widget(filter_bar(&exec.beam_filter), footer[1]);
    } else if search.is_active() {
        f.render_widget(search_bar(search), footer[1]);
    } else {
        crate::widgets::status_bar::render_hints_line(f, footer[1], exec.done);
    }

    if show_help {
        crate::widgets::help_popup::render_help_popup(
            f,
            area,
            crate::widgets::help_popup::HelpContext::Execution,
        );
    }
}

/// Prompt shown while typing the beam list filter.
fn filter_bar(filter: &str) -> Paragraph<'static> {
    let text = format!(" /{}   (Enter confirm, Esc clear) ", filter);
    Paragraph::new(text).style(Style::default().fg(Color::Yellow))
}

/// Status bar shown while searching in the logs.
fn search_bar(search: &LogSearch) -> Paragraph<'static> {
    let count = if search.query.is_empty() {
        String::new()
    } else if search.match_count() == 0 {
        "  [no results]".to_string()
    } else {
        format!("  [{}/{}]", search.current + 1, search.match_count())
    };
    let hint = if search.input_active {
        "   (Enter confirm, Esc cancel)"
    } else {
        "   [n/N] next/previous  [Esc] clear"
    };
    let text = format!(" /{}{}{} ", search.query, count, hint);
    Paragraph::new(text).style(Style::default().fg(Color::Yellow))
}
