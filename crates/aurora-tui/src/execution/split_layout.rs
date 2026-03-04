use crate::app::{ExecutionState, LogViewState};
use crate::execution::{beam_list, log_panel};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

pub fn render_execution(
    f: &mut Frame,
    exec: &ExecutionState,
    log_state: &LogViewState,
    tick: u64,
    show_help: bool,
) {
    let area = f.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Split 30/70
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);

    beam_list::render_beam_list(f, exec, tick, split[0]);

    let beam = &exec.beams[log_state.beam_index];
    log_panel::render_log_panel(f, beam, log_state, split[1]);

    crate::widgets::status_bar::render_status_bar(
        f,
        outer[1],
        crate::widgets::status_bar::StatusContext::Execution { done: exec.done.is_some() },
    );

    if show_help {
        crate::widgets::help_popup::render_help_popup(
            f,
            area,
            crate::widgets::help_popup::HelpContext::Execution,
        );
    }
}
